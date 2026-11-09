use ai_phone_api::wire;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State, rejection::BytesRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::sync::Mutex;
use uuid::Uuid;
mod action_store;
mod cursor;
mod gestures;
mod grounding;
mod lan;
mod location;
mod media;
mod native_tree;

const OWNER: &str = "local-user";
const MEDIA: &str = "application/x-protobuf";
const UI_TREE_BUDGET: Duration = Duration::from_millis(2800);

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    hash: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    gesture: Vec<u32>,
    completed: bool,
    success: bool,
    output: String,
    unknown: bool,
}
#[derive(Serialize, Deserialize)]
struct Ledger {
    #[serde(default)]
    archive_version: u32,
    epoch: u64,
    controller: String,
    sequence: u64,
    #[serde(default)]
    revision: u64,
    // Old binaries require `actions`; omit it so an unsafe downgrade fails closed.
    #[serde(default, alias = "actions", rename = "cached_actions")]
    actions: BTreeMap<String, Record>,
}
impl Default for Ledger {
    fn default() -> Self {
        Self {
            archive_version: 1,
            epoch: 1,
            controller: "manual".into(),
            sequence: 0,
            revision: 0,
            actions: BTreeMap::new(),
        }
    }
}
struct Runner {
    phone_id: String,
    media_url: String,
    adb: ai_phone_adb::Client,
    token: String,
    ledger_path: PathBuf,
    ledger: Mutex<Ledger>,
    tree_lock: Mutex<()>,
    cursor: Mutex<wire::DeviceCursor>,
    root_ready: AtomicBool,
    width: AtomicU32,
    height: AtomicU32,
    local: reqwest::Client,
    vision: PathBuf,
    grounding: Mutex<grounding::GroundingState>,
}

fn pb(status: StatusCode, value: impl Message) -> Response {
    (status, [("content-type", MEDIA)], value.encode_to_vec()).into_response()
}
fn err(status: StatusCode, code: &str, message: &str) -> Response {
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
    Rejection(Box::new(err(status, code, message)))
}
fn request<T: Message + Default>(
    runner: &Runner,
    headers: &HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<T, Rejection> {
    let expected = format!("Bearer {}", runner.token);
    if headers.get("authorization").and_then(|s| s.to_str().ok()) != Some(expected.as_str()) {
        return Err(rejected(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Runner pairing token required",
        ));
    }
    if headers.get("content-type").and_then(|s| s.to_str().ok()) != Some(MEDIA) {
        return Err(rejected(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            MEDIA,
        ));
    }
    let bytes = body.map_err(|e| {
        rejected(
            e.status(),
            "body_rejected",
            "Request too large or unreadable",
        )
    })?;
    T::decode(bytes).map_err(|_| {
        rejected(
            StatusCode::BAD_REQUEST,
            "invalid_protobuf",
            "Invalid protobuf message",
        )
    })
}
fn scope(runner: &Runner, phone: &str, owner: &str) -> Result<(), Rejection> {
    if phone != runner.phone_id || owner != OWNER {
        return Err(rejected(
            StatusCode::FORBIDDEN,
            "wrong_scope",
            "Phone does not belong to the local owner",
        ));
    }
    Ok(())
}
fn save(runner: &Runner, ledger: &Ledger) -> Result<(), String> {
    let tmp = runner.ledger_path.with_extension("tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec(ledger).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(tmp, &runner.ledger_path).map_err(|e| e.to_string())
}

async fn adb(runner: &Runner, args: Vec<String>) -> Result<(bool, Vec<u8>, String), String> {
    use ai_phone_adb::{Error, Limits, Output};
    let deadline = Duration::from_secs(20);
    let result = match args.as_slice() {
        [kind, command @ ..] if kind == "shell" && !command.is_empty() => {
            runner
                .adb
                .shell(
                    &command.join(" "),
                    Limits {
                        stdout: 2_097_152,
                        stderr: 65_536,
                    },
                    deadline,
                )
                .await
        }
        [kind, command, arguments @ ..] if kind == "exec-out" => {
            let service = format!(
                "exec:{}{}",
                command,
                arguments
                    .iter()
                    .map(|value| format!(" {}", guest_quote(value)))
                    .collect::<String>()
            );
            runner
                .adb
                .raw(&service, 2_097_152, deadline)
                .await
                .map(|stdout| Output {
                    code: 0,
                    stdout,
                    stderr: vec![],
                })
        }
        [kind] if kind == "get-state" => runner.adb.state(128, deadline).await.map(|mut stdout| {
            stdout.push(b'\n');
            Output {
                code: 0,
                stdout,
                stderr: vec![],
            }
        }),
        [kind] if kind == "root" => runner.adb.raw("root:", 4096, deadline).await.map(|stdout| {
            let failed = stdout.windows(6).any(|part| part == b"cannot");
            Output {
                code: u8::from(failed),
                stdout,
                stderr: vec![],
            }
        }),
        _ => return Err("Unsupported internal ADB command".into()),
    };
    match result {
        Ok(output) => Ok((
            output.code == 0,
            output.stdout,
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )),
        Err(Error::Rejected(message)) => Ok((false, vec![], message)),
        Err(error) => Err(error.to_string()),
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_string()).collect()
}
fn guest_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn ui_tree_command(path: &str, marker: &str) -> Vec<String> {
    // Both names are generated locally. The guest deadline also kills uiautomator's
    // process group if the ADB connection disappears before the host deadline.
    args(&[
        "shell",
        &format!(
            "tree={}; marker={}; trap 'rm -f \"$tree\"' EXIT; trap 'exit 143' HUP INT TERM; dump=\"$(timeout -s KILL 2.4 uiautomator dump \"$tree\")\" || exit $?; test -s \"$tree\" || exit 65; printf '%s\\n%s\\n' \"$dump\" \"$marker\"; cat \"$tree\"",
            guest_quote(path),
            guest_quote(marker)
        ),
    ])
}

fn ui_tree_from_output(output: &[u8], path: &str, marker: &str) -> Option<String> {
    let output = std::str::from_utf8(output).ok()?;
    let (dump, xml) = output.split_once(&format!("\n{marker}\n"))?;
    let confirmed_path = dump.lines().any(|line| {
        ["UI hierchary dumped to: ", "UI hierarchy dumped to: "]
            .iter()
            .any(|prefix| line.trim().strip_prefix(prefix) == Some(path))
    });
    if !confirmed_path {
        return None;
    }
    let xml = xml.trim();
    let root = if xml.starts_with("<?xml ") {
        xml.split_once("?>")?.1.trim_start()
    } else {
        xml
    };
    if !(root.starts_with("<hierarchy ") || root.starts_with("<hierarchy>"))
        || !root.ends_with("</hierarchy>")
    {
        return None;
    }
    Some(xml.to_string())
}

async fn capture_ui_tree(runner: &Runner) -> String {
    // Include lock contention in the budget: parallel observers must not queue
    // repeated idle waits and delay a usable screenshot indefinitely.
    tokio::time::timeout(UI_TREE_BUDGET, async {
        let _tree = runner.tree_lock.lock().await;
        capture_tree_sources(runner).await
    })
    .await
    .unwrap_or_default()
}

// The caller serializes requests and owns their end-to-end deadline.
async fn capture_tree_sources(runner: &Runner) -> String {
    let capture = Uuid::new_v4().simple().to_string();
    if let Ok((true, output, _)) = adb(runner, native_tree::command(&capture)).await
        && let Some(xml) = native_tree::from_output(&output, &capture)
    {
        return xml;
    }
    let path = format!("/data/local/tmp/ai-phone-window-{capture}.xml");
    let marker = format!("__AI_PHONE_UI_TREE_{capture}__");
    match adb(runner, ui_tree_command(&path, &marker)).await {
        Ok((true, output, _)) => ui_tree_from_output(&output, &path, &marker).unwrap_or_default(),
        _ => String::new(),
    }
}

async fn dimensions(runner: &Runner) -> (u32, u32) {
    if let Ok(response) = runner
        .local
        .get(format!("{}/status", runner.media_url))
        .timeout(Duration::from_millis(300))
        .bearer_auth(&runner.token)
        .send()
        .await
        && let Ok(bytes) = response.bytes().await
        && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let (Some(width), Some(height)) = (value["width"].as_u64(), value["height"].as_u64())
        && matches!((width, height), (720, 1440) | (1440, 720))
    {
        runner.width.store(width as u32, Ordering::Release);
        runner.height.store(height as u32, Ordering::Release);
    }
    (
        runner.width.load(Ordering::Acquire),
        runner.height.load(Ordering::Acquire),
    )
}
async fn notifications(
    State(runner): State<Arc<Runner>>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let input: wire::DeviceRequest = match request(&runner, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    if input.phone_id != runner.phone_id || input.owner_id != OWNER {
        return err(StatusCode::FORBIDDEN, "scope", "Unknown phone");
    }
    let mut output = wire::PhoneNotificationList::default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let read = |file| {
        adb(
            &runner,
            vec![
                "exec-out".into(),
                "cat".into(),
                format!("/data/user/0/dev.aiphone.input/files/{file}"),
            ],
        )
    };
    let (notifications, attention) =
        tokio::join!(read("notifications.json"), read("attention.json"));
    for (result, attention) in [(notifications, false), (attention, true)] {
        if let Ok((true, bytes, _)) = result
            && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            let checked = value["checked_at_ms"].as_u64().unwrap_or(0);
            let connected = value["connected"].as_bool().unwrap_or(false)
                && now.saturating_sub(checked) < 90000;
            if attention {
                output.attention_connected = connected;
            } else {
                output.connected = connected;
                output.checked_at_ms = checked;
            }
            if let Some(events) = value["notifications"].as_array() {
                for event in events.iter().take(if attention { 80 } else { 160 }) {
                    let string = |key: &str, max: usize| {
                        event[key]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(max)
                            .collect()
                    };
                    output.notifications.push(wire::PhoneNotification {
                        event_id: string("event_id", 64),
                        notification_key: string("notification_key", 1024),
                        package_name: string("package_name", 200),
                        app_name: string("app_name", 200),
                        title: string("title", 500),
                        text: string("text", 4000),
                        posted_at_ms: event["posted_at_ms"].as_u64().unwrap_or(0),
                    });
                }
            }
        }
    }
    output.notifications.sort_by_key(|event| event.posted_at_ms);
    if !output.connected {
        output.error = "通知监听已断开".into();
    }
    pb(StatusCode::OK, output)
}

async fn capture_frame(runner: &Runner) -> Result<(Vec<u8>, u32, u32), String> {
    let mut png = Vec::new();
    let (width, height);
    if let Ok(response) = runner
        .local
        .get(format!("{}/snapshot", runner.media_url))
        .timeout(Duration::from_secs(2))
        .bearer_auth(&runner.token)
        .send()
        .await
        && response.status().is_success()
    {
        png = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
    }
    if png.is_empty() {
        let (ok, data, errors) = adb(runner, args(&["exec-out", "screencap", "-p"])).await?;
        if !ok {
            return Err(errors);
        }
        png = data;
    }
    if png.len() < 24 || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Cannot capture device".into());
    }
    width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    runner.width.store(width, Ordering::Release);
    runner.height.store(height, Ordering::Release);
    Ok((png, width, height))
}

async fn observe(
    State(runner): State<Arc<Runner>>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let request: wire::DeviceRequest = match request(&runner, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    if let Err(e) = scope(&runner, &request.phone_id, &request.owner_id) {
        return e.into();
    }
    if request.include_elements {
        return match grounding::observe(&runner, &request).await {
            Ok(value) => pb(StatusCode::OK, value),
            Err(message) => err(StatusCode::CONFLICT, "grounding_unavailable", &message),
        };
    }
    let result: Result<wire::DeviceObservation, String> = async {
        // A tree dump may wait for accessibility to become idle. Capture the PNG
        // afterwards so the image does not predate the tree by that idle wait.
        // An unavailable tree is optional evidence, never a stale XML fallback.
        let ui_tree = if request.include_ui_tree {
            capture_ui_tree(&runner).await
        } else {
            String::new()
        };
        let (png, width, height) = if request.metadata_only {
            let (width, height) = dimensions(&runner).await;
            (Vec::new(), width, height)
        } else {
            capture_frame(&runner).await?
        };
        let mut ledger = runner.ledger.lock().await;
        ledger.sequence += 1;
        Ok(wire::DeviceObservation {
            phone_id: runner.phone_id.clone(),
            width,
            height,
            png,
            ui_tree,
            screen_sequence: ledger.sequence,
            control_epoch: ledger.epoch,
            controller: ledger.controller.clone(),
            root_identity: if runner.root_ready.load(Ordering::Acquire) {
                "0".into()
            } else {
                String::new()
            },
            ..Default::default()
        })
    }
    .await;
    match result {
        Ok(observation) => pb(StatusCode::OK, observation),
        Err(message) => err(
            StatusCode::SERVICE_UNAVAILABLE,
            "device_unavailable",
            &message,
        ),
    }
}

fn orientation_command(value: &wire::OrientationAction) -> Option<Vec<String>> {
    if value.rotation > 3 {
        return None;
    }
    let automatic = u8::from(value.automatic);
    Some(args(&[
        "shell",
        &format!(
            "settings put system user_rotation {} && settings put system accelerometer_rotation {} && settings get system accelerometer_rotation && settings get system user_rotation",
            value.rotation, automatic
        ),
    ]))
}
fn parse_orientation(bytes: &[u8]) -> Option<wire::OrientationAction> {
    let text = std::str::from_utf8(bytes).ok()?;
    let values: Vec<_> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if values.len() != 2 {
        return None;
    }
    let automatic = values[0].trim().parse::<u32>().ok()?;
    let rotation = values[1].trim().parse::<u32>().ok()?;
    if automatic > 1 || rotation > 3 {
        return None;
    }
    Some(wire::OrientationAction {
        automatic: automatic == 1,
        rotation,
    })
}
async fn read_orientation(
    State(runner): State<Arc<Runner>>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let input: wire::DeviceRequest = match request(&runner, &headers, body) {
        Ok(value) => value,
        Err(error) => return error.into(),
    };
    if let Err(error) = scope(&runner, &input.phone_id, &input.owner_id) {
        return error.into();
    }
    match adb(
        &runner,
        args(&[
            "shell",
            "settings get system accelerometer_rotation; settings get system user_rotation",
        ]),
    )
    .await
    {
        Ok((true, bytes, _)) => match parse_orientation(&bytes) {
            Some(value) => pb(StatusCode::OK, value),
            None => err(
                StatusCode::SERVICE_UNAVAILABLE,
                "orientation_unavailable",
                "无法读取 Android 屏幕旋转设置",
            ),
        },
        _ => err(
            StatusCode::SERVICE_UNAVAILABLE,
            "orientation_unavailable",
            "无法读取 Android 屏幕旋转设置",
        ),
    }
}

async fn take_control(
    State(runner): State<Arc<Runner>>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let request: wire::TakeControlRequest = match request(&runner, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    if let Err(e) = scope(&runner, &request.phone_id, &request.owner_id) {
        return e.into();
    }
    if !["manual", "agent"].contains(&request.controller.as_str()) {
        return err(
            StatusCode::BAD_REQUEST,
            "invalid_controller",
            "Choose manual or agent",
        );
    }
    let mut ledger = runner.ledger.lock().await;
    ledger.epoch += 1;
    ledger.controller = request.controller;
    if let Err(e) = save(&runner, &ledger) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "persistence_failed", &e);
    }
    pb(
        StatusCode::OK,
        wire::ControlState {
            phone_id: runner.phone_id.clone(),
            control_epoch: ledger.epoch,
            controller: ledger.controller.clone(),
        },
    )
}

async fn action(
    State(runner): State<Arc<Runner>>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let mut request: wire::DeviceActionRequest = match request(&runner, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    if let Err(e) = scope(&runner, &request.phone_id, &request.owner_id) {
        return e.into();
    }
    if !valid_id(&request.run_id) || !valid_id(&request.action_id) {
        return err(
            StatusCode::BAD_REQUEST,
            "invalid_identity",
            "run_id and action_id must be bounded identifiers",
        );
    }
    let hash = format!("{:x}", Sha256::digest(request.encode_to_vec()));
    let mut ledger = runner.ledger.lock().await;
    let old = match ledger.actions.get(&request.action_id).cloned() {
        Some(value) => Some(value),
        None => match action_store::read(&runner.ledger_path, &request.action_id) {
            Ok(value) => value,
            Err(message) => {
                return err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "persistence_failed",
                    &message,
                );
            }
        },
    };
    if let Some(old) = old {
        if old.hash != hash {
            return err(
                StatusCode::CONFLICT,
                "action_id_conflict",
                "action_id already belongs to another request",
            );
        }
        return pb(
            StatusCode::OK,
            wire::DeviceActionResponse {
                action_id: request.action_id,
                success: old.completed && old.success,
                output: if old.completed {
                    old.output.clone()
                } else {
                    "Previous outcome unknown; observe before deciding next action".into()
                },
                replayed: true,
                result_unknown: !old.completed || old.unknown,
            },
        );
    }
    if request.control_epoch != ledger.epoch || request.controller != ledger.controller {
        return err(
            StatusCode::CONFLICT,
            "stale_control",
            "Control was transferred; reacquire before acting",
        );
    }
    let mut scroll_attempt = None;
    match grounding::resolve(&runner, &request, ledger.epoch, ledger.revision).await {
        Ok(Some((resolved, scroll))) => {
            request.action = Some(resolved);
            scroll_attempt = scroll;
        }
        Ok(None) => {}
        Err(message) => return err(StatusCode::CONFLICT, "target_rejected", &message),
    }
    use wire::device_action_request::Action;
    let (kind, gesture) = match request.action.as_ref() {
        Some(Action::Tap(v)) => ("tap", vec![v.x, v.y]),
        Some(Action::Swipe(v)) => ("swipe", vec![v.x1, v.y1, v.x2, v.y2, v.duration_ms]),
        Some(Action::Drag(v)) => ("drag", vec![v.x1, v.y1, v.x2, v.y2, v.duration_ms]),
        Some(Action::LongPress(v)) => ("long_press", vec![v.x, v.y, v.duration_ms]),
        Some(Action::Gesture(v)) => ("gesture", vec![v.paths.len() as u32, v.duration_ms]),
        Some(Action::Motion(v)) => ("motion", vec![v.duration_ms]),
        Some(Action::Input(_)) => ("input", vec![]),
        Some(Action::Location(_)) => ("location", vec![]),
        Some(Action::Shell(_)) => ("shell", vec![]),
        Some(Action::Key(_)) => ("key", vec![]),
        Some(Action::Orientation(value)) => (
            "orientation",
            vec![u32::from(value.automatic), value.rotation],
        ),
        _ => ("", vec![]),
    };
    let (width, height) = dimensions(&runner).await;
    let complex = matches!(request.action, Some(Action::Gesture(_) | Action::Motion(_)));
    let fast_touch = complex
        || (((width == 720 && height == 1440) || (width == 1440 && height == 720))
            && matches!(
                request.action,
                Some(Action::Tap(_) | Action::Swipe(_) | Action::Drag(_) | Action::LongPress(_))
            ));
    let fast_body = fast_touch.then(|| request.encode_to_vec());
    let expected_orientation = match request.action.as_ref() {
        Some(Action::Orientation(value)) => Some(*value),
        _ => None,
    };
    let cursor_state = cursor::accepted(&request, width, height);
    let mut is_input = false;
    let command = match request.action {
        Some(Action::Location(v)) if location::command(&v).is_some() => {
            location::command(&v).unwrap()
        }
        Some(Action::Gesture(v)) if gestures::valid(&v, width, height) => vec![],
        Some(Action::Motion(v)) if gestures::motion(&v) => vec![],
        Some(Action::Orientation(value)) if value.rotation <= 3 => {
            orientation_command(&value).expect("bounded rotation checked")
        }
        Some(Action::Tap(v)) if v.x < width && v.y < height => {
            args(&["shell", "input", "tap", &v.x.to_string(), &v.y.to_string()])
        }
        Some(Action::Swipe(v))
            if v.x1 < width
                && v.x2 < width
                && v.y1 < height
                && v.y2 < height
                && (50..=2000).contains(&v.duration_ms) =>
        {
            args(&[
                "shell",
                "input",
                "swipe",
                &v.x1.to_string(),
                &v.y1.to_string(),
                &v.x2.to_string(),
                &v.y2.to_string(),
                &v.duration_ms.to_string(),
            ])
        }
        Some(Action::LongPress(v))
            if v.x < width && v.y < height && (500..=2000).contains(&v.duration_ms) =>
        {
            args(&[
                "shell",
                "input",
                "swipe",
                &v.x.to_string(),
                &v.y.to_string(),
                &v.x.to_string(),
                &v.y.to_string(),
                &v.duration_ms.to_string(),
            ])
        }
        Some(Action::Drag(v))
            if v.x1 < width
                && v.x2 < width
                && v.y1 < height
                && v.y2 < height
                && (100..=2000).contains(&v.duration_ms) =>
        {
            args(&[
                "shell",
                "input",
                "draganddrop",
                &v.x1.to_string(),
                &v.y1.to_string(),
                &v.x2.to_string(),
                &v.y2.to_string(),
                &v.duration_ms.max(1000).to_string(),
            ])
        }
        Some(Action::Key(v)) if v.key == "SELECT_ALL" => args(&[
            "shell",
            "input",
            "keycombination",
            "KEYCODE_CTRL_LEFT",
            "KEYCODE_A",
        ]),
        Some(Action::Key(v))
            if [
                "HOME",
                "BACK",
                "APP_SWITCH",
                "ENTER",
                "DEL",
                "WAKEUP",
                "FORWARD_DEL",
                "TAB",
                "DPAD_LEFT",
                "DPAD_RIGHT",
                "DPAD_UP",
                "DPAD_DOWN",
            ]
            .contains(&v.key.as_str()) =>
        {
            args(&["shell", "input", "keyevent", &format!("KEYCODE_{}", v.key)])
        }
        Some(Action::Input(v)) if !v.text.is_empty() && v.text.len() <= 16000 => {
            is_input = true;
            args(&[
                "shell",
                "am",
                "broadcast",
                "--receiver-foreground",
                "-a",
                "dev.aiphone.input.INPUT",
                "-p",
                "dev.aiphone.input",
                "--es",
                "text_b64",
                &STANDARD.encode(v.text),
                "--es",
                "action_id",
                &request.action_id,
            ])
        }
        Some(Action::Shell(v)) if !v.script.is_empty() && v.script.len() <= 4096 => args(&[
            "shell",
            &format!("timeout 12 sh -c {}", guest_quote(&v.script)),
        ]),
        _ => {
            return err(
                StatusCode::BAD_REQUEST,
                "invalid_action",
                "Invalid action, coordinates or limit",
            );
        }
    };
    if kind != "location" {
        ledger.revision = ledger.revision.saturating_add(1);
    }
    ledger.actions.insert(
        request.action_id.clone(),
        Record {
            hash,
            kind: kind.into(),
            gesture,
            ..Default::default()
        },
    );
    if let Err(e) = action_store::write(
        &runner.ledger_path,
        &request.action_id,
        &ledger.actions[&request.action_id],
    ) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "persistence_failed", &e);
    }
    if let Err(e) = save(&runner, &ledger) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "persistence_failed", &e);
    }
    if kind != "location" {
        *runner.cursor.lock().await = cursor_state;
    }
    let execution = async {
        if is_input {
            let (ok, selected, _) = adb(
                &runner,
                args(&["shell", "settings", "get", "secure", "default_input_method"]),
            )
            .await?;
            if !ok || String::from_utf8_lossy(&selected).trim() != "dev.aiphone.input/.UnicodeIme" {
                let (ok, _, errors) = adb(
                    &runner,
                    args(&["shell", "ime", "set", "dev.aiphone.input/.UnicodeIme"]),
                )
                .await?;
                if !ok {
                    return Err(format!("Cannot activate Unicode IME: {errors}"));
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
        if let Some(fast_body) = fast_body {
            match runner
                .local
                .post(format!("{}/action", runner.media_url))
                .bearer_auth(&runner.token)
                .header("content-type", MEDIA)
                .body(fast_body)
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    return Ok((true, Vec::new(), String::new()));
                }
                Ok(_) => return Err("Touch result unknown; observe before another action".into()),
                Err(error) if error.is_connect() && !complex => {}
                Err(_) => return Err("Touch result unknown; observe before another action".into()),
            }
        }
        let result = adb(&runner, command).await?;
        if let Some(expected) = expected_orientation
            && (!result.0 || parse_orientation(&result.1).as_ref() != Some(&expected))
        {
            return Err("屏幕旋转结果暂未核实，请读取当前设置后重试".into());
        }
        Ok(result)
    }
    .await;
    let (success, output, unknown) = match execution {
        Ok((success, output, errors)) => {
            let output = format!("{}{}", String::from_utf8_lossy(&output), errors);
            (
                success && (!is_input || output.contains("result=1")),
                output.chars().take(65536).collect::<String>(),
                false,
            )
        }
        Err(message) => (false, message, true),
    };
    if kind != "location" {
        runner.cursor.lock().await.status = if success { "completed" } else { "unknown" }.into();
    }
    grounding::record_scroll(&runner, scroll_attempt, success && !unknown).await;
    let mut record = ledger.actions[&request.action_id].clone();
    record.completed = true;
    record.success = success;
    record.output = output.clone();
    record.unknown = unknown;
    if let Err(e) = action_store::write(&runner.ledger_path, &request.action_id, &record) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "persistence_failed", &e);
    }
    ledger.actions.insert(request.action_id.clone(), record);
    action_store::trim(&mut ledger);
    if let Err(e) = save(&runner, &ledger) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "persistence_failed", &e);
    }
    pb(
        StatusCode::OK,
        wire::DeviceActionResponse {
            action_id: request.action_id,
            success,
            output,
            replayed: false,
            result_unknown: unknown,
        },
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("--lan") => {
            return lan::run()
                .await
                .map_err(|e| -> Box<dyn std::error::Error> { e });
        }
        Some("--media") => {
            return media::run()
                .await
                .map_err(|e| -> Box<dyn std::error::Error> { e });
        }
        Some("--media-contract") => {
            println!("{}", media::CONTRACT);
            return Ok(());
        }
        _ => {}
    }
    if std::env::args().any(|arg| arg == "--version") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let root = std::env::var_os("AI_PHONE_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let dir = std::env::var_os("AI_PHONE_RUNNER_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("data/phone-runner"));
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    let token = std::fs::read_to_string(dir.join("token"))?
        .trim()
        .to_string();
    if token.len() < 32 {
        return Err("Pairing token is too short".into());
    }
    let ledger_path = dir.join("ledger.json");
    let _ledger_lock = action_store::lock(&ledger_path).map_err(std::io::Error::other)?;
    let mut ledger: Ledger = if ledger_path.exists() {
        serde_json::from_slice(&std::fs::read(&ledger_path)?)?
    } else {
        Ledger::default()
    };
    action_store::migrate(&ledger_path, &mut ledger).map_err(std::io::Error::other)?;
    // Every runner incarnation invalidates outstanding control handles.
    ledger.epoch += 1;
    ledger.controller = "manual".into();
    let runner = Arc::new(Runner {
        phone_id: std::env::var("AI_PHONE_DEVICE_ID").unwrap_or_else(|_| "phone-local-01".into()),
        media_url: std::env::var("AI_PHONE_MEDIA_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:17865".into()),
        adb: ai_phone_adb::Client::new(
            15037,
            std::env::var("AI_PHONE_SERIAL").unwrap_or_else(|_| "emulator-5564".into()),
        )?,
        token,
        vision: ai_phone_vision::program(&root),
        grounding: Mutex::new(grounding::GroundingState::default()),
        ledger_path,
        ledger: Mutex::new(ledger),
        tree_lock: Mutex::new(()),
        cursor: Mutex::new(wire::DeviceCursor::default()),
        root_ready: AtomicBool::new(false),
        width: AtomicU32::new(720),
        height: AtomicU32::new(1440),
        local: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(20))
            .build()?,
    });
    save(&runner, &*runner.ledger.lock().await).map_err(std::io::Error::other)?;
    let monitor = runner.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let Ok(_guard) = monitor.ledger.try_lock() else {
                continue;
            };
            let _ = tokio::time::timeout(Duration::from_secs(3), async {
                let Ok((true, state, _)) = adb(&monitor, args(&["get-state"])).await else {
                    monitor.root_ready.store(false, Ordering::Release);
                    return;
                };
                if String::from_utf8_lossy(&state).trim() != "device" {
                    monitor.root_ready.store(false, Ordering::Release);
                    return;
                }
                let Ok((true, boot, _)) =
                    adb(&monitor, args(&["shell", "getprop", "sys.boot_completed"])).await
                else {
                    return;
                };
                if String::from_utf8_lossy(&boot).trim() != "1" {
                    monitor.root_ready.store(false, Ordering::Release);
                    return;
                }
                let Ok((true, uid, _)) = adb(&monitor, args(&["shell", "id", "-u"])).await else {
                    return;
                };
                monitor.root_ready.store(
                    String::from_utf8_lossy(&uid).trim() == "0",
                    Ordering::Release,
                );
                if String::from_utf8_lossy(&uid).trim() != "0" {
                    let _ = adb(&monitor, args(&["root"])).await;
                }
            })
            .await;
        }
    });
    let app = Router::new()
        .route("/v1/observe", post(observe))
        .route("/v1/notifications", post(notifications))
        .route("/v1/orientation", post(read_orientation))
        .route("/v1/cursor", post(cursor::read))
        .route("/v1/action", post(action))
        .route("/v1/control", post(take_control))
        .layer(DefaultBodyLimit::max(32768))
        .with_state(runner);
    let listener = tokio::net::TcpListener::bind(format!(
        "127.0.0.1:{}",
        std::env::var("AI_PHONE_RUNNER_PORT").unwrap_or_else(|_| "17862".into())
    ))
    .await?;
    println!("AI Phone runner ready on loopback");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    use std::{process::Stdio, time::Instant};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        process::Command,
    };

    const XML: &str = "<?xml version='1.0' encoding='UTF-8'?><hierarchy rotation=\"0\"><node text=\"current\" /></hierarchy>";

    struct Fixture(PathBuf, std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("ai-phone-observe-{}", Uuid::new_v4()));
            std::fs::create_dir(&path).unwrap();
            Self(path, std::sync::Mutex::new(vec![]))
        }
        fn executable(&self, name: &str, source: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, source).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            path
        }
        fn runner(&self, source: &str) -> Runner {
            Runner {
                phone_id: "phone-local-01".into(),
                media_url: "http://127.0.0.1:0".into(),
                adb: self.adb_server(source),
                token: "test-only".into(),
                vision: self.0.join("unavailable-ocr"),
                grounding: Mutex::new(grounding::GroundingState::default()),
                ledger_path: self.0.join("ledger.json"),
                ledger: Mutex::new(Ledger::default()),
                tree_lock: Mutex::new(()),
                cursor: Mutex::new(wire::DeviceCursor::default()),
                root_ready: AtomicBool::new(true),
                width: AtomicU32::new(720),
                height: AtomicU32::new(1440),
                local: reqwest::Client::builder().no_proxy().build().unwrap(),
            }
        }
        fn adb_server(&self, source: &str) -> ai_phone_adb::Client {
            let program = self.executable("adb-fixture", source);
            let socket = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            let port = socket.local_addr().unwrap().port();
            socket.set_nonblocking(true).unwrap();
            let listener = tokio::net::TcpListener::from_std(socket).unwrap();
            let task = tokio::spawn(async move {
                let mut workers = tokio::task::JoinSet::new();
                loop {
                    tokio::select! {
                        accepted=listener.accept()=>{
                            let Ok((socket,_))=accepted else { break };
                            let program=program.clone();workers.spawn(fake_adb(socket,program));
                        }
                        _=workers.join_next(),if !workers.is_empty()=>{}
                    }
                }
            });
            self.1.lock().unwrap().push(task);
            ai_phone_adb::Client::new(port, "emulator-5564".into()).unwrap()
        }
    }
    async fn fake_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut header = [0; 4];
        socket.read_exact(&mut header).await.unwrap();
        let length = usize::from_str_radix(std::str::from_utf8(&header).unwrap(), 16).unwrap();
        let mut body = vec![0; length];
        socket.read_exact(&mut body).await.unwrap();
        socket.write_all(b"OKAY").await.unwrap();
        String::from_utf8(body).unwrap()
    }
    async fn fake_packet(socket: &mut tokio::net::TcpStream, id: u8, body: &[u8]) {
        socket.write_u8(id).await.unwrap();
        socket.write_u32_le(body.len() as u32).await.unwrap();
        socket.write_all(body).await.unwrap();
    }
    async fn fake_adb(mut socket: tokio::net::TcpStream, program: PathBuf) {
        assert_eq!(
            fake_request(&mut socket).await,
            "host:transport:emulator-5564"
        );
        let service = fake_request(&mut socket).await;
        let text = service.strip_prefix("shell,v2,raw:").unwrap();
        let mut close = [0; 5];
        socket.read_exact(&mut close).await.unwrap();
        assert_eq!(close, [4, 0, 0, 0, 0]);
        let mut command = Command::new(program);
        command
            .args(["-P", "15037", "-s", "emulator-5564", "shell", text])
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut byte = [0];
        let output = tokio::select! {
            result=command.output()=>result.unwrap(),
            _=socket.read(&mut byte)=>return,
        };
        fake_packet(&mut socket, 1, &output.stdout).await;
        fake_packet(&mut socket, 2, &output.stderr).await;
        fake_packet(&mut socket, 3, &[output.status.code().unwrap_or(255) as u8]).await;
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            for task in self.1.get_mut().unwrap().drain(..) {
                task.abort();
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn location_updates_keep_screen_grounding_and_touch_cursor_and_are_idempotent() {
        let fixture = Fixture::new();
        let calls = fixture.0.join("location-calls");
        let mut device = fixture.runner(&format!(
            "#!/bin/sh\nprintf 'call\\n' >> '{}'\nprintf 'location-applied'\n",
            calls.display()
        ));
        device.media_url = "http://127.0.0.1:0".into();
        let runner = Arc::new(device);
        runner.ledger.lock().await.revision = 42;
        runner.cursor.lock().await.kind = "tap".into();
        runner.cursor.lock().await.status = "completed".into();
        let request = wire::DeviceActionRequest {
            phone_id: "phone-local-01".into(),
            owner_id: OWNER.into(),
            run_id: "location-test".into(),
            action_id: Uuid::new_v4().to_string(),
            control_epoch: 1,
            controller: "manual".into(),
            action: Some(wire::device_action_request::Action::Location(
                wire::LocationFix {
                    enabled: true,
                    point: Some(wire::GeoPoint {
                        latitude: 31.,
                        longitude: 121.,
                        accuracy_m: 5.,
                    }),
                    ..Default::default()
                },
            )),
            ..Default::default()
        };
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-only".parse().unwrap());
        headers.insert("content-type", MEDIA.parse().unwrap());
        for replayed in [false, true] {
            let response = action(
                State(runner.clone()),
                headers.clone(),
                Ok(Bytes::from(request.encode_to_vec())),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 10000)
                .await
                .unwrap();
            let result = wire::DeviceActionResponse::decode(bytes).unwrap();
            assert!(result.success);
            assert_eq!(result.replayed, replayed);
        }
        assert_eq!(runner.ledger.lock().await.revision, 42);
        assert_eq!(runner.cursor.lock().await.kind, "tap");
        assert_eq!(runner.cursor.lock().await.status, "completed");
        // Only the first action executes ADB; replay returns the durable receipt.
        assert_eq!(std::fs::read_to_string(calls).unwrap().lines().count(), 1);
    }

    #[tokio::test]
    async fn action_persistence_initial_failure_does_not_execute() {
        let (fixture, runner, request) = persistence_fixture("initial", CompletionFault::None);
        let hash = format!("{:x}", Sha256::digest(request.action_id.as_bytes()));
        let blocked = runner
            .ledger_path
            .with_extension("actions")
            .join(&hash[..2])
            .join(format!("{hash}.tmp"));
        std::fs::create_dir_all(blocked).unwrap();
        persistence_failure(persistence_request(&runner, &request).await).await;
        let replay = persistence_result(persistence_request(&runner, &request).await).await;
        assert!(replay.replayed && replay.result_unknown && !replay.success);
        assert!(!fixture.0.join("calls").exists());
    }

    #[tokio::test]
    async fn action_persistence_completion_failure_does_not_publish_success() {
        let (fixture, runner, request) =
            persistence_fixture("completion", CompletionFault::Archive);
        persistence_failure(persistence_request(&runner, &request).await).await;
        let stored = action_store::read(&runner.ledger_path, &request.action_id)
            .unwrap()
            .unwrap();
        assert!(!stored.completed && !stored.success);
        let replay = persistence_result(persistence_request(&runner, &request).await).await;
        assert!(replay.replayed && replay.result_unknown && !replay.success);
        let mut recovered: Ledger =
            serde_json::from_slice(&std::fs::read(&runner.ledger_path).unwrap()).unwrap();
        action_store::migrate(&runner.ledger_path, &mut recovered).unwrap();
        *runner.ledger.lock().await = recovered;
        let after_restart = persistence_result(persistence_request(&runner, &request).await).await;
        assert!(after_restart.replayed && after_restart.result_unknown && !after_restart.success);
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("calls")).unwrap(),
            "HOME\n"
        );
    }

    #[tokio::test]
    async fn action_persistence_snapshot_failure_keeps_durable_success() {
        let (fixture, runner, request) = persistence_fixture("snapshot", CompletionFault::Snapshot);
        persistence_failure(persistence_request(&runner, &request).await).await;
        let stored = action_store::read(&runner.ledger_path, &request.action_id)
            .unwrap()
            .unwrap();
        assert!(stored.completed && stored.success);
        let replay = persistence_result(persistence_request(&runner, &request).await).await;
        assert!(replay.replayed && replay.success && !replay.result_unknown);
        let mut recovered: Ledger =
            serde_json::from_slice(&std::fs::read(&runner.ledger_path).unwrap()).unwrap();
        assert!(!recovered.actions[&request.action_id].completed);
        action_store::migrate(&runner.ledger_path, &mut recovered).unwrap();
        *runner.ledger.lock().await = recovered;
        let after_restart = persistence_result(persistence_request(&runner, &request).await).await;
        assert!(after_restart.replayed && after_restart.success && !after_restart.result_unknown);
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("calls")).unwrap(),
            "HOME\n"
        );
    }

    #[tokio::test]
    async fn action_persistence_success_never_executes_a_duplicate() {
        let (fixture, runner, request) = persistence_fixture("success", CompletionFault::None);
        let first = persistence_result(persistence_request(&runner, &request).await).await;
        assert!(first.success && !first.replayed && !first.result_unknown);
        let replay = persistence_result(persistence_request(&runner, &request).await).await;
        assert!(replay.success && replay.replayed && !replay.result_unknown);
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("calls")).unwrap(),
            "HOME\n"
        );
    }

    enum CompletionFault {
        None,
        Archive,
        Snapshot,
    }

    fn persistence_fixture(
        id: &str,
        fault: CompletionFault,
    ) -> (Fixture, Arc<Runner>, wire::DeviceActionRequest) {
        let fixture = Fixture::new();
        let hash = format!("{:x}", Sha256::digest(id.as_bytes()));
        let injection = match fault {
            CompletionFault::None => String::new(),
            CompletionFault::Archive => {
                format!("mkdir \"$base/ledger.actions/{}/{hash}.tmp\"", &hash[..2])
            }
            CompletionFault::Snapshot => "mkdir \"$base/ledger.tmp\"".into(),
        };
        // The TCP peer fixture accepts HOME only, never invokes a host ADB,
        // and confines its writes to its own directory.
        let script = format!(
            r#"#!/bin/sh
set -eu
test "$*" = "-P 15037 -s emulator-5564 shell input keyevent KEYCODE_HOME"
base=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
printf 'HOME\n' >> "$base/calls"
{injection}
printf 'home-completed'
"#
        );
        let mut device = fixture.runner(&script);
        device.media_url = "http://127.0.0.1:0".into();
        let runner = Arc::new(device);
        let request = wire::DeviceActionRequest {
            phone_id: "phone-local-01".into(),
            owner_id: OWNER.into(),
            run_id: "persistence-fixture".into(),
            action_id: id.into(),
            control_epoch: 1,
            controller: "manual".into(),
            action: Some(wire::device_action_request::Action::Key(wire::KeyAction {
                key: "HOME".into(),
            })),
            ..Default::default()
        };
        (fixture, runner, request)
    }

    async fn persistence_request(
        runner: &Arc<Runner>,
        request: &wire::DeviceActionRequest,
    ) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-only".parse().unwrap());
        headers.insert("content-type", MEDIA.parse().unwrap());
        action(
            State(runner.clone()),
            headers,
            Ok(Bytes::from(request.encode_to_vec())),
        )
        .await
    }

    async fn persistence_failure(response: Response) {
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        assert_eq!(
            wire::ApiError::decode(bytes).unwrap().code,
            "persistence_failed"
        );
    }

    async fn persistence_result(response: Response) -> wire::DeviceActionResponse {
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        wire::DeviceActionResponse::decode(bytes).unwrap()
    }

    #[tokio::test]
    async fn archived_actions_replay_after_epoch_change_and_unknowns_never_execute_adb() {
        let fixture = Fixture::new();
        let marker = fixture.0.join("must-not-execute");
        let runner = Arc::new(fixture.runner(&format!(
            "#!/bin/sh\ntouch '{}'\nexit 1\n",
            marker.display()
        )));
        runner.ledger.lock().await.epoch = 99;
        let request = wire::DeviceActionRequest {
            phone_id: "phone-local-01".into(),
            owner_id: OWNER.into(),
            run_id: "archive-test".into(),
            action_id: "old-action".into(),
            control_epoch: 1,
            controller: "manual".into(),
            action: Some(wire::device_action_request::Action::Key(wire::KeyAction {
                key: "HOME".into(),
            })),
            ..Default::default()
        };
        let bytes = request.encode_to_vec();
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-only".parse().unwrap());
        headers.insert("content-type", MEDIA.parse().unwrap());
        for completed in [true, false] {
            action_store::write(
                &runner.ledger_path,
                &request.action_id,
                &Record {
                    hash: hash.clone(),
                    completed,
                    success: completed,
                    output: "retained outcome".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let response = action(
                State(runner.clone()),
                headers.clone(),
                Ok(Bytes::from(bytes.clone())),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let result = wire::DeviceActionResponse::decode(
                axum::body::to_bytes(response.into_body(), 8192)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert!(result.replayed);
            assert_eq!(result.success, completed);
            assert_eq!(result.result_unknown, !completed);
            assert!(!marker.exists());
        }
        let mut conflicting = request;
        conflicting.run_id = "different".into();
        assert_eq!(
            action(
                State(runner),
                headers,
                Ok(Bytes::from(conflicting.encode_to_vec()))
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        assert!(!marker.exists());
    }

    #[test]
    fn only_current_successful_dump_with_complete_xml_is_accepted() {
        let path = "/data/local/tmp/ai-phone-window-current.xml";
        let marker = "__AI_PHONE_UI_TREE_current__";
        for prefix in ["UI hierchary dumped to: ", "UI hierarchy dumped to: "] {
            let output = format!("{prefix}{path}\n{marker}\n{XML}");
            assert_eq!(
                ui_tree_from_output(output.as_bytes(), path, marker).as_deref(),
                Some(XML)
            );
        }
        for output in [
            format!(
                "UI hierchary dumped to: /data/local/tmp/ai-phone-window-old.xml\n{marker}\n{XML}"
            ),
            format!("ERROR: could not get idle state.\n{marker}\n{XML}"),
            format!("UI hierchary dumped to: {path}\n__AI_PHONE_UI_TREE_old__\n{XML}"),
            format!("UI hierchary dumped to: {path}\n{marker}\n<hierarchy><node"),
            format!("UI hierchary dumped to: {path}\n{marker}\n"),
        ] {
            assert!(ui_tree_from_output(output.as_bytes(), path, marker).is_none());
        }
    }

    #[tokio::test]
    async fn guest_script_requires_new_nonempty_file_and_always_cleans_it() {
        let fixture = Fixture::new();
        fixture.executable("timeout", "#!/bin/sh\nshift 3\nexec \"$@\"\n");
        fixture.executable("uiautomator", &format!(
            "#!/bin/sh\nif [ \"$FAKE_FILE\" = yes ]; then printf '%s' '{}' > \"$2\"; fi\nprintf 'UI hierchary dumped to: %s\\n' \"$2\"\nexit \"$FAKE_EXIT\"\n",
            XML.replace('\'', "'\\''")
        ));
        let path = fixture.0.join("current.xml");
        let path = path.to_str().unwrap();
        let marker = "__AI_PHONE_UI_TREE_test__";
        for (file, exit, accepted) in [("yes", "0", true), ("yes", "1", false), ("no", "0", false)]
        {
            let command = ui_tree_command(path, marker);
            let output = Command::new("/bin/sh")
                .args(["-c", &command[1]])
                .env("PATH", format!("{}:/usr/bin:/bin", fixture.0.display()))
                .env("FAKE_FILE", file)
                .env("FAKE_EXIT", exit)
                .output()
                .await
                .unwrap();
            let tree = output
                .status
                .success()
                .then(|| ui_tree_from_output(&output.stdout, path, marker))
                .flatten();
            assert_eq!(tree.is_some(), accepted);
            assert!(
                !std::path::Path::new(path).exists(),
                "owned XML must be removed even after a failed dump"
            );
        }
    }

    #[tokio::test]
    async fn live_native_hierarchy_avoids_the_legacy_idle_wait() {
        let fixture = Fixture::new();
        let legacy = fixture.0.join("legacy-called");
        let runner = fixture.runner(&format!(
            "#!/bin/sh\ncase \"$6\" in\n *dev.aiphone.input.DUMP_UI*) request=\"${{6##*request_id }}\"; printf 'Broadcast completed: result=-1, data=\"%s:{}\"\\n' \"$request\";;\n *) touch {}; exit 1;;\nesac\n",
            STANDARD.encode(XML),
            guest_quote(legacy.to_str().unwrap())
        ));
        assert_eq!(capture_tree_sources(&runner).await, XML);
        assert!(
            !legacy.exists(),
            "a current native tree must avoid uiautomator"
        );
    }

    #[tokio::test]
    async fn failed_adb_returns_empty_tree_and_each_attempt_has_a_unique_path() {
        let fixture = Fixture::new();
        let log = fixture.0.join("commands");
        let runner = fixture.runner(&format!(
            "#!/bin/sh\nprintf '%s\\n' \"$6\" >> {}\nprintf '<hierarchy><node text=\"stale\" /></hierarchy>'\nexit 1\n",
            guest_quote(log.to_str().unwrap())
        ));
        assert!(capture_ui_tree(&runner).await.is_empty());
        assert!(capture_ui_tree(&runner).await.is_empty());
        let commands = std::fs::read_to_string(log).unwrap();
        let paths: Vec<_> = commands
            .lines()
            .filter(|line| line.starts_with("tree="))
            .map(|line| line.split(';').next().unwrap())
            .collect();
        assert_eq!(
            commands.lines().count(),
            4,
            "both sources tried once per request"
        );
        let native: Vec<_> = commands
            .lines()
            .filter(|line| line.starts_with("am broadcast"))
            .collect();
        assert_eq!(native.len(), 2);
        assert_ne!(native[0], native[1], "native requests must also be fresh");
        assert_eq!(paths.len(), 2);
        assert_ne!(paths[0], paths[1]);
        assert!(paths.iter().all(
            |path| path.starts_with("tree='/data/local/tmp/ai-phone-window-")
                && path.ends_with(".xml'")
        ));
    }

    #[tokio::test]
    async fn host_deadline_closes_transport_and_stops_its_fixture() {
        let fixture = Fixture::new();
        let pid_file = fixture.0.join("pid");
        let runner = fixture.runner(&format!(
            "#!/bin/sh\nprintf '%s' \"$$\" > {}\nexec /bin/sleep 30\n",
            guest_quote(pid_file.to_str().unwrap())
        ));
        let start = Instant::now();
        assert!(capture_ui_tree(&runner).await.is_empty());
        assert!(start.elapsed() < Duration::from_secs(5));
        let pid = std::fs::read_to_string(pid_file).unwrap();
        for _ in 0..20 {
            let exists = Command::new("/bin/kill")
                .args(["-0", &pid])
                .output()
                .await
                .unwrap()
                .status
                .success();
            if !exists {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("timed-out transport must close its owned peer command");
    }

    #[tokio::test]
    async fn contended_tree_lock_does_not_queue_another_dump() {
        let fixture = Fixture::new();
        let started = fixture.0.join("started");
        let runner = fixture.runner(&format!(
            "#!/bin/sh\ntouch {}\n",
            guest_quote(started.to_str().unwrap())
        ));
        let _held = runner.tree_lock.lock().await;
        let start = Instant::now();
        assert!(capture_ui_tree(&runner).await.is_empty());
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(
            !started.exists(),
            "lock timeout must not start a delayed dump"
        );
    }
}

#[cfg(test)]
mod orientation_tests {
    use super::*;
    #[test]
    fn only_bounded_rotations_can_form_a_command() {
        for rotation in 0..=3 {
            assert!(
                orientation_command(&wire::OrientationAction {
                    automatic: false,
                    rotation
                })
                .is_some()
            );
        }
        for rotation in [4, 360, u32::MAX] {
            assert!(
                orientation_command(&wire::OrientationAction {
                    automatic: true,
                    rotation
                })
                .is_none()
            );
        }
    }
    #[test]
    fn policy_write_reads_both_actual_values_back() {
        let command = orientation_command(&wire::OrientationAction {
            automatic: false,
            rotation: 1,
        })
        .unwrap();
        assert_eq!(command[0], "shell");
        assert!(
            command[1].contains("user_rotation 1 && settings put system accelerometer_rotation 0")
        );
        assert!(command[1].ends_with(
            "settings get system accelerometer_rotation && settings get system user_rotation"
        ));
    }
    #[test]
    fn rotation_policy_parser_rejects_missing_or_invalid_evidence() {
        assert_eq!(
            parse_orientation(b"1\n0\n"),
            Some(wire::OrientationAction {
                automatic: true,
                rotation: 0
            })
        );
        assert_eq!(
            parse_orientation(b"0\n3\n"),
            Some(wire::OrientationAction {
                automatic: false,
                rotation: 3
            })
        );
        for input in ["", "null\n0", "2\n1", "0\n4", "0\n1\nunexpected"] {
            assert!(parse_orientation(input.as_bytes()).is_none());
        }
    }
}
