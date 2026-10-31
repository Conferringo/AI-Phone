"""A complete local container-host resource stages without downloading tools."""
import hashlib
import io
import json
import os
import shutil
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from prepare_client_host import prepare
from client_host_entry import stage_source


@unittest.skipUnless(os.name == 'posix', 'Container host components require POSIX file semantics')
class ContainerHostRuntimeTests(unittest.TestCase):
    def setUp(self):
        mocked=patch('container_host_runtime.verify_targets',return_value={'lima':'14.4'},create=True);mocked.start();self.addCleanup(mocked.stop)
        # These inputs are byte-sized stand-ins, not an installation. Keep fixture
        # admission deterministic; the reserve test overrides both threshold edges.
        space=patch('container_host_runtime.shutil.disk_usage',return_value=shutil._ntuple_diskusage(8*1024**3,2*1024**3,6*1024**3))
        space.start();self.addCleanup(space.stop)

    def inputs(self,root,host='macos-arm64'):
        arch='x86_64' if host=='linux-x64' else 'aarch64'
        executable=b'\x7fELF\x02\x01\x01'+bytes(11)+b'\x3e\x00'+bytes(44) if host=='linux-x64' else b'limactl fixture'
        archives=root/'inputs';archives.mkdir();lima=archives/'lima.tar.gz'
        with tarfile.open(lima,'w:gz') as archive:
            directory=tarfile.TarInfo('.');directory.type=tarfile.DIRTYPE;archive.addfile(directory)
            for name,data,mode in [('./bin/limactl',executable,0o755),('./share/lima/lima-guestagent.Linux-'+arch+'.gz',b'guest agent fixture',0o644)]:
                entry=tarfile.TarInfo(name);entry.size=len(data);entry.mode=mode;archive.addfile(entry,io.BytesIO(data))
        iso=archives/'alpine.iso';iso.write_bytes(b'clean iso fixture');packages=archives/'apks';packages.mkdir();package=packages/'docker-1.0-r0.apk';package.write_bytes(b'signed APK fixture')
        record=lambda path:{'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'size_bytes':path.stat().st_size,'url':'https://example.invalid/'+path.name}
        policy={'engine':'lima-qemu' if host=='linux-x64' else 'lima-vz','arch':arch,'lima':'2.2.1','alpine':'3.24.1','docker':'1.0','compose':'1.0','image':record(iso),'packages':[{'file':package.name,**record(package)}]}
        (root/'config').mkdir();(root/'config/container-host-runtime.json').write_text(json.dumps({'format':1,'hosts':{host:policy}}));(root/'config/host-bootstrap.json').write_text(json.dumps({'lima_hosts':{host:record(lima)}}))
        config=root/'apps/client/src-tauri/tauri.conf.json';config.parent.mkdir(parents=True);config.write_text(json.dumps({'bundle':{'resources':{'gen/host/source.payload':'host/source.payload','gen/host/entry.py':'host/entry.py'},'macOS':{'minimumSystemVersion':'14.4'}}}))
        return lima,iso,packages

    def test_linux_component_uses_its_own_qemu_and_ssh_after_staging(self):
        from container_host_runtime import build, selection, definition
        import linux_host_runtime as native
        from test_linux_ocr_runtime import LinuxOcrTests
        with tempfile.TemporaryDirectory() as temporary, patch('platform.system',return_value='Linux'), patch('platform.machine',return_value='x86_64'), patch('container_host_runtime.verify_targets',side_effect=AssertionError('macOS inspector used on Linux')):
            root=Path(temporary);lima,iso,packages=self.inputs(root,'linux-x64')
            archive=LinuxOcrTests().fixture(root,profile=native.PROFILE)
            native.build(root,archive,root/'.tools/linux-host-runtime')
            build(root,lima,iso,packages)
            (root/'scripts/client_host_entry.py').write_text('fixture')
            prepare(root,with_container_host=True)
            home=root/'installation';(home/'user/data').mkdir(parents=True);(home/'user/data/chat').write_text('keep')
            staged=stage_source(root/'apps/client/src-tauri/gen/host/source.payload',home)
            runtime=selection(staged);value=definition(home,runtime,'4GiB')
            self.assertEqual(runtime['command'],staged/'runtime/container-host/bin/limactl')
            self.assertIn('native/launchers',runtime['command'].read_text())
            self.assertEqual(value['vmType'],'qemu');self.assertEqual(value['mountType'],'9p')
            self.assertEqual((home/'user/data/chat').read_text(),'keep')
            manifest=staged/'runtime/container-host/manifest.json';original=json.loads(manifest.read_text())
            for targets in (None,{'lima':'macos-arm64'},{'lima':'linux-x64','extra':'linux-x64'}):
                manifest.write_text(json.dumps({**original,'native_targets':targets}))
                with self.assertRaisesRegex(ValueError,'target'):selection(staged)
            manifest.write_text(json.dumps(original))
            (staged/'runtime/container-host/native/bin/ssh').write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError,'integrity'):selection(staged)

    def test_real_resource_paths_remain_bounded_after_source_staging(self):
        from container_host_runtime import build,selection
        with tempfile.TemporaryDirectory() as temporary,patch('container_host_runtime.host_key',return_value='macos-arm64'):
            root=Path(temporary);lima,iso,packages=self.inputs(root);build(root,lima,iso,packages)
            (root/'scripts').mkdir();(root/'scripts/client_host_entry.py').write_text('fixture')
            prepare(root,with_container_host=True)
            home=root/'installation';(home/'user/data').mkdir(parents=True);(home/'user/data/chat').write_text('keep')
            staged=stage_source(root/'apps/client/src-tauri/gen/host/source.payload',home)
            runtime=selection(staged)
            self.assertEqual(runtime['command'].read_bytes(),b'limactl fixture');self.assertEqual(runtime['image'].read_bytes(),b'clean iso fixture')
            self.assertEqual((home/'user/data/chat').read_text(),'keep')
            runtime['command'].write_bytes(b'changed')
            with self.assertRaises(ValueError):selection(staged)

    def test_bad_factory_sources_never_publish_a_partial_component(self):
        from container_host_runtime import build
        with tempfile.TemporaryDirectory() as temporary,patch('container_host_runtime.host_key',return_value='macos-arm64'):
            root=Path(temporary);lima,iso,packages=self.inputs(root);iso.write_bytes(b'changed')
            with self.assertRaises(ValueError):build(root,lima,iso,packages)
            self.assertFalse((root/'.tools/container-host').exists())

    def test_invalid_runtime_metadata_and_tree_never_activate(self):
        from container_host_runtime import build,verify,archive_path
        with tempfile.TemporaryDirectory() as temporary,patch('container_host_runtime.host_key',return_value='macos-arm64'):
            root=Path(temporary);lima,iso,packages=self.inputs(root);build(root,lima,iso,packages);folder=root/'.tools/container-host';path=folder/'manifest.json';value=json.loads(path.read_text())
            for extra in ({'format':True},{'format':2},{'host':'linux-x64'},{'policy_sha256':'0'*64},{'native_targets':None},{'native_targets':{'lima':14.4}},{'files':None},{'files':[]}):
                path.write_text(json.dumps({**value,**extra}))
                with self.assertRaises(ValueError):verify(root,folder)
            path.write_text(json.dumps(value));(folder/'unexpected').write_text('unexpected')
            with self.assertRaises(ValueError):verify(root,folder)
            (folder/'unexpected').unlink();(folder/'alias').symlink_to(folder/'image.iso')
            with self.assertRaises(ValueError):verify(root,folder)
            for name,kind in (('../escape',tarfile.REGTYPE),('hardlink',tarfile.LNKTYPE)):
                entry=tarfile.TarInfo(name);entry.type=kind
                with self.assertRaises(ValueError):archive_path(entry)
            entry=tarfile.TarInfo('share/doc/lima/templates');entry.type=tarfile.SYMTYPE;entry.linkname='../../lima/templates';self.assertIsNone(archive_path(entry))

    def test_factory_and_cached_component_cannot_claim_a_newer_os_target(self):
        from container_host_runtime import build,verify
        with tempfile.TemporaryDirectory() as temporary,patch('container_host_runtime.host_key',return_value='macos-arm64'):
            root=Path(temporary);lima,iso,packages=self.inputs(root)
            with patch('container_host_runtime.verify_targets',side_effect=ValueError('newer macOS')):
                with self.assertRaisesRegex(ValueError,'newer macOS'):build(root,lima,iso,packages)
            self.assertFalse((root/'.tools/container-host').exists())
            build(root,lima,iso,packages);folder=root/'.tools/container-host';path=folder/'manifest.json';value=json.loads(path.read_text());value['native_targets']={'lima':'26.0'};path.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError,'target'):verify(root,folder)

    def test_prebuilt_definition_uses_local_signed_packages_without_proxy_inheritance(self):
        from container_host_runtime import definition
        root=Path('/owned/releases/version');home=Path('/owned');runtime={'command':root/'runtime/container-host/lima/bin/limactl','image':root/'runtime/container-host/image.iso','packages':root/'runtime/container-host/packages','arch':'aarch64','image_sha256':'a'*64}
        value=definition(home,runtime,'4GiB')
        self.assertFalse(value['propagateProxyEnv']);self.assertFalse(value['ssh']['loadDotSSHPubKeys'])
        self.assertFalse(value['ssh']['forwardAgent']);self.assertEqual(value['containerd'],{'system':False,'user':False});self.assertTrue(value['provision'][0]['skipDefaultDependencyResolution'])
        self.assertTrue(all(not str(v['location']).startswith('http') for v in value['images']))
        scripts='\n'.join(row['script'] for row in value['provision'])
        self.assertIn('--no-network',scripts);self.assertNotIn('apt-get',scripts);self.assertNotIn('curl',scripts)
        self.assertEqual(value['memory'],'4GiB');self.assertEqual(value['portForwards'],[{'guestPortRange':[17860,17869],'ignore':True}])
        self.assertEqual(value['mounts'],[{'location':str(home),'writable':True}])

    def test_archive_count_sizes_and_extraction_directory_reuse(self):
        from container_host_runtime import archive_entries,unpack_lima,LIMIT
        def entry(name,size):
            row=tarfile.TarInfo(name);row.size=size;return row
        self.assertEqual(len(list(archive_entries([entry('empty',0)]*2048))),2048)
        for entries in ([entry('empty',0)]*2049,[entry('negative',-1)],[entry('large',200*1024**2+1)]):
            with self.assertRaises(ValueError):list(archive_entries(entries))
        exact=[entry(str(i),size) for i,size in enumerate((200*1024**2,200*1024**2,LIMIT-400*1024**2))];self.assertEqual(len(list(archive_entries(exact))),3)
        with self.assertRaises(ValueError):list(archive_entries([*exact,entry('extra',1)]))
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);archive=root/'repeated.tar.gz'
            with tarfile.open(archive,'w:gz') as target:
                for name in ('bin/one','bin/two'):
                    row=entry(name,1);target.addfile(row,io.BytesIO(b'x'))
            unpack_lima(archive,root/'unpacked');self.assertEqual((root/'unpacked/bin/two').read_bytes(),b'x')

    def test_component_factory_reserve_pins_and_manifest_size_boundaries(self):
        from container_host_runtime import build,verify,pinned_file,package_inputs,RESERVE,LIMIT
        with tempfile.TemporaryDirectory() as temporary,patch('container_host_runtime.host_key',return_value='macos-arm64'):
            root=Path(temporary);lima,iso,packages=self.inputs(root)
            with patch('container_host_runtime.shutil.disk_usage',return_value=type('Space',(),{'free':RESERVE+LIMIT-1})()):
                with self.assertRaises(ValueError):build(root,lima,iso,packages)
            with patch('container_host_runtime.shutil.disk_usage',return_value=type('Space',(),{'free':RESERVE+LIMIT})()):build(root,lima,iso,packages)
            folder=root/'.tools/container-host';manifest=folder/'manifest.json';body=manifest.read_text();manifest.write_text(body+' '*(256*1024-len(body)));verify(root,folder)
            manifest.write_text(manifest.read_text()+' ')
            with self.assertRaises(ValueError):verify(root,folder)
            manifest.write_text(body);link=root/'iso-link';link.symlink_to(iso)
            pin={'sha256':hashlib.sha256(iso.read_bytes()).hexdigest(),'size_bytes':iso.stat().st_size}
            with self.assertRaises(ValueError):pinned_file(link,pin)
            row={'file':next(packages.iterdir()).name,'sha256':hashlib.sha256(next(packages.iterdir()).read_bytes()).hexdigest()}
            with self.assertRaises(ValueError):package_inputs(packages,[row,row])


if __name__=='__main__':unittest.main()
