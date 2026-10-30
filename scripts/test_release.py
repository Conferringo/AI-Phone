"""Isolation tests: never write to the live deployment, accounts or device disks."""
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch, MagicMock
from release import configure, default_profile, validate, service_ready
from release_package import create_package
from runtime_config import runtime_env
from release_stage import stage_bundle


class ReleaseTests(unittest.TestCase):
    def test_bundled_image_checkpoints_never_build_or_pull_missing_images(self):
        import release
        from install_state import InstallState
        from container_images import atomic_selection
        from test_container_images import ContainerImageTests
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);_,manifest=ContainerImageTests().fixture(root);atomic_selection(root,manifest)
            selected=runtime_env(root)
            self.assertEqual(selected['AI_PHONE_POSTGRES_IMAGE'],manifest['images']['postgres']['ref'])
            with InstallState(root) as state,patch('release.run') as run:
                with patch('release.image_id',side_effect=lambda ref:'verified-'+ref) as observed:
                    release.install_guest_images(state,root,selected)
                self.assertEqual([call.args[0] for call in observed.call_args_list],[manifest['images'][role]['ref'] for role in ('workspace','desktop')])
                with patch('release.image_id',return_value=''):
                    with self.assertRaisesRegex(ValueError,'verification failed'):release.install_guest_images(state,root,selected)
                run.assert_not_called()

    def test_sdk_checks_report_missing_and_unsupported_java(self):
        import release,subprocess
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);env={'AI_PHONE_SDK_ROOT':folder,'JAVA_HOME':''}
            self.assertFalse(next(row['passed'] for row in release.sdk_checks(env) if row['name']=='java17'))
            java=root/'bin/javac';java.parent.mkdir();java.touch();env['JAVA_HOME']=folder
            for version,expected in [('javac 17',True),('javac 8',False),('unreadable',False)]:
                with patch('release.subprocess.run',return_value=subprocess.CompletedProcess([],0,stdout=version,stderr='')):
                    self.assertEqual(next(row['passed'] for row in release.sdk_checks(env) if row['name']=='java-version'),expected)

    def test_capacity_rejects_insufficient_memory_or_disk(self):
        import release,shutil
        with tempfile.TemporaryDirectory() as folder,patch('release.shutil.disk_usage',return_value=shutil._ntuple_diskusage(100,100,0)):
            for system in ('Darwin','Linux'):
                for memory,expected in [(8*1024**3,False),(12*1024**3,True)]:
                    with patch('release.platform.system',return_value=system),patch('release.subprocess.check_output',return_value=str(memory)),patch('release.Path.read_text',return_value='MemFree: 1 kB\nMemTotal: '+str(memory//1024)+' kB\n'):
                        checks=release.capacity_checks(Path(folder),35)
                        self.assertEqual(checks[0]['passed'],expected);self.assertFalse(checks[-1]['passed'])

    def test_dependency_probe_rejects_absent_or_wrong_bun(self):
        from release import bootstrap_valid
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            self.assertFalse(bootstrap_valid(root))
            for name in ('apps/web/node_modules/next/package.json','packages/protocol/src/gen/ai_phone/v1/system_pb.ts'):
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('fixture')
            program=root/'.tools/bin/bun';program.parent.mkdir(parents=True)
            program.write_text('#!/bin/sh\necho 1.4.4\n');program.chmod(0o755)
            self.assertTrue(bootstrap_valid(root))
            program.write_text('#!/bin/sh\necho 1.5.0\n')
            self.assertFalse(bootstrap_valid(root))

    def test_start_observes_readiness_and_uses_only_the_requested_root(self):
        import release
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            with patch('host_platform.host_policy'),patch('service_start.start_missing') as services,patch('release.service_ready',side_effect=[ValueError('not yet'),None]),patch('time.sleep'),patch('release.open_client') as client:
                release.start(root=root)
                services.assert_called_once_with(root);client.assert_called_once_with(root)
            (root/'data').mkdir();(root/'data/restore-quarantine.json').write_text('{}')
            with patch('service_start.start_missing') as services,self.assertRaisesRegex(ValueError,'quarantined'):
                release.start(root=root)
            services.assert_not_called()

    def test_open_uses_native_client_or_browser_without_changing_root(self):
        import release
        with tempfile.TemporaryDirectory() as folder,patch('release.service_ready'),patch('release.run') as run:
            root=Path(folder)
            for system,application,expected in [('Darwin',root/'AI Phone.app',['open','-a',root/'AI Phone.app']),('Darwin',None,['open','http://127.0.0.1:17860/phone']),('Linux','/fixture/client',['/fixture/client']),('Linux',None,['xdg-open','http://127.0.0.1:17860/phone'])]:
                with patch('release.platform.system',return_value=system),patch('release.macos_client',return_value=application),patch('release.shutil.which',return_value=application):
                    release.open_client(root=root)
                    run.assert_called_with(expected,root=root)
            with patch('release.platform.system',return_value='Windows'),self.assertRaisesRegex(ValueError,'not implemented'):
                release.open_client(root=root)

    def test_macos_discovery_requires_the_application_identity(self):
        import release,plistlib
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            app=root/'apps/client/src-tauri/target/release/bundle/macos/AI Phone.app'
            path=app/'Contents/Info.plist';path.parent.mkdir(parents=True)
            path.write_bytes(plistlib.dumps({'CFBundleIdentifier':'dev.aiphone.client'}))
            self.assertEqual(release.macos_client(root),app)
            with patch('release.Path.read_bytes',side_effect=FileNotFoundError):
                self.assertIsNone(release.macos_client(root))

    def test_install_phases_keep_their_order_and_explicit_root(self):
        import release
        import subprocess
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            profile = root / release.PROFILE; profile.parent.mkdir(parents=True); profile.write_text('{}')
            environment = {'AI_PHONE_SDK_ROOT': str(root/'sdk'), 'AI_PHONE_ANDROID_ABI': 'arm64-v8a', 'AI_PHONE_CONTAINER_PROXY': ''}
            phases, commands = [], []
            state = MagicMock()
            def phase(name, digest, action, verify, **kwargs):
                self.assertEqual(len(digest), 64)
                phases.append((name, kwargs.get('reuse', True)))
                action()
            state.__enter__.return_value.phase.side_effect = phase
            def command(args, **kwargs): commands.append(([str(v) for v in args], kwargs)); return subprocess.CompletedProcess(args, 0, stdout='fixture-image', stderr='')
            checks=[{'name':'sdk/platform-tools/adb','passed':False,'fix':'SDK download required'},{'name':'fixture tool','passed':True,'fix':''}]
            with patch('release.doctor', return_value=checks), patch('release.environment', return_value=environment), patch('install_state.InstallState', return_value=state), patch('android_sdk.command', return_value=['sdk-fixture']), patch('release.run', side_effect=command), patch('release.subprocess.run', side_effect=command), patch('release.start') as start, patch('release.platform.system', return_value='Darwin'):
                release.install(root=root, launch_client=False)
            self.assertEqual(phases, [(name, name!='services') for name in ('sdk-packages', 'dependencies', 'native-apps', 'messaging-browser', 'workspace-image', 'desktop-image', 'services')])
            self.assertEqual([args[0] for args, _ in commands], ['sdk-fixture', './scripts/bootstrap.sh', str(root/'.tools/bin/bun'), str(root/'.tools/bin/bun'), 'docker', 'docker', 'docker'])
            for args, kwargs in commands:
                if args[:3] != ['docker', 'image', 'inspect']:
                    self.assertEqual(kwargs.get('root', kwargs.get('cwd')), root)
            start.assert_called_once_with(root=root, launch_client=False)

    def test_install_refuses_failed_prerequisites_before_starting_phases(self):
        import release
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);profile=root/release.PROFILE;profile.parent.mkdir(parents=True);profile.write_text('{}')
            with patch('release.doctor',return_value=[{'name':'fixture unavailable tool','passed':False,'fix':'Install fixture'}]),patch('install_state.InstallState') as state:
                with self.assertRaisesRegex(ValueError,'No devices were changed'):
                    release.install(root=root)
            state.assert_not_called()

    def test_native_install_uses_the_explicit_root_for_preflight(self):
        import release
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);(root/'data/deployment').mkdir(parents=True);(root/'data/deployment/profile.json').write_text('{}')
            def stop_at_preflight(actual,**kwargs):
                self.assertEqual(actual,root)
                raise RuntimeError('fixture stopped before installation')
            with patch('release.doctor',side_effect=stop_at_preflight):
                with self.assertRaisesRegex(RuntimeError,'fixture stopped before installation'):
                    release.install(root=root)

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
