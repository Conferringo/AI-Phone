"""Public source boundary tests; no GitHub requests or live checkout mutations."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from source_export import export

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
            self.assertEqual((out/'AGENTS.md').read_text(),'Public instructions');self.assertFalse((out/'scripts/clear-requested-chat.ts').exists());self.assertFalse((out/'scripts/verify-old.ts').exists());self.assertTrue((out/'scripts/runtime.py').exists())
            self.assertEqual(subprocess.check_output(['git','rev-list','--count','HEAD'],cwd=out,text=True).strip(),'1')
            self.assertEqual(subprocess.check_output(['git','remote'],cwd=out,text=True),'')
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

if __name__=='__main__':unittest.main()
