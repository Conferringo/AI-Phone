"""Browser ownership survives crashes without adopting another container/profile."""
import json
from pathlib import Path
import tempfile
import unittest
from browser_lease import BrowserContext,BrowserLease

IMAGE='sha256:'+'a'*64
REFERENCE='ai-phone-bundle/desktop:sha-'+'a'*64


class Engine:
    def __init__(self):self.containers={};self.creates=0;self.removed=[];self.unknown=False;self.offline=False
    def inspect(self,*,name=None,identity=None):
        if self.offline:raise OSError('daemon offline')
        return next((value for value in self.containers.values() if (identity is not None and value['Id']==identity) or (name is not None and value['Name']=='/'+name)),None)
    def create(self,context,record,args):
        self.creates+=1;identity=f'{self.creates:064x}'
        self.containers[identity]={'Id':identity,'Name':'/'+context.name,'Image':context.image_id,'Config':{'Labels':{'dev.aiphone.component':'channel-browser','dev.aiphone.profile':context.key,'dev.aiphone.launch':record['token']}}}
        if self.unknown:raise TimeoutError('create outcome unknown')
        return identity
    def remove(self,identity):self.removed.append(identity);self.containers.pop(identity,None)


def fixture(root):
    return BrowserContext(root,'fixture',REFERENCE)


class BrowserLeaseTests(unittest.TestCase):
    def test_ownership_fields_and_private_record_limits_are_independent(self):
        import os
        from browser_lease import private_json,checked_record,owned_container
        from private_files import atomic_json,protect
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);path=root/'record.json'
            value={'format':1,'padding':''};value['padding']='x'*(4096-len(json.dumps(value)))
            path.write_text(json.dumps(value));protect(path);self.assertEqual(path.stat().st_size,4096);self.assertEqual(private_json(path),value)
            path.write_text(json.dumps(value)+' ')
            with self.assertRaises(ValueError):private_json(path)
            atomic_json(path,{'format':1});os.chmod(path,0o644)
            if os.name!='nt':
                with self.assertRaises(ValueError):private_json(path)
            record={'format':1,'profile_hash':'key','token':'a'*64,'image_id':IMAGE,'container_id':'b'*64}
            for extra in ({'profile_hash':'other'},{'token':'bad'},{'image_id':'bad'},{'container_id':'bad'}):
                with self.assertRaises(ValueError):checked_record({**record,**extra},'key')
            context=fixture(root);engine=Engine()
            with BrowserLease(context,engine) as lease:
                identity=lease.create([]);saved=lease.record();container=engine.containers[identity]
                for extra in ({'Id':'bad'},{'Image':'wrong'}):
                    with self.assertRaises(ValueError):owned_container({**container,**extra},saved,context.key)
                lease.recover();marker=json.loads(context.marker.read_text());atomic_json(context.marker,{**marker,'owner_token':'bad'})
                with self.assertRaises(ValueError):lease.check_profile()

    def test_only_known_stale_chromium_links_are_removed_after_owned_stop(self):
        with tempfile.TemporaryDirectory() as temporary:
            context=fixture(Path(temporary));engine=Engine()
            with BrowserLease(context,engine) as lease:lease.create([])
            links={'SingletonLock':'64812d978735-8','SingletonSocket':'/tmp/org.chromium.Chromium.fixture/SingletonSocket','SingletonCookie':'123456'}
            for name,target in links.items():(context.profile/name).symlink_to(target)
            (context.profile/'account').write_text('keep')
            with BrowserLease(context,engine) as lease:
                with self.assertRaises(ValueError):lease.create([])
                self.assertTrue(all((context.profile/name).is_symlink() for name in links))
                lease.recover();lease.create([])
                self.assertFalse(any((context.profile/name).is_symlink() for name in links))
                self.assertEqual((context.profile/'account').read_text(),'keep');lease.recover()

    def test_unknown_ipc_is_never_deleted_and_validation_precedes_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            context=fixture(Path(temporary));engine=Engine()
            with BrowserLease(context,engine) as lease:lease.prepare_profile()
            lock=context.profile/'SingletonLock';lock.symlink_to('64812d978735-8')
            cookie=context.profile/'SingletonCookie';cookie.write_text('unknown')
            with BrowserLease(context,engine) as lease:
                with self.assertRaisesRegex(ValueError,'IPC'):lease.create([])
            self.assertTrue(lock.is_symlink());self.assertEqual(cookie.read_text(),'unknown');self.assertEqual(engine.creates,0)
            cookie.unlink();cookie.symlink_to('/outside')
            with BrowserLease(context,engine) as lease:
                with self.assertRaisesRegex(ValueError,'IPC'):lease.create([])
            self.assertTrue(lock.is_symlink());self.assertTrue(cookie.is_symlink())

    def test_a_late_old_owner_cannot_stop_or_erase_a_newer_connection(self):
        from browser_lease import BrowserOwnerChanged
        with tempfile.TemporaryDirectory() as temporary:
            context=fixture(Path(temporary));engine=Engine();old='1'*64;new='2'*64
            with BrowserLease(context,engine,owner=old) as lease:lease.create([]);lease.recover(expected_owner=old)
            (context.profile/'account').write_text('keep')
            with BrowserLease(context,engine,owner=new) as lease:identity=lease.create([])
            with BrowserLease(context,engine,owner=old) as late:
                with self.assertRaises(BrowserOwnerChanged):late.recover(expected_owner=old)
                with self.assertRaises(BrowserOwnerChanged):late.erase_profile(expected_owner=old)
            self.assertIn(identity,engine.containers)
            with BrowserLease(context,engine,owner=new) as lease:lease.recover(expected_owner=new)
            with BrowserLease(context,engine,owner=old) as late:
                with self.assertRaises(BrowserOwnerChanged):late.erase_profile(expected_owner=old)
            self.assertEqual((context.profile/'account').read_text(),'keep')

    def test_mutations_require_the_kernel_lock_and_restored_markers_are_path_independent(self):
        import shutil
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);context=fixture(root/'first');engine=Engine();lease=BrowserLease(context,engine)
            for action in (lease.prepare_profile,lease.recover,lambda:lease.create([]),lease.erase_profile):
                with self.assertRaisesRegex(RuntimeError,'lock'):action()
            with lease:lease.prepare_profile()
            restored=fixture(root/'restored');shutil.copytree(context.profile,restored.profile)
            with BrowserLease(restored,engine) as other:other.check_profile()
            self.assertNotEqual(context.key,restored.key)

    def test_profile_and_container_have_one_kernel_owner(self):
        with tempfile.TemporaryDirectory() as temporary:
            context=fixture(Path(temporary));engine=Engine()
            with BrowserLease(context,engine) as first:
                first.prepare_profile();identity=first.create([])
                with self.assertRaises(BlockingIOError):
                    with BrowserLease(context,engine):pass
                self.assertEqual(engine.creates,1)
                first.recover();self.assertEqual(engine.removed,[identity])

    def test_a_crashed_owned_container_is_recovered_before_a_new_launch(self):
        with tempfile.TemporaryDirectory() as temporary:
            context=fixture(Path(temporary));engine=Engine()
            with BrowserLease(context,engine) as first:first.prepare_profile();old=first.create([])
            (context.profile/'account-fixture').write_text('preserve')
            with BrowserLease(context,engine) as second:
                second.recover();new=second.create([])
                self.assertNotEqual(new,old);self.assertEqual(engine.removed,[old])
                self.assertEqual((context.profile/'account-fixture').read_text(),'preserve')
                second.recover()

    def test_unknown_create_result_is_observed_and_cleaned_without_replaying(self):
        with tempfile.TemporaryDirectory() as temporary:
            context=fixture(Path(temporary));engine=Engine();engine.unknown=True
            with BrowserLease(context,engine) as lease:
                lease.prepare_profile()
                with self.assertRaises(TimeoutError):lease.create([])
            with BrowserLease(context,engine) as resumed:resumed.recover()
            self.assertEqual(engine.creates,1);self.assertEqual(len(engine.removed),1)
            self.assertFalse(context.record.exists())

    def test_foreign_container_or_modified_ownership_is_never_removed(self):
        with tempfile.TemporaryDirectory() as temporary:
            context=fixture(Path(temporary));engine=Engine()
            engine.containers['b'*64]={'Id':'b'*64,'Name':'/'+context.name,'Image':IMAGE,'Config':{'Labels':{}}}
            with BrowserLease(context,engine) as lease:
                with self.assertRaises(ValueError):lease.recover()
            self.assertEqual(engine.removed,[])
            engine.containers.clear()
            with BrowserLease(context,engine) as lease:lease.prepare_profile();identity=lease.create([])
            engine.containers[identity]['Config']['Labels']['dev.aiphone.launch']='foreign'
            with BrowserLease(context,engine) as lease:
                with self.assertRaises(ValueError):lease.recover()
            self.assertEqual(engine.removed,[]);self.assertTrue(context.record.exists())

    def test_daemon_uncertainty_preserves_record_and_profile(self):
        with tempfile.TemporaryDirectory() as temporary:
            context=fixture(Path(temporary));engine=Engine()
            with BrowserLease(context,engine) as lease:lease.prepare_profile();lease.create([])
            (context.profile/'account-fixture').write_text('keep');engine.offline=True
            with BrowserLease(context,engine) as lease:
                with self.assertRaises(OSError):lease.erase_profile()
            self.assertTrue(context.record.exists());self.assertEqual((context.profile/'account-fixture').read_text(),'keep')

    def test_legacy_or_aliased_profiles_are_not_converted_or_deleted(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);context=fixture(root);engine=Engine();context.profile.mkdir(parents=True);(context.profile/'legacy-account').write_text('keep')
            with BrowserLease(context,engine) as lease:
                with self.assertRaises(ValueError):lease.prepare_profile()
                with self.assertRaises(ValueError):lease.erase_profile()
            self.assertEqual((context.profile/'legacy-account').read_text(),'keep');self.assertEqual(engine.creates,0)

    def test_explicit_erase_is_locked_and_only_removes_the_selected_owned_profile(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);context=fixture(root);engine=Engine();other=BrowserContext(root,'other',REFERENCE);other.profile.mkdir(parents=True);(other.profile/'account').write_text('other')
            with BrowserLease(context,engine) as lease:lease.prepare_profile();lease.create([])
            with BrowserLease(context,engine) as lease:lease.erase_profile()
            self.assertFalse(context.profile.exists());self.assertFalse(context.record.exists());self.assertEqual((other.profile/'account').read_text(),'other')

    def test_record_alias_or_invalid_schema_is_never_trusted(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);context=fixture(root);engine=Engine()
            with BrowserLease(context,engine) as lease:lease.prepare_profile();lease.create([])
            original=context.record.read_text();context.record.write_text('{}')
            with BrowserLease(context,engine) as lease:
                with self.assertRaises(ValueError):lease.recover()
            context.record.unlink();outside=root/'outside';outside.write_text(original);context.record.symlink_to(outside)
            with BrowserLease(context,engine) as lease:
                with self.assertRaises(ValueError):lease.recover()
            self.assertEqual(engine.removed,[])

    def test_context_requires_safe_session_and_immutable_image_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for session in ('../else','', 'x'*129):
                with self.assertRaises(ValueError):BrowserContext(root,session,REFERENCE)
            for image in ('latest', 'repo:latest', 'ai-phone-bundle/desktop:sha-bad'):
                with self.assertRaises(ValueError):BrowserContext(root,'fixture',image)
            context=fixture(root);self.assertEqual(context.image_id,IMAGE)


if __name__=='__main__':unittest.main()
