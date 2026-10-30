use crate::*;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::extract::State;
use sha2::{Digest, Sha256};

pub async fn set_admin(db: &PgPool, password: &str) -> Result<(), Box<dyn std::error::Error>> {
    if password.is_empty() || password.len() > 128 {
        return Err("Invalid administrator password".into());
    }
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes())
        .map_err(|_| "Cannot create password salt")?;
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| "Cannot hash password")?
        .to_string();
    sqlx::query("INSERT INTO admin_account(singleton,username,password_hash) VALUES(true,'admin',$1) ON CONFLICT(singleton) DO UPDATE SET password_hash=excluded.password_hash").bind(hash).execute(db).await?;
    sqlx::query("DELETE FROM admin_sessions")
        .execute(db)
        .await?;
    println!("Stored one administrator account (admin); password is Argon2id hashed.");
    Ok(())
}
pub async fn login(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let request: wire::LoginRequest = match decode(&app, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    {
        let mut attempts = app.login_attempts.lock().await;
        attempts.retain(|at| at.elapsed() < Duration::from_secs(60));
        if attempts.len() >= 20 {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "login_rate_limit",
                "尝试次数过多，请稍后再试",
            );
        }
        attempts.push(std::time::Instant::now());
    }
    let hash: Option<String> = match sqlx::query_scalar(
        "SELECT password_hash FROM admin_account WHERE singleton=true AND username='admin'",
    )
    .fetch_optional(&app.db)
    .await
    {
        Ok(v) => v,
        Err(e) => return db_error(e),
    };
    let Some(hash) = hash else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "admin_missing",
            "管理员账号尚未配置",
        );
    };
    if request.password.len() > 128 || request.username != "admin" {
        return error(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "账号或密码错误",
        );
    }
    let valid = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).is_ok_and(|hash| {
            Argon2::default()
                .verify_password(request.password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false);
    if !valid {
        return error(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "账号或密码错误",
        );
    }
    issue_session(&app).await
}
async fn issue_session(app: &App) -> Response {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    if let Err(e) = sqlx::query("INSERT INTO admin_sessions(token_hash) VALUES($1)")
        .bind(hash)
        .execute(&app.db)
        .await
    {
        return db_error(e);
    }
    pb(
        StatusCode::OK,
        wire::LoginResult {
            token,
            username: "admin".into(),
        },
    )
}
pub async fn enter(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    enter_local(
        app,
        headers,
        body,
        std::env::var("AI_PHONE_LOCAL_ENTRY").as_deref() == Ok("1"),
    )
    .await
}
async fn enter_local(
    app: Shared,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
    enabled: bool,
) -> Response {
    if let Err(e) = decode::<wire::LocalEntryRequest>(&app, &headers, body) {
        return e.into();
    }
    if !enabled {
        return error(
            StatusCode::FORBIDDEN,
            "local_entry_disabled",
            "此部署未启用本地入口",
        );
    }
    {
        let mut attempts = app.login_attempts.lock().await;
        attempts.retain(|at| at.elapsed() < Duration::from_secs(60));
        if attempts.len() >= 20 {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "login_rate_limit",
                "尝试次数过多，请稍后再试",
            );
        }
        attempts.push(std::time::Instant::now());
    }
    let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM admin_account)")
        .fetch_one(&app.db)
        .await;
    match exists {
        Err(e) => return db_error(e),
        Ok(false) => {
            let hash = tokio::task::spawn_blocking(|| {
                let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|_| ())?;
                Argon2::default()
                    .hash_password(Uuid::new_v4().to_string().as_bytes(), &salt)
                    .map(|v| v.to_string())
                    .map_err(|_| ())
            })
            .await;
            let Ok(Ok(hash)) = hash else {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "owner_init_failed",
                    "本地账户初始化失败",
                );
            };
            if let Err(e)=sqlx::query("INSERT INTO admin_account(singleton,username,password_hash) VALUES(true,'admin',$1) ON CONFLICT(singleton) DO NOTHING").bind(hash).execute(&app.db).await{return db_error(e);}
        }
        Ok(true) => {}
    }
    issue_session(&app).await
}
pub async fn session(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let request: wire::SessionRequest = match decode(&app, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let hash = format!("{:x}", Sha256::digest(request.token.as_bytes()));
    let valid: bool = match sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM admin_sessions WHERE token_hash=$1 AND expires_at>now())",
    )
    .bind(hash)
    .fetch_one(&app.db)
    .await
    {
        Ok(v) => v,
        Err(e) => return db_error(e),
    };
    pb(
        StatusCode::OK,
        wire::SessionState {
            authenticated: valid,
            username: if valid { "admin".into() } else { String::new() },
        },
    )
}
pub async fn logout(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let request: wire::SessionRequest = match decode(&app, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let hash = format!("{:x}", Sha256::digest(request.token.as_bytes()));
    if let Err(e) = sqlx::query("DELETE FROM admin_sessions WHERE token_hash=$1")
        .bind(hash)
        .execute(&app.db)
        .await
    {
        return db_error(e);
    }
    pb(
        StatusCode::OK,
        wire::SessionState {
            authenticated: false,
            username: String::new(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use sqlx::{Connection, postgres::PgConnectOptions};
    use std::{panic::AssertUnwindSafe, str::FromStr};
    #[tokio::test]
    #[ignore = "requires AI_PHONE_TEST_DATABASE_URL; isolated database, no production state"]
    async fn local_entry_preserves_owner_credentials_and_existing_sessions() {
        let url = std::env::var("AI_PHONE_TEST_DATABASE_URL").unwrap();
        let name = format!("aiphone_entry_test_{}", Uuid::new_v4().simple());
        let mut admin = sqlx::PgConnection::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut admin)
            .await
            .unwrap();
        let db = sqlx::postgres::PgPoolOptions::new()
            .max_connections(3)
            .connect_with(PgConnectOptions::from_str(&url).unwrap().database(&name))
            .await
            .unwrap();
        let root = std::env::temp_dir().join(&name);
        tokio::fs::create_dir_all(&root).await.unwrap();
        let app = Arc::new(App {
            db: db.clone(),
            root: root.clone(),
            token: "entry-test".into(),
            phone_token: "unused".into(),
            settings: settings::SettingsStore::test_configured(&root),
            local: reqwest::Client::new(),
            remote: reqwest::Client::new(),
            active: Mutex::new(None),
            login_attempts: Mutex::new(vec![]),
        });
        let outcome = AssertUnwindSafe(async {
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            let mut headers = HeaderMap::new();
            headers.insert("authorization", "Bearer entry-test".parse().unwrap());
            headers.insert("content-type", MEDIA.parse().unwrap());
            assert_eq!(
                enter_local(app.clone(), headers.clone(), Ok(Bytes::new()), false)
                    .await
                    .status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                enter_local(app.clone(), HeaderMap::new(), Ok(Bytes::new()), true)
                    .await
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                enter_local(app.clone(), headers.clone(), Ok(Bytes::new()), true)
                    .await
                    .status(),
                StatusCode::OK
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM admin_account")
                    .fetch_one(&db)
                    .await
                    .unwrap(),
                1
            );
            set_admin(&db, "existing-owner-password").await.unwrap();
            let hash: String = sqlx::query_scalar("SELECT password_hash FROM admin_account")
                .fetch_one(&db)
                .await
                .unwrap();
            let first = enter_local(app.clone(), headers.clone(), Ok(Bytes::new()), true).await;
            let bytes = axum::body::to_bytes(first.into_body(), 4096).await.unwrap();
            let first = wire::LoginResult::decode(bytes).unwrap();
            assert!(!first.token.is_empty());
            assert_eq!(
                enter_local(app.clone(), headers.clone(), Ok(Bytes::new()), true)
                    .await
                    .status(),
                StatusCode::OK
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>("SELECT password_hash FROM admin_account")
                    .fetch_one(&db)
                    .await
                    .unwrap(),
                hash
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM admin_sessions")
                    .fetch_one(&db)
                    .await
                    .unwrap(),
                2,
                "enter never logs out another window"
            );
            let body = wire::SessionRequest { token: first.token };
            let response = session(
                State(app.clone()),
                headers.clone(),
                Ok(Bytes::from(body.encode_to_vec())),
            )
            .await;
            let state = wire::SessionState::decode(
                axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert!(state.authenticated);
            *app.login_attempts.lock().await = vec![std::time::Instant::now(); 20];
            assert_eq!(
                enter_local(app.clone(), headers, Ok(Bytes::new()), true)
                    .await
                    .status(),
                StatusCode::TOO_MANY_REQUESTS
            );
        })
        .catch_unwind()
        .await;
        drop(app);
        db.close().await;
        sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .execute(&mut admin)
            .await
            .unwrap();
        let _ = tokio::fs::remove_dir_all(root).await;
        if let Err(error) = outcome {
            std::panic::resume_unwind(error);
        }
    }
}
