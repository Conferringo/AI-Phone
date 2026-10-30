"""Phone control applications are built once, verified and staged without Java."""
import hashlib
import json
from pathlib import Path
import tempfile
import subprocess
import unittest
import zipfile
from unittest.mock import patch
from client_host_entry import stage_source
from prepare_client_host import prepare

class PhoneAppTests(unittest.TestCase):
    def fixture(self,root):
        from prebuilt_phone import APPS,source_fingerprint
        folder=root/'.tools/phone-apps';folder.mkdir(parents=True)
        records=[]
        for role in APPS:
            path=folder/(role+'.apk')
            with zipfile.ZipFile(path,'w') as apk:apk.writestr('classes.dex',('apk fixture '+role).encode())
            records.append({'path':'runtime/phone-apps/'+role+'.apk','size':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'mode':0o644})
        manifest={'format':1,'source_sha256':source_fingerprint(root),'apps':{role:{'package':value,'signer_sha256':'a'*64} for role,value in APPS.items()},'files':records}
        (folder/'manifest.json').write_text(json.dumps(manifest));return folder

    def test_prepare_and_stage_choose_inventoried_apks_without_a_compiler(self):
        from prebuilt_phone import prepare_apps,application,available
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.fixture(root);(root/'scripts').mkdir();(root/'scripts/client_host_entry.py').write_text('fixture')
            self.assertEqual(len(prepare_apps(root)),3)
            prepare(root,with_phone_apps=True)
            home=root/'install';(home/'user/data').mkdir(parents=True);(home/'user/data/chat').write_text('keep')
            staged=stage_source(root/'apps/client/src-tauri/gen/host/source.payload',home)
            self.assertTrue(available(staged));self.assertFalse(available(root))
            for role in ('input','home'):
                with zipfile.ZipFile(application(staged,role)) as apk:self.assertEqual(apk.read('classes.dex'),('apk fixture '+role).encode())
            self.assertEqual((home/'user/data/chat').read_text(),'keep')
            (staged/'runtime/phone-apps/input.apk').write_bytes(b'changed')
            with self.assertRaises(ValueError):application(staged,'input')

    def test_changed_sources_never_package_stale_apks(self):
        from prebuilt_phone import prepare_apps,application
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.fixture(root)
            source=root/'apps/android-input/src/Changed.java';source.parent.mkdir(parents=True);source.write_text('changed')
            with self.assertRaisesRegex(ValueError,'source'):prepare_apps(root)
            self.assertIsNone(application(root,'input'))
            with self.assertRaises(ValueError):application(root,'unknown')

    def test_present_invalid_components_never_fall_back_to_source_build(self):
        from prebuilt_phone import application
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);folder=root/'runtime/phone-apps';folder.mkdir(parents=True)
            with self.assertRaises((ValueError,OSError)):application(root,'home')
            (folder/'manifest.json').write_text('{}')
            with self.assertRaises(ValueError):application(root,'home')

    def test_java_is_needed_until_sdk_and_phone_app_components_are_both_present(self):
        from client_host import selected_tools
        tools=['java@17','python@3.13.18']
        self.assertEqual(selected_tools(tools,False,True),tools)
        self.assertEqual(selected_tools(tools,False,False,(),True),tools)
        self.assertEqual(selected_tools(tools,False,True,(),True),['python@3.13.18'])

    def test_manifest_identity_inventory_and_file_boundaries(self):
        from prebuilt_phone import manifest,verify
        from copy import deepcopy
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);folder=self.fixture(root);path=folder/'manifest.json';value=json.loads(path.read_text())
            for changed in (None,[],{}, {**value,'format':True},{**value,'source_sha256':False},{**value,'apps':{}},{**value,'files':None},{**value,'files':[]}):
                path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):manifest(folder)
            for field,bad in [('package','wrong'),('signer_sha256',False)]:
                changed=deepcopy(value);changed['apps']['input'][field]=bad;path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):manifest(folder)
            for field,bad in [('path','../outside'),('mode',0o755),('size',0),('size',16*1024**2+1)]:
                changed=deepcopy(value);changed['files'][0][field]=bad;path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):manifest(folder)
            for size in (1,16*1024**2):
                changed=deepcopy(value);changed['files'][0]['size']=size;path.write_text(json.dumps(changed));manifest(folder)
            data=json.dumps(value);path.write_text(data+' '*(65536-len(data)));manifest(folder)
            with path.open('a') as out:out.write(' ')
            with self.assertRaises(ValueError):manifest(folder)
            path.write_text(data);(folder/'unlisted').touch()
            with self.assertRaises(ValueError):verify(root,folder)
            (folder/'unlisted').unlink();old=(folder/'input.apk').read_bytes();(folder/'input.apk').write_bytes(b'changed')
            with self.assertRaises(ValueError):verify(root,folder)
            (folder/'input.apk').write_bytes(old);alias=root/'alias';alias.symlink_to(folder,target_is_directory=True)
            with self.assertRaises(ValueError):verify(root,alias)

    def test_private_apk_members_are_rejected_even_when_compressed(self):
        from prebuilt_phone import inspect_apk
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'apk'
            for private in (b' sk-'+b'A'*32,(' sk-'+'A'*32).encode('utf-16-le'),(str(Path.home())+'/private').encode()):
                with zipfile.ZipFile(path,'w',compression=zipfile.ZIP_DEFLATED) as apk:apk.writestr('private',private)
                with self.assertRaisesRegex(ValueError,'Private material'):inspect_apk(path)

    def test_source_fallback_and_packaged_selection_do_not_cross(self):
        from prebuilt_phone import installer_apk
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            with patch('prebuilt_phone.subprocess.run') as run:
                self.assertEqual(installer_apk(root,'home'),root/'data/android/home/ai-phone-home.apk')
                self.assertEqual(run.call_count,1)
            with patch('prebuilt_phone.application',return_value=root/'verified.apk'),patch('prebuilt_phone.subprocess.run') as run:
                self.assertEqual(installer_apk(root,'home'),root/'verified.apk');run.assert_not_called()

    def test_build_is_cached_and_failed_publish_preserves_prior_user_files(self):
        from prebuilt_phone import build,source_fingerprint,prepare_apps
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'chat-fixture').write_text('keep');source=root/'apps/android-input';source.mkdir(parents=True);(root/'apps/android-home').mkdir()
            def compile(work,role,keys,env):
                path=work/(role+'.apk')
                with zipfile.ZipFile(path,'w') as apk:apk.writestr('classes.dex',role)
                return path
            with patch('prebuilt_phone.compile_app',side_effect=OSError('compiler failure')):
                with self.assertRaises(OSError):build(root,{})
            self.assertFalse((root/'.tools/phone-apps').exists());self.assertEqual(list((root/'.tools').glob('.phone-app-build-*')),[])
            with patch('prebuilt_phone.compile_app',side_effect=compile),patch('prebuilt_phone.check_apk',side_effect=lambda path,role,*_: {'package':'dev.aiphone.'+role,'signer_sha256':'a'*64}):
                result=build(root,{'ANDROID_HOME':'/isolated-sdk'});self.assertEqual(result['source_sha256'],source_fingerprint(root));self.assertTrue(build(root,{})['reused'])
            self.assertEqual((root/'chat-fixture').read_text(),'keep');self.assertEqual(len(prepare_apps(root)),3)

    def test_package_and_signer_results_are_checked_before_publication(self):
        from prebuilt_phone import check_apk
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);folder=self.fixture(root);path=folder/'input.apk'
            good='Signer #1 certificate SHA-256 digest: '+'a'*64
            package="package: name='dev.aiphone.input' versionCode='1'"
            def sdk_output(argv,**kwargs):
                output=good if 'apksigner' in str(argv[0]) else package
                return output if kwargs.get('text') else output.encode()
            with patch('prebuilt_phone.subprocess.check_output',side_effect=sdk_output):
                self.assertEqual(check_apk(path,'input',root,{}),{'package':'dev.aiphone.input','signer_sha256':'a'*64})
            for output in ('',good+'\n'+good):
                with patch('prebuilt_phone.subprocess.check_output',return_value=output),self.assertRaisesRegex(ValueError,'signer'):check_apk(path,'input',root,{})
            with patch('prebuilt_phone.subprocess.check_output',side_effect=[good,"package: name='other'"]),self.assertRaisesRegex(ValueError,'package'):check_apk(path,'input',root,{})

    def test_factory_signing_identity_is_retained_across_builds(self):
        from prebuilt_phone import compile_app
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);keys=root/'keys';keys.mkdir()
            def compiler(argv,**kwargs):
                key=root/'one/data/android/input/debug.jks';key.write_bytes(b'identity')
            with patch('prebuilt_phone.subprocess.run',side_effect=compiler):compile_app(root/'one','input',keys,{})
            self.assertEqual((keys/'input.jks').read_bytes(),b'identity');self.assertEqual((keys/'input.jks').stat().st_mode&0o777,0o600)
            with patch('prebuilt_phone.subprocess.run') as compiler:compile_app(root/'two','input',keys,{})
            self.assertEqual((root/'two/data/android/input/debug.jks').read_bytes(),b'identity');compiler.assert_called_once()

    def test_build_space_guard_preserves_an_unpublished_destination(self):
        from prebuilt_phone import build
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            with patch('prebuilt_phone.shutil.disk_usage',return_value=type('Usage',(),{'free':4*1024**3+32*1024**2-1})()):
                with self.assertRaisesRegex(ValueError,'4 GiB'):build(root,{})
            self.assertFalse((root/'.tools/phone-apps').exists());self.assertEqual(list((root/'.tools').glob('.phone-app-build-*')),[])
            for role in ('input','home'):(root/'apps'/('android-'+role)).mkdir(parents=True)
            with patch('prebuilt_phone.shutil.disk_usage',return_value=type('Usage',(),{'free':4*1024**3+32*1024**2})()),patch('prebuilt_phone.compile_app',side_effect=OSError('admitted compiler failure')):
                with self.assertRaisesRegex(OSError,'admitted compiler failure'):build(root,{})

    def test_failed_compiler_does_not_return_an_installable_apk(self):
        from prebuilt_phone import compile_app,installer_apk
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);keys=root/'keys';keys.mkdir()
            for role in ('input','home'):
                source=root/'apps'/('android-'+role);source.mkdir(parents=True);(source/'build.py').write_text('import sys;sys.exit(9)')
            with self.assertRaises(subprocess.CalledProcessError):compile_app(root,'input',keys,{})
            with self.assertRaises(subprocess.CalledProcessError):installer_apk(root,'home')

    def test_apk_header_budgets_are_inclusive_and_precede_body_reads(self):
        from prebuilt_phone import inspect_apk
        class Archive:
            def __init__(self,rows):self.rows=rows;self.reads=0
            def __enter__(self):return self
            def __exit__(self,*_):pass
            def infolist(self):return [type('Entry',(),{'file_size':size})() for size in self.rows]
            def read(self,entry):self.reads+=1;return b''
        for sizes,accepted in (([0]*256,True),([0]*257,False),([32*1024**2],True),([32*1024**2+1],False)):
            archive=Archive(sizes)
            with patch('zipfile.ZipFile',return_value=archive):
                if accepted:inspect_apk(Path('fixture'));self.assertEqual(archive.reads,len(sizes))
                else:
                    with self.assertRaisesRegex(ValueError,'budget'):inspect_apk(Path('fixture'))
                    self.assertEqual(archive.reads,0)

if __name__=='__main__':unittest.main()
