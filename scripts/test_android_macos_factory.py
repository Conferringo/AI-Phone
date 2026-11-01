"""Native Mac SDK identity must be proven before a sealed capsule is produced."""
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile
from unittest.mock import patch
import android_component_factory as factory
from android_runtime import stage,verify_sdk


def macho(cpu=0x0100000c):
    return b'\xcf\xfa\xed\xfe'+struct.pack('<7I',cpu,0,2,0,0,0,0)


def fixture(root, *, cpu=0x0100000c, image_abi='arm64-v8a'):
    pins={};(root/'config').mkdir()
    for role,program in [('emulator','emulator'),('platform-tools','adb')]:
        archive=root/(role+'.zip')
        with zipfile.ZipFile(archive,'w') as target:
            entry=zipfile.ZipInfo(role+'/'+program);entry.external_attr=0o100755<<16
            target.writestr(entry,macho(cpu));target.writestr(role+'/source.properties','Pkg.Revision = 37.0.1\n')
        pins[role]={'url':archive.as_uri(),'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'size_bytes':archive.stat().st_size,'version':'37.0.1','channel':'stable','host_os':'macosx','host_arch':'aarch64' if role=='emulator' else None}
    archive=root/'image.zip'
    with zipfile.ZipFile(archive,'w') as target:
        for name in ('system.img','vendor.img','kernel-ranchu','ramdisk.img','NOTICE.txt'):target.writestr('arm64-v8a/'+name,b'factory')
        target.writestr('arm64-v8a/source.properties',f'Pkg.Revision=2\nAndroidVersion.ApiLevel=35\nSystemImage.Abi={image_abi}\nSystemImage.TagId=default\n')
    image={'url':archive.as_uri(),'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'size_bytes':archive.stat().st_size,'version':'2','api':35,'abi':'arm64-v8a','channel':'stable'}
    value={'format':1,'api':35,'license_id':'android-sdk-license','hosts':{'macos-arm64':{'abi':'arm64-v8a','archives':pins}},'images':{'arm64-v8a':image}}
    (root/'config/android-factory.json').write_text(json.dumps(value));return value


class MacOSStockFactoryTests(unittest.TestCase):
    def setUp(self):
        for module in ('android_runtime','android_component_factory'):
            patcher=patch(module+'.host_key',return_value='macos-arm64');patcher.start();self.addCleanup(patcher.stop)

    def test_foreign_cpu_is_rejected_before_capsule_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);fixture(root,cpu=0x01000007)
            with self.assertRaisesRegex(ValueError,'ARM64'):factory.build(root,root/'work')
            self.assertFalse((root/'work/android.payload').exists())

    def test_image_metadata_cannot_lie_about_abi(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);fixture(root,image_abi='x86_64')
            with self.assertRaisesRegex(ValueError,'image metadata'):factory.build(root,root/'work')
            self.assertFalse((root/'work/android.payload').exists())

    def test_foreign_host_archive_policy_is_rejected_before_work_created(self):
        for field,value in [('host_os','linux'),('host_arch','x64')]:
            with self.subTest(field=field),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary);config=fixture(root);config['hosts']['macos-arm64']['archives']['emulator'][field]=value
                (root/'config/android-factory.json').write_text(json.dumps(config))
                with self.assertRaisesRegex(ValueError,'Mac'):factory.build(root,root/'work')
                self.assertFalse((root/'work').exists())

    def test_mac_stock_receipt_is_deterministic_and_stage_preserves_user_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);fixture(root);state=root/'user/chat';state.parent.mkdir();state.write_text('keep')
            output,receipt=factory.build(root,root/'work');stock=receipt['stock']
            self.assertEqual(stock['policy_sha256'],factory.stock_identity(factory.policy(root))['policy_sha256'])
            self.assertEqual(len(stock['producer_sha256']),64);self.assertEqual(stock['redistribution'],'review-pending')
            sdk=stage(output,root/'install');self.assertEqual(verify_sdk(sdk)['abi'],'arm64-v8a');self.assertEqual(state.read_text(),'keep')
            self.assertEqual(list((root/'work').glob('.sdk-*')),[])
            changed={**factory.policy(root),'emulator':{**factory.policy(root)['emulator'],'sha256':'0'*64}}
            self.assertNotEqual(stock['policy_sha256'],factory.stock_identity(changed)['policy_sha256'])




class MacOSStockBoundaryTests(unittest.TestCase):
    def test_fat_binary_requires_a_real_in_bounds_arm64_slice(self):
        from android_factory_stock import native_arm64
        arm=macho();intel=macho(0x01000007)
        with tempfile.TemporaryDirectory() as temporary:
            program=Path(temporary)/'program'
            for magic,endian,width in [(b'\xca\xfe\xba\xbe','>',20),(b'\xbe\xba\xfe\xca','<',20),(b'\xca\xfe\xba\xbf','>',32),(b'\xbf\xba\xfe\xca','<',32)]:
                offset=8+2*width;fmt=endian+('5I' if width==20 else 'IIQQII')
                intel_row=(0x01000007,0,offset,len(intel),0);arm_row=(0x0100000c,0,offset+len(intel),len(arm),0)
                if width==32:intel_row+= (0,);arm_row+=(0,)
                program.write_bytes(magic+struct.pack(endian+'I',2)+struct.pack(fmt,*intel_row)+struct.pack(fmt,*arm_row)+intel+arm)
                native_arm64(program)
            for value in [b'',b'\x7fELF'+b'x'*64,intel,b'\xca\xfe\xba\xbe'+struct.pack('>I',65),b'\xca\xfe\xba\xbe'+struct.pack('>I',1),b'\xca\xfe\xba\xbe'+struct.pack('>I',1)+struct.pack('>5I',0x0100000c,0,4096,32,0)]:
                program.write_bytes(value)
                with self.assertRaisesRegex(ValueError,'ARM64'):native_arm64(program)

    def test_reserve_boundary_and_no_work_on_initial_space_failure(self):
        from android_factory_stock import RESERVE,reserve
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);config=fixture(root);needed=sum(p['size_bytes'] for p in factory.policy(root).values())
            with patch('android_factory_stock.shutil.disk_usage',return_value=type('Usage',(),{'free':RESERVE+needed-1})()),patch('android_component_factory.host_key',return_value='macos-arm64'),patch('android_component_factory.download') as download:
                with self.assertRaisesRegex(ValueError,'reserve'):factory.build(root,root/'work')
                download.assert_not_called();self.assertFalse((root/'work').exists())
            with patch('android_factory_stock.shutil.disk_usage',return_value=type('Usage',(),{'free':RESERVE+needed})()):reserve(root/'missing/child',needed)

    def test_missing_duplicate_over_budget_or_foreign_image_properties_fail(self):
        from android_factory_stock import image_properties
        base=b'Pkg.Revision=2\nAndroidVersion.ApiLevel=35\nSystemImage.Abi=arm64-v8a\nSystemImage.TagId=default\n'
        image_properties(base,{'version':'2'})
        for value in [base+b'Pkg.Revision=2\n',base.replace(b'=35',b'=34'),base.replace(b'=default',b'=google_apis'),base.replace(b'Pkg.Revision=2\n',b''),b'x'*32769]:
            with self.subTest(value=value[:50]),self.assertRaisesRegex(ValueError,'image metadata'):image_properties(value,{'version':'2'})

    def test_qualification_requires_matching_build_and_payload_before_stage(self):
        with tempfile.TemporaryDirectory() as temporary,patch('android_component_factory.host_key',return_value='macos-arm64'),patch('android_runtime.host_key',return_value='macos-arm64'):
            root=Path(temporary);fixture(root);output,receipt=factory.build(root,root/'work')
            for built in [None,{**receipt,'stock':{}},{**receipt,'archive_sha256':'0'*64}]:
                with patch('android_runtime.stage') as stage_fixture,self.assertRaisesRegex(ValueError,'receipt'):factory.qualify(root,output,root/'home',built)
                stage_fixture.assert_not_called()

    def test_qualification_uses_only_generated_sdk_and_isolated_environment(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary,patch('android_component_factory.host_key',return_value='macos-arm64'),patch('android_runtime.host_key',return_value='macos-arm64'):
            root=Path(temporary);fixture(root);output,receipt=factory.build(root,root/'work')
            with patch.dict('os.environ',{'MODEL_KEY':'fixture-private','DYLD_INSERT_LIBRARIES':'fixture-private'}),patch('android_component_factory.subprocess.run',return_value=SimpleNamespace(stdout='version 37.0.1')) as run:
                result=factory.qualify(root,output,root/'home',receipt)
                self.assertEqual(result['guest_boot'],'not-executed');self.assertEqual(len(run.call_args_list),2)
                for call in run.call_args_list:
                    self.assertNotIn('MODEL_KEY',call.kwargs['env']);self.assertNotIn('DYLD_INSERT_LIBRARIES',call.kwargs['env'])
                    self.assertIn(str(root/'home'),str(call.args[0][0]))

    def test_policy_change_during_collection_never_publishes_a_capsule(self):
        real_build=factory.build_runtime
        with tempfile.TemporaryDirectory() as temporary,patch('android_component_factory.host_key',return_value='macos-arm64'),patch('android_runtime.host_key',return_value='macos-arm64'):
            root=Path(temporary);config=fixture(root)
            def changed(*args):
                result=real_build(*args);config['hosts']['macos-arm64']['archives']['emulator']['version']='37.0.2'
                (root/'config/android-factory.json').write_text(json.dumps(config));return result
            with patch('android_component_factory.build_runtime',side_effect=changed),self.assertRaisesRegex(ValueError,'changed during collection'):factory.build(root,root/'work')
            self.assertFalse((root/'work/android.payload').exists());self.assertEqual(list((root/'work').glob('.sdk-*')),[])

    def test_official_mac_policy_keeps_existing_platform_pins(self):
        value=json.loads((Path(__file__).resolve().parent.parent/'config/android-factory.json').read_text())
        self.assertEqual(value['hosts']['linux-x64']['archives']['emulator']['sha256'],'cbc7c3cfb83220b8a0f9c513fdab557712e74d339ec19ddffa43f23350ae9bdd')
        self.assertEqual(value['hosts']['windows-x64']['archives']['emulator']['sha256'],'625276a1d3ac6b15d00f8aad5e233e7dc3a7cb71435f97d64e3be298399e171d')
        with patch('android_component_factory.host_key',return_value='macos-arm64'):
            selected=factory.policy(Path(__file__).resolve().parent.parent)
        self.assertEqual(selected['emulator']['version'],'37.2.13');self.assertEqual(selected['system-image']['abi'],'arm64-v8a')
        self.assertFalse(value['redistribution_review_complete'])


class ExistingNativeFactoryContractTests(unittest.TestCase):
    def test_both_existing_hosts_keep_the_same_unextended_receipt_and_capsule(self):
        for host in ['linux-x64','windows-x64']:
            with self.subTest(host=host),tempfile.TemporaryDirectory() as temporary,patch('android_component_factory.host_key',return_value=host),patch('android_runtime.host_key',return_value=host):
                root=Path(temporary);config=fixture(root);pins=config['hosts']['macos-arm64']['archives']
                for role,program in [('emulator','emulator'),('platform-tools','adb')]:
                    archive=root/(role+'.zip');program+= '.exe' if host=='windows-x64' else ''
                    with zipfile.ZipFile(archive,'w') as target:
                        entry=zipfile.ZipInfo(role+'/'+program);entry.external_attr=0o100755<<16;target.writestr(entry,b'\x7fELFfixture')
                        target.writestr(role+'/source.properties','Pkg.Revision = 37.0.1\n')
                    pins[role].update({'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'size_bytes':archive.stat().st_size})
                archive=root/'image.zip'
                with zipfile.ZipFile(archive,'w') as target:
                    for name in ('system.img','vendor.img','kernel-ranchu','ramdisk.img','NOTICE.txt'):target.writestr('x86_64/'+name,b'factory')
                image={**config['images']['arm64-v8a'],'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'size_bytes':archive.stat().st_size}
                config['hosts']={host:{'abi':'x86_64','archives':pins}};config['images']={'x86_64':image}
                (root/'config/android-factory.json').write_text(json.dumps(config));output,result=factory.build(root,root/'work')
                self.assertEqual(set(result),{'host','abi','files','bytes','archive_sha256','redistribution'})
                self.assertEqual(result['host'],host);self.assertEqual(verify_sdk(stage(output,root/'install'))['abi'],'x86_64')


if __name__=='__main__':unittest.main()
