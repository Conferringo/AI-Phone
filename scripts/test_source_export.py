"""Public source boundary tests; no GitHub requests or live checkout mutations."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
import shutil
import sys
from source_export import export
from check_source import check
from source_manifest import refresh

class SourceExportTests(unittest.TestCase):
    def fixture(self,root):
        (root/'config').mkdir();(root/'config/release-policy.json').write_text(json.dumps({'publication_gates':{'clean_host_install':False}}))
        for name in ['README.md','README.en.md','SECURITY.md','DISCLAIMER.md','PRIVACY.md','THIRD_PARTY_NOTICES.md']:(root/name).write_text('fixture')
        (root/'.github/workflows').mkdir(parents=True);(root/'.github/workflows/source.yml').write_text('name: fixture')
        (root/'docs/public').mkdir(parents=True);(root/'docs/public/contributor-agents.md').write_text('Public instructions')
        (root/'AGENTS.md').write_text('Private local collaborator instructions')
        (root/'data').mkdir();(root/'data/private.txt').write_text('DO_NOT_EXPORT');(root/'.env').write_text('DO_NOT_EXPORT')
        (root/'scripts').mkdir();(root/'scripts/clear-requested-chat.ts').write_text('dangerous old task');(root/'scripts/verify-old.ts').write_text('private fixture ids');(root/'scripts/runtime.py').write_text('public runtime')
    def test_draft_export_excludes_state_history_and_internal_scripts(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);out=Path(folder)/'review'
            report=export(root,out,draft=True,git=True)
            self.assertFalse(report['remote_created']);self.assertFalse((out/'data').exists());self.assertFalse((out/'.env').exists())
            self.assertFalse((out/'AGENTS.md').exists());self.assertFalse((out/'scripts/clear-requested-chat.ts').exists());self.assertFalse((out/'scripts/verify-old.ts').exists());self.assertTrue((out/'scripts/runtime.py').exists())
            self.assertEqual(subprocess.check_output(['git','rev-list','--count','HEAD'],cwd=out,text=True).strip(),'1')
            self.assertEqual(subprocess.check_output(['git','remote'],cwd=out,text=True),'')
            self.assertEqual(subprocess.check_output(['git','branch','--show-current'],cwd=out,text=True).strip(),'master')
            self.assertTrue((out/'LOCAL-REVIEW-ONLY.txt').exists())
    def test_authorized_online_review_stays_distinct_from_public_release(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root)
            policy=root/'config/release-policy.json';value=json.loads(policy.read_text());value['source_preview']={'remote_review_authorized':True,'delivery':'private-github-review'};policy.write_text(json.dumps(value))
            out=Path(folder)/'review';export(root,out,draft=True,git=True)
            manifest=json.loads((out/'SOURCE-MANIFEST.json').read_text())
            self.assertTrue(manifest['remote_review_authorized']);self.assertFalse(manifest['remote_publication_authorized']);self.assertFalse(manifest['product_ready'])
            self.assertTrue((out/'REVIEW-ONLY.txt').exists());self.assertFalse((out/'LOCAL-REVIEW-ONLY.txt').exists());self.assertFalse((out/'data').exists())
    def test_public_requires_license_and_never_overwrites_destination(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);out=Path(folder)/'review'
            with self.assertRaisesRegex(ValueError,'required-public-file'):export(root,out)
            self.assertFalse(out.exists());(root/'LICENSE').write_text('licence fixture')
            export(root,out)
            with self.assertRaisesRegex(ValueError,'must not exist'):export(root,out)
            self.assertFalse((out/'LOCAL-REVIEW-ONLY.txt').exists())
    def test_authorized_source_preview_does_not_claim_product_acceptance(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            policy=root/'config/release-policy.json';value=json.loads(policy.read_text());value['source_preview']={'remote_publication_authorized':True};policy.write_text(json.dumps(value))
            out=Path(folder)/'preview';export(root,out)
            manifest=json.loads((out/'SOURCE-MANIFEST.json').read_text())
            self.assertTrue(manifest['remote_publication_authorized']);self.assertFalse(manifest['product_ready'])
            self.assertFalse(manifest['product_gates']['clean_host_install']);self.assertEqual(manifest['kind'],'source-preview')
    def test_home_paths_and_private_network_origin_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            for content in ['path='+'/Users/'+'privateuser/project/', 'origin=http://'+'192.168.99.123:17860']:
                (root/'scripts/runtime.py').write_text(content)
                with self.assertRaisesRegex(ValueError,'Source checks failed'):export(root,Path(folder)/'out')

    def test_development_metadata_is_excluded_while_runtime_skills_remain(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            internal=['apps/web/agent.md','apps/web/AGENT.md','apps/web/AGENTS.md','apps/web/agents.md','apps/web/AGENTS.override.md','apps/web/CLAUDE.md','apps/web/.codex/config.toml','apps/web/.agents/private.md']
            for name in internal:
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('Private developer configuration')
            skill=root/'skills/builtin/phone-use/SKILL.md';skill.parent.mkdir(parents=True);skill.write_text('Runtime phone-use instructions')
            out=Path(folder)/'review';export(root,out)
            for name in ['AGENTS.md','docs/public/contributor-agents.md',*internal]:
                self.assertFalse((out/name).exists(),name)
            self.assertEqual((out/'skills/builtin/phone-use/SKILL.md').read_text(),skill.read_text())
            manifest=json.loads((out/'SOURCE-MANIFEST.json').read_text());names={item['path'] for item in manifest['files']}
            self.assertFalse(names.intersection({'AGENTS.md','docs/public/contributor-agents.md',*internal}))
            self.assertIn('skills/builtin/phone-use/SKILL.md',names)

    def test_repository_index_rejects_private_metadata_and_preserves_local_file(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            subprocess.run(['git','init','--initial-branch=master'],cwd=root,check=True,stdout=subprocess.DEVNULL)
            subprocess.run(['git','add','--','AGENTS.md','scripts/runtime.py'],cwd=root,check=True)
            with self.assertRaisesRegex(ValueError,'private-development-metadata'):check(root,repository=True)
            subprocess.run(['git','rm','--cached','--','AGENTS.md'],cwd=root,check=True,stdout=subprocess.DEVNULL)
            self.assertTrue(check(root,repository=True)['source_scan_passed'])
            self.assertEqual((root/'AGENTS.md').read_text(),'Private local collaborator instructions')
            self.assertEqual(subprocess.check_output(['git','ls-files'],cwd=root,text=True).splitlines(),['scripts/runtime.py'])
    def test_repository_index_rejects_state_outside_the_source_allowlist(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            subprocess.run(['git','init','--initial-branch=master'],cwd=root,check=True,stdout=subprocess.DEVNULL)
            subprocess.run(['git','add','--','data/private.txt'],cwd=root,check=True)
            with self.assertRaisesRegex(ValueError,'outside-source-allowlist'):check(root,repository=True)
            self.assertEqual((root/'data/private.txt').read_text(),'DO_NOT_EXPORT')

    def test_repository_rejects_corrupt_or_stale_export_manifests(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            out=Path(folder)/'preview';export(root,out,git=True)
            self.assertTrue(check(out,repository=True)['source_scan_passed'])
            manifest=out/'SOURCE-MANIFEST.json';original=manifest.read_text()
            for content,rule in [('<<<<<<< conflict\n','source-manifest-invalid-json'),('[]','source-manifest-mismatch'),('{"format":1,"files":[]}','source-manifest-mismatch')]:
                with self.subTest(rule=rule,content=content):
                    manifest.write_text(content)
                    with self.assertRaisesRegex(ValueError,rule):check(out,repository=True)
            manifest.write_text(original)
            (out/'scripts/runtime.py').write_text('changed public runtime')
            with self.assertRaisesRegex(ValueError,'source-manifest-mismatch'):check(out,repository=True)

    def test_explicit_manifest_gate_requires_a_manifest_but_exports_can_generate_one(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            self.assertTrue(check(root)['source_scan_passed'])
            with self.assertRaisesRegex(ValueError,'source-manifest-unreadable'):check(root,manifest=True)
            out=Path(folder)/'preview';export(root,out)
            self.assertTrue(check(out,manifest=True)['source_scan_passed'])

    def test_manifest_rejects_duplicate_entries_invalid_types_and_external_links(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            out=Path(folder)/'preview';export(root,out)
            manifest=out/'SOURCE-MANIFEST.json';original=json.loads(manifest.read_text())
            for field,value in [('format',True),('format',1.0),('files',original['files']+[original['files'][0]])]:
                with self.subTest(field=field,value_type=type(value).__name__):
                    changed={**original,field:value};manifest.write_text(json.dumps(changed))
                    with self.assertRaisesRegex(ValueError,'source-manifest-mismatch'):check(out,manifest=True)
            target=Path(folder)/'outside.json';target.write_text(json.dumps(original));manifest.unlink();manifest.symlink_to(target)
            with self.assertRaisesRegex(ValueError,'source-manifest-symlink'):check(out,manifest=True)
            with self.assertRaisesRegex(ValueError,'regular file'):refresh(out)
            self.assertEqual(target.read_text(),json.dumps(original))

    def test_refresh_matches_export_and_repairs_a_stale_manifest(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            out=Path(folder)/'preview';export(root,out)
            manifest=out/'SOURCE-MANIFEST.json';original=manifest.read_bytes()
            self.assertEqual(json.loads(original)['format'],1)
            refresh(out)
            self.assertEqual(manifest.read_bytes(),original)
            (out/'scripts/runtime.py').write_text('new public runtime')
            manifest.write_text('<<<<<<< unresolved manifest\n')
            refresh(out)
            self.assertTrue(check(out,manifest=True)['source_scan_passed'])
            self.assertFalse(json.loads(manifest.read_text())['product_ready'])

    def test_product_readiness_requires_boolean_acceptance(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            for i,accepted in enumerate([False,1,'true',True]):
                (root/'config/release-policy.json').write_text(json.dumps({'publication_gates':{'clean_host_install':accepted}}))
                out=Path(folder)/str(i);export(root,out)
                self.assertIs(json.loads((out/'SOURCE-MANIFEST.json').read_text())['product_ready'],accepted is True)

    def test_manifest_cli_requires_and_refreshes_the_manifest(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            for name in ['check_source.py','source_manifest.py','release_package.py','native_backports.py']:
                shutil.copyfile(Path(__file__).parent/name,root/'scripts'/name)
            command=[sys.executable,str(root/'scripts/check_source.py'),'--manifest']
            missing=subprocess.run(command,capture_output=True,text=True)
            self.assertNotEqual(missing.returncode,0)
            self.assertIn('source-manifest-unreadable',missing.stderr)
            generated=subprocess.run([sys.executable,str(root/'scripts/source_manifest.py'),'--refresh'],capture_output=True,text=True)
            self.assertEqual(generated.returncode,0,generated.stderr)
            verified=subprocess.run(command,capture_output=True,text=True)
            self.assertEqual(verified.returncode,0,verified.stderr)

    def test_publication_authorization_requires_an_explicit_boolean(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();self.fixture(root);(root/'LICENSE').write_text('licence fixture')
            for i,authorized in enumerate([None,False,1,'true',True]):
                policy={'publication_gates':{'accepted':False},'source_preview':{'remote_publication_authorized':authorized}}
                (root/'config/release-policy.json').write_text(json.dumps(policy))
                out=Path(folder)/str(i);export(root,out)
                self.assertIs(json.loads((out/'SOURCE-MANIFEST.json').read_text())['remote_publication_authorized'],authorized is True)

if __name__=='__main__':unittest.main()
