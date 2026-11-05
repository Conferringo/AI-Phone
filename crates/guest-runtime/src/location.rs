//! Guest location transport and persistence. The public wire format remains protobuf.
use crate::{Error, Result};
use ai_phone_api::wire::{GeoPoint, LocationDevice, LocationFix};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::{Method, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use prost::Message;
use std::{
    fs::{self, OpenOptions},
    future::IntoFuture,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{io::AsyncReadExt, net::TcpListener, sync::Mutex};

mod browser;
use browser::Browser;

const FOLDER: &str = "/home/ai/.config/ai-phone-location";
const ENDPOINT: &str = "http://127.0.0.1:17867/fix";
const TARGETS: &str = "http://127.0.0.1:9222/json/list";
const LIMIT: usize = 4096;
const BROWSER_ERROR: &str = "请打开电脑浏览器后重试，定位仅应用于内置浏览器";

#[derive(Clone, Copy, Debug)]
enum Fix {
    Disabled,
    Enabled { point: GeoPoint, speed: f64 },
}

fn checked(bytes: &[u8]) -> Result<Fix> {
    if bytes.len() > LIMIT {
        return Err("invalid_location".into());
    }
    let fix = LocationFix::decode(bytes)?;
    if !(0.0..=500.0 / 3.6).contains(&fix.speed_mps) {
        return Err("invalid_location".into());
    }
    if !fix.enabled {
        return Ok(Fix::Disabled);
    }
    let point = fix.point.ok_or("invalid_location")?;
    if !(-90.0..=90.0).contains(&point.latitude)
        || !(-180.0..=180.0).contains(&point.longitude)
        || !(0.0..=100000.0).contains(&point.accuracy_m)
    {
        return Err("invalid_location".into());
    }
    Ok(Fix::Enabled {
        point,
        speed: fix.speed_mps,
    })
}

fn receipt(status: &str, error: &str) -> Vec<u8> {
    LocationDevice {
        kind: "desktop".into(),
        status: status.into(),
        error: error.into(),
        applied_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow Unix epoch")
            .as_millis() as u64,
        ..Default::default()
    }
    .encode_to_vec()
}

fn failure(error: &Error) -> Vec<u8> {
    receipt(
        "failed",
        if error.to_string() == "invalid_location" {
            "位置坐标无效"
        } else {
            BROWSER_ERROR
        },
    )
}

pub async fn client() -> Vec<u8> {
    let result: Result<Vec<u8>> = async {
        let mut bytes = Vec::new();
        tokio::io::stdin()
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .await?;
        checked(&bytes)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(8))
            .build()?;
        Ok(client
            .post(ENDPOINT)
            .header(header::CONTENT_TYPE, "application/x-protobuf")
            .body(bytes)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec())
    }
    .await;
    match result {
        Ok(bytes) => bytes,
        Err(error) => failure(&error),
    }
}

struct Controller {
    folder: PathBuf,
    fix: Fix,
    browser: Browser,
}

fn remember(folder: &Path, bytes: &[u8]) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)?;
    let temporary = folder.join("state.pb.tmp");
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?
        .write_all(bytes)?;
    fs::rename(temporary, folder.join("state.pb"))?;
    Ok(())
}

async fn update(State(shared): State<Arc<Mutex<Controller>>>, request: Request<Body>) -> Response {
    if request.method() != Method::POST {
        return StatusCode::NOT_FOUND.into_response();
    }
    let result: Result<&str> = async {
        let bytes = to_bytes(request.into_body(), LIMIT)
            .await
            .map_err(|_| "invalid_location")?;
        let fix = checked(&bytes)?;
        let mut state = shared.lock().await;
        remember(&state.folder, &bytes)?;
        state.fix = fix;
        state.browser.apply(fix).await?;
        Ok(if matches!(fix, Fix::Disabled) {
            "disabled"
        } else {
            "applied"
        })
    }
    .await;
    let bytes = match result {
        Ok(status) => receipt(status, ""),
        Err(error) => failure(&error),
    };
    ([(header::CONTENT_TYPE, "application/x-protobuf")], bytes).into_response()
}

pub async fn serve() -> Result<()> {
    serve_on(
        TcpListener::bind("127.0.0.1:17867").await?,
        PathBuf::from(FOLDER),
        TARGETS.into(),
    )
    .await
}

async fn serve_on(listener: TcpListener, folder: PathBuf, targets: String) -> Result<()> {
    let fix = match fs::read(folder.join("state.pb")) {
        Ok(bytes) => checked(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Fix::Disabled,
        Err(error) => return Err(error.into()),
    };
    let state = Arc::new(Mutex::new(Controller {
        folder,
        fix,
        browser: Browser::new(targets)?,
    }));
    let router = Router::new()
        .route("/fix", any(update))
        .with_state(state.clone());
    let refresh = async move {
        let mut interval = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(1),
            Duration::from_secs(1),
        );
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Ok(mut state) = state.try_lock() {
                let fix = state.fix;
                if matches!(fix, Fix::Enabled { .. }) {
                    let _ = state.browser.apply(fix).await;
                }
            }
        }
    };
    tokio::select! { result=axum::serve(listener,router).into_future()=>Ok(result?), ()=refresh=>Ok(()) }
}

#[cfg(test)]
mod tests;
