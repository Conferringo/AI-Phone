use super::*;
use std::{
    collections::{HashSet, VecDeque},
    time::Instant,
};
use tokio::io::AsyncWriteExt;

const MAX_TARGETS: usize = 160;
const MAX_AGE: Duration = Duration::from_secs(60);
#[derive(Clone)]
pub struct Target {
    pub element: wire::PhoneElement,
    key: String,
}
#[derive(Clone)]
pub struct Snapshot {
    id: String,
    epoch: u64,
    revision: u64,
    app: String,
    width: u32,
    height: u32,
    created: Instant,
    targets: Vec<Target>,
    native_hash: String,
    grid: Vec<u8>,
}
#[derive(Clone)]
pub struct ScrollAttempt {
    app: String,
    hash: String,
    container: String,
    direction: String,
    revision: u64,
}
#[derive(Default)]
pub struct GroundingState {
    snapshots: VecDeque<Snapshot>,
    pending_scroll: Option<ScrollAttempt>,
    blocked: Option<ScrollAttempt>,
}
fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn bounds(text: &str, width: u32, height: u32) -> Option<wire::PhoneBounds> {
    let values: Vec<_> = text
        .split(['[', ']', ','])
        .filter(|v| !v.is_empty())
        .map(str::parse::<i32>)
        .collect::<Result<_, _>>()
        .ok()?;
    if values.len() != 4 {
        return None;
    }
    let b = wire::PhoneBounds {
        left: values[0].max(0) as u32,
        top: values[1].max(0) as u32,
        right: (values[2].max(0) as u32).min(width),
        bottom: (values[3].max(0) as u32).min(height),
    };
    (b.right > b.left + 3 && b.bottom > b.top + 3).then_some(b)
}
fn rect_key(b: &wire::PhoneBounds) -> String {
    format!("{},{},{},{}", b.left, b.top, b.right, b.bottom)
}
fn container_key(e: &wire::PhoneElement) -> String {
    format!(
        "{}|{}|{}",
        e.resource_id,
        e.class_name,
        rect_key(e.bounds.as_ref().unwrap())
    )
}
fn native_hash(targets: &[Target]) -> String {
    digest(
        &targets
            .iter()
            .filter(|t| t.element.source == "native")
            .map(|t| t.key.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}
fn has_native_targets(targets: &[Target]) -> bool {
    targets
        .iter()
        .any(|t| t.element.clickable || t.element.scrollable || t.element.editable)
}
fn native_content_available(targets: &[Target], height: u32) -> bool {
    // Browser chrome and an empty WebView do not locate the page's content.
    let body: Vec<_> = targets
        .iter()
        .filter(|t| {
            let b = t.element.bounds.as_ref().unwrap();
            (b.top + b.bottom) / 2 > height * 15 / 100
                && (b.top + b.bottom) / 2 < height * 92 / 100
                && !t.element.text.is_empty()
        })
        .collect();
    body.iter()
        .any(|t| t.element.clickable || t.element.editable)
        || body.len() >= 3
}
fn parse_native(xml: &str, app: &str, width: u32, height: u32) -> Vec<Target> {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return vec![];
    };
    let package = app.split('/').next().unwrap_or("");
    let mut targets = vec![];
    for node in doc.descendants().filter(|n| n.has_tag_name("node")) {
        if node.ancestors().any(|n| {
            n.attribute("enabled") == Some("false")
                || n.attribute("visible-to-user") == Some("false")
        }) {
            continue;
        }
        if node
            .attribute("package")
            .is_some_and(|p| !p.is_empty() && p != package)
        {
            continue;
        }
        let clickable = node.attribute("clickable") == Some("true");
        let scrollable = node.attribute("scrollable") == Some("true");
        let class = node.attribute("class").unwrap_or("");
        let editable = class.ends_with("EditText");
        let own = node
            .attribute("text")
            .filter(|v| !v.is_empty())
            .or(node.attribute("content-desc"))
            .unwrap_or("");
        if !(clickable || scrollable || editable)
            && (own.is_empty()
                || node
                    .ancestors()
                    .skip(1)
                    .any(|n| n.attribute("clickable") == Some("true")))
        {
            continue;
        }
        let Some(mut b) = node
            .attribute("bounds")
            .and_then(|v| bounds(v, width, height))
        else {
            continue;
        };
        for parent in node.ancestors().skip(1) {
            if let Some(p) = parent
                .attribute("bounds")
                .and_then(|v| bounds(v, width, height))
            {
                b.left = b.left.max(p.left);
                b.top = b.top.max(p.top);
                b.right = b.right.min(p.right);
                b.bottom = b.bottom.min(p.bottom);
            }
        }
        if b.right <= b.left + 3 || b.bottom <= b.top + 3 {
            continue;
        }
        let label = if own.is_empty() && !scrollable {
            let mut seen = HashSet::new();
            node.descendants()
                .filter_map(|n| n.attribute("text").filter(|s| !s.is_empty()))
                .filter(|s| seen.insert(*s))
                .take(6)
                .collect::<Vec<_>>()
                .join(" · ")
        } else {
            own.into()
        };
        let e = wire::PhoneElement {
            id: format!("n{}", targets.len() + 1),
            text: label.chars().take(200).collect(),
            resource_id: node.attribute("resource-id").unwrap_or("").into(),
            class_name: class.into(),
            bounds: Some(b),
            clickable,
            scrollable,
            editable,
            source: "native".into(),
            confidence: 1.0,
        };
        let key = format!(
            "{}|{}|{}|{}|{}|{}",
            container_key(&e),
            e.text,
            clickable,
            scrollable,
            node.attribute("checked").unwrap_or(""),
            node.attribute("selected").unwrap_or("")
        );
        targets.push(Target { element: e, key });
        if targets.len() >= MAX_TARGETS {
            break;
        }
    }
    targets
}

#[derive(Deserialize)]
struct OcrBox {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}
#[derive(Deserialize)]
struct OcrRegion {
    text: String,
    confidence: f32,
    bounds: OcrBox,
}
#[derive(Deserialize)]
struct OcrCapture {
    width: u32,
    height: u32,
    regions: Vec<OcrRegion>,
    grid: Vec<u8>,
}
async fn ocr(
    runner: &Runner,
    png: &[u8],
    width: u32,
    height: u32,
) -> Result<(Vec<Target>, Vec<u8>), String> {
    let mut child = Command::new(&runner.vision)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            format!(
                "本机OCR启动失败（{:?}: {error}），原生控件仍可用",
                error.kind()
            )
        })?;
    let mut input = child.stdin.take().ok_or("OCR input unavailable")?;
    let mut output = child
        .stdout
        .take()
        .ok_or("OCR output unavailable")?
        .take(262145);
    let mut bytes = vec![];
    let status = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::try_join!(
            async move {
                input.write_all(png).await?;
                input.shutdown().await?;
                drop(input);
                Ok::<(), std::io::Error>(())
            },
            output.read_to_end(&mut bytes),
            child.wait()
        )
    })
    .await
    .map_err(|_| "本机OCR超时".to_string())?
    .map_err(|_| "本机OCR读取失败".to_string())?;
    if !status.2.success() || bytes.len() > 262144 {
        return Err("本机OCR失败".into());
    }
    let capture: OcrCapture =
        serde_json::from_slice(&bytes).map_err(|_| "OCR数据无效".to_string())?;
    if capture.width != width || capture.height != height || capture.grid.len() != 2048 {
        return Err("OCR画面尺寸不匹配".into());
    }
    let mut targets = vec![];
    for r in capture.regions.into_iter().take(MAX_TARGETS) {
        let b = wire::PhoneBounds {
            left: r.bounds.left,
            top: r.bounds.top,
            right: r.bounds.right,
            bottom: r.bounds.bottom,
        };
        if !r.confidence.is_finite()
            || r.confidence < 0.80
            || r.text.trim().is_empty()
            || b.right > width
            || b.bottom > height
            || b.right <= b.left + 3
            || b.bottom <= b.top + 3
            || b.top < height / 25
            || b.bottom > height - height / 25
        {
            continue;
        }
        let key = format!("{}|{}", r.text, rect_key(&b));
        targets.push(Target {
            key,
            element: wire::PhoneElement {
                id: format!("o{}", targets.len() + 1),
                text: r.text,
                resource_id: String::new(),
                class_name: String::new(),
                bounds: Some(b),
                clickable: false,
                scrollable: false,
                editable: false,
                source: "ocr".into(),
                confidence: r.confidence,
            },
        });
    }
    Ok((targets, capture.grid))
}
fn similar_grid(a: &[u8], b: &[u8]) -> bool {
    if a.len() != 2048 || b.len() != 2048 {
        return false;
    }
    // Ignore status/navigation bars; preserve the rest of the screen as a coarse guard.
    let differences: Vec<_> = a[128..1920]
        .iter()
        .zip(&b[128..1920])
        .map(|(a, b)| a.abs_diff(*b) as usize)
        .collect();
    differences.iter().sum::<usize>() < differences.len() * 7
        && differences.iter().filter(|d| **d > 24).count() * 100 < differences.len() * 8
}
fn foreground_from_dump(output: &str) -> Option<String> {
    let mut primary = !output.contains("Display: mDisplayId=");
    for line in output.lines() {
        if let Some(id) = line.trim().strip_prefix("Display: mDisplayId=") {
            primary = id.split_whitespace().next() == Some("0");
        }
        if primary && line.contains("mCurrentFocus=") {
            return line
                .split_whitespace()
                .find(|s| s.contains('/') && s.contains('.'))
                .map(|s| s.trim_end_matches('}').to_string());
        }
    }
    None
}
fn focused_window(output: &str) -> Option<&str> {
    let mut primary = !output.contains("Display: mDisplayId=");
    for line in output.lines() {
        if let Some(id) = line.trim().strip_prefix("Display: mDisplayId=") {
            primary = id.split_whitespace().next() == Some("0");
        }
        if primary && let Some(window) = line.trim().strip_prefix("mCurrentFocus=") {
            // Titles are app-defined and may be localized (e.g. "弹出式窗口").
            // Ownership and allowed window types are checked against the exact
            // focused window below, never inferred from its title.
            return (window.starts_with("Window{") && window.ends_with('}')).then_some(window);
        }
    }
    None
}
fn focused_surface_foreground(displays: &str, windows: &str) -> Option<String> {
    let focus = focused_window(displays)?;
    let mut matching = false;
    let mut section = String::new();
    for line in windows.lines() {
        if line.trim().starts_with("Window #") {
            if matching {
                break;
            }
            matching = line.trim().ends_with(&format!("{focus}:"));
        }
        if matching {
            section.push_str(line);
            section.push('\n');
        }
    }
    if !section.contains("mDisplayId=0 ") || !section.contains("isVisible=true") {
        return None;
    }
    let package = section
        .split_whitespace()
        .find_map(|word| word.strip_prefix("package="))?;
    // The shade has no Activity component. Resolve its actual focused,
    // visible window owner rather than borrowing the obscured application's
    // identity or accepting an app-controlled title as a system surface.
    if section.contains("ty=NOTIFICATION_SHADE") && package == "com.android.systemui" {
        return Some("com.android.systemui/NotificationShade".into());
    }
    if ![
        "ty=APPLICATION_PANEL",
        "ty=APPLICATION_SUB_PANEL",
        "ty=APPLICATION_ATTACHED_DIALOG",
    ]
    .iter()
    .any(|kind| section.contains(kind))
    {
        return None;
    }
    if package.is_empty() || !package.contains('.') {
        return None;
    }
    let title = focus.split_whitespace().last()?.trim_end_matches('}');
    let identity = if title.starts_with("PopupWindow:") {
        title.to_string()
    } else {
        let token = focus.strip_prefix("Window{")?.split_whitespace().next()?;
        if !token.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        format!("PopupWindow:{token}")
    };
    Some(format!("{package}/{identity}"))
}
async fn foreground(runner: &Runner) -> Result<String, String> {
    for attempt in 0..3 {
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            adb(runner, args(&["shell", "dumpsys", "window", "displays"])),
        )
        .await
        .map_err(|_| "前台应用读取超时")??;
        if !result.0 {
            return Err("前台应用读取失败".into());
        }
        let displays = String::from_utf8_lossy(&result.1);
        if let Some(app) = foreground_from_dump(&displays) {
            return Ok(app);
        }
        if focused_window(&displays).is_some() {
            let windows = tokio::time::timeout(
                Duration::from_secs(2),
                adb(runner, args(&["shell", "dumpsys", "window", "windows"])),
            )
            .await
            .map_err(|_| "弹出窗口读取超时")??;
            if windows.0
                && let Some(app) =
                    focused_surface_foreground(&displays, &String::from_utf8_lossy(&windows.1))
            {
                return Ok(app);
            }
        }
        if attempt < 2 {
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
    }
    Err("前台应用不可确认，请重新观察".into())
}

pub async fn observe(
    runner: &Runner,
    request: &wire::DeviceRequest,
) -> Result<wire::DeviceObservation, String> {
    if request.metadata_only {
        return Err("控件定位需要实际截图".into());
    }
    if !["", "auto", "always", "only"].contains(&request.ocr_mode.as_str()) {
        return Err("无效OCR模式".into());
    }
    let (epoch, revision) = {
        let ledger = runner.ledger.lock().await;
        (ledger.epoch, ledger.revision)
    };
    let app = foreground(runner).await?;
    let xml = if request.ocr_mode != "only" {
        capture_ui_tree(runner).await
    } else {
        String::new()
    };
    let (png, width, height) = capture_frame(runner).await?;
    let mut targets = parse_native(&xml, &app, width, height);
    let hash = native_hash(&targets);
    let mut grid = vec![];
    let mut note = String::new();
    if !native_content_available(&targets, height)
        || ["always", "only"].contains(&request.ocr_mode.as_str())
    {
        match ocr(runner, &png, width, height).await {
            Ok((mut recognized, visual)) => {
                grid = visual;
                targets.append(&mut recognized);
                note = "OCR文字仅为可见文字区域，不保证是按钮".into();
            }
            Err(error) => note = error,
        }
    }
    if foreground(runner).await? != app {
        return Err("观察期间应用已切换，请重新观察".into());
    }
    let mut ledger = runner.ledger.lock().await;
    if ledger.epoch != epoch || ledger.revision != revision {
        return Err("观察期间控制或画面已改变，请重新观察".into());
    }
    ledger.sequence += 1;
    let id = Uuid::new_v4().to_string();
    let mut cache = runner.grounding.lock().await;
    let mut feedback = String::new();
    if let Some(previous) = cache.pending_scroll.take()
        && previous.app == app
        && previous.revision == revision
    {
        if previous.hash == hash && has_native_targets(&targets) {
            feedback = format!(
                "上次向{}滚动后未观察到内容变化；同一区域的重复同向滚动已阻止，请检查边界、等待加载或换方向。",
                previous.direction
            );
            cache.blocked = Some(previous);
        } else {
            feedback = "上次滚动后可见内容已变化".into();
            cache.blocked = None;
        }
    }
    if feedback.is_empty()
        && let Some(blocked) = cache
            .blocked
            .as_ref()
            .filter(|v| v.app == app && v.hash == hash)
    {
        feedback = format!(
            "此页面向{}滚动已验证未观察到内容变化；不要重复同向滚动，请换方向或等待内容改变。",
            blocked.direction
        );
    }
    cache.snapshots.push_back(Snapshot {
        id: id.clone(),
        epoch,
        revision,
        app: app.clone(),
        width,
        height,
        created: Instant::now(),
        targets: targets.clone(),
        native_hash: hash,
        grid,
    });
    while cache.snapshots.len() > 4 {
        cache.snapshots.pop_front();
    }
    Ok(wire::DeviceObservation {
        phone_id: runner.phone_id.clone(),
        width,
        height,
        png,
        ui_tree: if request.include_ui_tree {
            xml
        } else {
            String::new()
        },
        screen_sequence: ledger.sequence,
        control_epoch: epoch,
        controller: ledger.controller.clone(),
        root_identity: if runner.root_ready.load(Ordering::Acquire) {
            "0".into()
        } else {
            String::new()
        },
        observation_id: id,
        elements: targets.into_iter().map(|t| t.element).collect(),
        foreground_app: app,
        grounding_note: note,
        scroll_feedback: feedback,
    })
}
fn validate_snapshot(snapshot: &Snapshot, epoch: u64, revision: u64) -> Result<(), String> {
    if snapshot.epoch != epoch
        || snapshot.revision != revision
        || snapshot.created.elapsed() > MAX_AGE
    {
        return Err("stale_target:观察已过期或手机已操作，请重新观察后选择控件".into());
    }
    Ok(())
}
fn scroll_gesture(b: &wire::PhoneBounds, direction: &str) -> Result<wire::SwipeAction, String> {
    let w = b.right.saturating_sub(b.left);
    let h = b.bottom.saturating_sub(b.top);
    if w < 80 || h < 80 {
        return Err("滚动区域过小，未执行".into());
    }
    let (cx, cy) = (b.left + w / 2, b.top + h / 2);
    let (left, right, top, bottom) = (
        b.left + w / 5,
        b.right - w / 5,
        b.top + h / 5,
        b.bottom - h / 5,
    );
    let (x1, y1, x2, y2) = match direction {
        "down" => (cx, bottom, cx, top),
        "up" => (cx, top, cx, bottom),
        "left" => (left, cy, right, cy),
        "right" => (right, cy, left, cy),
        _ => return Err("方向必须为up/down/left/right".into()),
    };
    Ok(wire::SwipeAction {
        x1,
        y1,
        x2,
        y2,
        duration_ms: 380,
    })
}

pub async fn resolve(
    runner: &Runner,
    request: &wire::DeviceActionRequest,
    epoch: u64,
    revision: u64,
) -> Result<Option<(wire::device_action_request::Action, Option<ScrollAttempt>)>, String> {
    use wire::device_action_request::Action;
    let (id, element_id, direction) = match request.action.as_ref() {
        Some(Action::TapElement(v)) => {
            (v.observation_id.as_str(), Some(v.element_id.as_str()), None)
        }
        Some(Action::ScrollElement(v)) => (
            v.observation_id.as_str(),
            Some(v.element_id.as_str()),
            Some(v.direction.as_str()),
        ),
        Some(
            Action::Tap(_)
            | Action::Swipe(_)
            | Action::LongPress(_)
            | Action::Drag(_)
            | Action::Gesture(_)
            | Action::Motion(_)
            | Action::Orientation(_),
        ) if !request.observation_id.is_empty() => (request.observation_id.as_str(), None, None),
        _ => return Ok(None),
    };
    let snapshot = {
        let cache = runner.grounding.lock().await;
        cache.snapshots.iter().find(|s| s.id == id).cloned()
    }
    .ok_or("stale_target:观察记录不存在或服务已重启，请重新观察")?;
    validate_snapshot(&snapshot, epoch, revision)?;
    if dimensions(runner).await != (snapshot.width, snapshot.height)
        || foreground(runner).await? != snapshot.app
    {
        return Err("stale_target:前台应用或方向已变化，请重新观察".into());
    }
    let target = element_id
        .map(|id| {
            snapshot
                .targets
                .iter()
                .find(|t| t.element.id == id)
                .ok_or("invalid_target:控件ID不属于这次观察")
        })
        .transpose()?;
    let requires_ocr = target
        .map(|t| t.element.source == "ocr")
        .unwrap_or(!snapshot.grid.is_empty() || !has_native_targets(&snapshot.targets));
    if requires_ocr {
        let (png, w, h) = capture_frame(runner).await?;
        let (fresh, grid) = ocr(runner, &png, w, h).await?;
        if (w, h) != (snapshot.width, snapshot.height) || !similar_grid(&snapshot.grid, &grid) {
            return Err("stale_target:画面已变化，请重新观察".into());
        }
        if let Some(target) = target {
            let b = target.element.bounds.as_ref().unwrap();
            let matches = fresh
                .iter()
                .filter(|candidate| {
                    let c = candidate.element.bounds.as_ref().unwrap();
                    candidate.element.text == target.element.text
                        && b.left.abs_diff(c.left) <= 4
                        && b.top.abs_diff(c.top) <= 4
                        && b.right.abs_diff(c.right) <= 4
                        && b.bottom.abs_diff(c.bottom) <= 4
                })
                .count();
            if matches != 1 {
                return Err("stale_target:OCR文字已变化或无法唯一核对，未执行".into());
            }
        }
    } else {
        let fresh = parse_native(
            &capture_ui_tree(runner).await,
            &snapshot.app,
            snapshot.width,
            snapshot.height,
        );
        if fresh.is_empty() {
            return Err("stale_target:原生控件无法复查，未执行；请重新观察使用OCR".into());
        }
        if let Some(target) = target {
            if fresh.iter().filter(|v| v.key == target.key).count() != 1 {
                return Err("stale_target:控件内容、位置或状态已变化，未执行".into());
            }
        } else if native_hash(&fresh) != snapshot.native_hash {
            return Err("stale_target:坐标所依据的界面已变化，未执行".into());
        }
    }
    if foreground(runner).await? != snapshot.app {
        return Err("stale_target:执行前应用切换，未执行".into());
    }
    if let Some(target) = target {
        let e = &target.element;
        let b = e.bounds.as_ref().unwrap();
        if let Some(direction) = direction {
            if e.source != "native" || !e.scrollable {
                return Err(
                    "invalid_target:必须选择原生可滚动区域，不能把OCR文字当作滚动容器".into(),
                );
            }
            let container = container_key(e);
            if runner
                .grounding
                .lock()
                .await
                .blocked
                .as_ref()
                .is_some_and(|v| {
                    v.app == snapshot.app
                        && v.hash == snapshot.native_hash
                        && v.container == container
                        && v.direction == direction
                })
            {
                return Err("scroll_no_change:此页面已验证同向滚动无变化，未重复执行；请换方向或等待内容改变".into());
            }
            let attempt = ScrollAttempt {
                app: snapshot.app,
                hash: snapshot.native_hash,
                container,
                direction: direction.into(),
                revision: revision + 1,
            };
            let mut swipe = scroll_gesture(b, direction)?;
            if let Some(Action::ScrollElement(value)) = request.action.as_ref() {
                let distance = if value.distance == 0.0 {
                    0.6
                } else {
                    value.distance
                };
                if !distance.is_finite() || !(0.1..=0.8).contains(&distance) {
                    return Err("滚动距离须为容器的0.1–0.8".into());
                }
                let scale = distance / 0.6;
                let cx = (swipe.x1 + swipe.x2) as f32 / 2.;
                let cy = (swipe.y1 + swipe.y2) as f32 / 2.;
                swipe.x1 = (cx + (swipe.x1 as f32 - cx) * scale).round() as u32;
                swipe.x2 = (cx + (swipe.x2 as f32 - cx) * scale).round() as u32;
                swipe.y1 = (cy + (swipe.y1 as f32 - cy) * scale).round() as u32;
                swipe.y2 = (cy + (swipe.y2 as f32 - cy) * scale).round() as u32;
                if value.duration_ms != 0 {
                    if !(100..=1500).contains(&value.duration_ms) {
                        return Err("滚动时长须为100–1500ms".into());
                    }
                    swipe.duration_ms = value.duration_ms;
                }
            }
            return Ok(Some((Action::Swipe(swipe), Some(attempt))));
        }
        if e.source == "native" && !e.clickable && !e.editable {
            return Err("invalid_target:这是只读文字或容器，不是可点击控件".into());
        }
        return Ok(Some((
            Action::Tap(wire::TapAction {
                x: b.left + (b.right - b.left) / 2,
                y: b.top + (b.bottom - b.top) / 2,
            }),
            None,
        )));
    }
    Ok(Some((request.action.clone().ok_or("缺少动作")?, None)))
}
pub async fn record_scroll(runner: &Runner, attempt: Option<ScrollAttempt>, success: bool) {
    if success && let Some(attempt) = attempt {
        runner.grounding.lock().await.pending_scroll = Some(attempt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_shade_is_observable_only_with_the_exact_system_window_owner() {
        let displays = "Display: mDisplayId=0 (organized)\n mCurrentFocus=Window{abc u0 NotificationShade}\n mFocusedApp=ActivityRecord{xyz u0 other.app/.Hidden}";
        let windows = " Window #3 Window{abc u0 NotificationShade}:\n mDisplayId=0 rootTaskId=1\n mOwnerUid=10122 package=com.android.systemui appop=NONE\n mAttrs={ty=NOTIFICATION_SHADE fmt=TRANSLUCENT}\n isVisible=true\n Window #4 Window{def u0 StatusBar}:\n package=com.android.systemui";
        assert_eq!(
            focused_surface_foreground(displays, windows).as_deref(),
            Some("com.android.systemui/NotificationShade")
        );
        for invalid in [
            windows.replace("package=com.android.systemui", "package=other.app"),
            windows.replace("NOTIFICATION_SHADE", "APPLICATION_OVERLAY"),
            windows.replace("isVisible=true", "isVisible=false"),
            windows.replace("mDisplayId=0", "mDisplayId=2"),
            windows.replace("Window{abc", "Window{different"),
        ] {
            assert!(focused_surface_foreground(displays, &invalid).is_none());
        }
    }
    #[test]
    fn popup_focus_requires_exact_visible_primary_application_panel_owner() {
        let displays = "Display: mDisplayId=2\n mCurrentFocus=null\n Display: mDisplayId=0 (organized)\n mCurrentFocus=Window{abc u0 PopupWindow:123}\n mFocusedApp=ActivityRecord{x u0 other.app/.Main}";
        let windows = " Window #1 Window{abc u0 PopupWindow:123}:\n mDisplayId=0 rootTaskId=77\n mOwnerUid=10157 package=com.tencent.mm appop=NONE\n mAttrs={ty=APPLICATION_PANEL}\n isVisible=true\n Window #2 Window{def u0 InputMethod}:\n package=keyboard.app";
        assert_eq!(
            focused_surface_foreground(displays, windows).as_deref(),
            Some("com.tencent.mm/PopupWindow:123")
        );
        assert_eq!(
            focused_surface_foreground(
                displays,
                &windows.replace("APPLICATION_PANEL", "APPLICATION_SUB_PANEL")
            )
            .as_deref(),
            Some("com.tencent.mm/PopupWindow:123")
        );
        assert_eq!(
            focused_surface_foreground(
                displays,
                &windows.replace("APPLICATION_PANEL", "APPLICATION_ATTACHED_DIALOG")
            )
            .as_deref(),
            Some("com.tencent.mm/PopupWindow:123")
        );
        for invalid in [
            windows.replace("mDisplayId=0", "mDisplayId=2"),
            windows.replace("isVisible=true", "isVisible=false"),
            windows.replace("APPLICATION_PANEL", "APPLICATION_OVERLAY"),
            windows.replace("{abc", "{other"),
        ] {
            assert!(focused_surface_foreground(displays, &invalid).is_none());
        }
        assert!(
            focused_surface_foreground(
                &displays.replace("Window{abc u0 PopupWindow:123}", "null"),
                windows
            )
            .is_none()
        );
    }
    #[test]
    fn localized_application_popup_uses_exact_window_token() {
        let displays =
            "Display: mDisplayId=0 (organized)\n mCurrentFocus=Window{fa697a9 u0 弹出式窗口}";
        let windows = " Window #7 Window{fa697a9 u0 弹出式窗口}:\n mDisplayId=0 rootTaskId=98\n mOwnerUid=1000 package=com.android.settings appop=NONE\n mAttrs={ty=APPLICATION_SUB_PANEL}\n isVisible=true\n Window #8 Window{bc87985 u0 com.android.settings/.Main}:\n package=com.android.settings";
        assert_eq!(
            focused_surface_foreground(displays, windows).as_deref(),
            Some("com.android.settings/PopupWindow:fa697a9")
        );
        assert_eq!(
            focused_surface_foreground(
                &displays.replace("弹出式窗口", "Custom menu title"),
                &windows.replace("弹出式窗口", "Custom menu title")
            )
            .as_deref(),
            Some("com.android.settings/PopupWindow:fa697a9")
        );
        assert!(
            focused_surface_foreground(displays, &windows.replace("fa697a9", "fffffff")).is_none()
        );
        assert!(
            focused_surface_foreground(
                displays,
                &windows.replace("APPLICATION_SUB_PANEL", "APPLICATION_OVERLAY")
            )
            .is_none()
        );
    }
    #[tokio::test]
    async fn ocr_process_receives_eof_and_filters_unreliable_regions() {
        let root = std::env::temp_dir().join(format!("aiphone-ocr-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let helper = root.join("ocr");
        let payload = serde_json::json!({"width":720,"height":1440,"grid":vec![100;2048],"regions":[
            {"text":"设置","confidence":0.95,"bounds":{"left":100,"top":200,"right":200,"bottom":250}},
            {"text":"不可靠","confidence":0.40,"bounds":{"left":100,"top":300,"right":200,"bottom":350}},
            {"text":"越界","confidence":1.0,"bounds":{"left":100,"top":400,"right":900,"bottom":450}}
        ]});
        // cat requires stdin EOF and returns exactly the bytes supplied by OCR.
        // A pre-existing executable avoids creating and executing a test script.
        let input = payload.to_string();
        std::fs::write(&helper, b"non-executable fixture").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut runner = Runner {
            phone_id: "phone-local-01".into(),
            serial: "emulator-5564".into(),
            media_url: "http://127.0.0.1:17865".into(),
            adb: root.join("unused"),
            token: String::new(),
            ledger_path: root.join("ledger"),
            ledger: Mutex::new(Ledger::default()),
            tree_lock: Mutex::new(()),
            cursor: Mutex::new(wire::DeviceCursor::default()),
            root_ready: AtomicBool::new(true),
            rotation: AtomicU32::new(0),
            width: AtomicU32::new(720),
            height: AtomicU32::new(1440),
            local: reqwest::Client::new(),
            vision: PathBuf::from("/bin/cat"),
            grounding: Mutex::new(GroundingState::default()),
        };
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            ocr(&runner, input.as_bytes(), 720, 1440),
        )
        .await;
        runner.vision = helper;
        let launch_error = ocr(&runner, b"fixture bytes", 720, 1440)
            .await
            .err()
            .expect("A non-executable OCR helper must fail without executing anything");
        std::fs::remove_dir_all(root).unwrap();
        assert!(launch_error.contains("PermissionDenied"), "{launch_error}");
        let (targets, grid) = result
            .expect("OCR must see stdin EOF without waiting for timeout")
            .unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].element.text, "设置");
        assert_eq!(targets[0].element.source, "ocr");
        assert!(!targets[0].element.clickable);
        assert_eq!(grid.len(), 2048);
    }
    #[test]
    fn native_targets_decode_text_inherit_disabled_and_clip_to_visible_parent() {
        let xml = r#"<hierarchy><node package="test.app" bounds="[0,0][720,1440]" enabled="true">
          <node class="android.widget.ScrollView" scrollable="true" bounds="[0,100][720,1200]">
            <node class="Row" clickable="true" bounds="[0,110][720,220]" resource-id="row"><node text="设置 &amp; 更多" bounds="[20,120][200,200]"/></node>
            <node clickable="true" text="下方" bounds="[0,1180][720,1300]"/>
            <node clickable="true" text="不可见" bounds="[0,1300][720,1400]"/>
            <node enabled="false" bounds="[0,300][720,400]"><node clickable="true" enabled="true" text="禁止" bounds="[0,300][720,400]"/></node>
          </node><node package="com.android.systemui" text="12:34" bounds="[0,0][100,50]"/></node></hierarchy>"#;
        let targets = parse_native(xml, "test.app/.Activity", 720, 1440);
        assert!(targets.iter().any(|t| t.element.scrollable));
        assert!(
            targets
                .iter()
                .any(|t| t.element.text == "设置 & 更多" && t.element.clickable)
        );
        let partial = targets.iter().find(|t| t.element.text == "下方").unwrap();
        assert_eq!(partial.element.bounds.as_ref().unwrap().bottom, 1200);
        for absent in ["不可见", "禁止", "12:34"] {
            assert!(!targets.iter().any(|t| t.element.text == absent));
        }
        assert!(parse_native("<broken", "test.app/.Activity", 720, 1440).is_empty());
    }
    #[test]
    fn identical_labels_keep_separate_observed_positions_and_readonly_is_not_clickable() {
        let xml = r#"<hierarchy><node package="test.app" bounds="[0,0][720,1440]"><node clickable="true" text="打开" bounds="[10,100][100,200]"/><node clickable="true" text="打开" bounds="[200,100][300,200]"/><node text="说明" bounds="[10,300][100,400]"/></node></hierarchy>"#;
        let targets = parse_native(xml, "test.app/.Activity", 720, 1440);
        assert_eq!(targets.len(), 3);
        assert_ne!(targets[0].key, targets[1].key);
        assert_ne!(targets[0].element.id, targets[1].element.id);
        assert!(!targets[2].element.clickable);
    }
    #[test]
    fn browser_toolbar_and_empty_webview_need_ocr_for_page_content() {
        let xml = r#"<hierarchy><node package="browser.app" bounds="[0,0][720,1440]"><node class="android.widget.EditText" text="https://local.test" bounds="[0,50][700,150]"/><node class="android.webkit.WebView" scrollable="true" bounds="[0,150][720,1300]"/></node></hierarchy>"#;
        let targets = parse_native(xml, "browser.app/.Main", 720, 1440);
        assert!(has_native_targets(&targets));
        assert!(
            !native_content_available(&targets, 1440),
            "toolbar and WebView alone do not locate canvas text"
        );
    }
    #[test]
    fn controls_and_mutations_expire_snapshot_even_when_labels_stay_same() {
        let mut s = Snapshot {
            id: "one".into(),
            epoch: 3,
            revision: 4,
            app: "test".into(),
            width: 720,
            height: 1440,
            created: Instant::now(),
            targets: vec![],
            native_hash: String::new(),
            grid: vec![],
        };
        assert!(validate_snapshot(&s, 3, 4).is_ok());
        assert!(validate_snapshot(&s, 4, 4).is_err());
        assert!(validate_snapshot(&s, 3, 5).is_err());
        s.created = Instant::now() - Duration::from_secs(61);
        assert!(validate_snapshot(&s, 3, 4).is_err());
    }
    #[test]
    fn gestures_stay_inside_container_and_follow_content_direction() {
        let b = wire::PhoneBounds {
            left: 100,
            top: 200,
            right: 600,
            bottom: 1100,
        };
        let down = scroll_gesture(&b, "down").unwrap();
        assert!(down.y1 > down.y2);
        let up = scroll_gesture(&b, "up").unwrap();
        assert!(up.y1 < up.y2);
        for dir in ["up", "down", "left", "right"] {
            let a = scroll_gesture(&b, dir).unwrap();
            assert!(
                a.x1 > b.left
                    && a.x1 < b.right
                    && a.x2 > b.left
                    && a.x2 < b.right
                    && a.y1 > b.top
                    && a.y1 < b.bottom
                    && a.y2 > b.top
                    && a.y2 < b.bottom
            );
        }
        assert!(scroll_gesture(&b, "diagonal").is_err());
    }
    #[test]
    fn ocr_visual_guard_tolerates_small_noise_but_rejects_changed_content() {
        let a = vec![120; 2048];
        let mut b = vec![122; 2048];
        assert!(similar_grid(&a, &b));
        b[0..100].fill(0);
        assert!(similar_grid(&a, &b));
        b[700..1200].fill(200);
        assert!(!similar_grid(&a, &b));
        assert!(!similar_grid(&a, &[]));
        assert_eq!(
            foreground_from_dump("mCurrentFocus=Window{abc u0 com.test/.Main}"),
            Some("com.test/.Main".into())
        );
        assert!(foreground_from_dump("mCurrentFocus=null").is_none());
        assert_eq!(
            foreground_from_dump(
                "Display: mDisplayId=2\n mCurrentFocus=Window{abc u0 other.app/.Hidden}\n Display: mDisplayId=0 (organized)\n mCurrentFocus=Window{def u0 com.test/.Main}"
            ),
            Some("com.test/.Main".into())
        );
    }
}
