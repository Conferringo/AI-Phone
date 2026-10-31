"""Behavioral contracts for an offline WSL backend, independent of Windows boot."""
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from wsl_backend import inspect_rootfs, publish_capsule, verify_capsule
from wsl_backend_factory import capacity, engine_script, factory_plan


def archive(path, rows):
    with tarfile.open(path,'w:gz') as target:
        for name,value in rows:
            entry=tarfile.TarInfo(name)
            if isinstance(value,tuple):entry.type=tarfile.SYMTYPE;entry.linkname=value[0];target.addfile(entry)
            else:entry.size=len(value);entry.mode=0o644;target.addfile(entry,io.BytesIO(value))


def rootfs(path):
    archive(path,[('etc/os-release',b'ID=ubuntu\n'),('etc/wsl.conf',b'[interop]\nenabled=false\n'),
        ('opt/aiphone/bundle/source.payload',b'fixture-source'),('opt/aiphone/bundle/images.payload',b'fixture-images'),
        ('opt/aiphone/tools/python/bin/python3',b'fixture-python'),('opt/aiphone/tools/node/bin/node',b'fixture-node'),
        ('opt/aiphone/bootstrap/wsl_backend_entry.py',b'fixture-entry'),('usr/local/bin/docker',b'fixture-docker'),
        ('usr/local/bin/dockerd',b'fixture-dockerd'),('lib64/ld-linux-x86-64.so.2',('/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2',)),
        ('usr/share/doc/base-files/copyright',b'fixture-license')])


class WslBackendTests(unittest.TestCase):
    def test_capsule_can_be_moved_verified_and_rejects_tampered_body(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);image=root/'rootfs.tar.gz';rootfs(image)
            output=root/'backend.payload';publish_capsule(image,output,{'fixture':True})
            moved=root/'renamed.payload';output.rename(moved)
            value=verify_capsule(moved);self.assertEqual(value['host'],'windows-x64')
            self.assertFalse(value['windows_boot_verified']);self.assertFalse(value['complete_device_bundle'])
            with moved.open('r+b') as body:body.seek(512);body.write(b'tampered')
            with self.assertRaises(ValueError):verify_capsule(moved)

    def test_existing_component_and_user_data_are_never_replaced(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);image=root/'rootfs.tar.gz';rootfs(image)
            target=root/'backend.payload';target.write_bytes(b'existing')
            state=root/'user/chat';state.parent.mkdir();state.write_text('keep')
            with self.assertRaises(FileExistsError):publish_capsule(image,target,{})
            self.assertEqual(target.read_bytes(),b'existing');self.assertEqual(state.read_text(),'keep')

    def test_rootfs_rejects_missing_backend_private_state_and_other_virtualizers(self):
        with tempfile.TemporaryDirectory() as folder:
            target=Path(folder)/'rootfs.tar.gz'
            for name in ('../escape','root/.ssh/id_rsa','var/lib/aiphone/user/chat','opt/aiphone/qemu-system-x86_64','opt/aiphone/lima/bin/limactl','opt/aiphone/android/emulator'):
                with self.subTest(name=name):
                    archive(target,[(name,b'bad')])
                    with self.assertRaises(ValueError):inspect_rootfs(target)
            archive(target,[('etc/os-release',b'ID=ubuntu')])
            with self.assertRaisesRegex(ValueError,'Incomplete'):inspect_rootfs(target)

    def test_rootfs_rejects_alias_parent_and_duplicate_file(self):
        with tempfile.TemporaryDirectory() as folder:
            target=Path(folder)/'rootfs.tar.gz'
            for rows in ([('opt/aiphone',('/tmp/foreign',)),('opt/aiphone/bundle/source.payload',b'bad')],
                         [('etc/os-release',b'one'),('etc/os-release',b'two')]):
                archive(target,rows)
                with self.assertRaises(ValueError):inspect_rootfs(target)

    def test_failed_inspection_does_not_publish_partial_component(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);bad=root/'bad.tar.gz';bad.write_bytes(b'not a tar')
            with self.assertRaises(ValueError):publish_capsule(bad,root/'out.payload',{})
            self.assertEqual({p.name for p in root.iterdir()},{'bad.tar.gz'})

    def test_capacity_guard_uses_peak_copies_and_preserves_four_gib_reserve(self):
        with patch('wsl_backend_factory.shutil.disk_usage') as usage:
            usage.return_value.free=3*1024**3
            with self.assertRaisesRegex(ValueError,'4 GiB reserve'):capacity(Path('.'),100,1000)
            usage.return_value.free=4*1024**3+4100;self.assertEqual(capacity(Path('.'),100,1000),4*1024**3+4100)
        plan=factory_plan(Path(__file__).resolve().parent.parent)
        self.assertEqual(plan['factory_capacity'],'context + 4 * rootfs_budget + 4 GiB reserve')
        self.assertEqual(plan['minimum_runtime_free_gb'],4)
        self.assertEqual(plan['backend_host'],'linux-x64')
        self.assertNotIn('lima',plan['runtime_components'])
        self.assertNotIn('android-runtime',plan['runtime_components'])

    def test_engine_environment_does_not_replace_general_backend_path(self):
        source=engine_script('docker')
        self.assertIn('LD_LIBRARY_PATH=',source)
        self.assertIn('XTABLES_LIBDIR="$engine/usr/lib/xtables"',source)
        self.assertIn('exec "$engine/usr/bin/docker"',source)
        self.assertNotIn('export PATH=',source)
        with self.assertRaises(ValueError):engine_script('../sh')


class WslSourceTests(unittest.TestCase):
    def payload(self,folder,host='linux-x64',extra=None):
        rows={}
        from client_host_entry import COMPONENT_MANIFESTS
        for role in ('web','native-services','js-services'):
            rows[COMPONENT_MANIFESTS[role]]=json.dumps({'host':host,'vision':'linux-tesseract-bundled'}).encode()
        if extra:rows.update(extra)
        records=[{'path':name,'size':len(data),'sha256':hashlib.sha256(data).hexdigest(),'mode':0o644} for name,data in rows.items()]
        manifest={'format':2,'components':[role for role in COMPONENT_MANIFESTS if role in ('web','native-services','js-services')],'files':records}
        path=folder/'source.payload';archive(path,[*rows.items(),('HOST-MANIFEST.json',json.dumps(manifest).encode())]);return path

    def test_source_admission_requires_verified_linux_components_and_offline_ocr(self):
        from wsl_backend_factory import source_input
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=self.payload(root);identity=source_input(source)
            self.assertEqual(identity,hashlib.sha256(source.read_bytes()).hexdigest())
            for host in ('windows-x64','macos-arm64','linux-arm64'):
                with self.subTest(host=host):
                    with self.assertRaisesRegex(ValueError,'Linux x64'):source_input(self.payload(root,host))
            source=self.payload(root,extra={'runtime/services/manifest.json':b'{"host":"linux-x64","vision":"system"}'})
            with self.assertRaisesRegex(ValueError,'offline OCR'):source_input(source)
            source=self.payload(root,extra={'user/chat.txt':b'private'})
            with self.assertRaises(ValueError):source_input(source)

    def test_export_targets_only_new_owned_container_identity(self):
        from wsl_backend_factory import export_owned
        identity='d'*64
        with tempfile.TemporaryDirectory() as folder,patch('wsl_backend_factory.run') as run,patch('wsl_backend_factory.subprocess.check_output') as inspect:
            run.return_value.stdout=identity+'\n';inspect.return_value=json.dumps([{'Config':{'Labels':{'dev.aiphone.factory':'wsl-backend-factory'},'Image':'factory:unit'}}])
            export_owned('factory:unit',Path(folder)/'image.tar','fixture')
            calls=[call.args[0] for call in run.call_args_list]
            self.assertIn(['docker','export',identity],calls);self.assertIn(['docker','rm',identity],calls)
            run.reset_mock();inspect.return_value=json.dumps([{'Config':{'Labels':{},'Image':'foreign'}}])
            with self.assertRaisesRegex(ValueError,'ownership'):export_owned('factory:unit',Path(folder)/'other.tar','fixture')
            self.assertFalse(any(call.args[0][1]=='rm' for call in run.call_args_list))

    def test_engine_receipt_comes_from_exact_pinned_archives(self):
        from wsl_engine_inputs import extract,licenses
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);config=root/'config';config.mkdir()
            package=root/'package.apk';archive(package,[('.PKGINFO',b'pkgname = docker-engine\npkgver = 1.0\nlicense = Apache-2.0\nurl = https://mobyproject.org\n')])
            pin={'file':'package.apk','size_bytes':package.stat().st_size,'sha256':hashlib.sha256(package.read_bytes()).hexdigest(),'url':'https://example.invalid/pinned.apk'}
            (config/'container-host-runtime.json').write_text(json.dumps({'hosts':{'linux-x64':{'packages':[pin]}}}))
            source=root/'engine.tar.gz';archive(source,[('packages/package.apk',package.read_bytes()),('lima/unneeded',b'never-extract')])
            with patch('wsl_engine_inputs.shutil.disk_usage') as usage:
                usage.return_value.free=4*1024**3+pin['size_bytes']
                receipt=extract(source,root,root/'packages')
                self.assertEqual(receipt['packages'],1);self.assertEqual({p.name for p in (root/'packages').iterdir()},{'package.apk'})
                self.assertEqual(licenses(root/'packages',[pin])[0]['license'],'Apache-2.0')
                with self.assertRaises(FileExistsError):extract(source,root,root/'packages')
                archive(source,[('packages/package.apk',b'tampered')])
                with self.assertRaises(ValueError):extract(source,root,root/'bad')
                self.assertFalse((root/'bad').exists())
                usage.return_value.free=4*1024**3
                with self.assertRaisesRegex(ValueError,'4 GiB reserve'):extract(source,root,root/'bad')

class WslEntryTests(unittest.TestCase):
    def test_verification_staging_never_activates_tools_or_populates_user_state(self):
        import wsl_backend_entry as entry
        with tempfile.TemporaryDirectory() as folder,patch('client_host_entry.stage_source') as stage,patch('installer_tools.select') as select:
            root=Path(folder);stage.return_value=root
            try:self.assertEqual(entry.staged(root),root)
            finally:__import__('sys').path.remove(str(root/'scripts'))
            select.assert_not_called();self.assertFalse((root/'data').exists())

    def test_initialize_serializes_writers_without_using_existing_installation(self):
        import wsl_backend_entry as entry
        from installer_lock import acquire
        with tempfile.TemporaryDirectory() as folder,patch.object(entry,'INSTALLATION',Path(folder)/'installation'),patch.object(entry,'initialize_locked') as initialize:
            initialize.return_value={'backend_started':True,'phone_ready':False}
            with acquire(Path(folder)/'initialize.lock'):
                with self.assertRaises(BlockingIOError):entry.initialize('fixture')
            initialize.assert_not_called()
            self.assertFalse(entry.initialize('fixture')['phone_ready'])

    def test_engine_wait_does_not_treat_an_unavailable_daemon_as_ready(self):
        import wsl_backend_entry as entry
        from types import SimpleNamespace
        from unittest.mock import Mock
        run=Mock(return_value=SimpleNamespace(returncode=1))
        with patch.object(entry.time,'monotonic',side_effect=[0,0,31]),patch.object(entry.time,'sleep'):
            with self.assertRaisesRegex(ValueError,'did not become ready'):entry.wait_engine(run)
        run.return_value.returncode=0
        with patch.object(entry.time,'monotonic',side_effect=[0,0]):entry.wait_engine(run)

class WslExecutionPlanTests(unittest.TestCase):
    def test_only_validated_top_level_bootstrap_scripts_enter_the_context(self):
        from wsl_backend_factory import payload_scripts
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'source.payload'
            archive(source,[('scripts/wsl_backend_entry.py',b'entry'),('scripts/helper.py',b'helper'),('user/chat',b'private'),('scripts/nested/file.py',b'nested')])
            payload_scripts(source,root/'scripts')
            self.assertEqual({p.name for p in (root/'scripts').iterdir()},{'wsl_backend_entry.py','helper.py'})
            archive(source,[('scripts/helper.py',b'helper')])
            with self.assertRaisesRegex(ValueError,'Rebuild'):payload_scripts(source,root/'missing')

    def test_wrong_host_and_low_capacity_never_materialize_user_state(self):
        import wsl_backend_entry as entry
        with patch.object(entry.platform,'system',return_value='Windows'):
            with self.assertRaises(ValueError):entry.require_linux()
        with patch.object(entry.platform,'system',return_value='Linux'),patch.object(entry.platform,'machine',return_value='x86_64'):
            entry.require_linux()
        with tempfile.TemporaryDirectory() as folder,patch.object(entry,'require_linux'),patch.object(entry,'staged') as stage,patch('client_host.check_request',return_value={}):
            request=Path(folder)/'request';request.write_text('{}')
            with patch.object(entry.shutil,'disk_usage') as usage:
                usage.return_value.free=3*1024**3
                with self.assertRaisesRegex(ValueError,'4 GiB'):entry.initialize_locked(request)
            stage.assert_not_called()

    def test_backend_start_uses_only_loaded_images_and_preserves_chat_file(self):
        import wsl_backend_entry as entry
        from unittest.mock import Mock
        from types import SimpleNamespace
        from contextlib import ExitStack
        run=Mock()
        with tempfile.TemporaryDirectory() as folder,ExitStack() as stack:
            stack.enter_context(patch.dict(entry.os.environ,dict(entry.os.environ)))
            root=Path(folder);(root/'chat').write_text('existing')
            for name in ('client_host.verify_container_ownership','linux_host.private_token','linux_host.start_web_service','linux_services.start_unit'):
                stack.enter_context(patch(name))
            stack.enter_context(patch('linux_services.unit_state',return_value=None))
            stack.enter_context(patch('container_images.stage',return_value=SimpleNamespace()))
            stack.enter_context(patch('container_images.activate',return_value={}))
            stack.enter_context(patch('runtime_config.runtime_env',return_value={}))
            stack.enter_context(patch('prebuilt_services.launch_spec',return_value={'argv':['assistant'],'environment':{}}))
            stack.enter_context(patch('prebuilt_js.launch_spec',side_effect=lambda _root,name:{'argv':[name],'environment':{}}))
            entry.start_backend(root,run)
            calls=[list(map(str,call.args[0])) for call in run.call_args_list]
            self.assertTrue(any('postgres' in argv and '--no-build' in argv and '--pull' in argv and 'never' in argv for argv in calls))
            self.assertTrue(any('desktop' in argv and '--no-build' in argv and 'never' in argv for argv in calls))
            self.assertFalse(any('emulator' in word or 'limactl' in word for argv in calls for word in argv))
            self.assertEqual((root/'chat').read_text(),'existing')

class WslQualificationGuards(unittest.TestCase):
    def test_qualification_guards_preserve_existing_factory_and_low_space(self):
        from wsl_backend_qualification import qualification_preflight
        with tempfile.TemporaryDirectory() as folder,patch('wsl_backend_qualification.host_key',return_value='windows-x64'):
            with self.assertRaisesRegex(ValueError,'Linux x64'):qualification_preflight(Path(folder))
        with tempfile.TemporaryDirectory() as folder,patch('wsl_backend_qualification.host_key',return_value='linux-x64'),patch('wsl_backend_qualification.check_docker'),patch('check_source.check'),patch('wsl_backend_qualification.shutil.disk_usage') as usage:
            root=Path(folder);usage.return_value.free=3*1024**3
            with self.assertRaisesRegex(ValueError,'4 GiB'):qualification_preflight(root)
            usage.return_value.free=4*1024**3
            work=qualification_preflight(root);self.assertFalse(work.exists())
            work.mkdir(parents=True);(work/'evidence').write_text('keep')
            with self.assertRaises(FileExistsError):qualification_preflight(root)
            self.assertEqual((work/'evidence').read_text(),'keep')


class WslEngineRelocationTests(unittest.TestCase):
    def test_raw_alpine_absolute_shell_link_stays_with_owned_engine(self):
        from wsl_engine_inputs import relocate_links
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);(root/'bin').mkdir();(root/'etc').mkdir()
            (root/'bin/busybox').write_bytes(b'fixture executable')
            (root/'bin/sh').symlink_to('/bin/busybox')
            (root/'etc/mtab').symlink_to('/proc/mounts')
            self.assertEqual(relocate_links(root),['bin/sh'])
            self.assertEqual((root/'bin/sh').resolve(),(root/'bin/busybox').resolve())
            self.assertEqual(__import__('os').readlink(root/'etc/mtab'),'/proc/mounts')
            self.assertEqual(relocate_links(root),[])


if __name__=='__main__':unittest.main()
