"""Native component admission, immutable bytes and persistent user data contracts."""
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from build_native_services import cached_build, source_fingerprint
from client_host_entry import stage_source
from host_platform import host_key
from prebuilt_services import ROLES, FIXTURE, binary_path, service_command, launch_spec, prepare_services, read_manifest, manifest_records
from check_native_services import require_test_database
from macos_target import verify_targets, swift_target
ROOT = Path(__file__).resolve().parent.parent


class NativeServiceTests(unittest.TestCase):
    def build_fixture(self, root, payload=b'generated binary fixture'):
        target=root/'target/release';target.mkdir(parents=True)
        fixture=root/'crates/assistant/fixtures/vision-probe.png';fixture.parent.mkdir(parents=True);fixture.write_bytes(b'generated image fixture')
        records={}
        for role,name in [('assistant','ai-phone-assistant'),('runner','ai-phone-runner'),('vision','phone-vision')]:
            path=target/name;path.write_bytes(payload)
            records[role]={'path':path.relative_to(root).as_posix(),'size':len(payload),'sha256':hashlib.sha256(payload).hexdigest()}
        value={'format':1,'host':host_key(),'source_sha256':source_fingerprint(root),'compiler':'fixture compiler','artifacts':records,
               'checks':{role:{'version':'0.1.0'} for role in ('assistant','runner')}}
        (target/'native-service-build.json').write_text(json.dumps(value))
        return value

    def test_packager_emits_canonical_host_matched_inventory_and_blocks_private_bytes(self):
        for system,backend in [('Darwin','macos-vision'),('Linux','linux-tesseract')]:
            with tempfile.TemporaryDirectory() as folder,patch('platform.system',return_value=system):
                root=Path(folder);receipt=self.build_fixture(root)
                files=prepare_services(root);mapping={name:(data,mode) for name,data,mode in files}
                manifest=json.loads(mapping['runtime/services/manifest.json'][0])
                self.assertEqual(manifest['format'],1);self.assertEqual(manifest['vision'],backend)
                self.assertEqual(manifest['host'],host_key());self.assertEqual(manifest['source_sha256'],receipt['source_sha256'])
                self.assertEqual(manifest['versions'],{'assistant':'0.1.0','runner':'0.1.0'})
                self.assertEqual(mapping['runtime/services/manifest.json'],(json.dumps(manifest,sort_keys=True).encode(),0o644))
                self.assertEqual(len(files),5)
                for role in ROLES:self.assertEqual(mapping[binary_path(role)],(b'generated binary fixture',0o755))
                self.assertEqual(mapping[FIXTURE],(b'generated image fixture',0o644))
        for data in [b'sk-'+b'x'*32,(str(Path.home())+'/private/source').encode()]:
            with tempfile.TemporaryDirectory() as folder:
                root=Path(folder);self.build_fixture(root,data)
                with self.assertRaisesRegex(ValueError,'Private material'):prepare_services(root)

    def test_manifest_size_and_type_boundaries_fail_explicitly(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);path=root/'runtime/services/manifest.json';path.parent.mkdir(parents=True)
            data=json.dumps({'format':1,'host':host_key()}).encode()
            path.write_bytes(data+b' '*(4*1024*1024-len(data)))
            self.assertEqual(read_manifest(root)['host'],host_key())
            with path.open('ab') as stream:stream.write(b' ')
            with self.assertRaisesRegex(ValueError,'Invalid'):read_manifest(root)
            for rows in (None,{},'not an inventory'):
                with self.assertRaisesRegex(ValueError,'Incomplete'):manifest_records({'files':rows})

    def component(self):
        files = {binary_path(role): ('fixture-' + role).encode() for role in ROLES}
        files[FIXTURE] = b'fixture image data'
        manifest = {'format': 1, 'host': host_key(), 'versions': {'assistant': '0.1.0', 'runner': '0.1.0'},
                    'files': [{'path': name, 'size': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'mode': 0o644 if name == FIXTURE else 0o755}
                              for name, data in files.items()]}
        files['runtime/services/manifest.json'] = json.dumps(manifest).encode()
        return files

    def archive(self, root, files, components):
        manifest = {'format': 2, 'components': components,
                    'files': [{'path': name, 'size': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'mode': 0o755 if name.startswith('runtime/services/bin/') else 0o644}
                              for name, data in files.items()]}
        path = root / 'component.payload'
        with tarfile.open(path, 'w:gz') as output:
            for name, data in {**files, 'HOST-MANIFEST.json': json.dumps(manifest).encode()}.items():
                entry = tarfile.TarInfo(name); entry.size = len(data); output.addfile(entry, io.BytesIO(data))
        return path

    def test_component_set_and_inventory_are_checked_before_activation(self):
        for components in ([], ['unknown'], ['web', 'web'], ['native-services', 'web'], ['web']):
            with self.subTest(components=components), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                with self.assertRaises(ValueError):
                    stage_source(self.archive(root, self.component(), components), root / 'install')
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); files = self.component(); files.pop('runtime/services/manifest.json')
            with self.assertRaisesRegex(ValueError, 'manifest'):
                stage_source(self.archive(root, files, ['native-services']), root / 'install')

    def test_native_only_archive_preserves_user_data_and_rejects_changed_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); installation = root / 'install'; user = installation / 'user/data'
            user.mkdir(parents=True); (user / 'chat').write_text('retain fixture history')
            archive = self.archive(root, self.component(), ['native-services'])
            staged = stage_source(archive, installation)
            self.assertEqual(stage_source(archive, installation), staged)
            self.assertEqual((staged / 'data/chat').read_text(), 'retain fixture history')
            (staged / binary_path('runner')).write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError, 'changed'):
                stage_source(archive, installation)
            self.assertEqual((user / 'chat').read_text(), 'retain fixture history')

    @unittest.skipUnless(platform.system() in ('Darwin', 'Linux'), 'native services target POSIX hosts')
    def test_launch_requires_matching_host_inventory_permissions_and_unchanged_bytes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for name, data in self.component().items():
                path = root / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data)
                path.chmod(0o755 if name.startswith('runtime/services/bin/') else 0o644)
            self.assertEqual(service_command(root, 'assistant'), [str(root.resolve() / binary_path('assistant'))])
            launch = launch_spec(root, 'runner')
            self.assertTrue(launch['prebuilt'])
            self.assertEqual(launch['environment']['AI_PHONE_OCR'], str(root.resolve() / binary_path('vision')))
            executable = root / binary_path('runner'); executable.chmod(0o644)
            with self.assertRaisesRegex(ValueError, 'permissions'): service_command(root, 'runner')
            executable.chmod(0o755)
            with self.assertRaisesRegex(ValueError, 'Unknown'):
                service_command(root, 'arbitrary')
            path = root / 'runtime/services/manifest.json'; original = path.read_bytes()
            for field, value in [('host', 'different'), ('files', []), ('format', 3)]:
                manifest = json.loads(original); manifest[field] = value; path.write_text(json.dumps(manifest))
                with self.assertRaises(ValueError): service_command(root, 'runner')
            manifest = json.loads(original); manifest['files'][0]['mode'] = 0o644; path.write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, 'executable'): service_command(root, 'runner')
            path.write_bytes(original); (root / binary_path('runner')).write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'integrity'): service_command(root, 'runner')

    def test_present_invalid_components_never_fall_back_to_source(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            self.assertFalse(launch_spec(root,'assistant')['prebuilt'])
            manifest=root/'runtime/services/manifest.json';manifest.parent.mkdir(parents=True)
            for value in ([], {'format':True,'host':host_key()}):
                manifest.write_text(json.dumps(value))
                with self.assertRaises(ValueError): launch_spec(root,'assistant')
            with patch('prebuilt_services.platform.system',return_value='Windows'),self.assertRaisesRegex(ValueError,'not implemented'):
                launch_spec(root,'assistant')
            manifest.unlink();manifest.symlink_to(root/'missing')
            with self.assertRaisesRegex(ValueError,'Invalid'):launch_spec(root,'assistant')

    def test_build_cache_cannot_admit_missing_files_or_an_external_path(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); target = root / 'target/release'; target.mkdir(parents=True)
            fingerprint = source_fingerprint(root)
            self.assertIsNone(cached_build(root, fingerprint))
            receipt = {'format': 1, 'host': host_key(), 'source_sha256': fingerprint, 'artifacts': {}}
            path = target / 'native-service-build.json'; path.write_text(json.dumps(receipt))
            self.assertIsNone(cached_build(root, fingerprint))
            for role, name in [('assistant', 'ai-phone-assistant'), ('runner', 'ai-phone-runner'), ('vision', 'phone-vision')]:
                file = target / name; file.write_bytes(name.encode())
                receipt['artifacts'][role] = {'path': file.relative_to(root).as_posix(), 'size': file.stat().st_size,
                                             'sha256': hashlib.sha256(file.read_bytes()).hexdigest()}
            path.write_text(json.dumps(receipt)); self.assertIsNotNone(cached_build(root, fingerprint))
            for malformed in ([], None, {**receipt, 'format': True}, {**receipt, 'artifacts': []}):
                path.write_text(json.dumps(malformed))
                self.assertIsNone(cached_build(root, fingerprint))
            receipt['artifacts']['runner']['path'] = '../outside'; path.write_text(json.dumps(receipt))
            self.assertIsNone(cached_build(root, fingerprint))
            (root / 'Cargo.toml').write_text('changed source')
            self.assertNotEqual(source_fingerprint(root), fingerprint)

    def test_native_helpers_cannot_exceed_client_minimum_os(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); config = root / 'apps/client/src-tauri/tauri.conf.json'
            config.parent.mkdir(parents=True)
            config.write_text(json.dumps({'bundle': {'macOS': {'minimumSystemVersion': '14.4'}}}))
            for arch in ('arm64','x86_64'):
                with patch('macos_target.platform.machine',return_value=arch):
                    self.assertEqual(swift_target(root),arch+'-apple-macosx14.4')
            with patch('macos_target.platform.machine',return_value='unknown'),self.assertRaisesRegex(ValueError,'architecture'):
                swift_target(root)
            for version in ('11.0', '14.4', '14.4.0'):
                def output(*_args, **kwargs):
                    text='    minos '+version+'\n'
                    return text if kwargs.get('text') else text.encode()
                with patch('macos_target.subprocess.check_output', side_effect=output):
                    self.assertEqual(verify_targets(root, {'vision': root/'vision'}), {'vision': version})
            for value in ('minos 26.0\n', 'minos 14.4.1\n', 'missing', 'minos 11.0\nminos 14.4\n'):
                with patch('macos_target.subprocess.check_output', return_value=value), self.assertRaises(ValueError):
                    verify_targets(root, {'vision': root/'vision'})

    def test_database_check_rejects_unowned_or_publicly_bound_fixtures(self):
        labels = {'dev.aiphone.test': 'native-runtime'}
        binding = {'5432/tcp': [{'HostIp': '127.0.0.1', 'HostPort': '55512'}]}
        with patch('check_native_services.subprocess.check_output', return_value=json.dumps(labels)+'\n'+json.dumps(binding)) as inspect:
            require_test_database('aiphone-native-fixture', 55512)
            self.assertEqual(inspect.call_count, 1)
            for container, port in [('ai-phone-postgres-1', 55512), ('aiphone-native-fixture', 55482), ('aiphone-native-fixture', True)]:
                with self.assertRaises(ValueError): require_test_database(container, port)
            self.assertEqual(inspect.call_count, 1)
        for bad_labels, bindings in [(None, binding), ({}, binding), (labels, None), (labels, {'5432/tcp': binding['5432/tcp']+[{'HostIp': '0.0.0.0', 'HostPort': '55512'}]})]:
            with patch('check_native_services.subprocess.check_output', return_value=json.dumps(bad_labels)+'\n'+json.dumps(bindings)), self.assertRaises(ValueError):
                require_test_database('aiphone-native-fixture', 55512)

    def test_database_verification_rejects_privileged_roles_or_missing_migrations(self):
        from check_native_services import database_receipt
        from database import RUNTIME
        denied=subprocess.CompletedProcess([],1,stdout='',stderr='permission denied')
        for status,count,ddl in [({'runtime':RUNTIME,'privileged':True,'ddl':False},'29',denied),
                                 ({'runtime':RUNTIME,'privileged':False,'ddl':False},'0',denied),
                                 ({'runtime':RUNTIME,'privileged':False,'ddl':False},'29',subprocess.CompletedProcess([],0,stdout='',stderr=''))]:
            with tempfile.TemporaryDirectory() as folder,patch('check_native_services.require_test_database'),patch('prebuilt_services.service_command',return_value=['/fixture/assistant']),patch('database.prepare'),patch('database.credentials',return_value={}),patch('check_native_services.subprocess.run',return_value=subprocess.CompletedProcess([],0,stdout=json.dumps(status))),patch('check_native_services.runtime_sql',side_effect=[subprocess.CompletedProcess([],0,stdout=count),ddl]),self.assertRaises(ValueError):
                database_receipt(Path(folder),'aiphone-native-fixture',55512)


@unittest.skipUnless(platform.system() == 'Darwin', 'launchd script contract')
class NativeLauncherTests(unittest.TestCase):
    SANDBOX = '(version 1) (allow default) (deny process-exec (literal "/bin/launchctl"))'
    def fixture(self, root, script):
        scripts = root / 'scripts'; scripts.mkdir()
        for name in [script, 'android-env.sh', 'runtime_config.py', 'prebuilt_services.py', 'client_host_entry.py', 'host_platform.py', 'release_package.py']:
            shutil.copyfile(ROOT / 'scripts' / name, scripts / name)
        (scripts / 'release.py').write_text('import os,shlex\nfrom pathlib import Path\nr=Path.cwd()\nfor k,v in {"AI_PHONE_PROJECT_ROOT":str(r),"AI_PHONE_DATA_ROOT":str(r/"data"),"AI_PHONE_SDK_ROOT":str(r/"sdk"),"AI_PHONE_JAVA_HOME":""}.items():print("export "+k+"="+shlex.quote(v))\n')
        (scripts / 'database.py').write_text('import os\nfrom pathlib import Path\np=Path(os.environ["AI_PHONE_TEST_LAUNCH_LOG"])\np.open("a").write("database prepare\\n")\n')
        (scripts / 'build-phone-vision.sh').write_text('#!/bin/sh\nexit 72\n')
        files = NativeServiceTests().component()
        program = b'#!/bin/sh\nprintf "service:%s\\n" "$*" >> "$AI_PHONE_TEST_LAUNCH_LOG"\n'
        for role in ROLES: files[binary_path(role)] = program
        manifest = json.loads(files['runtime/services/manifest.json'])
        for row in manifest['files']:
            row['size'] = len(files[row['path']]); row['sha256'] = hashlib.sha256(files[row['path']]).hexdigest()
        files['runtime/services/manifest.json'] = json.dumps(manifest).encode()
        for name, data in files.items():
            path = root / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data)
            path.chmod(0o755 if name.startswith('runtime/services/bin/') else 0o644)
        tools = root / '.tools/bin'; tools.mkdir(parents=True)
        for name, body in {'cargo': 'exit 71', 'docker': 'printf "docker\\n" >> "$AI_PHONE_TEST_LAUNCH_LOG"',
                           'launchctl': 'if [ "$1" = print ]; then exit 1; fi\nprintf "launchctl\\n" >> "$AI_PHONE_TEST_LAUNCH_LOG"'}.items():
            path = tools / name; path.write_text('#!/bin/sh\n'+body+'\n'); path.chmod(0o755)
        return {**os.environ, 'HOME': str(root / 'home'), 'PATH': str(tools)+os.pathsep+os.environ['PATH'],
                'AI_PHONE_TEST_LAUNCH_LOG': str(root / 'launch.log')}

    def test_prebuilt_launchers_use_bundled_programs_without_cargo_or_swift(self):
        for role, script, label in [('assistant', 'assistant-start.sh', 'dev.aiphone.assistant'),
                                     ('runner', 'phone-runner-start.sh', 'dev.aiphone.runner.p2')]:
            with self.subTest(role=role), tempfile.TemporaryDirectory() as folder:
                root = Path(folder).resolve(); env = self.fixture(root, script)
                denied = subprocess.run(['/usr/bin/sandbox-exec', '-p', self.SANDBOX, '/bin/launchctl', 'help'], capture_output=True)
                self.assertNotEqual(denied.returncode, 0, 'Fixture must not be able to execute the system launchctl')
                result = subprocess.run(['/usr/bin/sandbox-exec', '-p', self.SANDBOX, '/bin/sh', root / 'scripts' / script], cwd=root, env=env, capture_output=True, text=True, timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                path = root / 'home/Library/LaunchAgents' / (label+'.plist')
                config = plistlib.loads(path.read_bytes())
                self.assertEqual(config['ProgramArguments'], [str(root / binary_path(role))])
                if role == 'runner':self.assertEqual(config['EnvironmentVariables']['AI_PHONE_OCR'], str(root / binary_path('vision')))
                self.assertIn('launchctl', (root / 'launch.log').read_text())


@unittest.skipUnless(os.name == 'posix', 'POSIX native service installation policy')
class NativeHostToolsTests(unittest.TestCase):
    def test_linux_native_dependencies_keep_ocr_without_requiring_a_compiler(self):
        from linux_host import system_packages,HostPrerequisite
        for distro,version,audio in [('ubuntu','24.04','libasound2t64'),('ubuntu','22.04','libasound2'),('debian','13','libasound2t64'),('debian','12','libasound2')]:
            with patch('linux_host.Path.read_text',return_value=f'# fixture OS\nID={distro}\nVERSION_ID="{version}"\n'):
                native=system_packages(build_tools=False);source=system_packages()
                self.assertIn(audio,native);self.assertIn('tesseract-ocr-chi-sim',native)
                self.assertNotIn('build-essential',native)
                self.assertEqual(set(source)-set(native),{'build-essential','pkg-config','libssl-dev'})
        with patch('linux_host.Path.read_text',return_value='ID=unsupported\n'),self.assertRaises(HostPrerequisite):system_packages()

    def test_linux_service_resume_uses_prebuilt_commands_without_rebuilding_live_units(self):
        from linux_host import start_missing
        for prebuilt,active in [(False,False),(False,True),(True,False),(True,True)]:
            with self.subTest(prebuilt=prebuilt,active=active),tempfile.TemporaryDirectory() as folder:
                root=Path(folder).resolve();calls=[];units=[]
                spec=lambda _root,role:{'argv':[str(root/('runtime/services/bin/'+role if prebuilt else 'target/debug/ai-phone-'+role))],
                                          'environment':{'AI_PHONE_OCR':str(root/'runtime/services/bin/vision')} if prebuilt and role=='runner' else {},'prebuilt':prebuilt}
                def start(actual,name,argv,env):
                    self.assertEqual(actual,root);units.append((name,argv,env));return not active
                if prebuilt:(root/'runtime').mkdir();(root/'runtime/manifest.json').write_text('{}')
                with patch('prebuilt_services.launch_spec',side_effect=spec),patch('linux_host.runtime_env',return_value={'PATH':'/fixture'}),patch('linux_host.execute',side_effect=lambda args,**kwargs:calls.append([str(v) for v in args])),patch('linux_host.start_unit',side_effect=start),patch('linux_host.unit_state',return_value={'state':'running'} if active else None),patch('linux_host.subprocess.run',return_value=subprocess.CompletedProcess([],0,stdout='package:fixture')),patch('linux_host.shutil.which',return_value='/fixture/node'),patch('prebuilt_web.web_command',return_value=['/fixture/bun','/fixture/server.js']):
                    started=start_missing(root)
                expected=['emulator','runner','media','assistant','channels','web','lan']
                self.assertEqual([row[0] for row in units],expected)
                self.assertEqual(started,[] if active else expected)
                self.assertEqual(any(row[0]=='cargo' for row in calls),not prebuilt and not active)
                self.assertEqual(any('--migrate' in row for row in calls),not active)
                ready=next(row for row in calls if any(value.endswith('android-ready.sh') for value in row))
                self.assertEqual('--observe-only' in ready,active)
                if prebuilt:self.assertEqual(next(row[2] for row in units if row[0]=='runner')['AI_PHONE_OCR'],str(root/'runtime/services/bin/vision'))
        with tempfile.TemporaryDirectory() as folder,patch('prebuilt_services.launch_spec',side_effect=ValueError('invalid component')),patch('linux_host.execute') as execute:
            with self.assertRaisesRegex(ValueError,'invalid component'):start_missing(Path(folder))
            execute.assert_not_called()
            marker=Path(folder)/'data/restore-quarantine.json';marker.parent.mkdir();marker.write_text('{}')
            with self.assertRaisesRegex(ValueError,'quarantined'):start_missing(Path(folder))
            execute.assert_not_called()
        from linux_host import start_workbench_services
        with tempfile.TemporaryDirectory() as folder,patch('linux_host.shutil.which',return_value=None),patch('linux_host.unit_state',return_value=None),self.assertRaisesRegex(ValueError,'Node runtime unavailable'):
            start_workbench_services(Path(folder),{},lambda *_:None,lambda *_:None)

    def test_preflight_blocks_missing_capacity_before_installing_system_tools(self):
        from client_host import install_preflight
        with tempfile.TemporaryDirectory() as folder,patch('platform.system',return_value='Darwin'),patch('service_start.owned_jobs',return_value={}),patch('client_host.shutil.disk_usage',return_value=shutil._ntuple_diskusage(100,100,0)),patch('client_host.subprocess.run') as native:
            with self.assertRaisesRegex(ValueError,'free space'):
                install_preflight(Path(folder),Path(folder),{'minimum_free_gb':35},True,lambda *_args,**_kwargs:None)
            native.assert_not_called()

    def test_preflight_requests_build_tools_only_for_source_installations(self):
        from client_host import install_preflight
        with tempfile.TemporaryDirectory() as folder,patch('platform.system',return_value='Darwin'),patch('service_start.owned_jobs',return_value={}),patch('client_host.shutil.disk_usage',return_value=shutil._ntuple_diskusage(100*1024**3,0,100*1024**3)),patch('client_host.subprocess.run',return_value=subprocess.CompletedProcess([],1)) as native:
            events=[]
            self.assertFalse(install_preflight(Path(folder),Path(folder),{'minimum_free_gb':35},False,lambda *args,**kwargs:events.append((args,kwargs))))
            self.assertEqual(native.call_count,2)
            self.assertEqual(events[0][1]['error'],'command_line_tools')
        with tempfile.TemporaryDirectory() as folder,patch('platform.system',return_value='Linux'),patch('client_host.shutil.disk_usage',return_value=shutil._ntuple_diskusage(100*1024**3,0,100*1024**3)),patch('linux_host.prepare_system') as system:
            emit=lambda *_args,**_kwargs:None
            self.assertTrue(install_preflight(Path(folder),Path(folder),{'minimum_free_gb':35},True,emit))
            system.assert_called_once_with(emit,build_tools=False)

    def test_profile_creation_does_not_replace_existing_device_resources(self):
        from client_host import prepare_profile
        import release
        with tempfile.TemporaryDirectory() as folder,patch.dict(os.environ,{'JAVA_HOME':folder}):
            root=Path(folder);host={'phone_template':'android35-arm64'}
            request={'locale':'en','phone':{'cpus':2,'memory_mb':2048,'storage_gb':16},'desktop':{'cpus':2,'memory_mb':2560}}
            prepare_profile(root,root,request,host)
            path=root/release.PROFILE;before=path.read_bytes()
            prepare_profile(root,root,{**request,'locale':'de'},host)
            self.assertEqual(path.read_bytes(),before)
            with self.assertRaisesRegex(ValueError,'architecture'):
                prepare_profile(root,root,request,{'phone_template':'android35-x86_64'})
            self.assertEqual(path.read_bytes(),before)

    def test_prebuilt_install_does_not_install_rust_or_request_xcode(self):
        from client_host import install
        config={'minimum_free_gb':35,'minimum_resume_free_gb':2,'tools':['python@3.13.5','node@22','rust@1.96.0','java@17']}
        request={'locale':'en','phone':{'cpus':2,'memory_mb':2048,'storage_gb':16},'desktop':{'cpus':2,'memory_mb':2560}}
        for bundled in (False,True):
            with self.subTest(bundled=bundled),tempfile.TemporaryDirectory() as folder,patch('platform.system',return_value='Darwin'),patch('platform.machine',return_value='arm64'):
                root=Path(folder).resolve();NativeLauncherTests().fixture(root,'assistant-start.sh')
                if not bundled:shutil.rmtree(root/'runtime')
                calls=[]
                def stop_at_toolchain(args,**kwargs):
                    calls.append([str(value) for value in args]);raise RuntimeError('fixture reached toolchain')
                with patch('host_platform.host_policy',return_value=(config,{})),patch('service_start.owned_jobs',return_value={}),patch('client_host.shutil.disk_usage',return_value=shutil._ntuple_diskusage(100*1024**3,0,100*1024**3)),patch('client_host.subprocess.run',return_value=subprocess.CompletedProcess([],0)) as system,patch('client_host.execute',side_effect=stop_at_toolchain):
                    with self.assertRaisesRegex(RuntimeError,'fixture reached toolchain'):
                        install(root,root,request,root/'mise',lambda *_args,**_kwargs:None)
                self.assertEqual(any(value.startswith('rust@') for value in calls[0]),not bundled)
                self.assertEqual(system.called,not bundled)

    def test_doctor_checks_the_component_instead_of_absent_build_tools(self):
        import release
        with tempfile.TemporaryDirectory() as folder,patch('platform.system',return_value='Darwin'),patch('platform.machine',return_value='arm64'):
            root=Path(folder).resolve();NativeLauncherTests().fixture(root,'assistant-start.sh')
            with patch('host_platform.host_policy',return_value=({},{})),patch('release.shutil.which',return_value=None),patch('release.subprocess.check_output',return_value=str(16*1024**3)),patch('release.subprocess.run',return_value=subprocess.CompletedProcess([],0,stdout='javac 17',stderr='')):
                checks=release.doctor(root,minimum_free_gb=0)
            names={row['name'] for row in checks}
            self.assertNotIn('cargo',names);self.assertNotIn('rust-version',names);self.assertNotIn('swiftc',names)
            self.assertTrue(next(row['passed'] for row in checks if row['name']=='native-services'))


if __name__ == '__main__':
    unittest.main()
