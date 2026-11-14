//! Native installer device activation. No App, SQL, shell, Python, or Bun execution.
mod policy;
mod rpc;
mod store;
pub mod wsl;

use crate::wire;
use rpc::Rpc;
use std::{fmt, path::Path, time::Duration};
use store::{Intent, Store};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

pub const CONTRACT: &str = policy::CONTRACT;
#[derive(Debug)]
pub struct Error(&'static str);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

pub async fn run(
    root: &Path,
    cancelled: CancellationToken,
    require_notifications: bool,
) -> Result<String> {
    if root.join("data/restore-quarantine.json").exists() {
        return Err(Error("device_setup_restore_quarantined"));
    }
    let store = Store::new(root)?;
    let rpc = Rpc::new(
        store::token(root, "assistant")?,
        store::token(root, "phone-runner")?,
        cancelled,
    )?;
    let desktop = activate(&store, &rpc).await?;
    if require_notifications {
        verify_notifications(&rpc).await?;
    }
    Ok(desktop)
}

async fn verify_notifications(rpc: &Rpc) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let value = tokio::time::timeout_at(
            deadline,
            rpc.notifications(wire::DeviceRequest {
                phone_id: "phone-local-01".into(),
                owner_id: "local-user".into(),
                ..Default::default()
            }),
        )
        .await
        .map_err(|_| Error("device_setup_notifications_unavailable"))??;
        if value.connected && value.attention_connected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error("device_setup_notifications_unavailable"));
        }
        tokio::select! {biased;
            _=rpc.cancelled.cancelled()=>return Err(Error("device_setup_cancelled_remote_may_continue")),
            _=tokio::time::sleep(Duration::from_millis(250))=>{},
        }
    }
}

async fn activate(store: &Store, rpc: &Rpc) -> Result<String> {
    let state: wire::RegistryState = rpc.call("registry/state", wire::RegistryRequest {}).await?;
    verify_phone(rpc).await?;
    let saved = store.load()?;
    let desktop = if let Some(intent) = &saved {
        state
            .devices
            .iter()
            .find(|device| device.id == intent.device_id)
    } else {
        state.devices.iter().find(|device| device.kind == "desktop")
    };
    if let Some(device) = desktop {
        if device.kind != "desktop" {
            return Err(Error("device_setup_device_changed"));
        }
        if device.runtime_state == "managed" {
            verify_desktop(rpc, &device.id).await?;
            return Ok(device.id.clone());
        }
        if device.runtime_state == "archived" {
            return Err(Error("device_setup_device_archived"));
        }
    } else if saved.is_some() && state.devices.iter().any(|device| device.kind == "desktop") {
        return Err(Error("device_setup_device_changed"));
    }
    let intent = store.select(desktop.map(|device| device.id.as_str()), store.resources()?)?;
    if desktop.is_none() {
        register(rpc, store, &intent).await?;
    }
    provision(rpc, store, &intent).await?;
    verify_desktop(rpc, &intent.device_id).await?;
    Ok(intent.device_id)
}

async fn verify_phone(rpc: &Rpc) -> Result<()> {
    let phone = rpc
        .phone(wire::DeviceRequest {
            phone_id: "phone-local-01".into(),
            owner_id: "local-user".into(),
            ..Default::default()
        })
        .await?;
    policy::phone_frame(
        &phone.phone_id,
        &phone.root_identity,
        phone.width,
        phone.height,
        &phone.png,
    )
    .map_err(Error)
}
async fn verify_desktop(rpc: &Rpc, id: &str) -> Result<()> {
    let frame: wire::DesktopObservation = rpc
        .call(
            "desktop/observe",
            wire::DesktopRequest {
                device_id: id.into(),
            },
        )
        .await?;
    if frame.device_id != id {
        return Err(Error("device_setup_desktop_scope"));
    }
    policy::desktop_frame(frame.width, frame.height, &frame.png).map_err(Error)
}
async fn register(rpc: &Rpc, store: &Store, intent: &Intent) -> Result<()> {
    // The attempt marker is committed before dispatch. On retry, registry was already
    // observed above; an absent device after an attempted save is unknown, not replayable.
    if !store.begin(intent, "register")? {
        return Err(Error("device_setup_registration_unknown"));
    }
    let state: wire::RegistryState = rpc
        .call(
            "registry/device-save",
            wire::DeviceSaveRequest {
                id: intent.device_id.clone(),
                name: "Linux".into(),
                kind: "desktop".into(),
                ..Default::default()
            },
        )
        .await?;
    if !state
        .devices
        .iter()
        .any(|device| device.id == intent.device_id && device.kind == "desktop")
    {
        return Err(Error("device_setup_registration_unknown"));
    }
    Ok(())
}
async fn provision(rpc: &Rpc, store: &Store, intent: &Intent) -> Result<()> {
    let state: wire::DeviceLinkState = rpc.call("desktop/state", wire::RegistryRequest {}).await?;
    let existing = matching_job(&state, intent)?;
    let id = if let Some(job) = existing {
        job.id.clone()
    } else {
        if !store.begin(intent, "provision")? {
            return Err(Error("device_setup_provision_unknown"));
        }
        let job: wire::DesktopProvisionJob = rpc
            .call(
                "desktop/provision",
                wire::DesktopProvisionRequest {
                    device_id: intent.device_id.clone(),
                    request_id: intent.request_id.clone(),
                    cpus: intent.resources.cpus,
                    memory_mb: intent.resources.memory_mb,
                    template_id: "debian13-openbox".into(),
                },
            )
            .await?;
        check_job(&job, intent)?;
        job.id
    };
    wait_for_job(rpc, intent, &id).await
}
fn matching_job<'a>(
    state: &'a wire::DeviceLinkState,
    intent: &Intent,
) -> Result<Option<&'a wire::DesktopProvisionJob>> {
    let exact = state.jobs.iter().find(|job| job.id == intent.request_id);
    let active = state
        .jobs
        .iter()
        .filter(|job| {
            job.device_id == intent.device_id
                && ["queued", "running"].contains(&job.status.as_str())
        })
        .collect::<Vec<_>>();
    if active.len() > 1 {
        return Err(Error("device_setup_provision_conflict"));
    }
    let found = exact.or_else(|| active.first().copied());
    if let Some(job) = found {
        check_job(job, intent)?;
    }
    Ok(found)
}
fn check_job(job: &wire::DesktopProvisionJob, intent: &Intent) -> Result<()> {
    if job.id.is_empty()
        || job.device_id != intent.device_id
        || job.cpus != intent.resources.cpus
        || job.memory_mb != intent.resources.memory_mb
        || job.template_id != "debian13-openbox"
    {
        return Err(Error("device_setup_provision_conflict"));
    }
    if job.status == "failed" {
        report_job(job, false);
    }
    policy::job_state(&job.status).map_err(Error)?;
    Ok(())
}
async fn wait_for_job(rpc: &Rpc, intent: &Intent, id: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let state: wire::DeviceLinkState = tokio::time::timeout_at(
            deadline,
            rpc.call("desktop/state", wire::RegistryRequest {}),
        )
        .await
        .map_err(|_| Error("device_setup_provision_timed_out_remote_may_continue"))??;
        let job = state
            .jobs
            .iter()
            .find(|job| job.id == id)
            .ok_or(Error("device_setup_provision_unknown"))?;
        check_job(job, intent)?;
        if policy::job_state(&job.status).map_err(Error)? == policy::JobState::Completed {
            return Ok(());
        }
        if Instant::now() >= deadline {
            report_job(job, true);
            return Err(Error(
                "device_setup_provision_timed_out_remote_may_continue",
            ));
        }
        tokio::select! {
            biased;
            _=rpc.cancelled.cancelled()=>return Err(Error("device_setup_cancelled_remote_may_continue")),
            _=tokio::time::sleep_until((Instant::now()+Duration::from_secs(1)).min(deadline))=>{},
        }
    }
}

fn report_job(job: &wire::DesktopProvisionJob, timed_out: bool) {
    let (stage, flags) = policy::provision_diagnostic(&job.stage, &job.error);
    eprintln!(
        "Desktop provisioning {} (stage={stage}; flags={})",
        if timed_out { "timed out" } else { "failed" },
        flags.join(",")
    );
}

pub async fn interrupt(cancelled: CancellationToken) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=term.recv()=>{}, }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    cancelled.cancel();
}

#[cfg(test)]
mod tests;
