use super::*;
use wire::device_action_request::Action;
pub fn accepted(r: &wire::DeviceActionRequest, width: u32, height: u32) -> wire::DeviceCursor {
    let mut duration = 120;
    let point = |x, y, at_ms| wire::GesturePoint { x, y, at_ms };
    let points = match r.action.as_ref() {
        Some(Action::Tap(v)) => vec![point(v.x, v.y, 0), point(v.x, v.y, 120)],
        Some(Action::LongPress(v)) => {
            duration = v.duration_ms;
            vec![point(v.x, v.y, 0), point(v.x, v.y, duration)]
        }
        Some(Action::Swipe(v)) => {
            duration = v.duration_ms.clamp(100, 1500);
            vec![point(v.x1, v.y1, 0), point(v.x2, v.y2, duration)]
        }
        Some(Action::Drag(v)) => {
            duration = v.duration_ms.clamp(100, 1500) + 500;
            vec![
                point(v.x1, v.y1, 0),
                point(v.x1, v.y1, 500),
                point(v.x2, v.y2, duration),
            ]
        }
        _ => vec![],
    };
    let (kind, paths) = match r.action.as_ref() {
        Some(Action::Gesture(v)) => {
            duration = v.duration_ms;
            ("gesture", v.paths.clone())
        }
        Some(Action::Tap(_)) => (
            "tap",
            vec![wire::FingerPath {
                pointer_id: 0,
                points,
            }],
        ),
        Some(Action::LongPress(_)) => (
            "hold",
            vec![wire::FingerPath {
                pointer_id: 0,
                points,
            }],
        ),
        Some(Action::Swipe(_)) => (
            "swipe",
            vec![wire::FingerPath {
                pointer_id: 0,
                points,
            }],
        ),
        Some(Action::Drag(_)) => (
            "drag",
            vec![wire::FingerPath {
                pointer_id: 0,
                points,
            }],
        ),
        _ => ("", vec![]),
    };
    wire::DeviceCursor {
        device_id: r.phone_id.clone(),
        action_id: r.action_id.clone(),
        kind: kind.into(),
        status: "running".into(),
        started_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        duration_ms: duration,
        width,
        height,
        paths,
        actor: r.controller.clone(),
        ..Default::default()
    }
}
pub async fn read(
    State(runner): State<Arc<Runner>>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let r: wire::DeviceRequest = match request(&runner, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    if let Err(e) = scope(&runner, &r.phone_id, &r.owner_id) {
        return e.into();
    }
    let mut current = runner.cursor.lock().await.clone();
    let report_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    // Only the actual touch bridge knows which positions were acknowledged.
    // If it is unavailable or belongs to a different action, show no fabricated path.
    if !current.paths.is_empty() {
        let confirmed = async {
            let response = runner
                .local
                .get(format!("{}/cursor", runner.media_url))
                .bearer_auth(&runner.token)
                .timeout(Duration::from_millis(500))
                .send()
                .await
                .ok()?;
            if !response.status().is_success() {
                return None;
            }
            let value = wire::DeviceCursor::decode(response.bytes().await.ok()?).ok()?;
            (value.action_id == current.action_id
                && value.device_id == current.device_id
                && value.width == current.width
                && value.height == current.height
                && value.input_confirmed)
                .then_some(value)
        }
        .await;
        if let Some(mut value) = confirmed {
            if current.status == "unknown" {
                value.status = "unknown".into();
            }
            value.reported_at_ms = report_time;
            return pb(StatusCode::OK, value);
        }
    }
    current.paths.clear();
    current.input_confirmed = false;
    current.reported_at_ms = report_time;
    pb(StatusCode::OK, current)
}
