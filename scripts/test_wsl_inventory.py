"""Owned-rootfs normalization keeps admission strictly regular-file based."""
import hashlib
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import wsl_backend as backend
import wsl_backend_factory as factory


OS_RELEASE = b'ID=ubuntu\nVERSION_ID="24.04"\n'


def rootfs_tree(root):
    for name in backend.REQUIRED - {'etc/os-release'}:
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b'fixture-' + name.encode())
    vendor = root / 'usr/lib/os-release'
    vendor.parent.mkdir(parents=True, exist_ok=True)
    vendor.write_bytes(OS_RELEASE)
    (root / 'etc/os-release').symlink_to('../usr/lib/os-release')
    license_file = root / 'usr/share/doc/base-files/copyright'
    license_file.parent.mkdir(parents=True)
    license_file.write_bytes(b'fixture-license')


def archive_tree(root, destination):
    with tarfile.open(destination, 'w:gz', dereference=False) as archive:
        for path in sorted(root.rglob('*')):
            archive.add(path, arcname=path.relative_to(root).as_posix(), recursive=False)


class WslInventoryTests(unittest.TestCase):
    def test_factory_normalizes_pinned_base_alias_before_strict_admission(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / 'rootfs'
            rootfs_tree(root)
            archive = Path(folder) / 'rootfs.tar.gz'
            archive_tree(root, archive)
            with self.assertRaisesRegex(ValueError, 'Incomplete'):
                backend.inspect_rootfs(archive)
            # Production hook is absent in the failing recipe. A missing hook must
            # still fail the final real archive admission, rather than merely a mock.
            normalize = getattr(factory, 'normalize_os_release', None)
            if normalize is not None: normalize(root)
            archive_tree(root, archive)
            inventory = backend.inspect_rootfs(archive)
            self.assertFalse((root / 'etc/os-release').is_symlink())
            self.assertEqual(inventory['required_files']['etc/os-release'], hashlib.sha256(OS_RELEASE).hexdigest())
            self.assertEqual(set(inventory['required_files']), backend.REQUIRED)
            self.assertEqual(inventory['base_license_files'], ['usr/share/doc/base-files/copyright'])
            capsule = Path(folder) / 'backend.payload'
            backend.publish_capsule(archive, capsule, {'fixture': True})
            self.assertEqual(backend.verify_capsule(capsule)['inventory'], inventory)

    def test_recipe_invokes_real_normalizer_after_copy_and_before_verification(self):
        recipe = factory.dockerfile({'engine_image': 'fixture-engine', 'base_image': 'fixture-base'})
        calls = [shlex.split(line[4:]) for line in recipe.splitlines()
                 if line.startswith('RUN ') and 'normalize_os_release' in line]
        self.assertEqual(len(calls), 1)
        command = calls[0]
        self.assertEqual(command[:4], ['/opt/aiphone/tools/python/bin/python3', '-I', '-B', '-c'])
        self.assertLess(recipe.index('COPY scripts/'), recipe.index('normalize_os_release'))
        self.assertLess(recipe.index('normalize_os_release'), recipe.index('wsl_backend_entry.py --verify'))
        # Execute the exact inline Python with only the owned fixture root and
        # bootstrap path substituted; no Docker, shell package manager or host root.
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            rootfs_tree(root)
            scripts = str(Path(factory.__file__).parent)
            inline = command[4].replace('"/opt/aiphone/bootstrap"', repr(scripts)).replace('normalize_os_release("/")', 'normalize_os_release(' + repr(str(root)) + ')')
            bootstrap = 'import sys;sys.path.insert(0,' + repr(str(Path(backend.__file__).parent)) + ');sys.path.insert(1,' + repr(str(Path(__import__('artifact_io').__file__).parent)) + ');'
            subprocess.run([sys.executable, '-I', '-B', '-c', bootstrap + inline], check=True, capture_output=True)
            self.assertEqual((root / 'etc/os-release').read_bytes(), OS_RELEASE)
            self.assertFalse((root / 'etc/os-release').is_symlink())

    def test_regular_result_is_idempotent_and_both_expected_link_spellings_are_contained(self):
        for link in ('../usr/lib/os-release', '/usr/lib/os-release'):
            with self.subTest(link=link), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                rootfs_tree(root)
                release = root / 'etc/os-release'
                release.unlink(); release.symlink_to(link)
                factory.normalize_os_release(root)
                before = release.stat()
                factory.normalize_os_release(root)
                self.assertEqual((release.stat().st_ino, release.stat().st_mtime_ns), (before.st_ino, before.st_mtime_ns))
                self.assertEqual(release.read_bytes(), OS_RELEASE)
                self.assertEqual((root / 'usr/lib/os-release').read_bytes(), OS_RELEASE)
                self.assertEqual(release.stat().st_mode & 0o777, 0o644)

    def test_unknown_or_cyclic_or_missing_target_preserves_original_alias(self):
        for link in ('../missing', '../../../outside', 'os-release', '../usr/lib/another', '/foreign/os-release'):
            with self.subTest(link=link), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                rootfs_tree(root)
                release = root / 'etc/os-release'
                release.unlink(); release.symlink_to(link)
                with self.assertRaises(ValueError): factory.normalize_os_release(root)
                self.assertEqual(os.readlink(release), link)
                self.assertEqual((root / 'usr/lib/os-release').read_bytes(), OS_RELEASE)
                self.assertFalse(list((root / 'etc').glob('.aiphone-os-release-*')))

    def test_unverified_vendor_and_changed_regular_file_are_preserved(self):
        for kind in ('missing', 'link', 'directory', 'empty', 'oversized', 'changed-regular'):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                rootfs_tree(root)
                vendor = root / 'usr/lib/os-release'
                release = root / 'etc/os-release'
                if kind == 'changed-regular':
                    release.unlink(); release.write_bytes(b'preserve-unknown')
                else:
                    vendor.unlink()
                    if kind == 'link': vendor.symlink_to('../../etc/os-release')
                    elif kind == 'directory': vendor.mkdir()
                    elif kind == 'empty': vendor.write_bytes(b'')
                    elif kind == 'oversized': vendor.write_bytes(b'x' * 65537)
                with self.assertRaises(ValueError): factory.normalize_os_release(root)
                if kind == 'changed-regular': self.assertEqual(release.read_bytes(), b'preserve-unknown')
                else: self.assertEqual(os.readlink(release), '../usr/lib/os-release')
                self.assertFalse(list((root / 'etc').glob('.aiphone-os-release-*')))

    def test_link_parent_and_atomic_replace_failure_preserve_original_state(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            rootfs_tree(root)
            (root / 'usr/lib').rename(root / 'usr/elsewhere')
            (root / 'usr/lib').symlink_to('elsewhere', target_is_directory=True)
            with self.assertRaises(ValueError): factory.normalize_os_release(root)
            self.assertEqual(os.readlink(root / 'etc/os-release'), '../usr/lib/os-release')
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            rootfs_tree(root)
            with patch.object(factory.os, 'replace', side_effect=OSError('fixture write failure')):
                with self.assertRaises(OSError): factory.normalize_os_release(root)
            self.assertEqual(os.readlink(root / 'etc/os-release'), '../usr/lib/os-release')
            self.assertEqual((root / 'usr/lib/os-release').read_bytes(), OS_RELEASE)
            self.assertFalse(list((root / 'etc').glob('.aiphone-os-release-*')))

    def test_missing_diagnostic_has_only_fixed_required_paths_and_license_count(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / 'rootfs'
            rootfs_tree(root)
            factory.normalize_os_release(root)
            (root / 'opt/aiphone/tools/node/bin/node').unlink()
            (root / 'usr/share/doc/base-files/copyright').unlink()
            sensitive = root / 'opt/arbitrary-sensitive-filename'
            sensitive.write_text('fixture-sensitive-content')
            archive = Path(folder) / 'rootfs.tar.gz'
            archive_tree(root, archive)
            with self.assertRaises(ValueError) as raised: backend.inspect_rootfs(archive)
            self.assertEqual(str(raised.exception), 'Incomplete WSL backend rootfs or license inventory: missing=opt/aiphone/tools/node/bin/node; license_count=0')
            self.assertNotIn('sensitive', str(raised.exception))


if __name__ == '__main__': unittest.main()
