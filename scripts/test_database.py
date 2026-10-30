"""Real PostgreSQL privilege tests run only in an explicitly labelled disposable container."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import uuid
from database import prepare, credentials, env_values, MARKER, literal, password_matches
from private_files import is_private

ROOT=Path(__file__).resolve().parent.parent

class DatabaseUnitTests(unittest.TestCase):
    def test_credentials_are_private_stable_and_do_not_replace_bootstrap(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);env='POSTGRES_DB=fixture\nPOSTGRES_USER=bootstrap\nPOSTGRES_PASSWORD=keep-original\n';(root/'.env').write_text(env)
            first=credentials(root);second=credentials(root)
            self.assertEqual(first,second);self.assertNotEqual(first['runtime']['password'],first['migration']['password'])
            self.assertEqual((root/'.env').read_text(),env)
            self.assertTrue(is_private(root/'data/database/credentials.json'))
            if os.name!='nt':self.assertEqual((root/'data/database/credentials.json').stat().st_mode&0o777,0o600)
            first['database']='other';(root/'data/database/credentials.json').write_text(json.dumps(first))
            with self.assertRaises(ValueError):credentials(root)

@unittest.skipUnless(os.environ.get('AI_PHONE_TEST_PG_CONTAINER'),'explicit disposable PostgreSQL required')
class PostgreSQLRoleTests(unittest.TestCase):
    def setUp(self):
        self.container=os.environ['AI_PHONE_TEST_PG_CONTAINER'];self.port=os.environ['AI_PHONE_TEST_PG_PORT']
        label=subprocess.check_output(['docker','inspect','-f','{{index .Config.Labels "dev.aiphone.test"}}',self.container],text=True).strip()
        self.assertEqual(label,'permissions');self.assertTrue(self.container.startswith('aiphone-'))
        self.name='aiphone_roles_'+uuid.uuid4().hex;self.admin('CREATE DATABASE "'+self.name+'";',database='ai_phone')
        self.folder=tempfile.TemporaryDirectory(prefix='aiphone-role-test-');self.root=Path(self.folder.name)
        (self.root/'.env').write_text(f'POSTGRES_DB={self.name}\nPOSTGRES_USER=ai_phone\nPOSTGRES_PASSWORD=isolated-fixture-password\n')
    def tearDown(self):
        self.admin('DROP DATABASE "'+self.name+'" WITH (FORCE);',database='ai_phone');self.folder.cleanup()
    def admin(self,sql,database=None):
        r=subprocess.run(['docker','exec','-i',self.container,'psql','-X','-v','ON_ERROR_STOP=1','-U','ai_phone','-d',database or self.name,'-At'],input=sql,text=True,capture_output=True)
        if r.returncode:raise AssertionError('Isolated admin query failed')
        return r.stdout.strip()
    def query(self,role,sql):
        config=credentials(self.root)[role]
        # The shell reads the password from stdin, never from command arguments.
        command='IFS= read -r PGPASSWORD; export PGPASSWORD; exec psql -X -h 127.0.0.1 -U "$1" -d "$2" -v ON_ERROR_STOP=1 -At'
        return subprocess.run(['docker','exec','-i',self.container,'sh','-c',command,'role-test',config['username'],self.name],input=config['password']+'\n'+"\\set VERBOSITY sqlstate\n"+sql,text=True,capture_output=True)
    def binary(self,flag):
        return subprocess.run([str(ROOT/'target/debug/ai-phone-assistant'),flag],env={**os.environ,'AI_PHONE_PROJECT_ROOT':str(self.root),'AI_PHONE_PG_PORT':self.port},text=True,capture_output=True)
    def test_fresh_install_runtime_crud_and_denied_ddl_role_elevation(self):
        prepare(self.root,self.container);migrated=self.binary('--migrate');self.assertEqual(migrated.returncode,0,migrated.stderr)
        self.assertEqual(self.binary('--database-check').returncode,0)
        self.assertEqual(int(self.admin('SELECT count(*) FROM _sqlx_migrations;')),len(list((ROOT/'crates/assistant/migrations').glob('*.sql'))))
        created=self.query('migration','CREATE TABLE public.permission_fixture(id integer PRIMARY KEY,content text);');self.assertEqual(created.returncode,0)
        crud=self.query('runtime',"INSERT INTO permission_fixture VALUES(1,'before'); UPDATE permission_fixture SET content='after' WHERE id=1; SELECT content FROM permission_fixture WHERE id=1; DELETE FROM permission_fixture WHERE id=1;")
        self.assertEqual(crud.returncode,0);self.assertIn('after',crud.stdout)
        for sql in ['CREATE TABLE public.forbidden(id int);','ALTER TABLE permission_fixture ADD COLUMN forbidden int;','CREATE ROLE forbidden_role;', 'SET ROLE ai_phone_migrator;','SET ROLE ai_phone;','SELECT rolpassword FROM pg_authid;','DELETE FROM _sqlx_migrations;']:
            with self.subTest(sql=sql):
                denied=self.query('runtime',sql);self.assertNotEqual(denied.returncode,0);self.assertIn('42501',denied.stderr)
        self.assertEqual(self.admin("SELECT rolsuper FROM pg_roles WHERE rolname='ai_phone';"),'t')
    def test_legacy_ownership_migration_is_idempotent_and_transactional(self):
        prepare(self.root,self.container);self.assertEqual(self.binary('--migrate').returncode,0)
        self.admin("CREATE TABLE public.permission_fixture(id int PRIMARY KEY,content text); INSERT INTO permission_fixture VALUES(1,'preserve me'); ALTER TABLE messages OWNER TO ai_phone;")
        before=self.admin("SELECT md5(row_to_json(x)::text) FROM permission_fixture x;");passwords=credentials(self.root)
        prepare(self.root,self.container);prepare(self.root,self.container)
        self.assertEqual(credentials(self.root),passwords);self.assertEqual(self.admin("SELECT md5(row_to_json(x)::text) FROM permission_fixture x;"),before)
        self.assertEqual(self.admin("SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='messages'::regclass;"),'ai_phone_migrator')
        self.admin("COMMENT ON ROLE ai_phone_runtime IS 'foreign role';")
        with self.assertRaises(ValueError):prepare(self.root,self.container)
        self.assertEqual(self.query('migration','SELECT 1;').returncode,0)
        self.admin('COMMENT ON ROLE ai_phone_runtime IS '+literal(MARKER)+';')
        self.assertEqual(self.binary('--database-check').returncode,0)

    def test_restore_verifier_matches_postgresql_generated_formats(self):
        prepare(self.root,self.container)
        login=credentials(self.root)['runtime']
        for algorithm in ('scram-sha-256','md5'):
            self.admin("SET password_encryption="+literal(algorithm)+"; ALTER ROLE ai_phone_runtime PASSWORD "+literal(login['password'])+";")
            verifier=self.admin("SELECT rolpassword FROM pg_authid WHERE rolname='ai_phone_runtime';")
            self.assertTrue(password_matches(login['username'],login['password'],verifier),algorithm)
            self.assertFalse(password_matches(login['username'],'wrong-credential',verifier),algorithm)
        for verifier in (None,'unknown','SCRAM-SHA-256$999999999999:salt$bad:bad','SCRAM-SHA-256$4096:%%%$bad:bad'):
            self.assertFalse(password_matches(login['username'],login['password'],verifier))

if __name__=='__main__':unittest.main()
