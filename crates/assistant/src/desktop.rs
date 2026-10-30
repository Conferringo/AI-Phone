use crate::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
type Result<T> = std::result::Result<T, String>;
const CONTAINER: &str = "ai-phone-desktop";
const LIMIT: usize = 64 * 1024 * 1024;
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn uuid(s: &str) -> Result<Uuid> {
    Uuid::parse_str(s).map_err(|_| "无效操作编号".into())
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
async fn process(
    mut command: Command,
    input: Vec<u8>,
    limit: usize,
    seconds: u64,
) -> Result<Vec<u8>> {
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let work = async {
        let write = async {
            stdin.write_all(&input).await?;
            drop(stdin);
            Ok::<_, std::io::Error>(())
        };
        let read = async {
            let mut data = vec![];
            stdout
                .take((limit + 1) as u64)
                .read_to_end(&mut data)
                .await?;
            Ok::<_, std::io::Error>(data)
        };
        let errors = async {
            let mut data = vec![];
            let mut stream = stderr;
            let mut chunk = [0u8; 8192];
            loop {
                let n = stream.read(&mut chunk).await?;
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&chunk[..n]);
                if data.len() > 65536 {
                    data.drain(..data.len() - 65536);
                }
            }
            Ok::<_, std::io::Error>(data)
        };
        let (_, out, err) = tokio::try_join!(write, read, errors).map_err(|e| e.to_string())?;
        if out.len() > limit {
            return Err("结果超出大小限制".into());
        }
        let status = child.wait().await.map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(
                String::from_utf8_lossy(if out.is_empty() { &err } else { &out })
                    .chars()
                    .take(2000)
                    .collect(),
            );
        }
        Ok(out)
    };
    tokio::time::timeout(Duration::from_secs(seconds), work)
        .await
        .map_err(|_| "操作超时，结果可能未知，请先观察或核对文件".to_string())?
}
fn docker() -> Command {
    // The installer owns PATH and can select its private container host.
    Command::new("docker")
}
async fn helper(op: &str, args: Value) -> Result<Value> {
    let mut c = docker();
    c.args([
        "exec",
        "-i",
        "-u",
        "ai",
        CONTAINER,
        "/usr/local/bin/desktop-bridge",
        op,
    ]);
    let out = process(
        c,
        args.to_string().into_bytes(),
        12 * 1024 * 1024,
        if op == "run" { 95 } else { 15 },
    )
    .await?;
    serde_json::from_slice(&out).map_err(|e| e.to_string())
}
pub async fn notifications(app: &App, device: &str) -> Result<wire::PhoneNotificationList> {
    available(app, device).await?;
    let value = helper("notifications", json!({})).await?;
    let events = value["notifications"]
        .as_array()
        .ok_or("电脑通知格式无效")?;
    Ok(wire::PhoneNotificationList {
        connected: value["connected"].as_bool().unwrap_or(false),
        checked_at_ms: now(),
        notifications: events
            .iter()
            .rev()
            .take(256)
            .map(|event| {
                let string = |name: &str| event[name].as_str().unwrap_or("").to_string();
                wire::PhoneNotification {
                    event_id: string("event_id"),
                    notification_key: string("notification_key"),
                    package_name: string("package_name"),
                    app_name: string("app_name"),
                    title: string("title"),
                    text: string("text"),
                    posted_at_ms: event["posted_at_ms"].as_u64().unwrap_or(0),
                }
            })
            .collect(),
        ..Default::default()
    })
}
async fn adb(app: &App, device: &str, args: &[&str], limit: usize) -> Result<Vec<u8>> {
    adb_bounded(app, device, args, limit, 60).await
}
async fn adb_bounded(
    app: &App,
    device: &str,
    args: &[&str],
    limit: usize,
    seconds: u64,
) -> Result<Vec<u8>> {
    let target = crate::phone_system::target(app, device).await?;
    let mut c = Command::new(app.root.join("scripts/android-adb"));
    c.env("AI_PHONE_SERIAL", target.serial);
    c.args(args);
    process(c, vec![], limit, seconds).await
}
pub async fn available(app: &App, device: &str) -> Result<()> {
    let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_runtimes r JOIN devices d ON d.id=r.device_id WHERE d.owner_id='local-user' AND r.device_id=$1 AND r.instance_ref=$2 AND d.runtime_state='managed')").bind(device).bind(CONTAINER).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
    if !ok {
        return Err("电脑尚未接入".into());
    }
    Ok(())
}
pub async fn authorized(app: &App, run: Uuid, device: &str) -> Result<()> {
    let yes:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM run_device_bindings b JOIN runs r ON r.id=b.run_id JOIN agent_device_bindings current ON current.agent_id=r.agent_id AND current.device_id=b.device_id WHERE b.run_id=$1 AND b.device_id=$2)").bind(run).bind(device).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
    if yes {
        Ok(())
    } else {
        Err("该任务没有此设备的授权".into())
    }
}
pub async fn file_authorized(app: &App, run: Uuid, device: &str) -> Result<()> {
    if authorized(app, run, device).await.is_ok() {
        return Ok(());
    }
    let connected:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM run_device_bindings b JOIN runs r ON r.id=b.run_id JOIN agent_device_bindings a ON a.agent_id=r.agent_id AND a.device_id=b.device_id JOIN device_links l ON l.connected AND ((l.source_device_id=b.device_id AND l.target_device_id=$2) OR(l.target_device_id=b.device_id AND l.source_device_id=$2)) WHERE b.run_id=$1)").bind(run).bind(device).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
    if connected {
        Ok(())
    } else {
        Err("设备没有绑定给此Agent，数据线也未连接".into())
    }
}
async fn save_observation(
    app: &App,
    device: &str,
    epoch: i64,
    mut value: Value,
) -> Result<wire::DesktopObservation> {
    let png = STANDARD
        .decode(value["png"].take().as_str().ok_or("电脑没有返回画面")?)
        .map_err(|e| e.to_string())?;
    let id = Uuid::now_v7();
    let dir = app.root.join("data/desktop/observations");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let path = dir.join(format!("{id}.png"));
    tokio::fs::write(&path, &png)
        .await
        .map_err(|e| e.to_string())?;
    let title = value["title"].as_str().unwrap_or("Linux desktop");
    sqlx::query("INSERT INTO desktop_observations(id,device_id,epoch,title,png_path,sha256) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(device).bind(epoch).bind(title).bind(path.to_string_lossy().to_string()).bind(hash(&png)).execute(&app.db).await.map_err(|e|e.to_string())?;
    Ok(wire::DesktopObservation {
        device_id: device.into(),
        observation_id: id.to_string(),
        epoch: epoch as u64,
        captured_at_ms: now(),
        width: 1280,
        height: 800,
        title: title.into(),
        png,
    })
}
async fn control(app: &App, device: &str, run: Option<Uuid>, take: bool) -> Result<i64> {
    available(app, device).await?;
    if let Some(r) = run {
        authorized(app, r, device).await?;
    }
    if take && run.is_none() {
        crate::desktop_explore::pause(app, device, "你已接管电脑，自主探索已暂停").await?;
    }
    let owner = run
        .map(|v| v.to_string())
        .unwrap_or_else(|| "manual".into());
    let mut tx = app.db.begin().await.map_err(|e| e.to_string())?;
    let row=sqlx::query("SELECT epoch,controller,extract(epoch FROM now()-touched_at)::bigint idle FROM desktop_controls WHERE device_id=$1 FOR UPDATE").bind(device).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
    let old: String = row.get("controller");
    let mut epoch: i64 = row.get("epoch");
    if take && old != owner {
        if run.is_some() && !old.is_empty() && row.get::<i64, _>("idle") < 15 {
            return Err("电脑正在被用户或其他任务操作，请等待后重新观察".into());
        }
        epoch += 1;
    }
    if take {
        sqlx::query("UPDATE desktop_controls SET epoch=$2,controller=$3,touched_at=now() WHERE device_id=$1").bind(device).bind(epoch).bind(owner).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(epoch)
}
pub async fn observe(
    app: &App,
    device: &str,
    run: Option<Uuid>,
    take: bool,
) -> Result<wire::DesktopObservation> {
    let epoch = control(app, device, run, take).await?;
    let value = helper("observe", json!({})).await?;
    save_observation(app, device, epoch, value).await
}
fn action_value(r: &wire::DesktopActionRequest) -> Result<Value> {
    if !["click", "drag", "scroll", "key", "text"].contains(&r.kind.as_str()) {
        return Err("未知桌面操作".into());
    }
    if ["click", "drag", "scroll"].contains(&r.kind.as_str())
        && (!(0..1280).contains(&r.x) || !(0..800).contains(&r.y))
    {
        return Err("坐标超出屏幕".into());
    }
    if r.kind == "drag" && (!(0..1280).contains(&r.to_x) || !(0..800).contains(&r.to_y)) {
        return Err("拖拽终点超出屏幕".into());
    }
    if r.text.len() > 64000 || r.key.len() > 80 || r.button > 3 || r.clicks > 2 {
        return Err("无效输入".into());
    }
    Ok(
        json!({"kind":r.kind,"x":r.x,"y":r.y,"to_x":r.to_x,"to_y":r.to_y,"amount":r.amount,"button":r.button.max(1),"clicks":r.clicks.max(1),"key":r.key,"text":r.text}),
    )
}
fn observation_meta(o: &wire::DesktopObservation) -> Value {
    json!({"success":true,"device_id":o.device_id,"observation_id":o.observation_id,"epoch":o.epoch,"captured_at_ms":o.captured_at_ms,"width":o.width,"height":o.height,"title":o.title})
}
async fn stored_observation(app: &App, id: &str) -> Result<wire::DesktopObservation> {
    let r=sqlx::query("SELECT *, (extract(epoch FROM captured_at)*1000)::bigint ms FROM desktop_observations WHERE id=$1").bind(uuid(id)?).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
    Ok(wire::DesktopObservation {
        device_id: r.get("device_id"),
        observation_id: id.into(),
        epoch: r.get::<i64, _>("epoch") as u64,
        captured_at_ms: r.get::<i64, _>("ms") as u64,
        width: 1280,
        height: 800,
        title: r.get("title"),
        png: tokio::fs::read(r.get::<String, _>("png_path"))
            .await
            .map_err(|e| e.to_string())?,
    })
}
async fn cursor_write(app: &App, value: &wire::DeviceCursor) -> Result<()> {
    let folder = app.root.join("data/desktop/cursors");
    tokio::fs::create_dir_all(&folder)
        .await
        .map_err(|e| e.to_string())?;
    let path = folder.join(format!("{}.pb", value.device_id));
    let temp = path.with_extension("tmp");
    tokio::fs::write(&temp, value.encode_to_vec())
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::rename(temp, path)
        .await
        .map_err(|e| e.to_string())
}
async fn cursor_read(app: &App, device: &str) -> Result<wire::DeviceCursor> {
    available(app, device).await?;
    match tokio::fs::read(
        app.root
            .join("data/desktop/cursors")
            .join(format!("{device}.pb")),
    )
    .await
    {
        Ok(bytes) => {
            let mut current =
                wire::DeviceCursor::decode(bytes.as_slice()).map_err(|e| e.to_string())?;
            current.reported_at_ms = now();
            current.paths.clear();
            current.input_confirmed = false;
            if let Ok(bytes) =
                tokio::fs::read(app.root.join("data/desktop/home/.aiphone/cursor.json")).await
                && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                && value["action_id"] == current.action_id
                && let Some(points) = value["points"].as_array()
            {
                let points = points
                    .iter()
                    .filter_map(|p| {
                        Some(wire::GesturePoint {
                            x: u32::try_from(p["x"].as_u64()?).ok()?,
                            y: u32::try_from(p["y"].as_u64()?).ok()?,
                            at_ms: u32::try_from(p["at_ms"].as_u64()?).ok()?,
                        })
                    })
                    .filter(|p| p.x < 1280 && p.y < 800)
                    .take(128)
                    .collect::<Vec<_>>();
                current.input_confirmed = !points.is_empty();
                if let Some(last) = points.last() {
                    current.duration_ms = last.at_ms;
                    current.paths.push(wire::FingerPath {
                        pointer_id: 0,
                        points,
                    });
                }
            }
            Ok(current)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(wire::DeviceCursor::default()),
        Err(e) => Err(e.to_string()),
    }
}
pub async fn action(
    app: &App,
    r: wire::DesktopActionRequest,
    run: Option<Uuid>,
) -> Result<wire::DesktopObservation> {
    available(app, &r.device_id).await?;
    if let Some(id) = run {
        authorized(app, id, &r.device_id).await?;
    }
    let mut input = action_value(&r)?;
    let id = uuid(&r.action_id)?;
    let fingerprint = hash(&r.encode_to_vec());
    input["action_id"] = json!(r.action_id);
    let mut tx = app.db.begin().await.map_err(|e| e.to_string())?;
    let lock =
        sqlx::query("SELECT epoch,controller FROM desktop_controls WHERE device_id=$1 FOR UPDATE")
            .bind(&r.device_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    if let Some(old) =
        sqlx::query("SELECT input_hash,status,result FROM desktop_actions WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
    {
        if old.get::<String, _>("input_hash") != fingerprint {
            return Err("操作编号已用于不同请求".into());
        }
        if old.get::<String, _>("status") == "completed" {
            let value: Value = old.get("result");
            return stored_observation(app, value["observation_id"].as_str().ok_or("结果不完整")?)
                .await;
        }
        return Err("该操作已有执行记录，结果未知时不能重放，请重新观察".into());
    }
    let expected = run
        .map(|v| v.to_string())
        .unwrap_or_else(|| "manual".into());
    if lock.get::<i64, _>("epoch") != r.epoch as i64
        || lock.get::<String, _>("controller") != expected
    {
        return Err("电脑控制权已变化，请重新观察或接管".into());
    }
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM desktop_observations WHERE id=$1 AND device_id=$2 AND epoch=$3 AND captured_at>now()-interval '45 seconds' AND id=(SELECT id FROM desktop_observations WHERE device_id=$2 AND epoch=$3 ORDER BY captured_at DESC,id DESC LIMIT 1))").bind(uuid(&r.observation_id)?).bind(&r.device_id).bind(r.epoch as i64).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
    if !valid {
        return Err("观察已过期或不属于该电脑，请先computer_observe".into());
    }
    // Intent is committed independently before the side effect. A crash cannot replay it.
    sqlx::query("INSERT INTO desktop_actions(id,device_id,run_id,input_hash,status) VALUES($1,$2,$3,$4,'running')").bind(id).bind(&r.device_id).bind(run).bind(fingerprint).execute(&app.db).await.map_err(|e|e.to_string())?;
    let duration = if r.kind == "drag" { 300 } else { 120 };
    let point = |x: i32, y: i32, at_ms| wire::GesturePoint {
        x: x as u32,
        y: y as u32,
        at_ms,
    };
    let mut cursor = wire::DeviceCursor {
        device_id: r.device_id.clone(),
        action_id: r.action_id.clone(),
        kind: r.kind.clone(),
        status: "running".into(),
        started_at_ms: now(),
        duration_ms: duration,
        width: 1280,
        height: 800,
        actor: expected,
        input_confirmed: false,
        reported_at_ms: now(),
        paths: if ["click", "drag", "scroll"].contains(&r.kind.as_str()) {
            vec![wire::FingerPath {
                pointer_id: 0,
                points: vec![
                    point(r.x, r.y, 0),
                    point(
                        if r.kind == "drag" { r.to_x } else { r.x },
                        if r.kind == "drag" { r.to_y } else { r.y },
                        duration,
                    ),
                ],
            }]
        } else {
            vec![]
        },
    };
    cursor_write(app, &cursor).await?;
    let result = match helper("action", input).await {
        Ok(value) => save_observation(app, &r.device_id, r.epoch as i64, value).await,
        Err(e) => Err(e),
    };
    cursor.status = if result.is_ok() {
        "completed"
    } else {
        "unknown"
    }
    .into();
    cursor_write(app, &cursor).await?;
    match &result {
        Ok(o) => {
            sqlx::query("UPDATE desktop_actions SET status='completed',result=$2,finished_at=now() WHERE id=$1").bind(id).bind(observation_meta(o)).execute(&app.db).await.map_err(|e|e.to_string())?;
        }
        Err(e) => {
            sqlx::query("UPDATE desktop_actions SET status='unknown',result=$2,finished_at=now() WHERE id=$1").bind(id).bind(json!({"error":e})).execute(&app.db).await.map_err(|e|e.to_string())?;
        }
    }
    sqlx::query("UPDATE desktop_controls SET touched_at=now() WHERE device_id=$1")
        .bind(&r.device_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    result
}
fn safe_name(name: &str) -> Result<String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
        || name.len() > 240
    {
        return Err("无效文件名".into());
    }
    Ok(name.into())
}
pub(crate) async fn device_kind(app: &App, id: &str) -> Result<String> {
    let row =
        sqlx::query("SELECT kind,runtime_state FROM devices WHERE id=$1 AND owner_id='local-user'")
            .bind(id)
            .fetch_optional(&app.db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("设备不存在")?;
    if row.get::<String, _>("runtime_state") != "managed" {
        return Err("设备尚未安装".into());
    }
    let kind: String = row.get("kind");
    if kind == "phone" {
        crate::phone_system::target(app, id).await?;
    }
    if kind == "desktop" {
        available(app, id).await?;
    }
    Ok(kind)
}
async fn desktop_path(app: &App, value: &str) -> Result<PathBuf> {
    let base = app
        .root
        .join("data/desktop/home/Files")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let rel = value.strip_prefix("/home/ai/Files/").unwrap_or(value);
    let p = base.join(rel).canonicalize().map_err(|_| "文件不存在")?;
    if !p.starts_with(&base) {
        return Err("路径超出电脑工作文件目录".into());
    }
    Ok(p)
}
async fn phone_path(app: &App, device: &str, value: &str) -> Result<String> {
    if (value != "/sdcard" && !value.starts_with("/sdcard/")) || value.chars().any(char::is_control)
    {
        return Err("手机文件必须位于/sdcard共享存储".into());
    }
    let out = adb(
        app,
        device,
        &["shell", &format!("readlink -f {}", quote(value))],
        4096,
    )
    .await?;
    let real = String::from_utf8_lossy(&out).trim().to_string();
    if real != "/storage/emulated/0"
        && real != "/sdcard"
        && !real.starts_with("/storage/emulated/0/")
        && !real.starts_with("/sdcard/")
    {
        return Err("文件指向共享存储之外".into());
    }
    Ok(real)
}
pub async fn files(app: &App, r: wire::DeviceFilesRequest) -> Result<wire::DeviceFiles> {
    let kind = device_kind(app, &r.device_id).await?;
    if kind == "desktop" {
        let v = helper("files", json!({"path":r.path})).await?;
        return Ok(wire::DeviceFiles {
            files: v["files"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|v| wire::DeviceFile {
                    name: v["name"].as_str().unwrap_or("").into(),
                    path: v["path"].as_str().unwrap_or("").into(),
                    directory: v["directory"].as_bool().unwrap_or(false),
                    bytes: v["bytes"].as_u64().unwrap_or(0),
                })
                .collect(),
        });
    }
    let path = if r.path.is_empty() {
        "/sdcard/Download".into()
    } else {
        r.path
    };
    phone_path(app, &r.device_id, &path).await?;
    let script = format!(
        "find {} -mindepth 1 -maxdepth 1 -exec stat -c '%F|%s|%n' {{}} \\;",
        quote(&path)
    );
    let data = adb(app, &r.device_id, &["shell", &script], 65536).await?;
    Ok(wire::DeviceFiles {
        files: String::from_utf8_lossy(&data)
            .lines()
            .filter_map(|l| {
                let (kind, remaining) = l.split_once('|')?;
                let (b, p) = remaining.split_once('|')?;
                Some(wire::DeviceFile {
                    name: p.rsplit('/').next()?.into(),
                    path: p.into(),
                    directory: kind == "directory",
                    bytes: b.parse().ok()?,
                })
            })
            .take(100)
            .collect(),
    })
}
pub async fn link(app: &App, r: wire::DeviceLinkRequest) -> Result<()> {
    let a = device_kind(app, &r.source_device_id).await?;
    let b = device_kind(app, &r.target_device_id).await?;
    if a == b {
        return Err("数据线需要一台手机和一台电脑".into());
    }
    let mut ids = [r.source_device_id, r.target_device_id];
    ids.sort();
    sqlx::query("INSERT INTO device_links(id,source_device_id,target_device_id,connected) VALUES($1,$2,$3,$4) ON CONFLICT(source_device_id,target_device_id) DO UPDATE SET connected=excluded.connected,revision=device_links.revision+CASE WHEN device_links.connected<>excluded.connected THEN 1 ELSE 0 END,updated_at=now()")
 .bind(Uuid::now_v7()).bind(&ids[0]).bind(&ids[1]).bind(r.connected).execute(&app.db).await.map_err(|e|e.to_string())?;
    Ok(())
}
fn transfer_row(r: &sqlx::postgres::PgRow) -> wire::DeviceTransfer {
    wire::DeviceTransfer {
        id: r.get::<Uuid, _>("id").to_string(),
        source_device_id: r.get("source_device_id"),
        target_device_id: r.get("target_device_id"),
        source_path: r.get("source_path"),
        target_path: r.get("target_path"),
        name: r.get("name"),
        bytes: r.get::<i64, _>("bytes") as u64,
        sha256: r.get("sha256"),
        status: r.get("status"),
        error: r.get("error"),
        created_at_ms: r.get::<i64, _>("ms") as u64,
    }
}
pub async fn state(app: &App) -> Result<wire::DeviceLinkState> {
    let links = sqlx::query("SELECT * FROM device_links ORDER BY updated_at DESC")
        .fetch_all(&app.db)
        .await
        .map_err(|e| e.to_string())?;
    let transfers=sqlx::query("SELECT *, (extract(epoch FROM created_at)*1000)::bigint ms FROM device_transfers ORDER BY created_at DESC LIMIT 30").fetch_all(&app.db).await.map_err(|e|e.to_string())?;
    Ok(wire::DeviceLinkState {
        links: links
            .iter()
            .map(|r| wire::DeviceLink {
                id: r.get::<Uuid, _>("id").to_string(),
                source_device_id: r.get("source_device_id"),
                target_device_id: r.get("target_device_id"),
                connected: r.get("connected"),
                revision: r.get::<i64, _>("revision") as u64,
            })
            .collect(),
        transfers: transfers.iter().map(transfer_row).collect(),
        jobs: provision_jobs(app).await?,
        messages: crate::device_exchange::list(app).await?,
    })
}
async fn begin_transfer_publication(
    app: &App,
    id: Uuid,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>> {
    let mut tx = app.db.begin().await.map_err(|e| e.to_string())?;
    let t=sqlx::query("SELECT t.link_revision,l.connected,l.revision current_revision FROM device_transfers t JOIN device_links l ON l.id=t.link_id WHERE t.id=$1 FOR UPDATE OF l")
        .bind(id).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
    if !t.get::<bool, _>("connected")
        || t.get::<i64, _>("current_revision") != t.get::<i64, _>("link_revision")
    {
        return Err("数据线已断开或改变".into());
    }
    // Commit uncertainty before the external rename, independently of this link lock.
    // A lost reply or interrupted database commit must never claim the file was not sent.
    sqlx::query("UPDATE device_transfers SET status='unknown',error='交付结果尚未核对，请先检查目标文件；未自动重发' WHERE id=$1")
        .bind(id).execute(&app.db).await.map_err(|e| e.to_string())?;
    Ok(tx)
}

async fn perform_transfer(app: &App, id: Uuid) -> Result<()> {
    // Prepare and verify the bytes without a long-lived transaction or link lock.
    let t = sqlx::query("SELECT * FROM device_transfers WHERE id=$1")
        .bind(id)
        .fetch_one(&app.db)
        .await
        .map_err(|e| e.to_string())?;
    let source: String = t.get("source_device_id");
    let target: String = t.get("target_device_id");
    let src: String = t.get("source_path");
    let dst: String = t.get("target_path");
    if let Some(run) = t.get::<Option<Uuid>, _>("run_id") {
        file_authorized(app, run, &source).await?;
        file_authorized(app, run, &target).await?;
    }
    sqlx::query("UPDATE device_transfers SET status='copying' WHERE id=$1")
        .bind(id)
        .execute(&app.db)
        .await
        .map_err(|e| e.to_string())?;
    let bytes = if device_kind(app, &source).await? == "phone" {
        let p = phone_path(app, &source, &src).await?;
        let size = adb(
            app,
            &source,
            &["shell", &format!("stat -c %s {}", quote(&p))],
            1024,
        )
        .await?;
        let size = String::from_utf8_lossy(&size)
            .trim()
            .parse::<usize>()
            .map_err(|_| "无法读取文件大小")?;
        if size > LIMIT {
            return Err("文件超过64MiB".into());
        }
        adb(app, &source, &["exec-out", "cat", &p], LIMIT).await?
    } else {
        let p = desktop_path(app, &src).await?;
        if tokio::fs::metadata(&p)
            .await
            .map_err(|e| e.to_string())?
            .len()
            > LIMIT as u64
        {
            return Err("文件超过64MiB".into());
        }
        tokio::fs::read(p).await.map_err(|e| e.to_string())?
    };
    let digest = hash(&bytes);
    let spool = app.root.join("data/desktop/transfers");
    tokio::fs::create_dir_all(&spool)
        .await
        .map_err(|e| e.to_string())?;
    let local = spool.join(id.to_string());
    tokio::fs::write(&local, &bytes)
        .await
        .map_err(|e| e.to_string())?;
    let phone_target = device_kind(app, &target).await? == "phone";
    let (verified, mut publication) = if phone_target {
        let parent = dst.rsplit_once('/').unwrap().0;
        let temp = format!("{dst}.part");
        adb(
            app,
            &target,
            &["shell", &format!("mkdir -p {}", quote(parent))],
            1024,
        )
        .await?;
        phone_path(app, &target, parent).await?;
        adb(
            app,
            &target,
            &["push", local.to_str().unwrap(), &temp],
            4096,
        )
        .await?;
        let out = adb(
            app,
            &target,
            &["shell", &format!("sha256sum {}", quote(&temp))],
            1024,
        )
        .await?;
        let found = String::from_utf8_lossy(&out)
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        if found != digest {
            return Err("手机写入校验失败".into());
        }
        let publication = begin_transfer_publication(app, id).await?;
        // Only the final rename is serialized with disconnect; bound it below the
        // runtime's 30-second idle transaction deadline. Slow copies happen above.
        adb_bounded(
            app,
            &target,
            &[
                "shell",
                &format!(
                    "test ! -e {} && mv {} {}",
                    quote(&dst),
                    quote(&temp),
                    quote(&dst)
                ),
            ],
            1024,
            5,
        )
        .await?;
        (found, publication)
    } else {
        let base = app.root.join("data/desktop/home/Files");
        let final_path = base.join(&dst);
        let parent = final_path.parent().unwrap();
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
        let canonical_base = base.canonicalize().map_err(|e| e.to_string())?;
        if !parent
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(&canonical_base)
        {
            return Err("目标路径超出工作文件目录".into());
        }
        let temp = final_path.with_extension("part");
        tokio::fs::write(&temp, &bytes)
            .await
            .map_err(|e| e.to_string())?;
        let found = hash(&tokio::fs::read(&temp).await.map_err(|e| e.to_string())?);
        if found != digest {
            return Err("电脑写入校验失败".into());
        }
        if final_path.exists() {
            return Err("目标文件已存在，未覆盖".into());
        }
        let publication = begin_transfer_publication(app, id).await?;
        tokio::fs::rename(temp, &final_path)
            .await
            .map_err(|e| e.to_string())?;
        (found, publication)
    };
    sqlx::query("UPDATE device_transfers SET status='completed',bytes=$2,sha256=$3,error='',finished_at=now() WHERE id=$1").bind(id).bind(bytes.len()as i64).bind(verified).execute(&mut *publication).await.map_err(|e|e.to_string())?;
    publication.commit().await.map_err(|e| e.to_string())?;
    // Media indexing is optional and runs after the durable completion receipt.
    if phone_target {
        let _ = adb_bounded(
            app,
            &target,
            &[
                "shell",
                &format!(
                    "am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d {}",
                    quote(&format!("file://{dst}"))
                ),
            ],
            4096,
            5,
        )
        .await;
    }
    let _ = tokio::fs::remove_file(local).await;
    Ok(())
}
pub async fn transfer(
    app: &App,
    r: wire::TransferRequest,
    run: Option<Uuid>,
) -> Result<wire::DeviceTransfer> {
    let id = uuid(&r.request_id)?;
    let a = device_kind(app, &r.source_device_id).await?;
    let b = device_kind(app, &r.target_device_id).await?;
    if a == b {
        return Err("请选择手机与电脑之间的传输".into());
    }
    if let Some(run) = run {
        file_authorized(app, run, &r.source_device_id).await?;
        file_authorized(app, run, &r.target_device_id).await?;
    }
    let name = safe_name(if r.target_name.is_empty() {
        r.source_path.rsplit('/').next().unwrap_or("")
    } else {
        &r.target_name
    })?;
    let mut tx = app.db.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(old)=sqlx::query("SELECT *, (extract(epoch FROM created_at)*1000)::bigint ms FROM device_transfers WHERE id=$1").bind(id).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?{let old=transfer_row(&old);if old.source_device_id!=r.source_device_id||old.target_device_id!=r.target_device_id||old.source_path!=r.source_path||old.name!=name{return Err("传输编号已用于另一请求".into());}return Ok(old);}
    let mut ids = [r.source_device_id.clone(), r.target_device_id.clone()];
    ids.sort();
    let l=sqlx::query("SELECT id,revision FROM device_links WHERE source_device_id=$1 AND target_device_id=$2 AND connected FOR UPDATE").bind(&ids[0]).bind(&ids[1]).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?.ok_or("请先连接两台设备的数据线")?;
    let target = if b == "phone" {
        format!("/sdcard/Download/AI Phone/{id}/{name}")
    } else {
        format!("Received/{id}/{name}")
    };
    sqlx::query("INSERT INTO device_transfers(id,link_id,link_revision,run_id,source_device_id,target_device_id,source_path,target_path,name,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'pending')").bind(id).bind(l.get::<Uuid,_>("id")).bind(l.get::<i64,_>("revision")).bind(run).bind(&r.source_device_id).bind(&r.target_device_id).bind(&r.source_path).bind(target).bind(name).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    if let Err(e) = perform_transfer(app, id).await {
        sqlx::query("UPDATE device_transfers SET status=CASE WHEN status='unknown' THEN 'unknown' ELSE 'failed' END,error=CASE WHEN status='unknown' THEN '交付结果尚未核对，请先检查目标文件；未自动重发：'||$2 ELSE $2 END,finished_at=now() WHERE id=$1 AND status<>'completed'").bind(id).bind(&e).execute(&app.db).await.map_err(|e|e.to_string())?;
    }
    let row=sqlx::query("SELECT *, (extract(epoch FROM created_at)*1000)::bigint ms FROM device_transfers WHERE id=$1").bind(id).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
    Ok(transfer_row(&row))
}
pub async fn route(
    axum::extract::State(app): axum::extract::State<Shared>,
    axum::extract::Path(op): axum::extract::Path<String>,
    headers: HeaderMap,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Response {
    if let Err(e) = auth(&app, &headers) {
        return e.into();
    }
    macro_rules! read {
        ($ty:ty) => {
            match decode::<$ty>(&app, &headers, body) {
                Ok(v) => v,
                Err(e) => return Ok(e.into()),
            }
        };
    }
    let result: Result<Response> = async {
        match op.as_str() {
            "cursor" => {
                let r = read!(wire::DesktopRequest);
                Ok(pb(StatusCode::OK, cursor_read(&app, &r.device_id).await?))
            }

            "explore-state" => {
                let r = read!(wire::DesktopRequest);
                Ok(pb(
                    StatusCode::OK,
                    crate::desktop_explore::state(&app, &r.device_id).await?,
                ))
            }
            "explore-update" => {
                let r = read!(wire::DesktopExploreUpdate);
                Ok(pb(
                    StatusCode::OK,
                    crate::desktop_explore::update(&app, r).await?,
                ))
            }
            "message" => {
                let r = read!(wire::DeviceMessageRequest);
                Ok(pb(
                    StatusCode::OK,
                    crate::device_exchange::send(&app, r, None).await?,
                ))
            }
            "provision" => {
                let r = read!(wire::DesktopProvisionRequest);
                let job = provision(app.clone(), r).await?;
                Ok(pb(StatusCode::OK, job))
            }
            "observe" | "control" => {
                let r = read!(wire::DesktopRequest);
                Ok(pb(
                    StatusCode::OK,
                    observe(&app, &r.device_id, None, op == "control").await?,
                ))
            }
            "action" => Ok(pb(
                StatusCode::OK,
                action(&app, read!(wire::DesktopActionRequest), None).await?,
            )),
            "files" => Ok(pb(
                StatusCode::OK,
                files(&app, read!(wire::DeviceFilesRequest)).await?,
            )),
            "link" => {
                link(&app, read!(wire::DeviceLinkRequest)).await?;
                Ok(pb(StatusCode::OK, state(&app).await?))
            }
            "state" => {
                read!(wire::RegistryRequest);
                Ok(pb(StatusCode::OK, state(&app).await?))
            }
            "transfer" => Ok(pb(
                StatusCode::OK,
                transfer(&app, read!(wire::TransferRequest), None).await?,
            )),
            _ => Ok(error(StatusCode::NOT_FOUND, "not_found", "操作不存在")),
        }
    }
    .await;
    result.unwrap_or_else(|e| error(StatusCode::CONFLICT, "device_operation", &e))
}
fn blocks(o: wire::DesktopObservation) -> Value {
    json!([{"type":"input_text","text":observation_meta(&o).to_string()},{"type":"input_image","image_url":format!("data:image/png;base64,{}",STANDARD.encode(o.png))}])
}
fn plain(value: Value) -> Value {
    json!([{"type":"input_text","text":value.to_string()}])
}
pub async fn tool(app: &App, run: Uuid, call: &Value, args: &Value) -> Result<Value> {
    let name = call["name"].as_str().unwrap_or("");
    let device = args["device_id"].as_str().unwrap_or("");
    if name == "device_message" {
        let result = crate::device_exchange::send(
            app,
            wire::DeviceMessageRequest {
                request_id: call["local_action_id"].as_str().unwrap_or("").into(),
                source_device_id: args["source_device_id"].as_str().unwrap_or("").into(),
                target_device_id: args["target_device_id"].as_str().unwrap_or("").into(),
                text: args["text"].as_str().unwrap_or("").into(),
            },
            Some(run),
        )
        .await?;
        return Ok(plain(
            json!({"success":true,"message_id":result.id,"status":result.status}),
        ));
    }
    if name == "device_list" {
        let rows=sqlx::query("SELECT DISTINCT d.id,d.name,d.kind,d.runtime_state FROM devices d WHERE d.id IN (SELECT device_id FROM run_device_bindings WHERE run_id=$1) OR EXISTS(SELECT 1 FROM device_links l JOIN run_device_bindings b ON b.run_id=$1 AND ((b.device_id=l.source_device_id AND d.id=l.target_device_id) OR(b.device_id=l.target_device_id AND d.id=l.source_device_id)) WHERE l.connected)").bind(run).fetch_all(&app.db).await.map_err(|e|e.to_string())?;
        let links = state(app).await?.links;
        let messages = crate::device_exchange::list(app).await?;
        return Ok(plain(
            json!({"handoffs":messages.iter().filter(|m|rows.iter().any(|r|r.get::<String,_>("id")==m.source_device_id||r.get::<String,_>("id")==m.target_device_id)).map(|m|json!({"message_id":m.id,"source_device_id":m.source_device_id,"target_device_id":m.target_device_id,"status":m.status,"run_id":m.run_id,"error":m.error})).collect::<Vec<_>>(),"links":links.iter().filter(|l|l.connected).map(|l|json!({"source_device_id":l.source_device_id,"target_device_id":l.target_device_id})).collect::<Vec<_>>(),"devices":rows.iter().map(|r|json!({"device_id":r.get::<String,_>("id"),"name":r.get::<String,_>("name"),"kind":r.get::<String,_>("kind"),"runtime_state":r.get::<String,_>("runtime_state")})).collect::<Vec<_>>()}),
        ));
    }
    if name == "device_transfer_status" {
        let id = args["request_id"].as_str().unwrap_or("");
        let row=sqlx::query("SELECT *, (extract(epoch FROM created_at)*1000)::bigint ms FROM device_transfers WHERE id=$1").bind(uuid(id)?).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
        let t = transfer_row(&row);
        file_authorized(app, run, &t.source_device_id).await?;
        file_authorized(app, run, &t.target_device_id).await?;
        return Ok(transfer_value(t));
    }
    if name == "device_link" || name == "device_transfer" {
        let source = args["source_device_id"].as_str().unwrap_or("");
        let target = args["target_device_id"].as_str().unwrap_or("");
        if name == "device_link" {
            authorized(app, run, source).await?;
            authorized(app, run, target).await?;
            link(
                app,
                wire::DeviceLinkRequest {
                    source_device_id: source.into(),
                    target_device_id: target.into(),
                    connected: args["connected"].as_bool().unwrap_or(true),
                },
            )
            .await?;
            return Ok(plain(
                json!({"success":true,"source_device_id":source,"target_device_id":target,"connected":args["connected"].as_bool().unwrap_or(true)}),
            ));
        }
        return Ok(transfer_value(
            transfer(
                app,
                wire::TransferRequest {
                    request_id: call["local_action_id"].as_str().unwrap_or("").into(),
                    source_device_id: source.into(),
                    target_device_id: target.into(),
                    source_path: args["source_path"].as_str().unwrap_or("").into(),
                    target_name: args["target_name"].as_str().unwrap_or("").into(),
                },
                Some(run),
            )
            .await?,
        ));
    }
    if name == "device_files" {
        file_authorized(app, run, device).await?;
        let f = files(
            app,
            wire::DeviceFilesRequest {
                device_id: device.into(),
                path: args["path"].as_str().unwrap_or("").into(),
            },
        )
        .await?;
        return Ok(plain(
            json!({"success":true,"files":f.files.iter().map(|f|json!({"name":f.name,"path":f.path,"directory":f.directory,"bytes":f.bytes})).collect::<Vec<_>>()}),
        ));
    }
    authorized(app, run, device).await?;
    if name == "computer_observe" {
        return Ok(blocks(observe(app, device, Some(run), true).await?));
    }
    if name == "computer_run" {
        control(app, device, Some(run), true).await?;
        return Ok(plain(
            helper("run", json!({"script":args["script"]})).await?,
        ));
    }
    let observation =
        stored_observation(app, args["observation_id"].as_str().unwrap_or("")).await?;
    if name == "computer_zoom" {
        if observation.device_id != device {
            return Err("放大观察不属于当前电脑".into());
        }
        let epoch: i64 =
            sqlx::query_scalar("SELECT epoch FROM desktop_controls WHERE device_id=$1")
                .bind(device)
                .fetch_one(&app.db)
                .await
                .map_err(|e| e.to_string())?;
        if observation.epoch != epoch as u64
            || now().saturating_sub(observation.captured_at_ms) > 60_000
        {
            return Err("观察已过期，请重新computer_observe".into());
        }
        let latest:Uuid=sqlx::query_scalar("SELECT id FROM desktop_observations WHERE device_id=$1 ORDER BY captured_at DESC LIMIT 1").bind(device).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
        if latest.to_string() != observation.observation_id {
            return Err("请放大最新观察，不复用旧图".into());
        }
        let value=helper("zoom",json!({"png":STANDARD.encode(&observation.png),"x":args["x"],"y":args["y"],"width":args["width"],"height":args["height"]})).await?;
        return Ok(
            json!([{"type":"input_text","text":json!({"success":true,"device_id":device,"source_observation_id":observation.observation_id,"crop_origin":[args["x"],args["y"]],"scale":value["scale"],"width":value["width"],"height":value["height"],"coordinate_rule":"操作仍使用全屏坐标：x=局部x/scale+crop_origin[0]，y同理；放大不产生新观察授权。"}).to_string()},{"type":"input_image","image_url":format!("data:image/png;base64,{}",value["png"].as_str().ok_or("放大失败")?)}]),
        );
    }
    let int = |key: &str| args[key].as_i64().unwrap_or(0) as i32;
    let text = |key: &str| args[key].as_str().unwrap_or("").to_string();
    let r = wire::DesktopActionRequest {
        device_id: device.into(),
        action_id: call["local_action_id"].as_str().unwrap_or("").into(),
        observation_id: observation.observation_id,
        epoch: observation.epoch,
        kind: name.trim_start_matches("computer_").replace("type", "text"),
        x: int("x"),
        y: int("y"),
        to_x: int("to_x"),
        to_y: int("to_y"),
        amount: int("amount"),
        button: args["button"].as_u64().unwrap_or(1) as u32,
        clicks: args["clicks"].as_u64().unwrap_or(1) as u32,
        key: text("key"),
        text: text("text"),
    };
    Ok(blocks(action(app, r, Some(run)).await?))
}
fn transfer_value(t: wire::DeviceTransfer) -> Value {
    plain(
        json!({"success":t.status=="completed","request_id":t.id,"status":t.status,"source_device_id":t.source_device_id,"target_device_id":t.target_device_id,"source_path":t.source_path,"target_path":t.target_path,"bytes":t.bytes,"sha256":t.sha256,"error":t.error}),
    )
}

pub fn catalog() -> Vec<Value> {
    fn f(name: &str, desc: &str, props: Value, required: Vec<&str>) -> Value {
        json!({"type":"function","name":name,"description":desc,"parameters":{"type":"object","properties":props,"required":required,"additionalProperties":false}})
    }
    let id = json!({"type":"string"});
    let num = json!({"type":"integer"});
    vec![
        f(
            "device_message",
            "向已连线的另一设备Agent交接任务、文件路径或进展。对方空闲时自动唤醒；排队不等于已处理。禁止无意义循环回复。",
            json!({"source_device_id":id,"target_device_id":id,"text":id}),
            vec!["source_device_id", "target_device_id", "text"],
        ),
        f(
            "device_list",
            "列出本任务可使用的真实设备及安装状态。电脑和手机的工具不可混用。",
            json!({}),
            vec![],
        ),
        f(
            "computer_observe",
            "观察Linux真实桌面，返回截图、窗口标题、尺寸、observation_id与epoch。GUI操作必须基于本轮最新观察。",
            json!({"device_id":id}),
            vec!["device_id"],
        ),
        f(
            "computer_zoom",
            "只读放大当前观察的局部，适合小字、表格和密集控件。返回原图偏移及倍率；点击仍用全屏坐标，不把局部坐标直接送入动作。",
            json!({"device_id":id,"observation_id":id,"x":num,"y":num,"width":num,"height":num}),
            vec!["device_id", "observation_id", "x", "y", "width", "height"],
        ),
        f(
            "computer_click",
            "点击当前电脑画面中的目标；双击clicks=2，右键button=3。动作后自动返回新截图。",
            json!({"device_id":id,"observation_id":id,"x":num,"y":num,"button":num,"clicks":num}),
            vec!["device_id", "observation_id", "x", "y"],
        ),
        f(
            "computer_type",
            "向已聚焦的Linux输入区域粘贴Unicode文本（支持中文）。",
            json!({"device_id":id,"observation_id":id,"text":id}),
            vec!["device_id", "observation_id", "text"],
        ),
        f(
            "computer_key",
            "发送Linux按键，如Return、ctrl+l、ctrl+s、Alt+F4。不能替代观察。",
            json!({"device_id":id,"observation_id":id,"key":id}),
            vec!["device_id", "observation_id", "key"],
        ),
        f(
            "computer_scroll",
            "在指定坐标滚动。amount正数向下，负数向上，最多8格。",
            json!({"device_id":id,"observation_id":id,"x":num,"y":num,"amount":num}),
            vec!["device_id", "observation_id", "x", "y", "amount"],
        ),
        f(
            "computer_drag",
            "按住左键从x/y拖到to_x/to_y；返回真实结果画面。",
            json!({"device_id":id,"observation_id":id,"x":num,"y":num,"to_x":num,"to_y":num}),
            vec!["device_id", "observation_id", "x", "y", "to_x", "to_y"],
        ),
        f(
            "computer_run",
            "在这台Linux桌面容器运行处理命令，工作目录/home/ai/Files；支持Python/openpyxl/LibreOffice，可sudo -n取得客体root安装和配置系统。明确记录为命令执行，不能假称鼠标操作。不能访问macOS。",
            json!({"device_id":id,"script":id}),
            vec!["device_id", "script"],
        ),
        f(
            "device_files",
            "列出设备共享文件。电脑path相对/home/ai/Files，手机默认/sdcard/Download。",
            json!({"device_id":id,"path":id}),
            vec!["device_id"],
        ),
        f(
            "device_link",
            "连接或断开获授权手机与电脑的虚拟数据线（文件通道）。不赋予额外设备权限。",
            json!({"source_device_id":id,"target_device_id":id,"connected":{"type":"boolean"}}),
            vec!["source_device_id", "target_device_id", "connected"],
        ),
        f(
            "device_transfer",
            "沿已连接的数据线传文件，最高64MiB，双端SHA256验证。返回request_id和实际目标路径；completed仅表示文件到达目标设备，绝不代表微信发送或对方收到。",
            json!({"source_device_id":id,"target_device_id":id,"source_path":id,"target_name":id}),
            vec!["source_device_id", "target_device_id", "source_path"],
        ),
        f(
            "device_transfer_status",
            "核对原request_id的传输账本，unknown/pending不是完成，不得重复创建新传输。",
            json!({"request_id":id}),
            vec!["request_id"],
        ),
    ]
}
fn provision_row(r: &sqlx::postgres::PgRow) -> wire::DesktopProvisionJob {
    wire::DesktopProvisionJob {
        id: r.get::<Uuid, _>("id").to_string(),
        device_id: r.get("device_id"),
        status: r.get("status"),
        stage: r.get("stage"),
        error: r.get("error"),
        cpus: r.get::<i32, _>("cpus") as u32,
        memory_mb: r.get::<i32, _>("memory_mb") as u32,
        template_id: r.get("template_id"),
    }
}
async fn provision_jobs(app: &App) -> Result<Vec<wire::DesktopProvisionJob>> {
    Ok(
        sqlx::query("SELECT * FROM desktop_provision_jobs ORDER BY created_at DESC LIMIT 8")
            .fetch_all(&app.db)
            .await
            .map_err(|e| e.to_string())?
            .iter()
            .map(provision_row)
            .collect(),
    )
}
async fn install_step(app: &App, id: Uuid, stage: &str) -> Result<()> {
    sqlx::query("UPDATE desktop_provision_jobs SET stage=$2,updated_at=now() WHERE id=$1")
        .bind(id)
        .bind(stage)
        .execute(&app.db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
async fn install(app: &App, id: Uuid) -> Result<()> {
    let r=sqlx::query("UPDATE desktop_provision_jobs SET status='running',updated_at=now() WHERE id=$1 RETURNING *").bind(id).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
    let device: String = r.get("device_id");
    let cpus = r.get::<i32, _>("cpus");
    let memory = r.get::<i32, _>("memory_mb");
    let mut preflight = docker();
    preflight.args(["info", "--format", "{{.Architecture}} {{.MemTotal}}"]);
    let info = String::from_utf8_lossy(&process(preflight, vec![], 1024, 15).await?).to_string();
    if !matches!(
        info.split_whitespace().next(),
        Some("aarch64" | "arm64" | "x86_64" | "amd64")
    ) {
        return Err("Unsupported Docker CPU architecture".into());
    }
    let mem = info
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse::<u64>()
        .unwrap_or(0);
    if mem < (memory as u64 + 768) * 1024 * 1024 {
        return Err("Docker内存不足，请先提高Docker资源额度".into());
    }
    let mut disk = Command::new("df");
    disk.args(["-k", app.root.to_str().unwrap()]);
    let space = String::from_utf8_lossy(&process(disk, vec![], 4096, 5).await?).to_string();
    if space
        .lines()
        .nth(1)
        .and_then(|l| l.split_whitespace().nth(3))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
        < 6 * 1024 * 1024
    {
        return Err("安装至少需要6GiB可用空间".into());
    }
    let desktop_image = std::env::var("AI_PHONE_DESKTOP_IMAGE")
        .unwrap_or_else(|_| "ai-phone-desktop:20261028".into());
    let workspace_image = crate::workbench::workspace_image();
    let mut inspect = docker();
    inspect.args(["image", "inspect", &desktop_image, "--format", "{{.Id}}"]);
    if process(inspect, vec![], 4096, 15).await.is_err() {
        if std::env::var("AI_PHONE_IMAGE_PULL_POLICY").as_deref() == Ok("never") {
            return Err("Bundled desktop image is missing; run client initialization again".into());
        }
        install_step(app, id, "准备Clash客户端").await?;
        crate::device_network::install_apps(app).await?;
        install_step(app, id, "准备基础镜像").await?;
        let mut base = docker();
        base.args(["image", "inspect", &workspace_image, "--format", "{{.Id}}"]);
        if process(base, vec![], 4096, 15).await.is_err() {
            let mut build = docker();
            build.current_dir(&app.root).args([
                "build",
                "--build-arg",
                &format!(
                    "HTTP_PROXY={}",
                    std::env::var("AI_PHONE_CONTAINER_PROXY").unwrap_or_default()
                ),
                "--build-arg",
                &format!(
                    "HTTPS_PROXY={}",
                    std::env::var("AI_PHONE_CONTAINER_PROXY").unwrap_or_default()
                ),
                "-t",
                &workspace_image,
                "-f",
                "infra/workspace/Dockerfile",
                "infra/workspace",
            ]);
            process(build, vec![], 2 * 1024 * 1024, 1800).await?;
        }
        install_step(app, id, "安装Linux桌面和办公软件").await?;
        let mut build = docker();
        build.current_dir(&app.root).args([
            "compose",
            "-f",
            "compose.desktop.yaml",
            "build",
            "desktop",
        ]);
        process(build, vec![], 2 * 1024 * 1024, 1800).await?;
    }
    install_step(app, id, "启动桌面").await?;
    crate::device_network::prepare(app, &device).await?;
    let mut start = docker();
    start
        .current_dir(&app.root)
        .env("AI_PHONE_DESKTOP_CPUS", cpus.to_string())
        .env("AI_PHONE_DESKTOP_MEMORY", format!("{memory}m"))
        .args([
            "compose",
            "-f",
            "compose.desktop.yaml",
            "up",
            "-d",
            "desktop",
        ]);
    process(start, vec![], 16384, 90).await?;
    install_step(app, id, "验证画面与控制").await?;
    let mut ok = false;
    for _ in 0..30 {
        if helper("observe", json!({})).await.is_ok() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    if !ok {
        return Err("桌面没有返回可验证画面".into());
    }
    let mut tx = app.db.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO device_runtimes(device_id,provider,instance_ref) VALUES($1,'docker-desktop',$2) ON CONFLICT(device_id) DO NOTHING").bind(&device).bind(CONTAINER).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    sqlx::query("INSERT INTO desktop_controls(device_id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(&device)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("UPDATE devices SET runtime_state='managed',system='Debian 13 · Openbox · ARM64',revision=revision+1,updated_at=now() WHERE id=$1").bind(&device).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    sqlx::query("UPDATE desktop_provision_jobs SET status='completed',stage='已就绪',updated_at=now() WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}
async fn provision(
    app: Shared,
    r: wire::DesktopProvisionRequest,
) -> Result<wire::DesktopProvisionJob> {
    if r.template_id != "debian13-openbox"
        || !(1..=4).contains(&r.cpus)
        || !(1536..=4096).contains(&r.memory_mb)
    {
        return Err("系统模板或资源配置无效".into());
    }
    let id = uuid(&r.request_id)?;
    let mut tx = app.db.begin().await.map_err(|e| e.to_string())?;
    let d = sqlx::query(
        "SELECT kind,runtime_state FROM devices WHERE id=$1 AND owner_id='local-user' FOR UPDATE",
    )
    .bind(&r.device_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| e.to_string())?
    .ok_or("设备不存在")?;
    if d.get::<String, _>("kind") != "desktop" {
        return Err("此安装器仅用于Linux电脑".into());
    }
    if let Some(old)=sqlx::query("SELECT * FROM desktop_provision_jobs WHERE id=$1 OR (device_id=$2 AND status IN ('queued','running')) ORDER BY created_at DESC LIMIT 1").bind(id).bind(&r.device_id).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?{if old.get::<Uuid,_>("id")==id&&(old.get::<String,_>("device_id")!=r.device_id||old.get::<i32,_>("cpus")!=r.cpus as i32||old.get::<i32,_>("memory_mb")!=r.memory_mb as i32){return Err("安装编号冲突".into());}return Ok(provision_row(&old));}
    if d.get::<String, _>("runtime_state") == "managed" {
        return Err("此电脑已经安装；保留系统及文件，不重复安装".into());
    }
    let row=sqlx::query("INSERT INTO desktop_provision_jobs(id,device_id,status,cpus,memory_mb,template_id) VALUES($1,$2,'queued',$3,$4,$5) RETURNING *").bind(id).bind(&r.device_id).bind(r.cpus as i32).bind(r.memory_mb as i32).bind(&r.template_id).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    let result = provision_row(&row);
    tokio::spawn(async move {
        if let Err(e) = install(&app, id).await {
            let _=sqlx::query("UPDATE desktop_provision_jobs SET status='failed',error=$2,updated_at=now() WHERE id=$1").bind(id).bind(e).execute(&app.db).await;
        }
    });
    Ok(result)
}
pub async fn recover(app: Shared) {
    let ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM desktop_provision_jobs WHERE status IN ('queued','running')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap_or_default();
    for id in ids {
        let app = app.clone();
        tokio::spawn(async move {
            if let Err(e) = install(&app, id).await {
                let _=sqlx::query("UPDATE desktop_provision_jobs SET status='failed',error=$2,updated_at=now() WHERE id=$1").bind(id).bind(e).execute(&app.db).await;
            }
        });
    }
    let _=sqlx::query("UPDATE device_transfers SET status='unknown',error='服务重启时传输尚未提交，请核对原目标文件，未自动重发' WHERE status IN ('pending','copying')").execute(&app.db).await;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_names_and_coordinates() {
        for name in ["", "..", "../secret", "a/b", "a\\b", "a\nb"] {
            assert!(safe_name(name).is_err());
        }
        assert_eq!(safe_name("测试表格.xlsx").unwrap(), "测试表格.xlsx");
        assert!(
            action_value(&wire::DesktopActionRequest {
                kind: "click".into(),
                x: 1280,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            action_value(&wire::DesktopActionRequest {
                kind: "drag".into(),
                to_y: -1,
                ..Default::default()
            })
            .is_err()
        );
    }
}
