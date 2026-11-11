use ai_phone_api::wire;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use prost::Message;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use tonic::transport::{Channel, Endpoint};
mod actions;
mod desktop;
mod grpc;
mod hub;
mod phone;
pub const CONTRACT: &str = "ai-phone-media-v1";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
struct Config {
    device: String,
    serial: String,
    grpc_port: u16,
    port: u16,
    token: String,
}
struct Media {
    config: Config,
    channel: Channel,
    adb: ai_phone_adb::Client,
    width: AtomicU32,
    height: AtomicU32,
    rotation: AtomicU32,
    orientation_at: AtomicU64,
    orientation_lock: Mutex<()>,
    screen: Arc<hub::Hub>,
    desktop: Arc<hub::Hub>,
    cursor: Mutex<wire::DeviceCursor>,
}
impl Media {
    fn session(&self) -> Result<grpc::Session> {
        grpc::session(&self.config, &self.channel)
    }
    async fn orientation(&self) -> Result<u32> {
        let _lock = self.orientation_lock.lock().await;
        if now() - self.orientation_at.load(Ordering::Relaxed) < 500 {
            return Ok(self.rotation.load(Ordering::Relaxed));
        }
        let out = self
            .adb
            .shell(
                "dumpsys input",
                ai_phone_adb::Limits {
                    stdout: 262144,
                    stderr: 4096,
                },
                Duration::from_millis(1800),
            )
            .await?;
        let text = String::from_utf8(out.stdout)?;
        let view = text
            .lines()
            .find(|s| s.contains("Viewport INTERNAL: displayId=0") && s.contains("isActive=[1]"))
            .ok_or("internal display unavailable")?;
        let rotation = view
            .split("orientation=")
            .nth(1)
            .and_then(|s| s.chars().next())
            .and_then(|c| c.to_digit(10))
            .ok_or("display orientation unavailable")?;
        self.rotation.store(rotation, Ordering::Relaxed);
        self.width.store(
            if rotation % 2 == 1 { 1440 } else { 720 },
            Ordering::Relaxed,
        );
        self.height.store(
            if rotation % 2 == 1 { 720 } else { 1440 },
            Ordering::Relaxed,
        );
        self.orientation_at.store(now(), Ordering::Relaxed);
        Ok(rotation)
    }
}
fn failure(error: impl std::fmt::Display) -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, error.to_string()).into_response()
}
async fn auth(State(media): State<Arc<Media>>, request: Request, next: Next) -> Response {
    if request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        != Some(media.config.token.as_str())
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}
async fn status(State(media): State<Arc<Media>>) -> Response {
    let _ = media.orientation().await;
    let verified = media.screen.verified.load(Ordering::Relaxed);
    axum::Json(serde_json::json!({"width":media.width.load(Ordering::Relaxed),"height":media.height.load(Ordering::Relaxed),"rotation":media.rotation.load(Ordering::Relaxed),"media":{"viewers":media.screen.viewers.load(Ordering::Relaxed),"sequence":media.screen.sequence.load(Ordering::Relaxed),"verifiedAgeMs":if verified>0{Some(now()-verified)}else{None}}})).into_response()
}
fn screen_response(body: Body) -> Response {
    (
        [
            ("content-type", "multipart/x-mixed-replace; boundary=phone"),
            ("cache-control", "private,no-store"),
        ],
        body,
    )
        .into_response()
}
fn audio_response(body: Body) -> Response {
    (
        [
            ("content-type", "audio/L16;rate=48000;channels=2"),
            ("cache-control", "private,no-store"),
            ("x-sample-format", "s16le"),
        ],
        body,
    )
        .into_response()
}
async fn screen(State(media): State<Arc<Media>>) -> Response {
    if let Err(error) = media.session() {
        return failure(error);
    }
    screen_response(media.screen.body())
}
async fn snapshot(State(media): State<Arc<Media>>) -> Response {
    match phone::snapshot(&media).await {
        Ok((bytes, captured)) => {
            let mut response = (
                [("content-type", "image/png"), ("cache-control", "no-store")],
                bytes,
            )
                .into_response();
            response
                .headers_mut()
                .insert("x-capture-time", captured.to_string().parse().unwrap());
            response
                .headers_mut()
                .insert("x-verified-time", now().to_string().parse().unwrap());
            response
        }
        Err(error) => failure(error),
    }
}
async fn audio(State(media): State<Arc<Media>>) -> Response {
    match phone::audio(&media).await {
        Ok(body) => audio_response(body),
        Err(error) => failure(error),
    }
}
async fn desktop_screen(State(media): State<Arc<Media>>) -> Response {
    screen_response(media.desktop.body())
}
async fn desktop_audio() -> Response {
    match desktop::audio().await {
        Ok(body) => audio_response(body),
        Err(error) => failure(error),
    }
}
async fn cursor(State(media): State<Arc<Media>>) -> Response {
    let mut value = media.cursor.lock().await.clone();
    value.reported_at_ms = now();
    (
        [
            ("content-type", "application/x-protobuf"),
            ("cache-control", "no-store"),
        ],
        value.encode_to_vec(),
    )
        .into_response()
}
async fn action(State(media): State<Arc<Media>>, bytes: Bytes) -> Response {
    let result: Result<()> = async {
        let value = wire::DeviceActionRequest::decode(bytes)?;
        if value.phone_id != media.config.device {
            return Err("wrong device".into());
        }
        actions::execute(&media, value).await
    }
    .await;
    match result {
        Ok(()) => StatusCode::OK.into_response(),
        Err(error) => {
            media.cursor.lock().await.status = "unknown".into();
            (
                StatusCode::BAD_GATEWAY,
                format!("action_outcome_unknown: {error}"),
            )
                .into_response()
        }
    }
}
pub async fn run() -> Result<()> {
    let root = std::env::var_os("AI_PHONE_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let runner = std::env::var_os("AI_PHONE_RUNNER_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("data/phone-runner"));
    let config = Config {
        device: std::env::var("AI_PHONE_DEVICE_ID").unwrap_or("phone-local-01".into()),
        serial: std::env::var("AI_PHONE_SERIAL").unwrap_or("emulator-5564".into()),
        grpc_port: std::env::var("AI_PHONE_GRPC_PORT")
            .unwrap_or("8558".into())
            .parse()?,
        port: std::env::var("AI_PHONE_MEDIA_PORT")
            .unwrap_or("17865".into())
            .parse()?,
        token: format!(
            "Bearer {}",
            std::fs::read_to_string(runner.join("token"))?.trim()
        ),
    };
    let port = config.port;
    let channel =
        Endpoint::from_shared(format!("http://127.0.0.1:{}", config.grpc_port))?.connect_lazy();
    let adb = ai_phone_adb::Client::new(15037, config.serial.clone())?;
    let media = Arc::new(Media {
        config,
        channel,
        adb,
        width: AtomicU32::new(720),
        height: AtomicU32::new(1440),
        rotation: AtomicU32::new(0),
        orientation_at: AtomicU64::new(0),
        orientation_lock: Mutex::new(()),
        screen: hub::Hub::new(),
        desktop: hub::Hub::new(),
        cursor: Mutex::new(wire::DeviceCursor::default()),
    });
    let phone = tokio::spawn(phone::capture(media.clone()));
    let desktop = tokio::spawn(desktop::capture(media.clone()));
    let router = Router::new()
        .route("/status", get(status))
        .route("/screen", get(screen))
        .route("/snapshot", get(snapshot))
        .route("/audio", get(audio))
        .route("/cursor", get(cursor))
        .route("/action", post(action))
        .route("/desktop/screen", get(desktop_screen))
        .route("/desktop/audio", get(desktop_audio))
        .layer(DefaultBodyLimit::max(32768))
        .layer(middleware::from_fn_with_state(media.clone(), auth))
        .with_state(media);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    eprintln!("Private native media ready on {port}");
    let result = axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    phone.abort();
    desktop.abort();
    result?;
    Ok(())
}
