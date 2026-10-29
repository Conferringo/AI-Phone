import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from dependency_inventory import notices, supplemental_notices, runtime_notice_blocks

class NoticeTests(unittest.TestCase):
    def test_copyright_attribution_is_kept(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);(p/'COPYRIGHT-RUST.txt').write_text('Published copyright grant')
            self.assertEqual([x['file'] for x in notices(p)],['COPYRIGHT-RUST.txt'])
    def test_runtime_notice_preserves_text_and_rejects_tampering(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);self.assertEqual(runtime_notice_blocks(root),[])
            (root/'config').mkdir();text=root/'runtime.txt';text.write_text('Copyright holder\nFull licence terms\n')
            (root/'config/rust-runtime-notices.json').write_text(json.dumps({'packages':[{'package':'Rust@1','files':[{'file':'runtime.txt','url':'https://example.org/runtime','sha256':hashlib.sha256(text.read_bytes()).hexdigest()}]}]}))
            self.assertIn(text.read_text(),''.join(runtime_notice_blocks(root)))
            text.write_text('changed')
            with self.assertRaisesRegex(ValueError,'hash mismatch'):runtime_notice_blocks(root)
    def fixture(self, root):
        (root/'config').mkdir();(root/'docs/public/licenses/native').mkdir(parents=True)
        source=root/'crate';source.mkdir();(source/'Cargo.toml').write_text('license="MIT"')
        grant=hashlib.sha256((source/'Cargo.toml').read_bytes()).hexdigest()
        text=root/'docs/public/licenses/native/mit.txt';text.write_text('Complete MIT text')
        row={'package':'old@1.0','repository':None,'archive_sha256':'a'*64,'grant_files':[{'file':'Cargo.toml','sha256':grant}], 'files':[{'file':'docs/public/licenses/native/mit.txt','sha256':hashlib.sha256(text.read_bytes()).hexdigest(),'url':'https://example.org/MIT'}]}
        (root/'apps/client/src-tauri').mkdir(parents=True);(root/'apps/client/src-tauri/Cargo.lock').write_text('[[package]]\nname="old"\nversion="1.0"\nchecksum="'+'a'*64+'"\n')
        (root/'config/native-notice-supplements.json').write_text(json.dumps({'packages':[row]}))
        return source,{'name':'old','version':'1.0','repository':None}
    def test_legacy_archive_grant_is_bound_to_locked_checksum(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);source,package=self.fixture(root)
            self.assertEqual(len(supplemental_notices(root,package,source)),1)
            (source/'Cargo.toml').write_text('changed grant')
            with self.assertRaisesRegex(ValueError,'grant'):supplemental_notices(root,package,source)
    def test_legacy_archive_cannot_borrow_another_version_license(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);source,package=self.fixture(root)
            lock=root/'apps/client/src-tauri/Cargo.lock';lock.write_text(lock.read_text().replace('a'*64,'b'*64))
            with self.assertRaisesRegex(ValueError,'archive'):supplemental_notices(root,package,source)

if __name__=='__main__':unittest.main()
