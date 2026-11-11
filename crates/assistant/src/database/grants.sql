DO $database$
BEGIN
 EXECUTE format('REVOKE CREATE,TEMPORARY ON DATABASE %I FROM PUBLIC',current_database());
 EXECUTE format('REVOKE ALL ON DATABASE %I FROM ai_phone_runtime',current_database());
 EXECUTE format('GRANT CONNECT ON DATABASE %I TO ai_phone_runtime',current_database());
 EXECUTE format('GRANT CONNECT,CREATE ON DATABASE %I TO ai_phone_migrator',current_database());
END $database$;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
GRANT USAGE,CREATE ON SCHEMA public TO ai_phone_migrator;
GRANT USAGE ON SCHEMA public TO ai_phone_runtime;

-- Keep bootstrap-owned databases and extension objects outside the ownership transfer.
DO $objects$
DECLARE item record;
BEGIN
 FOR item IN SELECT c.relname,c.relkind FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
  WHERE n.nspname='public' AND c.relkind IN ('r','p','S','v','m','f')
  AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_class'::regclass AND d.objid=c.oid AND d.deptype='e')
  ORDER BY (c.relkind='S'),c.oid LOOP
  EXECUTE format('ALTER %s public.%I OWNER TO ai_phone_migrator',CASE item.relkind WHEN 'S' THEN 'SEQUENCE' WHEN 'v' THEN 'VIEW' WHEN 'm' THEN 'MATERIALIZED VIEW' WHEN 'f' THEN 'FOREIGN TABLE' ELSE 'TABLE' END,item.relname);
 END LOOP;
END $objects$;
GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO ai_phone_runtime;
GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO ai_phone_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE ai_phone_migrator IN SCHEMA public GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO ai_phone_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE ai_phone_migrator IN SCHEMA public GRANT USAGE,SELECT ON SEQUENCES TO ai_phone_runtime;
DO $migrations$
BEGIN
 IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
  REVOKE ALL ON public._sqlx_migrations FROM ai_phone_runtime;
  GRANT SELECT ON public._sqlx_migrations TO ai_phone_runtime;
 END IF;
END $migrations$;
