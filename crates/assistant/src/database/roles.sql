DO $roles$
DECLARE name text;
BEGIN
 FOR name IN SELECT unnest(ARRAY['ai_phone_migrator','ai_phone_runtime']) LOOP
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=name) THEN
   IF COALESCE((SELECT shobj_description(oid,'pg_authid') FROM pg_roles WHERE rolname=name),'') <> 'AI Phone managed database role v1' THEN
    RAISE EXCEPTION 'Database role already exists outside AI Phone management';
   END IF;
  ELSE
   EXECUTE format('CREATE ROLE %I LOGIN',name);
  END IF;
  EXECUTE format('ALTER ROLE %I WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD %L',name,
   current_setting(CASE name WHEN 'ai_phone_migrator' THEN 'aiphone.migration_password' ELSE 'aiphone.runtime_password' END));
  EXECUTE format('COMMENT ON ROLE %I IS %L',name,'AI Phone managed database role v1');
 END LOOP;
END $roles$;

DO $membership$
DECLARE item record;
BEGIN
 FOR item IN SELECT p.rolname AS parent,c.rolname AS child FROM pg_auth_members m JOIN pg_roles p ON p.oid=m.roleid JOIN pg_roles c ON c.oid=m.member
  WHERE c.rolname IN ('ai_phone_runtime','ai_phone_migrator') LOOP
  EXECUTE format('REVOKE %I FROM %I',item.parent,item.child);
 END LOOP;
END $membership$;
