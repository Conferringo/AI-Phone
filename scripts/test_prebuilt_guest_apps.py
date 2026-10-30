"""Pinned vendor APKs are staged with both guest ABIs and no runtime SDK/download."""
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import zipfile
from unittest.mock import patch

SIGNER='a'*64


def fixture(root):
    config={};cache=root/'.tools/clash-apps/android';cache.mkdir(parents=True)
    for abi,key in [('arm64-v8a','android'),('x86_64','android-x86_64')]:
        path=cache/(abi+'.apk')
        with zipfile.ZipFile(path,'w') as apk:apk.writestr('classes.dex',abi)
        value={'file':path.name,'size_bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'signer_sha256':SIGNER,'version':'v1.0.0'}
        config[key]=value
    (root/'config').mkdir();(root/'config/clash-apps.json').write_text(json.dumps({'android':config['android'],'alternatives':{'android-x86_64':config['android-x86_64']}}))
    (root/'scripts').mkdir();(root/'scripts/prebuilt_guest_apps.py').write_text('producer fixture')
    (root/'scripts/apk_review.py').write_text('review fixture')
    (root/'scripts/client_host_entry.py').write_text('entry fixture')
    return cache


class GuestAppsTests(unittest.TestCase):
    def build_fixture(self,root):
        from prebuilt_guest_apps import build
        fixture(root)
        with patch('prebuilt_guest_apps.check_apk'):
            return build(root,Path('/fixture-sdk'),{})

    def test_build_stage_and_select_both_abis_preserves_existing_chat(self):
        from prebuilt_guest_apps import prepare_apps,application,environment
        from prepare_client_host import prepare,payload_components
        from client_host_entry import stage_source
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.build_fixture(root)
            self.assertEqual(len(prepare_apps(root)),3)
            prepare(root,with_guest_apps=True)
            self.assertEqual(payload_components(root),['guest-apps'])
            home=root/'installation';(home/'user/data').mkdir(parents=True);(home/'user/data/chat').write_text('keep')
            staged=stage_source(root/'apps/client/src-tauri/gen/host/source.payload',home)
            for abi in ('arm64-v8a','x86_64'):
                path=application(staged,abi)
                with zipfile.ZipFile(path) as apk:self.assertEqual(apk.read('classes.dex').decode(),abi)
                self.assertEqual(environment(staged,abi),{'AI_PHONE_CLASH_ANDROID_APK':str(path)})
            self.assertEqual((home/'user/data/chat').read_text(),'keep')
            self.assertIsNone(application(root,'arm64-v8a'))
            self.assertEqual(environment(root,'x86_64'),{})
            with self.assertRaisesRegex(ValueError,'ABI'):application(root,'unknown')

    def test_corruption_and_rewritten_manifest_hash_cannot_replace_pinned_apk(self):
        from prebuilt_guest_apps import application,verify
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.build_fixture(root);folder=root/'runtime/guest-apps'
            folder.parent.mkdir();shutil.copytree(root/'.tools/guest-apps',folder)
            target=folder/'clash-arm64-v8a.apk';original=target.read_bytes();target.write_bytes(b'x'*len(original))
            with self.assertRaises(ValueError):application(root,'arm64-v8a')
            path=folder/'manifest.json';value=json.loads(path.read_text());value['files'][0]['sha256']=hashlib.sha256(target.read_bytes()).hexdigest();path.write_text(json.dumps(value))
            with self.assertRaises(ValueError):verify(root,folder)

    def test_present_invalid_or_stale_component_never_falls_back_to_download(self):
        from prebuilt_guest_apps import application,environment,prepare_apps
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.build_fixture(root);folder=root/'runtime/guest-apps';folder.mkdir(parents=True)
            with self.assertRaises((ValueError,OSError)):environment(root,'arm64-v8a')
            shutil.rmtree(folder);shutil.copytree(root/'.tools/guest-apps',folder)
            (root/'scripts/prebuilt_guest_apps.py').write_text('changed producer')
            for action in [lambda:application(root,'arm64-v8a'),lambda:prepare_apps(root)]:
                with self.assertRaisesRegex(ValueError,'source'):action()

    def test_environment_discovery_does_not_rehash_apks_for_every_device_command(self):
        from prebuilt_guest_apps import environment
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.build_fixture(root);(root/'runtime').mkdir();shutil.copytree(root/'.tools/guest-apps',root/'runtime/guest-apps')
            with patch('prebuilt_guest_apps.verify_file',side_effect=AssertionError('binary rehash on environment discovery')):
                self.assertTrue(environment(root,'x86_64')['AI_PHONE_CLASH_ANDROID_APK'].endswith('clash-x86_64.apk'))

    def test_changed_source_during_sdk_inspection_does_not_publish_a_component(self):
        from prebuilt_guest_apps import build
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);fixture(root)
            def change(*args):(root/'scripts/prebuilt_guest_apps.py').write_text('concurrent change')
            with patch('prebuilt_guest_apps.check_apk',side_effect=change):
                with self.assertRaisesRegex(ValueError,'source'):build(root,Path('/sdk'),{})
            self.assertFalse((root/'.tools/guest-apps').exists())

    def test_pins_reject_missing_alternatives_and_non_object_configuration(self):
        from prebuilt_guest_apps import pins
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);fixture(root);path=root/'config/clash-apps.json'
            for value in ([],None,{'alternatives':[]},{'android':{},'alternatives':{}}):
                path.write_text(json.dumps(value))
                with self.assertRaises(ValueError):pins(root)

    def test_pin_types_names_versions_and_archive_review_budgets_are_exact(self):
        from prebuilt_guest_apps import pins,checked_pin,MAX_APK
        from apk_review import inspect_apk
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);cache=fixture(root);value=pins(root)['arm64-v8a']
            self.assertEqual(checked_pin({**value,'size_bytes':1})['size_bytes'],1)
            self.assertEqual(checked_pin({**value,'size_bytes':MAX_APK})['size_bytes'],MAX_APK)
            for field,bad in [('size_bytes',True),('size_bytes',0),('size_bytes',MAX_APK+1),('file','../escape.apk'),('file',None),('sha256','no'),('signer_sha256','b'),('version','latest')]:
                with self.assertRaises(ValueError):checked_pin({**value,field:bad})
            path=cache/'review.apk'
            with zipfile.ZipFile(path,'w') as apk:apk.writestr('data',b'123')
            inspect_apk(path,max_entries=1,max_unpacked=3)
            for limits in ({'max_entries':0},{'max_unpacked':2}):
                with self.assertRaisesRegex(ValueError,'budget'):inspect_apk(path,**limits)

    def test_manifest_size_and_lightweight_discovery_still_reject_aliases_and_directories(self):
        from prebuilt_guest_apps import environment,verify
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.build_fixture(root);(root/'runtime').mkdir();folder=root/'runtime/guest-apps';shutil.copytree(root/'.tools/guest-apps',folder)
            path=folder/'manifest.json';data=path.read_bytes();path.write_bytes(data+b' '*(65536-len(data)));verify(root,folder)
            path.write_bytes(path.read_bytes()+b' ')
            with self.assertRaises(ValueError):environment(root,'arm64-v8a')
            path.write_bytes(data);outside=root/'metadata';outside.write_bytes(data);path.unlink();path.symlink_to(outside)
            with self.assertRaises(ValueError):environment(root,'arm64-v8a')
            path.unlink();path.write_bytes(data);apk=folder/'clash-arm64-v8a.apk';body=apk.read_bytes();apk.unlink();outside=root/'aliased.apk';outside.write_bytes(body);apk.symlink_to(outside)
            with self.assertRaises(ValueError):environment(root,'arm64-v8a')
            apk.unlink();apk.mkdir()
            with self.assertRaises(ValueError):environment(root,'arm64-v8a')
            value=json.loads(data);value['files']=None;path.write_text(json.dumps(value))
            with self.assertRaises(ValueError):verify(root,folder)

    def test_cached_build_and_exact_free_space_boundary_do_not_reinspect_or_overwrite(self):
        from prebuilt_guest_apps import build,pins,RESERVE
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);fixture(root);required=sum(pin['size_bytes'] for pin in pins(root).values())
            space=lambda free:shutil._ntuple_diskusage(free,0,free)
            with patch('prebuilt_guest_apps.shutil.disk_usage',return_value=space(RESERVE+required-1)),self.assertRaisesRegex(ValueError,'4 GiB'):build(root,Path('/sdk'),{})
            with patch('prebuilt_guest_apps.shutil.disk_usage',return_value=space(RESERVE+required)),patch('prebuilt_guest_apps.check_apk'):
                first=build(root,Path('/sdk'),{})
            with patch('prebuilt_guest_apps.check_apk',side_effect=AssertionError('cached SDK execution')):self.assertEqual(build(root,Path('/sdk'),{}),first)
            shutil.rmtree(root/'.tools/guest-apps');(root/'.tools/guest-apps').symlink_to(root/'missing',target_is_directory=True)
            with self.assertRaises(ValueError):build(root,Path('/sdk'),{})

    def test_unsigned_changed_or_aliased_input_never_reaches_sdk_commands(self):
        from prebuilt_guest_apps import check_apk,pins,build
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);cache=fixture(root);pin=pins(root)['arm64-v8a'];path=cache/pin['file'];body=path.read_bytes()
            with patch('prebuilt_guest_apps.subprocess.check_output',side_effect=AssertionError('SDK called for unpinned input')):
                for altered in [b'changed',b'x'*len(body)]:
                    path.write_bytes(altered)
                    with self.assertRaises(ValueError):check_apk(path,'arm64-v8a',pin,Path('/sdk'),{})
                path.unlink();outside=root/'outside.apk';outside.write_bytes(body);path.symlink_to(outside)
                with self.assertRaises(ValueError):check_apk(path,'arm64-v8a',pin,Path('/sdk'),{})
                with self.assertRaises(ValueError):build(root,Path('/sdk'),{})

    def test_manifest_shapes_identities_and_inventory_are_not_advisory(self):
        from prebuilt_guest_apps import verify
        from copy import deepcopy
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.build_fixture(root);folder=root/'.tools/guest-apps';path=folder/'manifest.json';value=json.loads(path.read_text())
            variants=[{},[],{**value,'format':True},{**value,'files':[]},{**value,'apps':{}},{**value,'redistribution_review':'cleared'}]
            for field,bad in [('package','wrong'),('signer_sha256','b'*64),('version','v2')]:
                changed=deepcopy(value);changed['apps']['arm64-v8a'][field]=bad;variants.append(changed)
            for field,bad in [('path','../escape'),('size',True),('mode',0o755),('sha256','c'*64)]:
                changed=deepcopy(value);changed['files'][0][field]=bad;variants.append(changed)
            for changed in variants:
                path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):verify(root,folder)
            path.write_text(json.dumps(value));(folder/'extra').touch()
            with self.assertRaises(ValueError):verify(root,folder)

    def test_build_failure_cleanup_and_disk_guard_do_not_touch_user_data(self):
        from prebuilt_guest_apps import build
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);fixture(root);(root/'chat-fixture').write_text('keep')
            with patch('prebuilt_guest_apps.shutil.disk_usage',return_value=shutil._ntuple_diskusage(1,1,1)):
                with self.assertRaisesRegex(ValueError,'4 GiB'):build(root,Path('/sdk'),{})
            with patch('prebuilt_guest_apps.check_apk',side_effect=ValueError('signer mismatch')):
                with self.assertRaisesRegex(ValueError,'signer'):build(root,Path('/sdk'),{})
            self.assertFalse((root/'.tools/guest-apps').exists())
            self.assertEqual(list((root/'.tools').glob('.guest-apps-*')),[])
            self.assertEqual((root/'chat-fixture').read_text(),'keep')

    def test_aliases_and_missing_files_are_rejected(self):
        from prebuilt_guest_apps import verify
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.build_fixture(root);folder=root/'.tools/guest-apps';alias=root/'alias';alias.symlink_to(folder,target_is_directory=True)
            with self.assertRaises(ValueError):verify(root,alias)
            file=folder/'clash-arm64-v8a.apk';original=file.read_bytes();file.unlink();outside=root/'other';outside.write_bytes(original);file.symlink_to(outside)
            with self.assertRaises(ValueError):verify(root,folder)
            file.unlink()
            with self.assertRaises((ValueError,OSError)):verify(root,folder)

    def test_sdk_checks_signature_package_version_and_guest_abi(self):
        from prebuilt_guest_apps import check_apk,pins
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);cache=fixture(root);pin=pins(root)['arm64-v8a'];path=cache/pin['file']
            signer='Signer #1 certificate SHA-256 digest: '+SIGNER
            metadata="package: name='com.github.metacubex.clash.meta' versionCode='1' versionName='1.0.0.Meta'\nnative-code: 'arm64-v8a'\n"
            for outputs in [(signer,metadata), (signer+'\n'+signer,metadata),(signer.replace(SIGNER,'b'*64),metadata),(signer,metadata.replace('clash.meta','wrong')),(signer,metadata.replace('1.0.0.Meta','2.0.0.Meta')),(signer,metadata.replace('arm64-v8a','x86_64'))]:
                with patch('prebuilt_guest_apps.subprocess.check_output',side_effect=outputs):
                    if outputs==(signer,metadata):check_apk(path,'arm64-v8a',pin,Path('/sdk'),{})
                    else:
                        with self.assertRaises(ValueError):check_apk(path,'arm64-v8a',pin,Path('/sdk'),{})

    def test_runtime_profile_selects_matching_guest_abi_without_migrating_devices(self):
        from runtime_config import runtime_env
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.build_fixture(root);(root/'runtime').mkdir();shutil.copytree(root/'.tools/guest-apps',root/'runtime/guest-apps')
            (root/'data/deployment').mkdir(parents=True);profile=root/'data/deployment/profile.json'
            for template,abi in [('android35-x86_64','x86_64'),('android35-arm64','arm64-v8a')]:
                text=json.dumps({'phone':{'template':template}});profile.write_text(text)
                self.assertTrue(runtime_env(root)['AI_PHONE_CLASH_ANDROID_APK'].endswith('clash-'+abi+'.apk'))
                self.assertEqual(profile.read_text(),text)


if __name__=='__main__':unittest.main()
