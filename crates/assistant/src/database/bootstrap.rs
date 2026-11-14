//! Explicit install-time preparation, using the existing credential file and SQLx.
use super::{Credentials, Login};
use sqlx::{Connection, PgConnection, postgres::PgConnectOptions};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const CONTRACT: &str = "ai-phone-database-prepare-v1";

fn configuration(root: &Path) -> Result<HashMap<String, String>> {
    let values: HashMap<_, _> = fs::read_to_string(root.join(".env"))?
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect();
    if !["POSTGRES_DB", "POSTGRES_USER", "POSTGRES_PASSWORD"]
        .iter()
        .all(|key| values.get(*key).is_some_and(|value| !value.is_empty()))
    {
        return Err("Private PostgreSQL bootstrap configuration is incomplete".into());
    }
    if matches!(
        values["POSTGRES_USER"].as_str(),
        "ai_phone_runtime" | "ai_phone_migrator"
    ) {
        return Err("Bootstrap and application roles must be separate".into());
    }
    Ok(values)
}

fn password() -> Result<String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| "Cannot generate database credentials")?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn credentials(root: &Path, database: &str) -> Result<Credentials> {
    let path = root.join("data/database/credentials.json");
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
                return Err("Database credentials must be a private regular file".into());
            }
            let value: Credentials = serde_json::from_slice(&fs::read(path)?)?;
            if value.schema_version != 1
                || value.database != database
                || value.runtime.username != "ai_phone_runtime"
                || value.migration.username != "ai_phone_migrator"
                || value.runtime.password.len() < 32
                || value.migration.password.len() < 32
            {
                return Err("Database credentials do not match this installation".into());
            }
            Ok(value)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let value = Credentials {
                schema_version: 1,
                database: database.into(),
                runtime: Login {
                    username: "ai_phone_runtime".into(),
                    password: password()?,
                },
                migration: Login {
                    username: "ai_phone_migrator".into(),
                    password: password()?,
                },
            };
            let parent = path.parent().ok_or("Invalid credential path")?;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)?;
            if fs::symlink_metadata(parent)?.file_type().is_symlink() {
                return Err("Database credential directory cannot be an alias".into());
            }
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            // Persist before touching PostgreSQL so an interrupted install reuses its passwords.
            crate::private_file::atomic_private(&path, &serde_json::to_vec(&value)?)?;
            Ok(value)
        }
        Err(error) => Err(error.into()),
    }
}

async fn apply(connection: &mut PgConnection, value: &Credentials) -> Result<()> {
    let mut transaction = connection.begin().await?;
    sqlx::raw_sql("SET LOCAL lock_timeout='5s'; SET LOCAL statement_timeout='60s';")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("SELECT set_config('aiphone.runtime_password',$1,true), set_config('aiphone.migration_password',$2,true)")
        .bind(&value.runtime.password).bind(&value.migration.password)
        .execute(&mut *transaction).await?;
    sqlx::raw_sql(concat!(
        include_str!("roles.sql"),
        "\n",
        include_str!("grants.sql")
    ))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn prepare(root: &Path) -> Result<()> {
    let environment = configuration(root)?;
    let value = credentials(root, &environment["POSTGRES_DB"])?;
    let options = PgConnectOptions::new()
        .host("127.0.0.1")
        .port(
            std::env::var("AI_PHONE_PG_PORT")
                .unwrap_or_else(|_| "55482".into())
                .parse()?,
        )
        .database(&environment["POSTGRES_DB"])
        .username(&environment["POSTGRES_USER"])
        .password(&environment["POSTGRES_PASSWORD"]);
    let mut connection = PgConnection::connect_with(&options).await?;
    if let Err(error) = apply(&mut connection, &value).await {
        let mut message = error.to_string();
        for secret in [
            &environment["POSTGRES_PASSWORD"],
            &value.runtime.password,
            &value.migration.password,
        ] {
            message = message.replace(secret, "[redacted]");
        }
        let path = root.join("data/database/prepare-error.log");
        let mut output = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)?;
        output.write_all(message.as_bytes())?;
        return Err("Database role preparation failed; transaction rolled back. See private data/database/prepare-error.log".into());
    }
    println!(
        "{}",
        serde_json::json!({"prepared":true,"runtime":"ai_phone_runtime","migration":"ai_phone_migrator","bootstrap_preserved":true})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_keep_the_existing_schema_passwords_and_private_mode() {
        let root = std::env::temp_dir().join(format!("aiphone-rust-db-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let first = credentials(&root, "fixture").unwrap();
        let path = root.join("data/database/credentials.json");
        let bytes = fs::read(&path).unwrap();
        let resumed = credentials(&root, "fixture").unwrap();
        assert_eq!(resumed.runtime.password, first.runtime.password);
        assert_eq!(resumed.migration.password, first.migration.password);
        assert_ne!(first.runtime.password, first.migration.password);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(credentials(&root, "other").is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_dir_all(root).unwrap();
    }
}
