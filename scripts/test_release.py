"""Isolation tests: never write to the live deployment, accounts or device disks."""
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from release import configure, default_profile, validate, service_ready
from release_package import create_package
from runtime_config import runtime_env
from release_stage import stage_bundle


class ReleaseTests(unittest.TestCase):
    def test_startup_probe_supplies_origin_without_disabling_server_checks(self):
        from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
        import threading
        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                accepted=self.path=='/api/setup/status' and self.headers.get('Origin')==f'http://127.0.0.1:{self.server.server_port}' and self.headers.get('Content-Type')=='application/x-protobuf'
                self.send_response(200 if accepted else 403);self.send_header('content-type','application/x-protobuf');self.end_headers()
            def log_message(self,*args):pass
        server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        try: service_ready(f'http://127.0.0.1:{server.server_port}')
        finally:server.shutdown();server.server_close();thread.join()
    def test_staging_keeps_user_files_and_does_not_activate_an_update(self):
        with tempfile.TemporaryDirectory() as name:
            base=Path(name); root=base/'source'; (root/'config').mkdir(parents=True)
            policy=root/'config/release-policy.json'
            policy.write_text(json.dumps(dict(version='v1', publication_gates={})))
            (root/'ai-phone').write_text('launch-v1')
            installation=base/'installation'
            first=stage_bundle(create_package(root,base/'bundles'),installation)
            (installation/'user/data/important.txt').write_text('keep my conversation')
            (installation/'user/.env').write_text('POSTGRES_PASSWORD=existing')
            policy.write_text(json.dumps(dict(version='v2', publication_gates={})))
            (root/'ai-phone').write_text('launch-v2')
            second=stage_bundle(create_package(root,base/'bundles'),installation)
            self.assertEqual((installation/'current').resolve(),first)
            self.assertEqual((second/'data/important.txt').read_text(),'keep my conversation')
            self.assertEqual((second/'.env').read_text(),'POSTGRES_PASSWORD=existing')
            self.assertEqual((second/'ai-phone').read_text(),'launch-v2')
            self.assertNotEqual(first,second)

    def test_staging_rejects_tampered_file_before_creating_a_release(self):
        import io
        with tempfile.TemporaryDirectory() as name:
            root=Path(name); (root/'config').mkdir()
            (root/'config/release-policy.json').write_text(json.dumps(dict(version='test',publication_gates={})))
            (root/'ai-phone').write_text('original')
            bundle=create_package(root,root/'bundles')
            tampered=root/'bad.tar.gz'
            with tarfile.open(bundle) as src,tarfile.open(tampered,'w:gz') as dest:
                for item in src:
                    data=src.extractfile(item).read()
                    if item.name.endswith('/ai-phone'):data=b'modified'
                    item.size=len(data);dest.addfile(item,io.BytesIO(data))
            with self.assertRaisesRegex(ValueError,'checksum mismatch'):stage_bundle(tampered,root/'install')
            self.assertFalse((root/'install/app/releases/test').exists())
    def test_configuration_preserves_database_and_devices(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            (root / '.env').write_text('POSTGRES_PASSWORD=private-fixture\n')
            (root / 'data/android').mkdir(parents=True)
            disk = root / 'data/android/userdata.img'; disk.write_bytes(b'existing-disk')
            profile = default_profile(root); profile['java_home'] = '/fixture/java'
            source = root / 'profile.json'; source.write_text(json.dumps(profile))
            configure(root, source)
            self.assertEqual((root / '.env').read_text(), 'POSTGRES_PASSWORD=private-fixture\n')
            self.assertEqual(disk.read_bytes(), b'existing-disk')
            self.assertEqual((root / 'data/deployment/profile.json').stat().st_mode & 0o777, 0o600)
            self.assertEqual(runtime_env(root)['AI_PHONE_PHONE_STORAGE'], '16')
            profile['host_proxy']='http://127.0.0.1:7890'
            (root/'data/deployment/profile.json').write_text(json.dumps(profile))
            self.assertEqual(runtime_env(root)['AI_PHONE_HOST_PROXY'],profile['host_proxy'])
            self.assertEqual(runtime_env(root)['AI_PHONE_EMULATOR_PROXY'],'', 'Guest network must not inherit a new host proxy')

    def test_invalid_and_authenticated_proxy_rejected(self):
        for invalid in [None,[],{},dict(schema_version=1,phone=None,desktop={})]:
            with self.assertRaises(ValueError):validate(invalid)
        profile = default_profile(); profile['java_home'] = '/fixture/java'
        for proxy in ['http://user:password@example.test:1', 'file:///tmp/a', 'http://example.test/path']:
            with self.subTest(proxy=proxy):
                profile['host_proxy'] = proxy
                with self.assertRaises(ValueError): validate(profile)

    def test_bundle_excludes_private_state_and_checks_all_publication_gates(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            for folder in ['config', 'apps/web', 'apps/web/node_modules', 'data', 'docs/evidence', 'docs/public', 'apps/web/.AppleDouble']:
                (root / folder).mkdir(parents=True)
            (root / 'config/release-policy.json').write_text(json.dumps(dict(version='test', publication_gates={'clean_install': False})))
            for path in ['.env', 'data/token', 'apps/web/node_modules/private', 'docs/evidence/chat.txt', 'apps/web/.DS_Store', 'apps/web/._icon.png', 'apps/web/.AppleDouble/metadata']:
                (root / path).write_text('private fixture')
            (root / 'apps/web/public.ts').write_text('export const safe = true;')
            bundle = create_package(root, root / 'dist')
            with tarfile.open(bundle) as t:
                names = t.getnames()
                self.assertIn('ai-phone-test/apps/web/public.ts', names)
                self.assertIn('ai-phone-test/NOT-FOR-PUBLICATION.txt', names)
                self.assertFalse(any('/data/' in n or '/node_modules/' in n or n.endswith('/.env') for n in names))
                self.assertFalse(any('.DS_Store' in n or '/._' in n or '/.AppleDouble/' in n for n in names))
            with self.assertRaisesRegex(ValueError, 'Publication blocked'): create_package(root, root / 'dist', public=True)

    def test_secret_and_symlink_rejected_without_printing_secret(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name); (root / 'config').mkdir(); (root / 'apps').mkdir()
            (root / 'config/release-policy.json').write_text(json.dumps(dict(version='test', publication_gates={})))
            source = root / 'apps/leak.ts'; source.write_text('sk-' + 'x' * 30)
            with self.assertRaisesRegex(ValueError, 'Possible credential in source file: apps/leak.ts'): create_package(root, root / 'dist')
            source.unlink(); source.symlink_to('/etc/passwd')
            with self.assertRaisesRegex(ValueError, 'Symlink not allowed'): create_package(root, root / 'dist')


if __name__ == '__main__': unittest.main()
