use ai_phone_api::wire;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, rejection::BytesRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use prost::Message;
use sqlx::PgPool;
#[cfg(test)]
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

mod agent_loop;
mod anthropic_messages;
mod app_learning;
mod auth;
mod chat_completions;
mod completion_audit;
mod database;
mod desktop;
mod desktop_explore;
mod device_exchange;
mod device_network;
mod exploration_policy;
mod http;
mod location;
mod location_devices;
mod location_motion;
mod model_adapter;
mod model_context;
mod model_sse;
mod notification_policy;
mod phone_system;
mod proactive;
mod registry;
mod run_notice;
mod settings;
mod setup;
mod tool_catalog;
mod voice_input;
mod workbench;
mod worker;

const MEDIA: &str = "application/x-protobuf";
struct App {
    db: PgPool,
    root: PathBuf,
    token: String,
    phone_token: String,
    settings: settings::SettingsStore,
    local: reqwest::Client,
    remote: reqwest::Client,
    active: Mutex<Option<(Uuid, CancellationToken)>>,
    login_attempts: Mutex<Vec<std::time::Instant>>,
}
type Shared = Arc<App>;
fn pb(status: StatusCode, value: impl Message) -> Response {
    (
        status,
        [("content-type", MEDIA), ("cache-control", "no-store")],
        value.encode_to_vec(),
    )
        .into_response()
}
fn error(status: StatusCode, code: &str, message: &str) -> Response {
    pb(
        status,
        wire::ApiError {
            code: code.into(),
            message: message.into(),
        },
    )
}
// Keep transport responses off the success-path Result stack.
struct Rejection(Box<Response>);
impl From<Rejection> for Response {
    fn from(rejection: Rejection) -> Self {
        *rejection.0
    }
}
fn rejected(status: StatusCode, code: &str, message: &str) -> Rejection {
    Rejection(Box::new(error(status, code, message)))
}
fn auth(app: &App, headers: &HeaderMap) -> Result<(), Rejection> {
    if headers.get("authorization").and_then(|v| v.to_str().ok())
        != Some(format!("Bearer {}", app.token).as_str())
    {
        return Err(rejected(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "未配对的工作台",
        ));
    }
    Ok(())
}
fn decode<T: Message + Default>(
    app: &App,
    headers: &HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<T, Rejection> {
    auth(app, headers)?;
    if headers.get("content-type").and_then(|v| v.to_str().ok()) != Some(MEDIA) {
        return Err(rejected(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            MEDIA,
        ));
    }
    T::decode(body.map_err(|e| rejected(e.status(), "body_rejected", "请求过大或无法读取"))?)
        .map_err(|_| rejected(StatusCode::BAD_REQUEST, "invalid_protobuf", "请求格式错误"))
}
fn db_error(_: impl std::fmt::Display) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "storage_error",
        "保存失败，请重试",
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|arg| arg == "--version") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let setup_admin = std::env::args().any(|arg| arg == "--set-admin");
    let migrate_database = std::env::args().any(|arg| arg == "--migrate");
    let check_database = std::env::args().any(|arg| arg == "--database-check");
    let sync_skills = std::env::args().any(|arg| arg == "--sync-builtin-skills");
    let root = std::env::var_os("AI_PHONE_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    if !setup_admin
        && !sync_skills
        && !migrate_database
        && !check_database
        && root.join("data/restore-quarantine.json").exists()
    {
        return Err("Restored state is quarantined; verify recovery and stop the original instance before activation".into());
    }
    let listener = if setup_admin || sync_skills || migrate_database || check_database {
        None
    } else {
        Some(tokio::net::TcpListener::bind("127.0.0.1:17863").await?)
    };
    let db = database::connect(&root, migrate_database).await?;
    if migrate_database {
        println!("Database migrations applied by dedicated migration login");
        return Ok(());
    }
    if check_database {
        let user: String = sqlx::query_scalar("SELECT current_user::text")
            .fetch_one(&db)
            .await?;
        println!(
            "{}",
            serde_json::json!({"runtime":user,"privileged":false,"ddl":false})
        );
        return Ok(());
    }
    let settings = settings::SettingsStore::open(&root)?;
    if setup_admin {
        use std::io::Read;
        let mut password = String::new();
        std::io::stdin().read_to_string(&mut password)?;
        return auth::set_admin(&db, password.trim_end_matches(['\r', '\n'])).await;
    }
    if sync_skills {
        workbench::initialize(&db, &root).await?;
        println!("Built-in skills synchronized; running tasks were not changed.");
        return Ok(());
    }
    worker::recover_interrupted_runs(&db).await?;
    sqlx::query("UPDATE runs SET model_revision=$1 WHERE model_revision=0")
        .bind(settings.current() as i64)
        .execute(&db)
        .await?;
    registry::initialize(&db, &settings).await?;
    setup::initialize(&db, &root).await?;
    let mut remote = reqwest::Client::builder().no_proxy();
    if let Ok(proxy) = std::env::var("AI_PHONE_HOST_PROXY")
        && !proxy.is_empty()
    {
        remote = remote.proxy(reqwest::Proxy::all(&proxy)?);
    }
    let app = Arc::new(App {
        db,
        token: std::fs::read_to_string(root.join("data/assistant/token"))?
            .trim()
            .into(),
        phone_token: std::fs::read_to_string(root.join("data/phone-runner/token"))?
            .trim()
            .into(),
        root,
        settings,
        local: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()?,
        remote: remote
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(180))
            .build()?,
        active: Mutex::new(None),
        login_attempts: Mutex::new(Vec::new()),
    });
    workbench::initialize(&app.db, &app.root).await?;
    app_learning::backfill(&app).await?;
    desktop::recover(app.clone()).await;
    phone_system::recover(app.clone()).await;
    tokio::spawn(worker::work(app.clone()));
    tokio::spawn(proactive::work(app.clone()));
    tokio::spawn(location::work(app.clone()));
    tokio::spawn(desktop_explore::work(app.clone()));
    tokio::spawn(device_exchange::work(app.clone()));
    tokio::spawn(device_network::recover(app.clone()));
    let routes = Router::new()
        .route("/v1/registry/{operation}", post(registry::route))
        .route("/v1/location/{operation}", post(location::route))
        .route(
            "/v1/device-network/{operation}",
            post(device_network::route),
        )
        .route("/v1/desktop/{operation}", post(desktop::route))
        .route("/v1/phone-system/{operation}", post(phone_system::route))
        .route("/v1/settings/{operation}", post(settings::route))
        .route("/v1/proactive/{operation}", post(proactive::route))
        .route("/v1/setup/{operation}", post(setup::route))
        .route("/v1/login", post(auth::login))
        .route("/v1/enter", post(auth::enter))
        .route("/v1/session", post(auth::session))
        .route("/v1/logout", post(auth::logout))
        .route("/v1/history", post(http::history))
        .route("/v1/send", post(http::send))
        .route("/v1/run", post(http::run_state))
        .route("/v1/workbench/{operation}", post(workbench::route))
        .route("/v1/workbench/file", axum::routing::get(workbench::file))
        .route("/v1/cancel", post(http::cancel))
        .route("/v1/resume", post(http::resume))
        .layer(DefaultBodyLimit::max(65536));
    let routes = routes
        .route(
            "/v1/workbench/upload-file",
            post(workbench::upload_file).layer(DefaultBodyLimit::max(12 * 1024 * 1024)),
        )
        .route(
            "/v1/upload",
            post(http::upload).layer(DefaultBodyLimit::max(20 * 1024 * 1024)),
        )
        .route("/v1/attachment/{id}", axum::routing::get(http::attachment))
        .with_state(app);
    println!("AI Phone assistant listening on 127.0.0.1:17863");
    axum::serve(listener.ok_or("Missing listener")?, routes)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
