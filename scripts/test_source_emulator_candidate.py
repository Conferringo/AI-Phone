import errno
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from artifact_io import digest
import package_delivery as delivery
import source_emulator_candidate as candidate


SOURCE = 'a' * 40


def write_json(path, value):
    path.write_text(json.dumps(value))


class SourceEmulatorCandidateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / 'SOURCE-MANIFEST.json').write_text('reviewed source fixture')
        self.work = self.root / 'attempt'
        dist = self.work / 'build/dist'
        dist.mkdir(parents=True)
        # Derive the fixture name from the pinned upstream table, not the retainer.
        toolchain = candidate.ROOT / 'scripts/emulator-source-build/fixtures/android/build/python/aemu/platform/toolchains.py'
        distribution = candidate.producer.checked_distribution(toolchain, candidate.TARGET)
        self.archive = dist / ('sdk-repo-' + distribution + '-emulator-' + candidate.producer.BUILD_ID + '.zip')
        with zipfile.ZipFile(self.archive, 'w') as archive:
            archive.writestr('emulator/fixture.txt', 'synthetic source-emulator output')
        self.artifact = {'path': self.archive.name, 'bytes': self.archive.stat().st_size, 'sha256': digest(self.archive)}
        self.receipt = {'format': 1, 'host': candidate.HOST, 'stage': 'complete', 'status': 'built',
                        'source_commit': SOURCE, 'source_manifest_sha256': digest(self.root / 'SOURCE-MANIFEST.json'),
                        'source_lock_sha256': candidate.producer.LOCK_SHA256, 'selected_projects': 66,
                        'artifact_bytes': self.artifact['bytes'], 'artifact_sha256': self.artifact['sha256']}
        self.result = {'producer': 'aosp-source-main-emulator', 'host': candidate.HOST, 'target': candidate.TARGET,
                       'build_status': 'built', 'build_id': candidate.producer.BUILD_ID,
                       'source_lock_sha256': candidate.producer.LOCK_SHA256, 'artifact': self.artifact}
        self.save_receipts()

    def save_receipts(self):
        write_json(self.work / 'public-receipt.json', self.receipt)
        write_json(self.work / 'build/build-result.json', self.result)

    def native_fixture(self, root, operation, path, *args):
        if operation == 'verify':
            value = json.loads(path.read_text())
            return {'verified': True, 'sha256': value['sha256'], 'size': value['size']}
        self.assertEqual(operation, 'split')
        folder, expected = args
        self.assertEqual(digest(path), expected)
        folder.mkdir()
        part = folder / (path.name + '.part-0001')
        shutil.copyfile(path, part)
        manifest = folder / (path.name + '.volumes.json')
        write_json(manifest, {'format': 1, 'file_name': path.name, 'size': path.stat().st_size,
                             'sha256': expected, 'part_size': 1024**3,
                             'parts': [{'name': part.name, 'size': part.stat().st_size, 'sha256': digest(part)}]})
        return {'manifest': manifest.name, 'manifest_sha256': digest(manifest)}

    def stage(self):
        with patch.object(delivery, 'native', side_effect=self.native_fixture):
            return candidate.stage(self.root, self.work, SOURCE)

    def test_current_upstream_distribution_is_retained_without_claiming_client_qualification(self):
        original = self.archive.read_bytes()
        report = self.stage()
        (report.parent / 'private-build.log').write_text('must not upload')
        with patch.object(delivery, 'native', side_effect=self.native_fixture):
            plan = candidate.upload_plan(self.root, report, SOURCE)
        self.assertEqual(plan['kind'], 'source-emulator-review')
        self.assertFalse(plan['binary_publication'])
        names = {row['name'] for row in plan['files']}
        self.assertEqual(names, {'source-emulator-review.json', self.archive.name + '.volumes.json',
                                 self.archive.name + '.part-0001'})
        self.assertEqual(self.archive.read_bytes(), original)
        self.assertFalse((report.parent / self.archive.name).exists())
        for row in plan['files']:
            self.assertEqual(digest(self.root / 'dist' / row['path']), row['sha256'])
        value = json.loads(report.read_text())
        self.assertEqual(value['android35_root_boot'], 'not-executed')
        self.assertEqual(value['source_dependency_closure'], 'unverified')
        self.assertEqual(value['redistribution'], 'review-pending')
        self.assertFalse(value['user_state_bundled'])
        with self.assertRaises((KeyError, ValueError)):
            delivery.upload_plan(self.root, value, SOURCE)

    def test_mismatched_source_lock_inventory_and_noncomplete_receipts_stop_before_staging(self):
        for field, replacement in [('source_commit', 'b'*40), ('source_manifest_sha256', '0'*64),
                                   ('source_lock_sha256', '0'*64), ('selected_projects', 65),
                                   ('status', 'unknown'), ('stage', 'compile'), ('artifact_bytes', 0),
                                   ('artifact_sha256', '0'*64)]:
            with self.subTest(field=field):
                write_json(self.work / 'public-receipt.json', {**self.receipt, field: replacement})
                with self.assertRaises(ValueError):
                    candidate.stage(self.root, self.work, SOURCE)
                self.assertFalse((self.root / 'dist').exists())

    def test_unexpected_producer_and_archive_path_are_rejected(self):
        for field, replacement in [('host', 'macos-arm64'), ('target', 'darwin_aarch64'),
                                   ('build_id', 'another-build'), ('build_status', 'planned')]:
            with self.subTest(field=field):
                write_json(self.work / 'build/build-result.json', {**self.result, field: replacement})
                with self.assertRaises(ValueError):
                    candidate.checked_build(self.root, self.work, SOURCE)
        for name in ['../../private.zip', 'sdk-repo-linux_x64-emulator-' + candidate.producer.BUILD_ID + '.zip']:
            write_json(self.work / 'build/build-result.json', {**self.result, 'artifact': {**self.artifact, 'path': name}})
            with self.assertRaises(ValueError):
                candidate.checked_build(self.root, self.work, SOURCE)

    def test_changed_non_zip_or_aliased_build_output_is_rejected(self):
        self.archive.write_bytes(b'not a zip')
        self.artifact.update(bytes=self.archive.stat().st_size, sha256=digest(self.archive))
        self.receipt.update(artifact_bytes=self.artifact['bytes'], artifact_sha256=self.artifact['sha256'])
        self.save_receipts()
        with self.assertRaisesRegex(ValueError, 'not a ZIP'):
            candidate.checked_build(self.root, self.work, SOURCE)
        self.archive.unlink()
        self.archive.symlink_to(self.root / 'SOURCE-MANIFEST.json')
        with self.assertRaisesRegex(ValueError, 'differs from the built'):
            candidate.checked_build(self.root, self.work, SOURCE)

    def test_failure_cleans_only_owned_staging_and_preserves_original_build(self):
        dist = self.root / 'dist'
        dist.mkdir()
        unrelated = dist / 'retained-evidence.txt'
        unrelated.write_bytes(b'keep')
        with patch.object(delivery, 'native', side_effect=RuntimeError('synthetic split failure')):
            with self.assertRaisesRegex(RuntimeError, 'synthetic split failure'):
                candidate.stage(self.root, self.work, SOURCE)
        self.assertEqual(list(dist.iterdir()), [unrelated])
        self.assertEqual(digest(self.archive), self.artifact['sha256'])

    def test_plan_rejects_changed_parts_wrong_source_and_approval_flags(self):
        report = self.stage()
        original = json.loads(report.read_text())
        for field, value in [('source_commit', 'b'*40), ('source_manifest_sha256', '0'*64),
                             ('binary_publication', True), ('user_state_bundled', True),
                             ('binary_publication', 0), ('user_state_bundled', 0),
                             ('redistribution', 'approved'), ('android35_root_boot', 'passed')]:
            with self.subTest(field=field, value=value):
                write_json(report, {**original, field: value})
                with self.assertRaises(ValueError):
                    candidate.upload_plan(self.root, report, SOURCE)
        write_json(report, original)
        part = next(report.parent.glob('*.volumes/*.part-*'))
        part.write_bytes(b'changed')
        with patch.object(delivery, 'native', side_effect=self.native_fixture):
            with self.assertRaisesRegex(ValueError, 'volume changed'):
                candidate.upload_plan(self.root, report, SOURCE)

    def test_cross_device_staging_copies_but_other_link_errors_are_preserved(self):
        destination = self.root / 'copy.zip'
        with patch.object(candidate.os, 'link', side_effect=OSError(errno.EXDEV, 'cross-device')):
            candidate.link_or_copy(self.archive, destination)
        self.assertEqual(destination.read_bytes(), self.archive.read_bytes())
        with patch.object(candidate.os, 'link', side_effect=PermissionError('denied')):
            with self.assertRaises(PermissionError):
                candidate.link_or_copy(self.archive, self.root / 'denied.zip')
        self.assertFalse((self.root / 'denied.zip').exists())


if __name__ == '__main__':
    unittest.main()
