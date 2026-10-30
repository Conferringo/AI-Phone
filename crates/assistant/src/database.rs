//! Separate DML runtime credentials from the DDL migration login.
use serde::Deserialize;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{os::unix::fs::PermissionsExt, path::Path, time::Duration};

#[derive(Deserialize)]
struct Login {
    username: String,
    password: String,
}
#[derive(Deserialize)]
struct Credentials {
    schema_version: u32,
    database: String,
    runtime: Login,
    migration: Login,
}

pub async fn connect(root: &Path, migration: bool) -> Result<PgPool, Box<dyn std::error::Error>> {
    let path = root.join("data/database/credentials.json");
    if !path.is_file() {
        return Err(
            "Database role configuration missing; run scripts/database.py prepare before starting"
                .into(),
        );
    }
    if std::fs::metadata(&path)?.permissions().mode() & 0o077 != 0 {
        return Err("Database credentials must have mode 0600".into());
    }
    let credentials: Credentials = serde_json::from_slice(&std::fs::read(path)?)?;
    if credentials.schema_version != 1
        || credentials.runtime.username != "ai_phone_runtime"
        || credentials.migration.username != "ai_phone_migrator"
    {
        return Err("Unsupported database role configuration".into());
    }
    let env = std::fs::read_to_string(root.join(".env"))?;
    let database = env
        .lines()
        .find_map(|line| line.strip_prefix("POSTGRES_DB="))
        .unwrap_or("")
        .trim();
    if credentials.database != database {
        return Err("Database credentials do not match this installation".into());
    }
    let login = if migration {
        &credentials.migration
    } else {
        &credentials.runtime
    };
    if login.password.len() < 32 {
        return Err("Database role credentials are incomplete".into());
    }
    let options = PgConnectOptions::new()
        .host("127.0.0.1")
        .port(
            std::env::var("AI_PHONE_PG_PORT")
                .unwrap_or_else(|_| "55482".into())
                .parse()?,
        )
        .username(&login.username)
        .password(&login.password)
        .database(database);
    let db=PgPoolOptions::new().max_connections(if migration{1}else{5}).acquire_timeout(Duration::from_secs(5))
 .after_connect(|connection,_|Box::pin(async move{sqlx::query("SELECT set_config('application_name','ai-phone-assistant',false),set_config('statement_timeout','30000',false),set_config('lock_timeout','5000',false),set_config('idle_in_transaction_session_timeout','30000',false)").execute(connection).await?;Ok(())})).connect_with(options).await?;
    let unsafe_role:bool=sqlx::query_scalar("SELECT rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&db).await?;
    if unsafe_role {
        return Err("Refusing a privileged database service login".into());
    }
    if migration {
        sqlx::migrate!().run(&db).await?;
        // Default table grants must not make the schema history writable by the runtime.
        sqlx::query("REVOKE INSERT,UPDATE,DELETE ON _sqlx_migrations FROM ai_phone_runtime")
            .execute(&db)
            .await?;
    } else {
        let ddl:bool=sqlx::query_scalar("SELECT has_schema_privilege(current_user,'public','CREATE') OR has_database_privilege(current_user,current_database(),'CREATE')").fetch_one(&db).await?;
        if ddl {
            return Err("Runtime database login unexpectedly has DDL privileges".into());
        }
        let rows =
            sqlx::query("SELECT version,checksum,success FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&db)
                .await?;
        let expected = sqlx::migrate!();
        if rows.len() != expected.migrations.len()
            || rows.iter().any(|row| {
                !row.get::<bool, _>("success")
                    || !expected.migrations.iter().any(|migration| {
                        migration.version == row.get::<i64, _>("version")
                            && migration.checksum.as_ref() == row.get::<Vec<u8>, _>("checksum")
                    })
            })
        {
            return Err(
                "Database schema differs from this release; run the explicit migration command"
                    .into(),
            );
        }
    }
    Ok(db)
}
