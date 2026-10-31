"""Offline OCR admission rejects wrong bytes, host ABI and incomplete closures."""
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import linux_ocr_runtime as ocr

ROOT = Path(__file__).resolve().parent.parent


@unittest.skipUnless(os.name == 'posix', 'Linux dependency files require POSIX permissions')
class LinuxOcrTests(unittest.TestCase):
    def setUp(self):
        capacity=patch('linux_dependencies.shutil.disk_usage',return_value=shutil._ntuple_diskusage(8*1024**3,2*1024**3,6*1024**3))
        capacity.start();self.addCleanup(capacity.stop)

    def fixture(self, root, members=None, *, profile=ocr.PROFILE):
        for name in profile.source:
            target = root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            if name != profile.config:
                shutil.copyfile(ROOT / name, target)
        elf = b'\x7fELF\x02\x01\x01' + bytes(11) + b'\x3e\x00' + bytes(44)
        files = {name: b'fixture data' for name in profile.required}
        files.update({name: elf for name in profile.required if name.startswith(('bin/', 'lib/')) or name.endswith('.so')})
        archive = root / 'ocr.tar.gz'
        with archive.open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, filename='') as compressed, tarfile.open(fileobj=compressed, mode='w') as output:
            for name, data in (files if members is None else members).items():
                entry = tarfile.TarInfo(name); entry.size = len(data)
                output.addfile(entry, io.BytesIO(data))
        pin = {'format': 1, 'host': 'linux-x64', 'archive': {'file': archive.name, 'size_bytes': archive.stat().st_size, 'sha256': hashlib.sha256(archive.read_bytes()).hexdigest()}}
        (root / profile.config).write_text(json.dumps(pin))
        return archive

    def test_build_produces_repeatable_inventory_and_keeps_existing_component(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); archive = self.fixture(root); target = root / 'component'
            value = ocr.build(root, archive, target)
            self.assertEqual(value['host'], 'linux-x64')
            self.assertEqual(ocr.build(root, archive, target), value)
            self.assertEqual(ocr.verify(root, target, host='linux-x64'), value)
            with patch('linux_ocr_runtime.host_key', return_value='macos-arm64'), self.assertRaisesRegex(ValueError, 'host'):
                ocr.verify(root, target)
            (target / 'tessdata/eng.traineddata').write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'integrity'):
                ocr.build(root, archive, target)
            self.assertEqual((target / 'tessdata/eng.traineddata').read_bytes(), b'tampered')

    def test_source_changes_extra_files_modes_and_aliases_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); archive = self.fixture(root); target = root / 'component'
            ocr.build(root, archive, target)
            extra = target / 'extra'; extra.write_bytes(b'not inventoried')
            with self.assertRaisesRegex(ValueError, 'inventory'): ocr.verify(root, target, host='linux-x64')
            extra.unlink()
            program = target / 'bin/tesseract'; program.chmod(0o644)
            with self.assertRaisesRegex(ValueError, 'permissions'): ocr.verify(root, target, host='linux-x64')
            program.chmod(0o755)
            alias = root / 'alias'; alias.symlink_to(target, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'alias'): ocr.verify(root, alias, host='linux-x64')
            (root / 'scripts/linux-ocr/run.py').write_text('changed launcher')
            with self.assertRaisesRegex(ValueError, 'source'): ocr.verify(root, target, host='linux-x64')

    def test_archive_checksum_missing_members_paths_and_elf_abi_fail_before_activation(self):
        for scenario in ('checksum', 'missing', 'path', 'abi'):
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory() as folder:
                root = Path(folder); archive = self.fixture(root)
                if scenario == 'checksum': archive.write_bytes(archive.read_bytes() + b'changed')
                elif scenario == 'missing': archive = self.fixture(root, {'bin/tesseract': b'only one member'})
                elif scenario == 'path': archive = self.fixture(root, {'../outside': b'no'})
                else:
                    elf = b'\x7fELF\x02\x01\x01' + bytes(11) + b'\xb7\x00' + bytes(44)
                    archive = self.fixture(root, {'bin/tesseract': elf})
                with self.assertRaises(ValueError): ocr.build(root, archive, root / 'component')
                self.assertFalse((root / 'component').exists())
                self.assertFalse((root / 'outside').exists())

    def test_component_parent_paths_and_budget_are_bounded(self):
        from client_host_entry import checked_record
        row = {'path': ocr.PREFIX + 'run.py', 'size': 1, 'sha256': 'a' * 64, 'mode': 0o644}
        self.assertEqual(ocr.bundled_records([row]), [checked_record(row)])
        for rows in ([row, row], [{**row, 'path': 'runtime/services/bin/other'}], [{**row, 'size': ocr.MAX_BYTES + 1}]):
            with self.assertRaises(ValueError): ocr.bundled_records(rows)

    def test_malformed_component_metadata_fails_as_an_admission_error(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); archive = self.fixture(root); target = root / 'component'
            original = ocr.build(root, archive, target); path = target / 'manifest.json'
            for rows in (None, 'bad', [None], [], original['files'] * 2):
                path.write_text(json.dumps({**original, 'files': rows}))
                with self.assertRaises(ValueError): ocr.verify(root, target, host='linux-x64')

    def test_archive_entry_budgets_and_types_are_checked_before_writing(self):
        entry = tarfile.TarInfo('tessdata/eng.traineddata'); entry.size = 1
        self.assertEqual(ocr.archive_member(entry, set(), ocr.MAX_BYTES - 1), (entry.name, ocr.MAX_BYTES))
        empty = tarfile.TarInfo('licenses/empty.txt')
        self.assertEqual(ocr.archive_member(empty, set(), 0), (empty.name, 0))
        seen = {str(i) for i in range(ocr.MAX_FILES - 1)}
        self.assertEqual(ocr.archive_member(entry, seen, 0), (entry.name, 1))
        for names, total in [({entry.name}, 0), (seen | {'last'}, 0), (set(), ocr.MAX_BYTES)]:
            with self.assertRaises(ValueError): ocr.archive_member(entry, names, total)
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.DIRTYPE):
            entry.type = kind
            with self.assertRaises(ValueError): ocr.archive_member(entry, set(), 0)
        entry.type = tarfile.REGTYPE; entry.size = -1
        with self.assertRaises(ValueError): ocr.archive_member(entry, set(), 0)

    def test_elf_header_and_host_identity_require_exact_supported_values(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'native'
            valid = b'\x7fELF\x02\x01\x01' + bytes(11) + b'\x3e\x00' + bytes(44)
            path.write_bytes(valid); ocr.elf_check(path)
            for data in (valid[:-1], b'bad!' + valid[4:], valid[:5] + b'\x02' + valid[6:], valid[:18] + b'\xb7\x00' + valid[20:]):
                path.write_bytes(data)
                with self.assertRaises(ValueError): ocr.elf_check(path)
        for value in (None, {}, {'format': True, 'host': 'linux-x64'}, {'format': 2, 'host': 'linux-x64'}, {'format': 1, 'host': 'macos-arm64'}):
            with self.assertRaises(ValueError): ocr.checked_identity(value, 'linux-x64')
        with self.assertRaises(ValueError): ocr.checked_identity({'format': 1, 'host': 'macos-arm64'}, 'macos-arm64')

    def test_directory_only_trees_are_bounded_before_inventory_sorting(self):
        import linux_dependencies as dependencies
        with tempfile.TemporaryDirectory() as temporary, patch('linux_dependencies.MAX_FILES', 3):
            root = Path(temporary)
            for i in range(6): (root / str(i)).mkdir()
            self.assertEqual(dependencies.inventory(root), [])
            (root / 'seventh').mkdir()
            with self.assertRaisesRegex(ValueError, 'budget'): dependencies.inventory(root)

    def test_dependency_factory_preserves_the_exact_space_reserve(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);archive=self.fixture(root);destination=root/'component'
            minimum=ocr.RESERVE+ocr.MAX_BYTES
            with patch('linux_dependencies.shutil.disk_usage',return_value=shutil._ntuple_diskusage(minimum,1,minimum-1)):
                with self.assertRaisesRegex(ValueError,'4 GiB'):ocr.build(root,archive,destination)
            self.assertFalse(destination.exists())
            with patch('linux_dependencies.shutil.disk_usage',return_value=shutil._ntuple_diskusage(minimum,0,minimum)):
                self.assertEqual(ocr.build(root,archive,destination)['host'],'linux-x64')


if __name__ == '__main__':
    unittest.main()
