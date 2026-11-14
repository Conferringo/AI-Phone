//! Serial-bound installer input/notification capabilities, separate from model tools.
mod applications;
mod device;
mod policy;
mod probe;
mod state;
use crate::{Error, Result, filesystem, native_ipc};
use ai_phone_api::wire::{NativeAndroidInputReady, NativeAndroidInputSetup};
use device::Device;
use state::{Stage, State};
use std::{ffi::OsString, path::Path, time::Duration};
use tokio::sync::watch;
const PACKAGE: &str = "dev.aiphone.input";
const IME: &str = "dev.aiphone.input/.UnicodeIme";
const ATTENTION: &str = "dev.aiphone.input/.PhoneAttention";
const NOTIFICATION: &str = "dev.aiphone.input/.PhoneNotifications";
const REMOTE_PROBE: &str = "/data/local/tmp/ai-phone-input-notification-probe.jar";
pub const CONTRACT: &str = "ai-phone-input-setup-v2";

pub async fn run(arguments: &[OsString]) -> Result<()> {
    if !arguments.is_empty() {
        return Err(Error::Invalid("phone_input_arguments"));
    }
    let request: NativeAndroidInputSetup =
        native_ipc::read_bounded(&mut tokio::io::stdin(), 16384).await?;
    if request.version != 1 {
        return Err(Error::Invalid("phone_input_request"));
    }
    let (end, stop) = watch::channel(false);
    let signal = tokio::spawn(async move {
        let _ = crate::component::interrupt().await;
        let _ = end.send(true);
    });
    let result = setup(request, stop).await;
    signal.abort();
    let _ = signal.await;
    let changed = result?;
    native_ipc::write_bounded(
        &mut tokio::io::stdout(),
        &NativeAndroidInputReady {
            version: 1,
            changed,
        },
        512,
    )
    .await?;
    Ok(())
}
pub async fn setup(request: NativeAndroidInputSetup, stop: watch::Receiver<bool>) -> Result<bool> {
    if request.version != 1 {
        return Err(Error::Invalid("phone_input_request"));
    }
    if *stop.borrow() {
        return Err(Error::Cancelled);
    }
    let client = ai_phone_adb::Client::new(15037, request.serial.clone())
        .map_err(|_| Error::Invalid("phone_input_serial"))?;
    let state = State::lock(Path::new(&request.project_root), &request.serial)?;
    let pending = state.load()?;
    let mut adb = device::Adb {
        program: request.adb_program.into(),
        client,
        serial: request.serial,
        stop,
    };
    let apps = if request.offline_apps {
        Some(
            applications::Applications::load(Path::new(&request.project_root), &state, &mut adb)
                .await?,
        )
    } else {
        None
    };
    let mut changed = configure(
        &state,
        &mut adb,
        pending,
        Path::new(&request.apk_path),
        Path::new(&request.notification_probe_path),
        request.observe_only,
    )
    .await?;
    if let Some(apps) = apps {
        changed |= apps
            .configure(&state, &mut adb, request.observe_only)
            .await?;
    }
    Ok(changed)
}
async fn package(adb: &mut impl Device) -> Result<bool> {
    policy::package(
        &adb.execute(
            &["shell", "pm", "list", "packages", "--user", "0", PACKAGE],
            10,
        )
        .await?,
        PACKAGE,
    )
}
async fn setting(adb: &mut impl Device, key: &str) -> Result<String> {
    adb.execute(
        &["shell", "settings", "--user", "0", "get", "secure", key],
        10,
    )
    .await
}
async fn write_setting(adb: &mut impl Device, key: &str, value: &str) -> Result<()> {
    adb.execute(
        &[
            "shell", "settings", "--user", "0", "put", "secure", key, value,
        ],
        10,
    )
    .await
    .map(|_| ())
}
fn resource(path: &Path, maximum: u64) -> Result<&str> {
    let file = filesystem::regular(path)?;
    if file.metadata()?.len() == 0 || file.metadata()?.len() > maximum {
        return Err(Error::Invalid("phone_input_resource"));
    }
    path.to_str().ok_or(Error::Invalid("phone_input_resource"))
}
async fn registration(adb: &mut impl Device) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if adb.cancelled() {
            return Err(Error::Cancelled);
        }
        let value = tokio::time::timeout_at(
            deadline,
            adb.execute(&["shell", "ime", "list", "-a", "-s", "--user", "0"], 5),
        )
        .await
        .map_err(|_| Error::Invalid("phone_input_registration"))??;
        let rows = value.lines().collect::<Vec<_>>();
        if !rows.iter().all(|row| policy::component(row)) {
            return Err(Error::Invalid("phone_input_observation"));
        }
        if policy::contains(&rows, IME) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Invalid("phone_input_registration"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
async fn ime(adb: &mut impl Device, observe: bool) -> Result<bool> {
    let mut changed = false;
    let value = setting(adb, "enabled_input_methods").await?;
    if !policy::contains(&policy::components(&value, true)?, IME) {
        if observe {
            return Err(Error::Invalid("phone_input_not_ready"));
        }
        adb.execute(&["shell", "ime", "enable", "--user", "0", IME], 10)
            .await?;
        changed = true;
        if !policy::contains(
            &policy::components(&setting(adb, "enabled_input_methods").await?, true)?,
            IME,
        ) {
            return Err(Error::Invalid("phone_input_enable_unknown"));
        }
    }
    let value = setting(adb, "default_input_method").await?;
    let rows = policy::components(&value, false)?;
    if rows.len() > 1 {
        return Err(Error::Invalid("phone_input_observation"));
    }
    if !policy::contains(&rows, IME) {
        if observe {
            return Err(Error::Invalid("phone_input_not_ready"));
        }
        adb.execute(&["shell", "ime", "set", "--user", "0", IME], 10)
            .await?;
        changed = true;
        if !policy::contains(
            &policy::components(&setting(adb, "default_input_method").await?, false)?,
            IME,
        ) {
            return Err(Error::Invalid("phone_input_selection_unknown"));
        }
    }
    Ok(changed)
}
async fn enabled(adb: &mut impl Device, key: &str, observe: bool) -> Result<bool> {
    match setting(adb, key).await?.as_str() {
        "1" => Ok(false),
        "0" | "null" => {
            if observe {
                return Err(Error::Invalid("phone_input_not_ready"));
            }
            write_setting(adb, key, "1").await?;
            if setting(adb, key).await? != "1" {
                return Err(Error::Invalid("phone_input_setting_unknown"));
            }
            Ok(true)
        }
        _ => Err(Error::Invalid("phone_input_observation")),
    }
}
async fn notification(adb: &mut impl Device) -> Result<bool> {
    match adb
        .execute(
            &[
                "shell",
                "env",
                &format!("CLASSPATH={REMOTE_PROBE}"),
                "app_process",
                "/",
                "dev.aiphone.input.probe.NotificationAccess",
            ],
            10,
        )
        .await?
        .as_str()
    {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Error::Invalid("phone_input_notification_unknown")),
    }
}
async fn services(adb: &mut impl Device, observe: bool) -> Result<bool> {
    let mut changed = enabled(adb, "show_ime_with_hard_keyboard", observe).await?;
    if !notification(adb).await? {
        if observe {
            return Err(Error::Invalid("phone_input_not_ready"));
        }
        adb.execute(
            &[
                "shell",
                "cmd",
                "notification",
                "allow_listener",
                NOTIFICATION,
                "0",
            ],
            10,
        )
        .await?;
        changed = true;
        if !notification(adb).await? {
            return Err(Error::Invalid("phone_input_notification_unknown"));
        }
    }
    let value = setting(adb, "enabled_accessibility_services").await?;
    let mut rows = policy::components(&value, false)?;
    if !policy::contains(&rows, ATTENTION) {
        if observe {
            return Err(Error::Invalid("phone_input_not_ready"));
        }
        rows.push(ATTENTION);
        write_setting(adb, "enabled_accessibility_services", &rows.join(":")).await?;
        changed = true;
        if !policy::contains(
            &policy::components(
                &setting(adb, "enabled_accessibility_services").await?,
                false,
            )?,
            ATTENTION,
        ) {
            return Err(Error::Invalid("phone_input_accessibility_unknown"));
        }
    }
    changed |= enabled(adb, "accessibility_enabled", observe).await?;
    Ok(changed)
}
async fn configure(
    state: &State,
    adb: &mut impl Device,
    pending: Option<Stage>,
    apk: &Path,
    probe: &Path,
    observe: bool,
) -> Result<bool> {
    let present = package(adb).await?;
    if pending.is_some() && !present {
        return Err(Error::Invalid("phone_input_install_unknown"));
    }
    if adb
        .execute(&["shell", "am", "get-current-user"], 10)
        .await?
        != "0"
    {
        return Err(Error::Invalid("phone_input_user"));
    }
    let probe = if observe {
        None
    } else {
        Some(probe::Probe::load(probe)?)
    };
    let mut changed = false;
    if !present {
        if observe {
            return Err(Error::Invalid("phone_input_not_ready"));
        }
        let apk = resource(apk, 16 * 1024 * 1024)?;
        state.save(Stage::InstallUnknown)?;
        // An unknown exit is observed, never reinstalled by this invocation.
        let _ = adb
            .execute(
                &["install", "--no-incremental", "-r", "--user", "0", apk],
                60,
            )
            .await;
        if !package(adb).await? {
            return Err(Error::Invalid("phone_input_install_unknown"));
        }
        changed = true;
    }
    if !observe && pending != Some(Stage::Configure) {
        state.save(Stage::Configure)?;
    }
    if let Some(probe) = probe {
        changed |= probe.prepare(adb).await?;
    }
    registration(adb).await?;
    changed |= ime(adb, observe).await?;
    changed |= services(adb, observe).await?;
    if !observe {
        state.finish()?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests;
