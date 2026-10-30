//! Device routing and bounded Android provisioning. No user-provided address or command.
use crate::*;
use axum::extract::State;
use serde_json::Value;
use sqlx::Row;
use tokio::io::{AsyncBufReadExt, BufReader};

pub struct Target {
    pub runner: u16,
    pub token: String,
    pub serial: String,
}
pub async fn target(app: &App, id: &str) -> Result<Target, String> {
    if id == registry::PHONE {
        return Ok(Target {
            runner: 17862,
            token: app.phone_token.clone(),
            serial: "emulator-5564".into(),
        });
    }
    if app.root.join("data/phones/second/archived").exists() {
        return Err("手机已停用，数据已保留".into());
    }
    Uuid::parse_str(id).map_err(|_| "未知手机".to_string())?;
    let value: Value = serde_json::from_slice(
        &tokio::fs::read(app.root.join("data/phones/second.json"))
            .await
            .map_err(|_| "第二台手机尚未安装")?,
    )
    .map_err(|_| "手机配置不可读")?;
    if value["device_id"] != id || value["slot"] != 2 {
        return Err("手机没有匹配的独立运行时".into());
    }
    let token = tokio::fs::read_to_string(app.root.join("data/phones/second/runner/token"))
        .await
        .map_err(|_| "手机控制尚未就绪")?;
    Ok(Target {
        runner: 17872,
        token: token.trim().into(),
        serial: "emulator-5566".into(),
    })
}
pub async fn for_run(app: &App, run: Uuid) -> Result<String, String> {
    let primary: String = sqlx::query_scalar("SELECT device_id FROM runs WHERE id=$1")
        .bind(run)
        .fetch_one(&app.db)
        .await
        .map_err(|e| e.to_string())?;
    let ids:Vec<String>=sqlx::query_scalar("SELECT d.id FROM run_device_bindings b JOIN devices d ON d.id=b.device_id WHERE b.run_id=$1 AND d.kind='phone' AND d.runtime_state='managed' ORDER BY d.created_at,d.id").bind(run).fetch_all(&app.db).await.map_err(|e|e.to_string())?;
    let id = if ids.contains(&primary) {
        primary
    } else {
        ids.first().cloned().ok_or("此Agent未授权手机")?
    };
    desktop::authorized(app, run, &id).await?;
    Ok(id)
}
fn row(r: &sqlx::postgres::PgRow) -> wire::PhoneProvisionJob {
    wire::PhoneProvisionJob {
        id: r.get::<Uuid, _>("id").to_string(),
        device_id: r.get("device_id"),
        status: r.get("status"),
        stage: r.get("stage"),
        error: r.get("error"),
        template_id: r.get("template_id"),
        cpus: r.get::<i32, _>("cpus") as u32,
        memory_mb: r.get::<i32, _>("memory_mb") as u32,
        storage_gb: r.get::<i32, _>("storage_gb") as u32,
        identity_profile: r.get("identity_profile"),
    }
}
async fn install(app: &App, id: Uuid) -> Result<(), String> {
    let r=sqlx::query("UPDATE phone_provision_jobs SET status='running',error='',updated_at=now() WHERE id=$1 RETURNING *").bind(id).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
    let v = row(&r);
    let logdir = app.root.join("data/phones/second");
    tokio::fs::create_dir_all(&logdir)
        .await
        .map_err(|e| e.to_string())?;
    let mut child = tokio::process::Command::new("python3")
        .arg(app.root.join("scripts/android-second.py"))
        .args([
            "--device-id",
            &v.device_id,
            "--cpus",
            &v.cpus.to_string(),
            "--memory-mb",
            &v.memory_mb.to_string(),
            "--storage-gb",
            &v.storage_gb.to_string(),
            "--identity",
            &v.identity_profile,
        ])
        .current_dir(&app.root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::fs::File::create(logdir.join("install-error.log")).map_err(|e| e.to_string())?)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let work = async {
        while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
            if line.len() < 180 {
                sqlx::query(
                    "UPDATE phone_provision_jobs SET stage=$2,updated_at=now() WHERE id=$1",
                )
                .bind(id)
                .bind(line)
                .execute(&app.db)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        let result = child.wait().await.map_err(|e| e.to_string())?;
        if !result.success() {
            return Err("系统创建未完成；数据已保留，检查安装日志后可重试".to_string());
        }
        Ok(())
    };
    tokio::time::timeout(Duration::from_secs(720), work)
        .await
        .map_err(|_| "安装超时；保留系统盘，请重试".to_string())??;
    let mut verified = false;
    for _ in 0..20 {
        let check = worker::phone::<_, wire::DeviceObservation>(
            app,
            "observe",
            wire::DeviceRequest {
                phone_id: v.device_id.clone(),
                owner_id: "local-user".into(),
                ..Default::default()
            },
        )
        .await;
        if matches!(check,Ok(ref o) if o.phone_id==v.device_id&&o.root_identity=="0"&&!o.png.is_empty())
        {
            verified = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    if !verified {
        return Err("系统未返回独立画面和root控制，不登记为已接入".into());
    }
    let mut tx = app.db.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO device_runtimes(device_id,provider,instance_ref) VALUES($1,'android-emulator','ai_phone_second') ON CONFLICT(device_id) DO NOTHING").bind(&v.device_id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    sqlx::query("UPDATE devices SET runtime_state='managed',system='Android 15 · ARM64',revision=revision+1,updated_at=now() WHERE id=$1").bind(&v.device_id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    sqlx::query("UPDATE phone_provision_jobs SET status='completed',stage='已就绪',updated_at=now() WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())
}
fn spawn(app: Shared, id: Uuid) {
    tokio::spawn(async move {
        if let Err(e) = install(&app, id).await {
            let _=sqlx::query("UPDATE phone_provision_jobs SET status='failed',error=$2,updated_at=now() WHERE id=$1").bind(id).bind(e).execute(&app.db).await;
        }
    });
}
pub async fn recover(app: Shared) {
    let ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM phone_provision_jobs WHERE status IN ('queued','running')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap_or_default();
    for id in ids {
        spawn(app.clone(), id);
    }
}
async fn provision(
    app: Shared,
    r: wire::PhoneProvisionRequest,
) -> Result<wire::PhoneProvisionJob, String> {
    if r.template_id != "android35-arm64"
        || ![1, 2, 4].contains(&r.cpus)
        || ![1536, 2048, 3072, 4096].contains(&r.memory_mb)
        || ![16, 24, 32].contains(&r.storage_gb)
        || !["aiphone", "xiaomi8"].contains(&r.identity_profile.as_str())
    {
        return Err("系统模板或配置无效".into());
    }
    let id = Uuid::parse_str(&r.request_id).map_err(|_| "无效安装编号")?;
    Uuid::parse_str(&r.device_id).map_err(|_| "只能安装新增手机，现有手机保持不变")?;
    let mut tx = app.db.begin().await.map_err(|e| e.to_string())?;
    let device =
        sqlx::query("SELECT * FROM devices WHERE id=$1 AND owner_id='local-user' FOR UPDATE")
            .bind(&r.device_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("设备不存在")?;
    if device.get::<String, _>("kind") != "phone" {
        return Err("请选择Android手机".into());
    }
    if device.get::<String, _>("runtime_state") == "archived" {
        return Err("设备已停用；请先显式恢复，不能通过安装请求重新启动".into());
    }
    if let Some(old)=sqlx::query("SELECT * FROM phone_provision_jobs WHERE id=$1 OR (device_id=$2 AND status IN ('queued','running')) ORDER BY created_at DESC LIMIT 1").bind(id).bind(&r.device_id).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?{let v=row(&old);if v.id==id.to_string()&&(v.device_id!=r.device_id||v.cpus!=r.cpus||v.memory_mb!=r.memory_mb||v.storage_gb!=r.storage_gb||v.identity_profile!=r.identity_profile){return Err("安装编号对应不同配置".into());}return Ok(v);}
    if device.get::<String, _>("runtime_state") == "managed" {
        return Err("系统已安装，不覆盖已有数据".into());
    }
    let value=sqlx::query("INSERT INTO phone_provision_jobs(id,device_id,status,template_id,cpus,memory_mb,storage_gb,identity_profile) VALUES($1,$2,'queued',$3,$4,$5,$6,$7) RETURNING *").bind(id).bind(&r.device_id).bind(r.template_id).bind(r.cpus as i32).bind(r.memory_mb as i32).bind(r.storage_gb as i32).bind(r.identity_profile).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    spawn(app, id);
    Ok(row(&value))
}
pub async fn route(
    State(app): State<Shared>,
    axum::extract::Path(op): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let r: wire::PhoneProvisionRequest = match decode(&app, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let result=match op.as_str(){"state"=>match sqlx::query("SELECT DISTINCT ON(device_id) * FROM phone_provision_jobs ORDER BY device_id,created_at DESC").fetch_all(&app.db).await{Ok(rows)=>Ok(pb(StatusCode::OK,wire::PhoneProvisionState{jobs:rows.iter().map(row).collect()})),Err(e)=>Err(e.to_string())},"provision"=>provision(app,r).await.map(|v|pb(StatusCode::OK,v)),_=>Err("没有这项系统操作".into())};
    result.unwrap_or_else(|e| error(StatusCode::BAD_REQUEST, "phone_system", &e))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routing_ignores_request_specific_wire_types() {
        let id = "second-phone";
        let messages = vec![
            wire::DeviceRequest {
                phone_id: id.into(),
                include_elements: true,
                ..Default::default()
            }
            .encode_to_vec(),
            wire::TakeControlRequest {
                phone_id: id.into(),
                controller: "assistant".into(),
                ..Default::default()
            }
            .encode_to_vec(),
            wire::DeviceActionRequest {
                phone_id: id.into(),
                run_id: "run-1".into(),
                control_epoch: 5,
                ..Default::default()
            }
            .encode_to_vec(),
        ];
        for bytes in messages {
            assert_eq!(
                wire::PhoneScope::decode(bytes.as_slice()).unwrap().phone_id,
                id
            );
        }
    }
}
