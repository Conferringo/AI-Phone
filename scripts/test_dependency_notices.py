import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from dependency_inventory import inventory, notices, supplemental_notices, runtime_notice_blocks

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


class OwnedCrateTests(unittest.TestCase):
    def test_native_dependency_notices_distinguish_project_members_from_vendored_crates(self):
        from dependency_inventory import rust_notices
        import subprocess
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            (root/'Cargo.toml').write_text('[workspace]\nmembers=["crates/owned"]\nresolver="2"\n')
            for name,folder in [('owned',root/'crates/owned'),('local-vendor',root/'vendor/lib'),('client',root/'apps/client')]:
                folder.mkdir(parents=True);(folder/'src').mkdir();(folder/'src/lib.rs').write_text('')
                body=f'[package]\nname="{name}"\nversion="0.1.0"\nedition="2024"\nlicense="MIT"\n'
                if name=='client':body+='[workspace]\n[dependencies]\nowned={path="../../crates/owned"}\nlocal-vendor={path="../../vendor/lib"}\n'
                elif name=='local-vendor':body+='[workspace]\n'
                (folder/'Cargo.toml').write_text(body)
            (root/'vendor/lib/LICENSE').write_text('Fixture third-party grant')
            for path in ['Cargo.toml','apps/client/Cargo.toml']:
                subprocess.run(['cargo','generate-lockfile','--offline','--manifest-path',str(root/path)],check=True,capture_output=True)
            text,missing=rust_notices(root,'apps/client/Cargo.toml','fixture')
            self.assertEqual(missing,[])
            self.assertIn('Fixture third-party grant',text)
            self.assertNotIn('owned 0.1.0',text)


class JavascriptInventoryTests(unittest.TestCase):
    def report(self, metadata):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            (root/'Cargo.lock').write_text('')
            (root/'bun.lock').write_text('{}')
            folder=root/'node_modules/legacy-package'
            folder.mkdir(parents=True)
            (folder/'package.json').write_text(json.dumps({'name':'legacy-package','version':'1.0.0',**metadata}))
            (folder/'LICENSE').write_text('Retained upstream grant')
            locked=json.dumps({'legacy-package':['legacy-package@1.0.0']})
            with patch('dependency_inventory.subprocess.check_output',return_value=locked):
                return inventory(root)

    def test_single_legacy_license_is_reported_with_its_retained_grant(self):
        report=self.report({'licenses':[{'type':'MIT','url':'https://example.org/LICENSE'}]})
        self.assertEqual(report['javascript'][0]['license'],'MIT')
        self.assertEqual(report['missing_license_metadata']['javascript'],[])
        self.assertEqual(report['javascript'][0]['notices'],[{'file':'LICENSE','sha256':hashlib.sha256(b'Retained upstream grant').hexdigest()}])

    def test_current_license_takes_precedence_over_legacy_metadata(self):
        report=self.report({'license':'Apache-2.0','licenses':[{'type':'MIT'}]})
        self.assertEqual(report['javascript'][0]['license'],'Apache-2.0')

    def test_multiple_legacy_licenses_are_not_assigned_an_invented_expression(self):
        report=self.report({'licenses':[{'type':'MIT'},{'type':'Apache-2.0'}]})
        self.assertIsNone(report['javascript'][0]['license'])
        self.assertEqual(report['missing_license_metadata']['javascript'],['legacy-package@1.0.0'])

if __name__=='__main__':unittest.main()
