//! Android and Chromium location adapters; own protocol is binary protobuf.
use super::location::now;
use crate::*;
use sqlx::Row;
async fn phone(app: &App, id: &str, fix: wire::LocationFix) -> Result<(), String> {
    let obs: wire::DeviceObservation = worker::phone(
        app,
        "observe",
        wire::DeviceRequest {
            phone_id: id.into(),
            owner_id: "local-user".into(),
            metadata_only: true,
            ..Default::default()
        },
    )
    .await?;
    let response: wire::DeviceActionResponse = worker::phone(
        app,
        "action",
        wire::DeviceActionRequest {
            phone_id: id.into(),
            owner_id: "local-user".into(),
            run_id: if obs.controller == "manual" {
                "location-manual".into()
            } else {
                obs.controller.clone()
            },
            action_id: Uuid::new_v4().to_string(),
            control_epoch: obs.control_epoch,
            controller: obs.controller,
            action: Some(wire::device_action_request::Action::Location(fix)),
            ..Default::default()
        },
    )
    .await?;
    if !response.success || response.result_unknown {
        return Err("手机位置结果未确认，运动已暂停，请检查设备".into());
    }
    Ok(())
}
async fn desktop_bytes(fix: wire::LocationFix) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncWriteExt;
    let bin = if std::path::Path::new("/usr/local/bin/docker").exists() {
        "/usr/local/bin/docker"
    } else {
        "docker"
    };
    let mut child = tokio::process::Command::new(bin)
        .args([
            "exec",
            "-i",
            "-u",
            "ai",
            "ai-phone-desktop",
            "/usr/local/bin/bun",
            "/usr/local/lib/ai-phone/location.js",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "电脑位置服务未启动")?;
    child
        .stdin
        .take()
        .ok_or("电脑输入通道不可用")?
        .write_all(&fix.encode_to_vec())
        .await
        .map_err(|_| "电脑位置通道中断")?;
    let out = tokio::time::timeout(Duration::from_secs(12), child.wait_with_output())
        .await
        .map_err(|_| "电脑位置结果未知，运动已暂停")?
        .map_err(|_| "电脑位置通道中断")?;
    if !out.status.success() {
        return Err("请打开电脑浏览器后重试，定位仅应用于内置浏览器".into());
    }
    Ok(out.stdout)
}
async fn desktop(fix: wire::LocationFix) -> Result<(), String> {
    let bytes = desktop_bytes(fix).await?;
    let receipt = wire::LocationDevice::decode(bytes.as_slice()).map_err(|_| "电脑位置响应无效")?;
    if receipt.status != "applied" && receipt.status != "disabled" {
        return Err(receipt.error);
    }
    Ok(())
}
pub(super) async fn apply(app: &App, s: &mut wire::LocationState) -> Result<(), String> {
    let devices=sqlx::query("SELECT id,kind FROM devices WHERE owner_id='local-user' AND runtime_state='managed' AND kind IN ('phone','desktop') ORDER BY id").fetch_all(&app.db).await.map_err(|e|e.to_string())?;
    s.devices.clear();
    for row in devices {
        let id: String = row.get("id");
        let kind: String = row.get("kind");
        let fix = wire::LocationFix {
            enabled: s.enabled,
            point: s.point,
            speed_mps: if s.moving { s.speed_kph / 3.6 } else { 0. },
        };
        let result = if kind == "phone" {
            phone(app, &id, fix).await
        } else {
            desktop(fix).await
        };
        s.devices.push(wire::LocationDevice {
            device_id: id,
            kind,
            status: if result.is_ok() {
                if s.enabled { "applied" } else { "disabled" }
            } else {
                "failed"
            }
            .into(),
            error: result.err().unwrap_or_default(),
            applied_at_ms: now(),
        });
    }
    if s.devices.iter().any(|d| d.status == "failed") {
        s.moving = false;
        s.error = "部分设备未应用；运动已暂停".into();
    } else {
        s.error.clear();
    }
    Ok(())
}
