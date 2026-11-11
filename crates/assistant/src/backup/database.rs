use super::{Result, command, read_json};
use base64::{Engine, engine::general_purpose::STANDARD};
use md5::Md5;
use ring::{hmac, pbkdf2};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    num::NonZeroU32,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::{Command, Stdio},
};

const ROLES: [&str; 2] = ["ai_phone_runtime", "ai_phone_migrator"];
const MARKER: &str = "AI Phone managed database role v1";
pub(super) fn environment(root: &Path) -> Result<BTreeMap<String, String>> {
    let values: BTreeMap<_, _> = fs::read_to_string(root.join(".env"))?
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().into(), value.trim().into()))
        .collect();
    if ["POSTGRES_DB", "POSTGRES_USER", "POSTGRES_PASSWORD"]
        .iter()
        .any(|key| values.get(*key).is_none_or(String::is_empty))
    {
        return Err("Private PostgreSQL configuration incomplete".into());
    }
    Ok(values)
}
fn identifier(value: &str) -> Result<String> {
    if value.is_empty() || value.contains('\0') {
        return Err("Invalid PostgreSQL identifier".into());
    }
    Ok(format!("\"{}\"", value.replace('"', "\"\"")))
}
fn literal(value: &str) -> Result<String> {
    if value.contains('\0') {
        return Err("Invalid PostgreSQL value".into());
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}
pub(super) fn sql(container: &str, admin: &str, database: &str, sql: &str) -> Result<String> {
    Ok(String::from_utf8(command(
        "docker",
        &[
            "exec",
            "-i",
            container,
            "psql",
            "-X",
            "-v",
            "ON_ERROR_STOP=1",
            "-U",
            admin,
            "-d",
            database,
            "-At",
        ],
        Some(sql.as_bytes()),
    )?)?
    .trim()
    .into())
}
pub(super) fn fingerprints(container: &str, admin: &str, database: &str) -> Result<Value> {
    let mut result = Map::new();
    for table in sql(
        container,
        admin,
        database,
        "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename;",
    )?
    .lines()
    {
        let query = format!(
            "SELECT json_build_object('rows',count(*),'digest',md5(COALESCE(string_agg(digest,'' ORDER BY digest COLLATE \"C\"),''))) FROM (SELECT md5(row_to_json(t)::text) AS digest FROM public.{} t) rows;",
            identifier(table)?
        );
        result.insert(
            table.into(),
            serde_json::from_str(&sql(container, admin, database, &query)?)?,
        );
    }
    Ok(Value::Object(result))
}
fn password_matches(role: &str, password: &str, verifier: &str) -> Result<bool> {
    if !password.is_ascii() {
        return Ok(false);
    }
    if verifier.starts_with("md5") {
        return Ok(verifier == format!("md5{:x}", Md5::digest(format!("{password}{role}"))));
    }
    let Some(verifier) = verifier.strip_prefix("SCRAM-SHA-256$") else {
        return Ok(false);
    };
    let Some((parameters, keys)) = verifier.split_once('$') else {
        return Ok(false);
    };
    let Some((iterations, salt)) = parameters.split_once(':') else {
        return Ok(false);
    };
    let Some((stored, server)) = keys.split_once(':') else {
        return Ok(false);
    };
    let iterations: u32 = iterations.parse()?;
    if !(1..=1_000_000).contains(&iterations) {
        return Ok(false);
    }
    let salt = STANDARD.decode(salt)?;
    let stored = STANDARD.decode(stored)?;
    let server = STANDARD.decode(server)?;
    if stored.len() != 32 || server.len() != 32 {
        return Ok(false);
    }
    let mut salted = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(iterations).ok_or("Invalid SCRAM iteration count")?,
        &salt,
        password.as_bytes(),
        &mut salted,
    );
    let key = hmac::Key::new(hmac::HMAC_SHA256, &salted);
    let client = hmac::sign(&key, b"Client Key");
    let server_key = hmac::sign(&key, b"Server Key");
    Ok(Sha256::digest(client.as_ref())[..] == stored[..]
        && server_key.as_ref() == server.as_slice())
}
fn credentials(root: &Path) -> Result<Value> {
    let value = read_json(&root.join("data/database/credentials.json"))?;
    let env = environment(root)?;
    if value["schema_version"] != 1
        || value["database"] != env["POSTGRES_DB"]
        || value["runtime"]["username"] != ROLES[0]
        || value["migration"]["username"] != ROLES[1]
        || ["runtime", "migration"].iter().any(|role| {
            value[role]["password"]
                .as_str()
                .is_none_or(|password| password.len() < 32)
        })
    {
        return Err("Database credential configuration differs".into());
    }
    Ok(value)
}
fn restore_roles(
    container: &str,
    admin: &str,
    database: &str,
    value: &Value,
) -> Result<BTreeSet<String>> {
    let existing: Map<String, Value> = serde_json::from_str(&sql(
        container,
        admin,
        database,
        "SELECT COALESCE(json_object_agg(rolname,rolpassword),'{}'::json) FROM pg_authid WHERE rolname IN ('ai_phone_runtime','ai_phone_migrator');",
    )?)?;
    for kind in ["runtime", "migration"] {
        let login = &value[kind];
        let role = login["username"].as_str().ok_or("Database role missing")?;
        if let Some(verifier) = existing.get(role)
            && !password_matches(
                role,
                login["password"]
                    .as_str()
                    .ok_or("Database password missing")?,
                verifier.as_str().ok_or("Database verifier missing")?,
            )?
        {
            return Err(
                "Existing role credentials differ; use an empty dedicated restore cluster".into(),
            );
        }
    }
    Ok(existing.into_iter().map(|(name, _)| name).collect())
}
fn preparation_sql(value: &Value, existing: &BTreeSet<String>) -> Result<String> {
    let mut output = String::new();
    for kind in ["migration", "runtime"] {
        let login = &value[kind];
        let role = login["username"].as_str().ok_or("Database role missing")?;
        let quoted = identifier(role)?;
        let role_literal = literal(role)?;
        let configure = format!(
            "ALTER ROLE {quoted} WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD {}; COMMENT ON ROLE {quoted} IS {};",
            literal(
                login["password"]
                    .as_str()
                    .ok_or("Database password missing")?
            )?,
            literal(MARKER)?
        );
        let (present, missing) = if existing.contains(role) {
            (
                format!(
                    "IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname={role_literal} AND (rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls OR NOT rolcanlogin)) OR EXISTS(SELECT 1 FROM pg_auth_members WHERE member=(SELECT oid FROM pg_roles WHERE rolname={role_literal})) THEN RAISE EXCEPTION 'Unsafe existing database role'; END IF;"
                ),
                "RAISE EXCEPTION 'A database role disappeared after preflight';".into(),
            )
        } else {
            (
                "RAISE EXCEPTION 'A database role appeared after preflight';".into(),
                format!("CREATE ROLE {quoted} LOGIN; {configure}"),
            )
        };
        output.push_str(&format!("DO $role$ BEGIN IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname={role_literal}) THEN IF COALESCE((SELECT shobj_description(oid,'pg_authid') FROM pg_roles WHERE rolname={role_literal}),'') <> {} THEN RAISE EXCEPTION 'Foreign database role'; END IF; {present} ELSE {missing} END IF; END $role$;\n",literal(MARKER)?));
    }
    output.push_str(include_str!("../database/grants.sql"));
    Ok(output)
}
pub(super) fn restore(
    root: &Path,
    container: &str,
    admin: &str,
    database: &str,
    dump: &Path,
) -> Result<()> {
    if sql(
        container,
        admin,
        database,
        "SELECT count(*) FROM pg_tables WHERE schemaname='public';",
    )? != "0"
    {
        return Err("Restore database must be empty".into());
    }
    let value = credentials(root)?;
    let existing = restore_roles(container, admin, database, &value)?;
    let log = root.join("data/database/restore-error.log");
    let log = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(log)?;
    let mut producer = Command::new("docker")
        .args([
            "exec",
            "-i",
            container,
            "pg_restore",
            "--no-owner",
            "--no-acl",
            "--file",
            "-",
        ])
        .stdin(Stdio::from(std::fs::File::open(dump)?))
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log.try_clone()?))
        .spawn()?;
    let mut consumer = Command::new("docker")
        .args([
            "exec",
            "-i",
            container,
            "psql",
            "-X",
            "-v",
            "ON_ERROR_STOP=1",
            "-U",
            admin,
            "-d",
            database,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .spawn()?;
    let result = (|| {
        let mut input = consumer.stdin.take().ok_or("Restore stdin missing")?;
        input.write_all(
            b"BEGIN;\nSET LOCAL lock_timeout='5s'; SET LOCAL statement_timeout='60s';\n",
        )?;
        io::copy(
            &mut producer
                .stdout
                .take()
                .ok_or("Database decoder stdout missing")?,
            &mut input,
        )?;
        if !producer.wait()?.success() {
            return Err("Database dump cannot be decoded".into());
        }
        input.write_all(preparation_sql(&value, &existing)?.as_bytes())?;
        input.write_all(b"\nCOMMIT;\n")?;
        drop(input);
        if !consumer.wait()?.success() {
            return Err("Database restore transaction failed".into());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = producer.kill();
        drop(consumer.stdin.take());
        let _ = consumer.wait();
        let _ = producer.wait();
    }
    result
}
