"""Native image handoff retains source and byte identity without user state."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

from artifact_io import digest
import factory_image_candidate as candidate
import test_container_images as fixtures

SOURCE = 'a' * 40


class FactoryImageCommandTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='aiphone-image-command-')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        (self.root/'dist').mkdir()
        self.report = self.root/'dist'/'report.json'
        self.report.write_text('{"fixture": true}\n')
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        subprocess.run(['git', '-C', str(self.root), 'add', 'dist/report.json'], check=True)
        subprocess.run(['git', '-C', str(self.root), '-c', 'user.name=Fixture',
                        '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture'], check=True)
        self.result = self.root/'result.json'

    def command(self, *, fail=False, result=True):
        arguments = ['build', str(self.root/'work')]
        if result:
            arguments += ['--result', str(self.result)]
        script = '''
import subprocess, sys
from pathlib import Path
import factory_image_candidate as candidate
candidate.ROOT = Path(sys.argv[1])
def build(*arguments):
    print('image preparation log', flush=True)
    subprocess.run([sys.executable, '-c', 'print("native child output")'], check=True)
    if sys.argv[2] == 'fail':
        raise RuntimeError('native producer failed')
    return candidate.ROOT/'dist'/'report.json'
candidate.build = build
candidate.main(sys.argv[3:])
'''
        environment = {**os.environ, 'PYTHONPATH': str(Path(candidate.__file__).parent)}
        return subprocess.run([sys.executable, '-B', '-c', script, str(self.root),
                               'fail' if fail else 'success', *arguments],
                              env=environment, capture_output=True, text=True)

    def test_build_result_is_parseable_when_python_and_native_children_log(self):
        process = self.command()
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertIn('image preparation log', process.stdout)
        self.assertIn('native child output', process.stdout)
        self.assertEqual(json.loads(self.result.read_bytes()), {
            'report': 'dist/report.json', 'metadata_sha256': digest(self.report)})

    def test_failed_native_producer_does_not_emit_a_success_receipt(self):
        process = self.command(fail=True)
        self.assertNotEqual(process.returncode, 0)
        self.assertIn('native producer failed', process.stderr)
        self.assertFalse(self.result.exists())

    def test_missing_result_file_is_rejected_before_running_the_producer(self):
        process = self.command(result=False)
        self.assertEqual(process.returncode, 2)
        self.assertIn('--result', process.stderr)
        self.assertNotIn('image preparation log', process.stdout)
        self.assertFalse(self.result.exists())


class FactoryImageCandidateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='aiphone-image-handoff-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        (self.root / 'SOURCE-MANIFEST.json').write_text('source fixture')
        self.payload, self.inventory = fixtures.ContainerImageTests().fixture(self.root)
        self.report = candidate.stage(self.root, self.payload, SOURCE)
        self.identity = digest(self.report)

    def test_relocated_capsule_is_source_bound_and_only_two_explicit_assets_are_retained(self):
        (self.report.parent / 'private.log').write_text('must stay local')
        plan = candidate.upload_plan(self.root, self.report, SOURCE)
        self.assertEqual({row['name'] for row in plan['files']}, {candidate.BODY, candidate.METADATA})
        prepared = candidate.admit(self.root, self.report.parent, self.identity, SOURCE)
        self.assertEqual(prepared.manifest, self.inventory)
        self.assertEqual(self.payload.read_bytes(), prepared.payload.read_bytes())
        self.assertFalse(plan['binary_publication'])
        self.assertFalse(json.loads(self.report.read_text())['user_state_bundled'])

    def test_changed_source_publication_or_component_kind_cannot_be_readmitted(self):
        original = json.loads(self.report.read_text())
        for field, value in [('source_commit', 'b'*40), ('source_manifest_sha256', '0'*64),
                             ('binary_publication', True), ('binary_publication', 0), ('user_state_bundled', True),
                             ('redistribution', 'approved'), ('architecture', 'amd64'),
                             ('kind', 'complete-client'), ('format', True)]:
            with self.subTest(field=field):
                self.report.write_text(json.dumps({**original, field: value}))
                with self.assertRaises(ValueError):
                    candidate.admit(self.root, self.report.parent, digest(self.report), SOURCE)

    def test_wrong_metadata_digest_changed_payload_and_alias_are_rejected(self):
        with self.assertRaises(ValueError):
            candidate.admit(self.root, self.report.parent, '0'*64, SOURCE)
        body = self.report.parent / candidate.BODY
        original = body.read_bytes()
        body.write_bytes(b'changed payload')
        with self.assertRaises(ValueError):
            candidate.admit(self.root, self.report.parent, self.identity, SOURCE)
        body.unlink()
        self.payload.write_bytes(original)
        body.symlink_to(self.payload)
        with self.assertRaises(ValueError):
            candidate.admit(self.root, self.report.parent, self.identity, SOURCE)

    def remote(self):
        plan = candidate.upload_plan(self.root, self.report, SOURCE)
        return {'draft': True, 'published_at': None, 'target_commitish': SOURCE,
                'tag_name': 'candidate-'+SOURCE[:12]+'-123-'+'b'*32,
                'assets': [{'name': row['name'], 'size': row['size'], 'state': 'uploaded',
                            'digest': 'sha256:'+row['sha256']} for row in plan['files']]}

    def test_remote_handoff_requires_unpublished_matching_source_and_exact_asset_inventory(self):
        remote = self.remote()
        candidate.checked_remote(remote, remote['tag_name'], self.identity, SOURCE)
        for changes in [{'draft': False}, {'published_at': 'now'}, {'target_commitish': 'b'*40},
                        {'tag_name': 'unrelated'}, {'assets': remote['assets'][:1]},
                        {'assets': [*remote['assets'], remote['assets'][0]]}]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                candidate.checked_remote({**remote, **changes}, remote['tag_name'], self.identity, SOURCE)

    def test_foreign_remote_cannot_reach_download_or_deletion(self):
        remote = self.remote()
        with patch.object(candidate, 'github', return_value=json.dumps({**remote, 'draft': False})) as gh:
            with self.assertRaises(ValueError):
                candidate.fetch(self.root, remote['tag_name'], self.identity, SOURCE, self.root/'download')
            self.assertEqual(gh.call_count, 1)
            gh.reset_mock()
            with self.assertRaises(ValueError):
                candidate.discard(remote['tag_name'], self.identity, SOURCE)
            self.assertEqual(gh.call_count, 1)

    def test_owned_draft_discard_checks_the_metadata_digest_before_deleting(self):
        remote = self.remote()
        with patch.object(candidate, 'github', return_value=json.dumps(remote)) as gh:
            candidate.discard(remote['tag_name'], self.identity, SOURCE)
            self.assertEqual(gh.call_args.args[:3], ('release', 'delete', remote['tag_name']))
            gh.reset_mock()
            with self.assertRaises(ValueError):
                candidate.discard(remote['tag_name'], '0'*64, SOURCE)
            self.assertEqual(gh.call_count, 1)

    def test_downloaded_asset_identity_is_checked_before_returning_the_handoff(self):
        remote = self.remote()
        folder = self.root/'download'
        def gh(*args):
            if args[0] == 'api': return json.dumps(remote)
            self.assertEqual(args[:3], ('release', 'download', remote['tag_name']))
            for name in [candidate.BODY, candidate.METADATA]:
                (folder/name).write_bytes((self.report.parent/name).read_bytes())
            return ''
        with patch.object(candidate, 'github', side_effect=gh):
            self.assertEqual(candidate.fetch(self.root, remote['tag_name'], self.identity, SOURCE, folder), folder)
        self.assertEqual(digest(folder/candidate.BODY), digest(self.report.parent/candidate.BODY))
        self.assertEqual(digest(folder/candidate.METADATA), self.identity)

    def test_downloaded_corruption_cannot_be_handed_to_the_mac_factory(self):
        remote = self.remote();folder=self.root/'corrupt-download'
        def gh(*args):
            if args[0]=='api':return json.dumps(remote)
            (folder/candidate.METADATA).write_bytes(self.report.read_bytes())
            (folder/candidate.BODY).write_bytes(b'corrupted transfer')
            return ''
        with patch.object(candidate,'github',side_effect=gh),self.assertRaises(ValueError):
            candidate.fetch(self.root,remote['tag_name'],self.identity,SOURCE,folder)

    def test_foreign_build_host_cannot_select_or_export_images(self):
        work=self.root/'build'
        with patch('host_platform.host_key',return_value='macos-arm64'),patch('linux_client_factory.check_docker') as docker:
            with self.assertRaises(ValueError):candidate.build(self.root,work,SOURCE)
        docker.assert_not_called();self.assertFalse(work.exists())

    def test_native_assembly_binds_arm_platform_source_and_fresh_workspace(self):
        work=self.root/'build'
        environment={'GITHUB_ACTIONS':'true','RUNNER_ENVIRONMENT':'github-hosted','GITHUB_WORKSPACE':str(self.root)}
        with patch.dict('os.environ',environment),patch('host_platform.host_key',return_value='linux-arm64'), \
             patch.object(candidate.shutil,'disk_usage',return_value=SimpleNamespace(free=40*1024**3)), \
             patch('check_source.check') as check,patch('linux_client_factory.check_docker') as docker, \
             patch('linux_client_factory.prepare_images',return_value=(self.payload,{})) as images:
            report=candidate.build(self.root,work,SOURCE)
            check.assert_called_once_with(self.root,repository=True,manifest=True)
            images.assert_called_once_with(self.root,work,digest(self.root/'SOURCE-MANIFEST.json'),architecture='arm64')
            docker.assert_called_once()
            self.assertTrue(candidate.admit(self.root,report.parent,digest(report),SOURCE))
            with self.assertRaises(FileExistsError):candidate.build(self.root,work,SOURCE)

    def test_source_change_during_native_assembly_is_not_sealed(self):
        def changed(*args,**kwargs):
            (self.root/'SOURCE-MANIFEST.json').write_text('changed')
            return self.payload,{}
        environment={'GITHUB_ACTIONS':'true','RUNNER_ENVIRONMENT':'github-hosted','GITHUB_WORKSPACE':str(self.root)}
        with patch.dict('os.environ',environment),patch('host_platform.host_key',return_value='linux-arm64'), \
             patch.object(candidate.shutil,'disk_usage',return_value=SimpleNamespace(free=40*1024**3)), \
             patch('check_source.check'),patch('linux_client_factory.check_docker'), \
             patch('linux_client_factory.prepare_images',side_effect=changed),patch.object(candidate,'stage') as seal:
            with self.assertRaisesRegex(ValueError,'inventory changed'):candidate.build(self.root,self.root/'build',SOURCE)
        seal.assert_not_called()


if __name__ == '__main__':
    unittest.main()
