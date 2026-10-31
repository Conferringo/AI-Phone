use crate::*;
use serde_json::{Value, json};
use tokio::{io::AsyncWriteExt, process::Command};
static MUTATION: Mutex<()> = Mutex::const_new(());

async fn invoke(app: &App, value: Value) -> Result<Value, String> {
    let mut child =
        Command::new(std::env::var_os("AI_PHONE_PYTHON").unwrap_or_else(|| "python3".into()))
            .arg(app.root.join("scripts/prebuilt_js.py"))
            .args(["--role", "device-network", "--run", "--root"])
            .arg(&app.root)
            .env("AI_PHONE_PROJECT_ROOT", &app.root)
            .current_dir(&app.root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "网络管理进程不可用")?;
    child
        .stdin
        .take()
        .ok_or("网络配置无法提交")?
        .write_all(value.to_string().as_bytes())
        .await
        .map_err(|_| "网络配置无法提交")?;
    let output = tokio::time::timeout(Duration::from_secs(210), child.wait_with_output())
        .await
        .map_err(|_| "网络操作超时；请读取设备状态后再操作")?
        .map_err(|_| "网络进程未返回结果")?;
    if output.stdout.len() > 2 * 1024 * 1024 {
        return Err("网络状态响应过大".into());
    }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| "网络状态无法读取")?;
    if let Some(error) = value["error"].as_str()
        && !output.status.success()
    {
        return Err(error.chars().take(200).collect());
    }
    if !output.status.success() {
        return Err("设备网络操作失败".into());
    }
    Ok(value)
}
async fn kind(app: &App, id: &str) -> Result<String, String> {
    sqlx::query_scalar("SELECT kind FROM devices WHERE id=$1 AND owner_id='local-user' AND runtime_state='managed'")
 .bind(id).fetch_optional(&app.db).await.map_err(|_|"设备注册表不可用")?.ok_or("设备尚未接入或已归档".into())
}
fn state(v: Value) -> wire::DeviceNetworkState {
    let s = |key: &str| v[key].as_str().unwrap_or("").to_string();
    let n = |key: &str| v[key].as_u64().unwrap_or(0);
    wire::DeviceNetworkState {
        device_id: s("deviceId"),
        revision: n("revision"),
        enabled: v["enabled"].as_bool().unwrap_or(false),
        has_subscription: v["hasSubscription"].as_bool().unwrap_or(false),
        status: s("status"),
        error: s("error"),
        core_version: s("coreVersion"),
        tun_active: v["tunActive"].as_bool().unwrap_or(false),
        node_count: n("nodeCount") as u32,
        checked_at_ms: n("checkedAtMs"),
        checks: v["checks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| wire::DeviceNetworkCheck {
                name: c["name"].as_str().unwrap_or("").into(),
                passed: c["passed"].as_bool().unwrap_or(false),
                detail: c["detail"].as_str().unwrap_or("").into(),
                duration_ms: c["durationMs"].as_u64().unwrap_or(0),
            })
            .collect(),
    }
}
pub async fn prepare(app: &App, device: &str) -> Result<(), String> {
    invoke(
        app,
        json!({"operation":"prepare","deviceId":device,"kind":"desktop"}),
    )
    .await?;
    Ok(())
}
pub async fn install_apps(app: &App) -> Result<(), String> {
    invoke(app, json!({"operation":"install-apps"})).await?;
    Ok(())
}

pub async fn recover(app: Shared) {
    loop {
        if let Ok(_guard) = MUTATION.try_lock() {
            let devices=sqlx::query_as::<_,(String,String)>("SELECT id,kind FROM devices WHERE owner_id='local-user' AND runtime_state='managed' AND kind IN ('phone','desktop')").fetch_all(&app.db).await.unwrap_or_default();
            for (id, kind) in devices {
                let _ = invoke(
                    &app,
                    json!({"operation":"resume","deviceId":id,"kind":kind}),
                )
                .await;
            }
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}
pub async fn route(
    axum::extract::State(app): axum::extract::State<Shared>,
    axum::extract::Path(operation): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Response {
    let result:Result<Response,String>=async{
  if operation=="save"{
   let input=match decode::<wire::DeviceNetworkSaveRequest>(&app,&headers,body){Ok(v)=>v,Err(e)=>return Ok(e.into())};
   Uuid::parse_str(&input.request_id).map_err(|_|"网络请求编号无效")?;
   let _gate=MUTATION.lock().await;let device_kind=kind(&app,&input.device_id).await?;
   let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs r JOIN run_device_bindings b ON b.run_id=r.id WHERE b.device_id=$1 AND r.status IN ('queued','running'))")
    .bind(&input.device_id).fetch_one(&app.db).await.map_err(|_|"无法检查运行任务")?;
   if busy{return Ok(error(StatusCode::CONFLICT,"device_busy","设备任务正在运行，请结束后应用网络配置"));}
   let value=invoke(&app,json!({"operation":"apply","deviceId":input.device_id,"kind":device_kind,"expectedRevision":input.expected_revision,"enabled":input.enabled,"subscriptionUrl":input.subscription_url,"useSavedSubscription":input.use_saved_subscription,"requestId":input.request_id})).await?;
   return Ok(pb(StatusCode::OK,state(value)));
  }
  let input=match decode::<wire::DeviceNetworkRequest>(&app,&headers,body){Ok(v)=>v,Err(e)=>return Ok(e.into())};
  if !["state","credential","probe"].contains(&operation.as_str()){return Ok(error(StatusCode::NOT_FOUND,"not_found","未知网络操作"));}
  let device_kind=kind(&app,&input.device_id).await?;
  let value=invoke(&app,json!({"operation":operation,"deviceId":input.device_id,"kind":device_kind})).await?;
  if operation=="credential"{return Ok(pb(StatusCode::OK,wire::DeviceNetworkCredential{subscription_url:value["subscriptionUrl"].as_str().unwrap_or("").into(),revision:value["revision"].as_u64().unwrap_or(0)}));}
  Ok(pb(StatusCode::OK,state(value)))
 }.await;
    match result {
        Ok(v) => v,
        Err(message) => error(StatusCode::BAD_REQUEST, "device_network_error", &message),
    }
}
