use crate::*;
use serde_json::Value;

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
pub async fn recover(app: Shared) {
    loop {
        if let Ok(_guard) = app.network_mutation.try_lock() {
            let devices=sqlx::query_as::<_,(String,String)>("SELECT id,kind FROM devices WHERE owner_id='local-user' AND runtime_state='managed' AND kind IN ('phone','desktop')").fetch_all(&app.db).await.unwrap_or_default();
            for (id, kind) in devices {
                if let Ok(network) = ai_phone_host_runtime::network_observation::Controller::new(
                    &app.root, &kind, &id,
                ) {
                    let _ = network
                        .execute(ai_phone_host_runtime::network_observation::Operation::Resume)
                        .await;
                }
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
    let result: Result<Response, String> = async {
        let input = match decode::<wire::DeviceNetworkRequest>(&app, &headers, body) {
            Ok(v) => v,
            Err(e) => return Ok(e.into()),
        };
        if !["state", "credential", "probe"].contains(&operation.as_str()) {
            return Ok(error(StatusCode::NOT_FOUND, "not_found", "未知网络操作"));
        }
        let device_kind = kind(&app, &input.device_id).await?;
        let value = ai_phone_host_runtime::network_observation::Observer::new(
            &app.root,
            &device_kind,
            &input.device_id,
        )
        .map_err(|e| e.to_string())?
        .read(&operation)
        .await
        .map_err(|e| {
            if e.code() == "device_network_metadata" {
                "设备网络配置损坏，请从备份恢复".to_owned()
            } else {
                e.to_string()
            }
        })?;
        if operation == "credential" {
            return Ok(pb(
                StatusCode::OK,
                wire::DeviceNetworkCredential {
                    subscription_url: value["subscriptionUrl"].as_str().unwrap_or("").into(),
                    revision: value["revision"].as_u64().unwrap_or(0),
                },
            ));
        }
        Ok(pb(StatusCode::OK, state(value)))
    }
    .await;
    match result {
        Ok(v) => v,
        Err(message) => error(StatusCode::BAD_REQUEST, "device_network_error", &message),
    }
}
