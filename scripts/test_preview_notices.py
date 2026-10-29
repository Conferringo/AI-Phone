import json
from pathlib import Path
import tempfile
import unittest
from client_artifacts import notice_manifest

class PreviewNoticeTests(unittest.TestCase):
    def test_missing_notice_evidence_blocks_preview_metadata(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/'docs/public').mkdir(parents=True);(root/'config').mkdir()
            p=root/'docs/public/dependency-licenses.json';p.write_text(json.dumps({'native_client_missing_notice_files':['unknown@1']}))
            with self.assertRaisesRegex(ValueError,'notice'):notice_manifest(root)
    def test_complete_notice_manifest_hashes_the_shipped_texts(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/'docs/public').mkdir(parents=True);(root/'config').mkdir()
            (root/'docs/public/dependency-licenses.json').write_text(json.dumps({'native_client_missing_notice_files':[]}))
            (root/'config/android-dependency-notices.json').write_text(json.dumps({'packages':[{'package':'test:lib:1'}]}))
            for name in ['native-client-licenses.txt','android-client-licenses.txt']:(root/'docs/public'/name).write_text('Full notice '+name)
            first=notice_manifest(root);self.assertEqual(first['rust_missing_files'],[]);self.assertEqual(first['android_runtime_modules'],1)
            (root/'docs/public/native-client-licenses.txt').write_text('updated')
            self.assertNotEqual(notice_manifest(root)['texts'],first['texts'])

if __name__=='__main__':unittest.main()
