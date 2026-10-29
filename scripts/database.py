"""Project-local PostgreSQL role preparation. Passwords never appear in argv/output."""
import argparse
import base64
import hashlib
import hmac
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile

RUNTIME = 'ai_phone_runtime'
MIGRATION = 'ai_phone_migrator'
MARKER = 'AI Phone managed database role v1'

def env_values(root):
    values = {}
    for line in (Path(root) / '.env').read_text().splitlines():
        if '=' in line and not line.lstrip().startswith('#'):
            key, value = line.split('=', 1)
            values[key.strip()] = value.strip()
    if not all(values.get(key) for key in ['POSTGRES_DB', 'POSTGRES_USER', 'POSTGRES_PASSWORD']):
        raise ValueError('Private PostgreSQL bootstrap configuration is incomplete')
    return values

def atomic_json(path, value):
    path = Path(path); path.parent.mkdir(parents=True, exist_ok=True, mode=0o700); path.parent.chmod(0o700)
    fd, temp = tempfile.mkstemp(dir=path.parent, prefix='.credentials-')
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, 'w') as file:
            json.dump(value, file, indent=2); file.write('\n'); file.flush(); os.fsync(file.fileno())
        os.replace(temp, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try: os.fsync(directory)
        finally: os.close(directory)
    finally:
        if os.path.exists(temp): os.unlink(temp)

def identifier(value):
    if not value or '\x00' in value: raise ValueError('Invalid PostgreSQL identifier')
    return '"' + value.replace('"', '""') + '"'

def literal(value):
    if '\x00' in value: raise ValueError('Invalid PostgreSQL value')
    return "'" + value.replace("'", "''") + "'"

def credentials(root):
    root = Path(root); env = env_values(root); path = root / 'data/database/credentials.json'
    if path.exists():
        value = json.loads(path.read_text())
        if value.get('schema_version') != 1 or value.get('database') != env['POSTGRES_DB']:
            raise ValueError('Database credentials belong to a different database or format')
        for name, role in [('runtime', RUNTIME), ('migration', MIGRATION)]:
            if value.get(name, {}).get('username') != role or len(value[name].get('password', '')) < 32:
                raise ValueError('Invalid private database role configuration')
        if path.stat().st_mode & 0o077: raise ValueError('Database credentials must be private (0600)')
        return value
    value = {'schema_version': 1, 'database': env['POSTGRES_DB'],
             'runtime': {'username': RUNTIME, 'password': secrets.token_hex(32)},
             'migration': {'username': MIGRATION, 'password': secrets.token_hex(32)}}
    # Persist before the transaction so an interrupted attempt reuses the same credentials.
    atomic_json(path, value)
    return value

def password_matches(role, password, verifier):
    """Verify our generated ASCII credentials against PG's stored format.

    https://www.postgresql.org/docs/17/catalog-pg-authid.html
    SCRAM derivation: RFC 5802 section 3, using SHA-256 (RFC 7677).
    Unknown formats/normalization are refused rather than changing existing roles.
    """
    if not password.isascii() or not isinstance(verifier,str):return False
    if verifier.startswith('md5'):
        return hmac.compare_digest(verifier,'md5'+hashlib.md5((password+role).encode()).hexdigest())
    try:
        algorithm,parameters,keys=verifier.split('$')
        iterations,salt=parameters.split(':');stored,server=keys.split(':');iterations=int(iterations)
        if algorithm!='SCRAM-SHA-256' or not 1<=iterations<=1000000:return False
        salt=base64.b64decode(salt,validate=True);stored=base64.b64decode(stored,validate=True);server=base64.b64decode(server,validate=True)
        if len(stored)!=32 or len(server)!=32:return False
        salted=hashlib.pbkdf2_hmac('sha256',password.encode(),salt,iterations)
        client=hmac.digest(salted,b'Client Key','sha256')
        return hmac.compare_digest(hashlib.sha256(client).digest(),stored) & hmac.compare_digest(hmac.digest(salted,b'Server Key','sha256'),server)
    except (ValueError,TypeError):return False

def verify_restore_roles(container, admin, database, value):
    """Existing cluster logins may be reused, never rotated by a state restore."""
    # The privileged bootstrap pipe stays private. Neither hashes nor passwords
    # enter argv, logs or returned diagnostics. A login probe is insufficient:
    # a container's pg_hba.conf may trust loopback regardless of its password.
    query="SELECT COALESCE(json_object_agg(rolname,rolpassword),'{}'::json) FROM pg_authid WHERE rolname IN ('ai_phone_runtime','ai_phone_migrator');"
    result=subprocess.run(['docker','exec','-i',container,'psql','-X','-v','ON_ERROR_STOP=1','-At','-U',admin,'-d',database],
                          input=query,capture_output=True,text=True,timeout=15)
    if result.returncode:raise ValueError('Cannot inspect existing database roles before restore')
    existing=json.loads(result.stdout)
    for login in (value['runtime'],value['migration']):
        if login['username'] not in existing:continue
        if not password_matches(login['username'],login['password'],existing[login['username']]):
            raise ValueError('Existing database role credentials differ or cannot be verified; use a dedicated empty PostgreSQL container for this backup')
    return set(existing)

def preparation_sql(env, value, *, verified_existing_roles=None):
    database = identifier(env['POSTGRES_DB']); admin = env['POSTGRES_USER']
    if admin in (RUNTIME, MIGRATION): raise ValueError('Bootstrap and application roles must be separate')
    sql = ['BEGIN;', "SET LOCAL lock_timeout='5s';", "SET LOCAL statement_timeout='60s';"]
    for name in ('migration', 'runtime'):
        role = value[name]['username']; password = value[name]['password']
        configure=f'ALTER ROLE {identifier(role)} WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD {literal(password)};\n COMMENT ON ROLE {identifier(role)} IS {literal(MARKER)};'
        existing=configure
        missing=f'CREATE ROLE {identifier(role)} LOGIN;\n {configure}'
        if verified_existing_roles is not None:
            # A credential check happens before decoding/importing the dump. Recheck
            # membership/flags here, but never mutate an existing cluster-global role.
            existing=(f"IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname={literal(role)} AND (rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls OR NOT rolcanlogin)) OR EXISTS(SELECT 1 FROM pg_auth_members WHERE member=(SELECT oid FROM pg_roles WHERE rolname={literal(role)})) THEN RAISE EXCEPTION 'Unsafe existing database role; use a dedicated restore cluster'; END IF;"
                      if role in verified_existing_roles else "RAISE EXCEPTION 'An existing database role appeared after restore preflight';")
            if role in verified_existing_roles:missing="RAISE EXCEPTION 'An existing database role disappeared after restore preflight';"
        sql.append(f"""DO $role$
BEGIN
 IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname={literal(role)}) THEN
  IF COALESCE((SELECT shobj_description(oid,'pg_authid') FROM pg_roles WHERE rolname={literal(role)}),'') <> {literal(MARKER)} THEN
   RAISE EXCEPTION 'Database role already exists outside AI Phone management';
  END IF;
  {existing}
 ELSE
  {missing}
 END IF;
END $role$;""")
    # Remove unexpected membership left by an older/manual deployment.
    if verified_existing_roles is None:sql.append(f"""DO $membership$ DECLARE item record; BEGIN
 FOR item IN SELECT p.rolname AS parent,c.rolname AS child FROM pg_auth_members m JOIN pg_roles p ON p.oid=m.roleid JOIN pg_roles c ON c.oid=m.member WHERE c.rolname IN ({literal(RUNTIME)},{literal(MIGRATION)}) LOOP
  EXECUTE format('REVOKE %I FROM %I',item.parent,item.child);
 END LOOP;
END $membership$;""")
    sql += [f'REVOKE CREATE,TEMPORARY ON DATABASE {database} FROM PUBLIC;',
            f'REVOKE ALL ON DATABASE {database} FROM {identifier(RUNTIME)};',
            f'GRANT CONNECT ON DATABASE {database} TO {identifier(RUNTIME)};',
            f'GRANT CONNECT,CREATE ON DATABASE {database} TO {identifier(MIGRATION)};',
            'REVOKE CREATE ON SCHEMA public FROM PUBLIC;',
            f'GRANT USAGE,CREATE ON SCHEMA public TO {identifier(MIGRATION)};',
            f'GRANT USAGE ON SCHEMA public TO {identifier(RUNTIME)};']
    # Never REASSIGN OWNED: that could transfer bootstrap-owned template databases.
    sql.append(f"""DO $objects$ DECLARE item record; BEGIN
 FOR item IN SELECT c.relname,c.relkind FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
  WHERE n.nspname='public' AND c.relkind IN ('r','p','S','v','m','f')
  AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_class'::regclass AND d.objid=c.oid AND d.deptype='e')
  ORDER BY (c.relkind='S'),c.oid LOOP
  EXECUTE format('ALTER %s public.%I OWNER TO %I',CASE item.relkind WHEN 'S' THEN 'SEQUENCE' WHEN 'v' THEN 'VIEW' WHEN 'm' THEN 'MATERIALIZED VIEW' WHEN 'f' THEN 'FOREIGN TABLE' ELSE 'TABLE' END,item.relname,{literal(MIGRATION)});
 END LOOP;
END $objects$;""")
    sql += [f'GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO {identifier(RUNTIME)};',
            f'GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO {identifier(RUNTIME)};',
            f'ALTER DEFAULT PRIVILEGES FOR ROLE {identifier(MIGRATION)} IN SCHEMA public GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO {identifier(RUNTIME)};',
            f'ALTER DEFAULT PRIVILEGES FOR ROLE {identifier(MIGRATION)} IN SCHEMA public GRANT USAGE,SELECT ON SEQUENCES TO {identifier(RUNTIME)};',
            f"DO $migrations$ BEGIN IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN REVOKE ALL ON public._sqlx_migrations FROM {identifier(RUNTIME)}; GRANT SELECT ON public._sqlx_migrations TO {identifier(RUNTIME)}; END IF; END $migrations$;",
            'COMMIT;']
    return '\n'.join(sql)

def prepare(root, container=None):
    root = Path(root).resolve(); env = env_values(root); value = credentials(root)
    container = container or os.environ.get('AI_PHONE_PG_CONTAINER', 'ai-phone-postgres-1')
    result = subprocess.run(['docker', 'exec', '-i', container, 'psql', '-X', '-v', 'ON_ERROR_STOP=1', '-U', env['POSTGRES_USER'], '-d', env['POSTGRES_DB']],
                            input=preparation_sql(env, value), capture_output=True, text=True)
    if result.returncode:
        error = result.stderr[-12000:]
        for secret in [env['POSTGRES_PASSWORD'], value['runtime']['password'], value['migration']['password']]: error = error.replace(secret, '[redacted]')
        path=root/'data/database/prepare-error.log';path.write_text(error);path.chmod(0o600)
        raise ValueError('Database role preparation failed; transaction rolled back. See private data/database/prepare-error.log')
    return {'prepared': True, 'runtime': RUNTIME, 'migration': MIGRATION, 'bootstrap_preserved': True}

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('operation',choices=['prepare']);parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parent.parent);parser.add_argument('--container')
    args=parser.parse_args();print(json.dumps(prepare(args.root,args.container)))

if __name__=='__main__':
    try:main()
    except (ValueError,OSError,subprocess.SubprocessError) as error:raise SystemExit(str(error))
