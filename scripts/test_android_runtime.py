"""Offline Android package admission and device creation preserve existing state."""
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from android_runtime import stage, verify_sdk, checked_manifest
from android_avd import create
from host_platform import host_key


class AndroidRuntimeTests(unittest.TestCase):
    def setUp(self):
        host=patch('android_runtime.host_key',return_value='macos-arm64');host.start();self.addCleanup(host.stop)

    def fixture(self, root, *, changes=None, body=None):
        abi='arm64-v8a'
        base=f'sdk/system-images/android-35/default/{abi}/'
        files={name:b'factory fixture' for name in ['sdk/emulator/emulator','sdk/platform-tools/adb',base+'system.img',base+'vendor.img',base+'kernel-ranchu',base+'ramdisk.img',base+'NOTICE.txt']}
        manifest={'format':1,'host':'macos-arm64','api':35,'abi':abi,'redistribution':'review-pending','files':[{'path':name,'size':len(data),'sha256':hashlib.sha256(data).hexdigest(),'mode':0o755 if name.endswith(('adb','emulator')) else 0o644} for name,data in files.items()]}
        if changes:manifest.update(changes)
        payload=root/'android.payload'
        with tarfile.open(payload,'w:gz') as target:
            for name,data in [('ANDROID-MANIFEST.json',json.dumps(manifest).encode()),*((body or files).items())]:
                entry=tarfile.TarInfo(name);entry.size=len(data);entry.mode=0o644;target.addfile(entry,io.BytesIO(data))
        return payload,manifest

    def test_relocation_offline_admission_and_tamper_detection(self):
        with tempfile.TemporaryDirectory(prefix='android fixture ') as temp:
            root=Path(temp);payload,manifest=self.fixture(root);home=root/'installation'
            sdk=stage(payload,home);self.assertEqual(verify_sdk(sdk)['abi'],manifest['abi'])
            self.assertEqual(stage(payload,home),sdk)
            (sdk/'platform-tools/adb').write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError,'integrity'):stage(payload,home)

    def test_schema_and_paths_fail_before_touching_user_state(self):
        for changes in [{'format':True},{'host':'another-host'},{'api':36},{'abi':'wrong'},{'files':[]},{'redistribution':'cleared'}]:
            with self.subTest(changes=changes),tempfile.TemporaryDirectory() as temp:
                root=Path(temp);payload,_=self.fixture(root,changes=changes);user=root/'install/user/data';user.mkdir(parents=True);(user/'chat').write_text('keep')
                with self.assertRaises(ValueError):stage(payload,root/'install')
                self.assertEqual((user/'chat').read_text(),'keep')
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);_,manifest=self.fixture(root)
            for name in ['sdk/../user/chat','sdk/emulator/../../escape','sdk//emulator/x','sdk/emulator/./x','sdk/emulator/userdata-qemu.img','sdk/emulator/x\\y','sdk/emulator/.env','sdk/emulator/snapshots/x','sdk/system-images/android-35/default/wrong/system.img']:
                row={**manifest['files'][0],'path':name}
                with self.subTest(path=name),self.assertRaises(ValueError):checked_manifest({**manifest,'files':[row,*manifest['files'][1:]]})
            for field,value in [('size',True),('size',4*1024**3+1),('sha256','x'*64),('mode',0o777)]:
                rows=[{**manifest['files'][0],field:value},*manifest['files'][1:]]
                with self.assertRaises(ValueError):checked_manifest({**manifest,'files':rows})

    def test_missing_extra_duplicate_or_corrupt_bodies_never_activate(self):
        for mode in ['missing','extra','corrupt']:
            with self.subTest(mode=mode),tempfile.TemporaryDirectory() as temp:
                root=Path(temp);payload,manifest=self.fixture(root)
                # Build a second inventory-compliant header with a bad body set.
                body={r['path']:b'factory fixture' for r in manifest['files']}
                if mode=='missing':body.pop('sdk/platform-tools/adb')
                if mode=='extra':body['sdk/emulator/extra']=b'unlisted'
                if mode=='corrupt':body['sdk/platform-tools/adb']=b'changed fixture'
                payload,_=self.fixture(root,body=body)
                with self.assertRaises(ValueError):stage(payload,root/'installation')
                self.assertFalse(any((root/'installation/components/android').glob('[0-9a-f]'*64)))

    def test_links_and_space_shortfall_cannot_redirect_or_half_install(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,_=self.fixture(root);outside=root/'outside';outside.mkdir();home=root/'home';home.mkdir();(home/'components').symlink_to(outside,target_is_directory=True)
            with self.assertRaises(ValueError):stage(payload,home)
            self.assertEqual(list(outside.iterdir()),[])
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,_=self.fixture(root)
            with patch('android_runtime.shutil.disk_usage',return_value=type('Usage',(),{'free':4*1024**3})()):
                with self.assertRaisesRegex(ValueError,'free space'):stage(payload,root/'home')

    def test_offline_avd_creation_and_resource_preservation(self):
        with tempfile.TemporaryDirectory(prefix='phone fixture ') as temp:
            root=Path(temp);payload,manifest=self.fixture(root);sdk=stage(payload,root/'home');avd=root/'user/avd';avd.mkdir(parents=True)
            self.assertTrue(create(sdk,avd,manifest['abi'],2,2048,24))
            config=avd/'phone-local-01.avd/config.ini';text=config.read_text()
            for value in ['hw.cpu.ncore=2','hw.ramSize=2048','disk.dataPartition.size=24G','hw.lcd.height=1440','showDeviceFrame=no','hw.cpu.arch=arm64']:self.assertIn(value,text)
            self.assertFalse(create(sdk,avd,manifest['abi'],4,4096,32));self.assertEqual(config.read_text(),text)
            (avd/'ai_phone_p2.ini').unlink()
            with self.assertRaisesRegex(ValueError,'incomplete'):create(sdk,avd,manifest['abi'],2,2048,24)
            self.assertEqual(config.read_text(),text)

    def test_bundled_install_never_invokes_sdk_manager_or_java(self):
        import release
        from client_host import prepare_profile, deployment_sdk, selected_tools
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,manifest=self.fixture(root);sdk=stage(payload,root/'home')
            env={'AI_PHONE_SDK_ROOT':str(sdk),'AI_PHONE_ANDROID_ABI':manifest['abi'],'JAVA_HOME':''}
            with patch('release.subprocess.run') as run:
                checks=release.sdk_checks(env)
                self.assertTrue(all(row['passed'] for row in checks));run.assert_not_called()
            phases=[]
            state=SimpleNamespace(phase=lambda name,fingerprint,operation,valid:phases.append((name,valid())))
            with patch('release.run') as run:
                release.install_sdk(state,root,env,lambda *_:self.fail('Online SDK requested'))
                run.assert_not_called()
            self.assertEqual(phases,[('sdk-packages',True)])
            tools=['python@3.13.5','node@22','rust@1.96.0','java@17']
            self.assertEqual(selected_tools(tools,True,True,(),True),tools[:2])
            request={'locale':'en','phone':{'cpus':2,'memory_mb':2048,'storage_gb':16},'desktop':{'cpus':2,'memory_mb':2560}}
            host={'phone_template':'android35-arm64' if manifest['abi']=='arm64-v8a' else 'android35-x86_64'}
            with patch.dict('os.environ',{'JAVA_HOME':''}):prepare_profile(root,root,request,host,sdk)
            saved=(root/release.PROFILE).read_text();self.assertEqual(json.loads(saved)['sdk_root'],str(sdk))
            self.assertEqual(deployment_sdk(root,root/'different'),sdk)
            prepare_profile(root,root,request,host,root/'different')
            self.assertEqual((root/release.PROFILE).read_text(),saved)

    def test_builder_streams_only_factory_images_and_is_deterministic(self):
        import zipfile
        from android_runtime import build
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);_,manifest=self.fixture(root);abi=manifest['abi'];sdk=root/'source-sdk'
            for name in ('emulator/emulator','emulator/source.properties','platform-tools/adb','platform-tools/source.properties','platform-tools/empty'):
                path=sdk/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('' if path.name=='empty' else 'fixture component')
                if path.name in ('emulator','adb'):path.chmod(0o755)
            image=root/'factory.zip'
            with zipfile.ZipFile(image,'w') as source:
                for name in ('system.img','vendor.img','kernel-ranchu','ramdisk.img','NOTICE.txt'):source.writestr(abi+'/'+name,b'factory fixture')
            expected=hashlib.sha256(image.read_bytes()).hexdigest();first=root/'first.payload';second=root/'second.payload'
            with self.assertRaisesRegex(ValueError,'checksum'):build(sdk,image,first,'0'*64)
            first=root/'nested/output/first.payload'
            build(sdk,image,first,expected);build(sdk,image,second,expected)
            self.assertEqual(first.read_bytes(),second.read_bytes());verify_sdk(stage(first,root/'installation'))
            (sdk/'emulator/private-link').symlink_to(root/'android.payload')
            with self.assertRaisesRegex(ValueError,'link'):build(sdk,image,second,expected)
            (sdk/'emulator/private-link').unlink();(sdk/'platform-tools/source.properties').unlink()
            with self.assertRaisesRegex(ValueError,'metadata'):build(sdk,image,second,expected)

    def test_budget_duplicate_modes_and_required_inventory_are_strict(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);_,manifest=self.fixture(root)
            rows=manifest['files']
            variants=[[rows[0],*rows],rows[1:],[{**rows[0],'mode':0o644},*rows[1:]],rows+[{'path':'sdk/emulator/large-'+str(i),'size':4*1024**3,'sha256':'0'*64,'mode':0o644} for i in range(3)]]
            for variant in variants:
                with self.assertRaises(ValueError):checked_manifest({**manifest,'files':variant})
            for name in [None,'','sdk/emulator/space'+chr(0),'/sdk/emulator/absolute']:
                with self.assertRaises(ValueError):checked_manifest({**manifest,'files':[{**rows[0],'path':name},*rows[1:]]})
            for value in [None,[],{}, {'format':1}, {**manifest,'format':2}]:
                with self.assertRaises(ValueError):checked_manifest(value)
            with patch('android_runtime.host_key',return_value='windows-x64'):
                with self.assertRaises(ValueError):checked_manifest(manifest)

    def test_runtime_links_extra_files_and_executable_modes_are_not_adopted(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,_=self.fixture(root);sdk=stage(payload,root/'home')
            program=sdk/'platform-tools/adb';program.chmod(0o644)
            with self.assertRaisesRegex(ValueError,'integrity'):verify_sdk(sdk)
            program.chmod(0o755);extra=sdk/'emulator/unlisted';extra.write_text('unlisted')
            with self.assertRaisesRegex(ValueError,'inventory'):verify_sdk(sdk)
            extra.unlink();extra.symlink_to(program)
            with self.assertRaisesRegex(ValueError,'symlink'):verify_sdk(sdk)

    def test_metadata_boundaries_and_types_match_the_declared_contract(self):
        from android_runtime import checked_file,checked_files,checked_path,MAX_FILE,MAX_TOTAL,RESERVE
        from android_avd import resources
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,manifest=self.fixture(root);rows=manifest['files'];abi=manifest['abi']
            for value in [0,MAX_FILE]:checked_file({**rows[0],'size':value},abi)
            for value in [[],{**rows[0],'extra':1},{**rows[0],'mode':float(0o755)},{**rows[0],'sha256':None}]:
                with self.assertRaises(ValueError):checked_file(value,abi)
            with self.assertRaises(ValueError):checked_files(tuple(rows),abi)
            for value in [35.0,True]:
                with self.assertRaises(ValueError):checked_manifest({**manifest,'api':value})
            exact=[*rows,{'path':'sdk/emulator/large-a','size':MAX_FILE,'sha256':'0'*64,'mode':0o644},{'path':'sdk/emulator/large-b','size':MAX_TOTAL-MAX_FILE-sum(row['size'] for row in rows),'sha256':'0'*64,'mode':0o644}]
            self.assertEqual(len(checked_files(exact,abi)),len(exact))
            self.assertEqual(checked_path('sdk/emulator/a space',abi).as_posix(),'sdk/emulator/a space')
            for value in ['sdk/emulator/._metadata','outside/file','sdk/emulator/snapshots/a']:
                with self.assertRaises(ValueError):checked_path(value,abi)
            free=RESERVE+sum(row['size'] for row in rows)
            with patch('android_runtime.shutil.disk_usage',return_value=type('Usage',(),{'free':free})()):stage(payload,root/'exact-space')
            for cpu in (1,2,4):self.assertEqual(resources(cpu,2048,16)['hw.cpu.ncore'],cpu)
            with self.assertRaises(ValueError):resources(0,2048,16)

    def test_member_shape_is_rejected_before_opening_a_body(self):
        from android_runtime import write_member,manifest_entry,image_inventory
        import zipfile
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);_,manifest=self.fixture(root);row=manifest['files'][0]
            header=tarfile.TarInfo(row['path']);header.size=row['size']+1
            with self.assertRaises(ValueError):write_member(None,header,root,row)
            header.size=row['size'];header.type=tarfile.SYMTYPE
            with self.assertRaises(ValueError):write_member(None,header,root,row)
            for entry in [None,tarfile.TarInfo('wrong')]:
                with self.assertRaises(ValueError):manifest_entry(None,iter([] if entry is None else [entry]))
            image=root/'unsafe.zip'
            for name,mode in [('wrong-prefix/system.img',0o100644),('arm64-v8a/link',0o120777)]:
                with zipfile.ZipFile(image,'w') as archive:
                    entry=zipfile.ZipInfo(name);entry.external_attr=mode<<16;archive.writestr(entry,b'fixture')
                with zipfile.ZipFile(image) as archive,self.assertRaises(ValueError):image_inventory(archive,'arm64-v8a')

    def test_avd_resources_and_image_identity_are_checked(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);payload,manifest=self.fixture(root);sdk=stage(payload,root/'home')
            for cpu,ram,storage in [(True,2048,16),(3,2048,16),(2,100000,16),(2,2048,1)]:
                with self.assertRaises(ValueError):create(sdk,root/'avd',manifest['abi'],cpu,ram,storage)
            with self.assertRaises(ValueError):create(sdk,root/'avd','wrong',2,2048,16)
            self.assertFalse((root/'avd').exists())


if __name__=='__main__':unittest.main()
