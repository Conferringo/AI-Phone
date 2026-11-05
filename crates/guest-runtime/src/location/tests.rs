use super::*;
use axum::{
    Json,
    extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
    routing::get,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::sync::watch;

struct Chrome {
    pages: Mutex<Vec<String>>,
    connections: Mutex<HashMap<usize, Value>>,
    opened: AtomicUsize,
    queries: AtomicUsize,
    cleared: AtomicUsize,
    close: watch::Sender<usize>,
    websocket: String,
}

async fn list(State(chrome): State<Arc<Chrome>>) -> Json<Vec<Value>> {
    chrome.queries.fetch_add(1, Ordering::SeqCst);
    Json(
        chrome
            .pages
            .lock()
            .await
            .iter()
            .map(|id| json!({"id":id,"type":"page","webSocketDebuggerUrl":chrome.websocket}))
            .collect(),
    )
}

async fn socket(mut socket: WebSocket, chrome: Arc<Chrome>) {
    let id = chrome.opened.fetch_add(1, Ordering::SeqCst);
    chrome.connections.lock().await.insert(id, Value::Null);
    let mut close = chrome.close.subscribe();
    loop {
        tokio::select! {
            _=close.changed()=>break,
            message=socket.recv()=> {
                let Some(Ok(WsMessage::Text(text)))=message else { break; };
                let call:Value=serde_json::from_str(&text).unwrap();
                let point=if call["method"]=="Emulation.clearGeolocationOverride" {
                    chrome.cleared.fetch_add(1,Ordering::SeqCst); Value::Null
                } else { call["params"].clone() };
                chrome.connections.lock().await.insert(id,point);
                if socket.send(WsMessage::Text(json!({"id":call["id"],"result":{}}).to_string().into())).await.is_err() { break; }
            }
        }
    }
    chrome.connections.lock().await.remove(&id);
}

async fn chrome() -> (Arc<Chrome>, String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (close, _) = watch::channel(0);
    let state = Arc::new(Chrome {
        pages: Mutex::new(vec!["page".into()]),
        connections: Mutex::new(HashMap::new()),
        opened: AtomicUsize::new(0),
        queries: AtomicUsize::new(0),
        cleared: AtomicUsize::new(0),
        close,
        websocket: format!("ws://{address}/cdp"),
    });
    let router = Router::new()
        .route("/json/list", get(list))
        .route(
            "/cdp",
            get(
                |State(state): State<Arc<Chrome>>, upgrade: WebSocketUpgrade| async {
                    upgrade.on_upgrade(|ws| socket(ws, state))
                },
            ),
        )
        .with_state(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (state, format!("http://{address}/json/list"), server)
}

async fn service(
    folder: PathBuf,
    targets: String,
) -> (String, tokio::task::JoinHandle<Result<()>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/fix", listener.local_addr().unwrap());
    (endpoint, tokio::spawn(serve_on(listener, folder, targets)))
}

fn fix(latitude: f64) -> Vec<u8> {
    LocationFix {
        enabled: true,
        point: Some(GeoPoint {
            latitude,
            longitude: 121.0,
            accuracy_m: 0.0,
        }),
        speed_mps: 2.0,
    }
    .encode_to_vec()
}

async fn post(client: &reqwest::Client, endpoint: &str, bytes: Vec<u8>) -> LocationDevice {
    let response = client.post(endpoint).body(bytes).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/x-protobuf"
    );
    LocationDevice::decode(response.bytes().await.unwrap()).unwrap()
}

async fn wait_points(chrome: &Chrome, count: usize, latitude: f64) {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let points = chrome.connections.lock().await;
            if points.len() == count
                && points
                    .values()
                    .all(|p| p["latitude"].as_f64() == Some(latitude))
            {
                return;
            }
            drop(points);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[test]
fn input_bounds_preserve_enabled_and_disabled_contracts() {
    assert!(matches!(checked(&[]).unwrap(), Fix::Disabled));
    assert!(matches!(checked(&fix(90.0)).unwrap(), Fix::Enabled { .. }));
    for latitude in [f64::NAN, f64::INFINITY, 90.01] {
        assert!(checked(&fix(latitude)).is_err());
    }
    for speed in [-1.0, f64::NAN, 500.0 / 3.6 + 0.001] {
        assert!(
            checked(
                &LocationFix {
                    speed_mps: speed,
                    ..Default::default()
                }
                .encode_to_vec()
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn disabled_first_boot_and_invalid_requests_do_not_contact_browser_or_persist_input() {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path().join("state");
    let (chrome, targets, browser) = chrome().await;
    let (endpoint, server) = service(folder.clone(), targets).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    assert_eq!(
        client.get(&endpoint).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    for bytes in [
        LocationFix {
            enabled: true,
            ..Default::default()
        }
        .encode_to_vec(),
        vec![0; 4097],
    ] {
        let reply = post(&client, &endpoint, bytes).await;
        assert_eq!(reply.status, "failed");
        assert_eq!(reply.error, "位置坐标无效");
        assert!(!folder.join("state.pb").exists());
    }
    let reply = post(&client, &endpoint, vec![]).await;
    assert_eq!(reply.status, "disabled");
    assert_eq!(reply.kind, "desktop");
    assert!(reply.applied_at_ms > 0);
    assert_eq!(
        fs::metadata(&folder).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(folder.join("state.pb"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(chrome.queries.load(Ordering::SeqCst), 0);
    server.abort();
    browser.abort();
}

#[tokio::test]
async fn sessions_survive_movement_reconnect_include_new_tabs_and_close_on_disable() {
    let temporary = tempfile::tempdir().unwrap();
    let (chrome, targets, browser) = chrome().await;
    let (endpoint, server) = service(temporary.path().join("state"), targets).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    assert_eq!(post(&client, &endpoint, fix(31.0)).await.status, "applied");
    wait_points(&chrome, 1, 31.0).await;
    assert_eq!(post(&client, &endpoint, fix(32.0)).await.status, "applied");
    wait_points(&chrome, 1, 32.0).await;
    assert_eq!(chrome.opened.load(Ordering::SeqCst), 1);
    chrome.pages.lock().await.push("second".into());
    wait_points(&chrome, 2, 32.0).await;
    assert!(
        chrome
            .connections
            .lock()
            .await
            .values()
            .all(|p| p["accuracy"].as_f64() == Some(1.0) && p["speed"].as_f64() == Some(2.0))
    );
    chrome.close.send_replace(1);
    wait_points(&chrome, 2, 32.0).await;
    tokio::time::timeout(Duration::from_secs(4), async {
        while chrome.opened.load(Ordering::SeqCst) < 4 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(post(&client, &endpoint, vec![]).await.status, "disabled");
    wait_points(&chrome, 0, 0.0).await;
    assert!(chrome.cleared.load(Ordering::SeqCst) >= 2);
    server.abort();
    browser.abort();
}

#[tokio::test]
async fn saved_fix_recovers_on_restart_after_browser_unavailability() {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path().join("state");
    let (chrome, targets, browser) = chrome().await;
    chrome.pages.lock().await.clear();
    let (endpoint, server) = service(folder.clone(), targets.clone()).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let bytes = fix(33.0);
    assert_eq!(
        post(&client, &endpoint, bytes.clone()).await.status,
        "failed"
    );
    assert_eq!(fs::read(folder.join("state.pb")).unwrap(), bytes);
    server.abort();
    let _ = server.await;
    chrome.pages.lock().await.push("recovered".into());
    let (_, server) = service(folder, targets).await;
    wait_points(&chrome, 1, 33.0).await;
    server.abort();
    browser.abort();
}
