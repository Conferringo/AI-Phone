use super::{
    RpcError, State,
    store::{Job, Result, Stage, now, scope},
    transport::Payload,
    valid_png,
};
use ai_phone_api::wire;
use base64::{Engine, engine::general_purpose::STANDARD};
use prost::Message;
use std::{sync::Arc, time::Duration};

async fn update(state: &State, job: &Job, apply: impl FnOnce(&mut Job)) -> Result<bool> {
    let mut store = state.store.lock().await;
    let active = store
        .vault
        .configs
        .get(&scope(&job.agent_id, &job.provider))
        .is_some_and(|config| config.version == job.version);
    let Some(current) = store
        .vault
        .jobs
        .iter_mut()
        .find(|current| current.agent_id == job.agent_id && current.id == job.id)
    else {
        return Ok(false);
    };
    if !matches!(
        current.stage,
        Stage::Queued | Stage::Running | Stage::Sending
    ) {
        return Ok(false);
    }
    if !active && current.stage != Stage::Sending {
        return Ok(false);
    }
    apply(current);
    store.save()?;
    Ok(true)
}
async fn screenshot(state: &State, agent: &str, device: &str) -> Result<String> {
    let registry: wire::RegistryState = state
        .rpc("registry/state", wire::RegistryRequest {})
        .await?;
    if !registry
        .agents
        .iter()
        .any(|owner| owner.id == agent && owner.device_ids.iter().any(|id| id == device))
    {
        return Err("screenshot_device_unavailable".into());
    }
    let selected = registry
        .devices
        .iter()
        .find(|selected| selected.id == device && selected.runtime_state == "managed")
        .ok_or("screenshot_device_unavailable")?;
    let png = if selected.kind == "desktop" {
        let observation: wire::DesktopObservation = state
            .rpc(
                "desktop/observe",
                wire::DesktopRequest {
                    device_id: device.into(),
                },
            )
            .await?;
        if observation.device_id != device {
            return Err("screenshot_device_mismatch".into());
        }
        observation.png
    } else {
        if device != "phone-local-01" {
            return Err("screenshot_device_unavailable".into());
        }
        let response = state
            .local
            .post("http://127.0.0.1:17862/v1/observe")
            .bearer_auth(&state.runner_token)
            .header("content-type", crate::MEDIA)
            .timeout(Duration::from_secs(12))
            .body(
                wire::DeviceRequest {
                    phone_id: device.into(),
                    owner_id: "local-user".into(),
                    ..Default::default()
                }
                .encode_to_vec(),
            )
            .send()
            .await?;
        if !response.status().is_success() {
            return Err("screenshot_unavailable".into());
        }
        let observation = wire::DeviceObservation::decode(response.bytes().await?)?;
        if observation.phone_id != device {
            return Err("screenshot_device_mismatch".into());
        }
        observation.png
    };
    if !valid_png(&png) {
        return Err("screenshot_unavailable".into());
    }
    Ok(STANDARD.encode(png))
}
async fn process(state: &State, mut job: Job) -> Result<()> {
    let key = scope(&job.agent_id, &job.provider);
    let connection = state.connections.read().await.get(&key).cloned();
    let Some(connection) = connection else {
        return Ok(());
    };
    if connection.context.live.read().unwrap().state != "connected" {
        return Ok(());
    }
    if now().saturating_sub(job.at) > 3_600_000 && job.run_id.is_none() {
        update(state, &job, |job| {
            job.stage = Stage::Failed;
            job.error = "Connection timed out before execution".into();
            job.png = None;
        })
        .await?;
        return Ok(());
    }
    let mut text = job.text.clone();
    if !job.outbound {
        if job.device_id.is_none() {
            let device = state.device(&job.agent_id).await?;
            if !update(state, &job, |job| job.device_id = Some(device.clone())).await? {
                return Ok(());
            }
            job.device_id = Some(device);
        }
        let device = job.device_id.as_deref().unwrap();
        if job.screenshot_only {
            match screenshot(state, &job.agent_id, device).await {
                Ok(png) => {
                    job.png = Some(png);
                    text = "Current device screenshot".into();
                }
                Err(_) => text = "Unable to capture the device screen".into(),
            }
        } else {
            if job.run_id.is_none() {
                let run: wire::ChatState = state
                    .rpc(
                        "send",
                        wire::ChatSendRequest {
                            text: job.text.clone(),
                            request_id: job.id.clone(),
                            agent_id: job.agent_id.clone(),
                            device_id: device.into(),
                            receipt_only: true,
                            ..Default::default()
                        },
                    )
                    .await?;
                if !update(state, &job, |job| {
                    job.run_id = Some(run.run_id.clone());
                    job.stage = Stage::Running;
                })
                .await?
                {
                    let _: Result<wire::ChatState> = state
                        .rpc(
                            "cancel",
                            wire::ChatRunRequest {
                                run_id: run.run_id,
                                ..Default::default()
                            },
                        )
                        .await;
                    return Ok(());
                }
                job.run_id = Some(run.run_id);
            }
            let run: wire::ChatState = state
                .rpc(
                    "run",
                    wire::ChatRunRequest {
                        run_id: job.run_id.clone().unwrap(),
                        ..Default::default()
                    },
                )
                .await?;
            if matches!(run.run_status.as_str(), "queued" | "running") {
                return Ok(());
            }
            text = if run.run_status == "succeeded" {
                run.messages
                    .first()
                    .map(|message| message.text.clone())
                    .unwrap_or_else(|| "Task finished".into())
            } else if !run.error.is_empty() {
                run.error
            } else {
                run.messages
                    .first()
                    .map(|message| message.text.clone())
                    .unwrap_or_else(|| "Task stopped".into())
            };
            if job.screenshot || run.run_status == "paused" {
                match screenshot(state, &job.agent_id, device).await {
                    Ok(png) => job.png = Some(png),
                    Err(_) => text.push_str("\nUnable to capture the current screen"),
                }
            }
        }
    }
    if !update(state, &job, |current| {
        current.stage = Stage::Sending;
        current.png = job.png.clone();
    })
    .await?
    {
        return Ok(());
    }
    let outcome = async {
        if !text.is_empty() && !job.text_sent {
            connection
                .send(
                    &job.target,
                    Payload::Text(text.chars().take(5900).collect()),
                    &job.id,
                )
                .await?;
            if !update(state, &job, |job| job.text_sent = true).await? {
                return Ok(());
            }
        }
        if let Some(png) = job.png.as_ref().filter(|_| !job.image_sent) {
            connection
                .send(&job.target, Payload::Image(STANDARD.decode(png)?), &job.id)
                .await?;
            if !update(state, &job, |job| job.image_sent = true).await? {
                return Ok(());
            }
        }
        update(state, &job, |job| {
            job.stage = Stage::Delivered;
            job.error.clear();
            job.png = None;
        })
        .await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;
    if outcome.is_err() {
        update(state, &job, |job| {
            job.stage = Stage::Unknown;
            job.error = "Delivery unconfirmed; check the original app".into();
            job.png = None;
        })
        .await?;
    }
    Ok(())
}
pub(super) async fn run(state: Arc<State>) {
    loop {
        let jobs: Vec<_> = state
            .store
            .lock()
            .await
            .vault
            .jobs
            .iter()
            .filter(|job| job.stage.pending())
            .cloned()
            .collect();
        let pending = !jobs.is_empty();
        for job in jobs {
            if state.stop.is_cancelled() {
                return;
            }
            if let Err(error) = process(&state, job.clone()).await {
                if error
                    .downcast_ref::<RpcError>()
                    .is_some_and(|error| !matches!(error.0, 409 | 500 | 503))
                {
                    if let Err(error) = update(&state, &job, |job| {
                        job.stage = Stage::Failed;
                        job.error = "Instruction rejected before execution".into();
                    })
                    .await
                    {
                        eprintln!("Channel job update failed: {error}");
                    }
                } else {
                    eprintln!("Channel job suspended: {error}");
                }
            }
        }
        tokio::select! {
            _ = state.stop.cancelled()=>return,
            _ = state.changed.notified()=>{},
            _ = tokio::time::sleep(Duration::from_millis(300)),if pending=>{},
        }
    }
}
