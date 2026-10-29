"""Backups use empty fixtures and explicitly labelled disposable PostgreSQL only."""
import io
import json
import os
from pathlib import Path
import subprocess
import socket
import tarfile
import tempfile
import unittest
import uuid
from unittest.mock import patch
from database import credentials, prepare, MARKER, literal
from state_backup import create_backup, read_backup, restore_backup, restore_state, write_state, fingerprints, psql, relocate_text

ROOT=Path(__file__).resolve().parent.parent
QUIET=lambda *args,**kwargs:None

class StateArchiveTests(unittest.TestCase):
    def test_path_relocation_is_one_pass_and_idempotent(self):
        old='/private/source';data=old+'/data';destination=Path(old+'/data/backups/recovered')
        source='path='+data+'/android/avd\nsource='+old+'/scripts'
        expected='path='+str(destination)+'/data/android/avd\nsource='+str(destination)+'/scripts'
        actual=relocate_text(source,old,data,destination)
        self.assertEqual(actual,expected)
        self.assertEqual(relocate_text(actual,old,data,destination),expected)

    def test_private_sparse_files_links_and_hardlinks_roundtrip(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'source';root.mkdir();(root/'.env').write_text('private-config')
            data=root/'data';data.mkdir();(data/'text').write_text('original content');(data/'work.log').write_text('retain user log');(data/'Cargo.lock').write_text('retain user lock');os.link(data/'text',data/'hardlink')
            (data/'link').symlink_to('text');(data/'backups').mkdir();(data/'backups/old').write_text('exclude')
            with (data/'sparse').open('wb') as f:f.seek(16*1024**2);f.write(b'end')
            sock=socket.socket(socket.AF_UNIX);sock.bind(str(data/'live.sock'))
            try:payload=Path(folder)/'state.gz';records,excluded=write_state(root,payload,{})
            finally:sock.close()
            self.assertIn('live.sock',excluded)
            out=Path(folder)/'restored';out.mkdir();restore_state(payload,out,{'records':records})
            self.assertEqual((out/'data/text').read_text(),'original content');self.assertEqual((out/'data/hardlink').read_text(),'original content')
            self.assertEqual((out/'data/work.log').read_text(),'retain user log');self.assertEqual((out/'data/Cargo.lock').read_text(),'retain user lock');self.assertEqual((out/'data/link').read_text(),'original content');self.assertEqual((out/'data/sparse').stat().st_size,16*1024**2+3)
            self.assertLess((out/'data/sparse').stat().st_blocks*512,1024**2);self.assertIn('backups',excluded)
    def test_traversal_unlisted_and_symlink_parent_rejected(self):
        for name in ('../escape','/escape','data/../escape','data/unlisted'):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as folder:
                payload=Path(folder)/'state.gz'
                with tarfile.open(payload,'w:gz') as tar:
                    info=tarfile.TarInfo(name);info.size=1;tar.addfile(info,io.BytesIO(b'x'))
                out=Path(folder)/'out';out.mkdir()
                with self.assertRaises(ValueError):restore_state(payload,out,{'records':{}})
                self.assertFalse((Path(folder)/'escape').exists())

@unittest.skipUnless(os.environ.get('AI_PHONE_TEST_PG_CONTAINER'),'explicit disposable PostgreSQL required')
class FullRestoreTests(unittest.TestCase):
    def setUp(self):
        self.container=os.environ['AI_PHONE_TEST_PG_CONTAINER'];self.port=os.environ['AI_PHONE_TEST_PG_PORT']
        label=subprocess.check_output(['docker','inspect','-f','{{index .Config.Labels "dev.aiphone.test"}}',self.container],text=True).strip()
        self.assertEqual(label,'permissions');self.assertTrue(self.container.startswith('aiphone-'))
        self.names=[];self.folder=tempfile.TemporaryDirectory(prefix='aiphone-state-test-');self.root=Path(self.folder.name)/'source';self.root.mkdir()
        self.source=self.new_database();(self.root/'.env').write_text(f'POSTGRES_DB={self.source}\nPOSTGRES_USER=ai_phone\nPOSTGRES_PASSWORD=isolated-fixture-password\n')
        prepare(self.root,self.container)
        result=subprocess.run([str(ROOT/'target/debug/ai-phone-assistant'),'--migrate'],env={**os.environ,'AI_PHONE_PROJECT_ROOT':str(self.root),'AI_PHONE_PG_PORT':self.port},capture_output=True)
        self.assertEqual(result.returncode,0)
        psql(self.container,'ai_phone',self.source,"CREATE TABLE restore_fixture(id int PRIMARY KEY,content text); INSERT INTO restore_fixture VALUES(1,'retain account and message fixture'); UPDATE proactive_state SET exploring=true,notifications_enabled=true;")
        (self.root/'data/phone-runner').mkdir();(self.root/'data/phone-runner/ledger.actions').mkdir();(self.root/'data/phone-runner/ledger.actions/known.json').write_text('{"unknown":true}')
        self.backup=Path(self.folder.name)/'backup.tar'
    def new_database(self):
        name='aiphone_restore_'+uuid.uuid4().hex;psql(self.container,'ai_phone','ai_phone','CREATE DATABASE "'+name+'";');self.names.append(name);return name
    def tearDown(self):
        for name in self.names:psql(self.container,'ai_phone','ai_phone','DROP DATABASE "'+name+'" WITH (FORCE);')
        self.folder.cleanup()
    def backup_state(self):
        return create_backup(self.root,self.backup,pg_container=self.container,desktop_container=None,quiesce=False,progress=QUIET)
    def test_real_dump_restore_equal_rows_and_runtime_role(self):
        self.backup=Path(self.folder.name)/'complete.backup'
        result=self.backup_state();target=self.new_database();out=Path(self.folder.name)/'restored'
        before=psql(self.container,'ai_phone','ai_phone',"SELECT rolname,md5(rolpassword) FROM pg_authid WHERE rolname IN ('ai_phone_runtime','ai_phone_migrator') ORDER BY rolname;")
        report=restore_backup(self.backup,out,pg_container=self.container,database=target)
        after=psql(self.container,'ai_phone','ai_phone',"SELECT rolname,md5(rolpassword) FROM pg_authid WHERE rolname IN ('ai_phone_runtime','ai_phone_migrator') ORDER BY rolname;")
        self.assertEqual(before,after,'matching existing role verifiers must remain byte-for-byte unchanged')
        self.assertTrue(report['database_verified']);self.assertTrue(report['quarantined']);self.assertGreater(report['tables'],30)
        source=fingerprints(self.container,'ai_phone',self.source);restored=fingerprints(self.container,'ai_phone',target)
        self.assertEqual(source['restore_fixture'],restored['restore_fixture']);self.assertEqual(source['messages'],restored['messages'])
        self.assertEqual((out/'data/phone-runner/ledger.actions/known.json').read_text(),'{"unknown":true}')
        self.assertEqual(psql(self.container,'ai_phone',target,'SELECT exploring OR notifications_enabled FROM proactive_state;'),'f')
        check=subprocess.run([str(ROOT/'target/debug/ai-phone-assistant'),'--database-check'],env={**os.environ,'AI_PHONE_PROJECT_ROOT':str(out),'AI_PHONE_PG_PORT':self.port},capture_output=True)
        self.assertEqual(check.returncode,0,check.stderr.decode())
        with self.assertRaises(ValueError):restore_backup(self.backup,out,pg_container=self.container,database=target)
    def test_failed_privilege_restore_rolls_back_schema_and_rows(self):
        self.backup_state();target=self.new_database();out=Path(self.folder.name)/'refused'
        psql(self.container,'ai_phone','ai_phone',"COMMENT ON ROLE ai_phone_runtime IS 'foreign role';")
        try:
            with self.assertRaises(ValueError):restore_backup(self.backup,out,pg_container=self.container,database=target)
            self.assertEqual(psql(self.container,'ai_phone',target,"SELECT count(*) FROM pg_tables WHERE schemaname='public';"),'0')
            self.assertFalse(out.exists())
        finally:
            psql(self.container,'ai_phone','ai_phone','COMMENT ON ROLE ai_phone_runtime IS '+literal(MARKER)+';')

    def test_other_installation_restore_cannot_rotate_shared_cluster_roles(self):
        original=credentials(self.root)
        before=psql(self.container,'ai_phone','ai_phone',"SELECT rolname,md5(rolpassword) FROM pg_authid WHERE rolname IN ('ai_phone_runtime','ai_phone_migrator') ORDER BY rolname;")
        foreign=json.loads(json.dumps(original))
        foreign['runtime']['password']='another-installation-runtime-credential-'+'a'*32
        foreign['migration']['password']='another-installation-migration-credential-'+'b'*32
        path=self.root/'data/database/credentials.json';path.write_text(json.dumps(foreign))
        self.backup_state();path.write_text(json.dumps(original))
        target=self.new_database();out=Path(self.folder.name)/'foreign-restored'
        with self.assertRaisesRegex(ValueError,'credentials|existing.*role'):
            restore_backup(self.backup,out,pg_container=self.container,database=target)
        after=psql(self.container,'ai_phone','ai_phone',"SELECT rolname,md5(rolpassword) FROM pg_authid WHERE rolname IN ('ai_phone_runtime','ai_phone_migrator') ORDER BY rolname;")
        self.assertEqual(before,after,'restoring a different installation must not alter cluster-global credentials')
        self.assertEqual(psql(self.container,'ai_phone',target,"SELECT count(*) FROM pg_tables WHERE schemaname='public';"),'0')
        self.assertFalse(out.exists())

    def test_restore_creates_roles_when_the_target_cluster_has_none(self):
        self.backup_state()
        # This explicitly labelled disposable cluster is exclusive to this test.
        psql(self.container,'ai_phone','ai_phone','DROP DATABASE "'+self.source+'" WITH (FORCE);')
        self.names.remove(self.source)
        psql(self.container,'ai_phone','ai_phone','DROP ROLE ai_phone_runtime; DROP ROLE ai_phone_migrator;')
        target=self.new_database();out=Path(self.folder.name)/'fresh-restored'
        report=restore_backup(self.backup,out,pg_container=self.container,database=target)
        self.assertTrue(report['database_verified'])
        check=subprocess.run([str(ROOT/'target/debug/ai-phone-assistant'),'--database-check'],env={**os.environ,'AI_PHONE_PROJECT_ROOT':str(out),'AI_PHONE_PG_PORT':self.port},capture_output=True)
        self.assertEqual(check.returncode,0,check.stderr.decode())

    def test_corrupt_payload_and_occupied_database_are_refused(self):
        self.backup_state();target=self.new_database();out=Path(self.folder.name)/'restored'
        psql(self.container,'ai_phone',target,'CREATE TABLE do_not_erase(id int); INSERT INTO do_not_erase VALUES(7);')
        with self.assertRaises(ValueError):restore_backup(self.backup,out,pg_container=self.container,database=target)
        self.assertEqual(psql(self.container,'ai_phone',target,'SELECT id FROM do_not_erase;'),'7');self.assertFalse(out.exists())
        with self.backup.open('r+b') as f:
            with tarfile.open(fileobj=f,mode='r:') as tar:offset=tar.getmember('state.tar.gz').offset_data
            f.seek(offset+32);value=f.read(1);f.seek(offset+32);f.write(bytes([value[0]^1]))
        with tempfile.TemporaryDirectory() as work:
            with self.assertRaises(ValueError):read_backup(self.backup,Path(work))

if __name__=='__main__':unittest.main()
