//! Final device activation inside the already staged, application-owned WSL guest.
use super::{Error, Result, store};
use ai_phone_api::wire::{
    NativeAndroidInputSetup, NativeWslDeviceSetupRequest, NativeWslDevicesReady,
};
use ai_phone_host_runtime::{android::input, device_bridge::paired_phone, native_ipc};
use std::path::{Path, PathBuf};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

fn request(value: &NativeWslDeviceSetupRequest) -> Result<()> {
    if value.version != 1
        || value.bridge_id.len() != 32
        || !value.bridge_id.bytes().all(|v| v.is_ascii_hexdigit())
    {
        return Err(Error("wsl_device_setup_request"));
    }
    Ok(())
}
fn source_root(bundle: &Path, installation: &Path) -> Result<PathBuf> {
    let identity = String::from_utf8(store::read(&bundle.join("source.sha256"), 66)?)
        .map_err(|_| Error("wsl_device_setup_source"))?;
    let identity = identity.trim();
    if identity.len() != 64 || !identity.bytes().all(|v| v.is_ascii_hexdigit()) {
        return Err(Error("wsl_device_setup_source"));
    }
    let root = installation.join("releases").join(identity);
    if !root.is_dir() || root.is_symlink() {
        return Err(Error("wsl_device_setup_source"));
    }
    Ok(root)
}
pub async fn run(cancelled: CancellationToken) -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Err(Error("wsl_device_setup_requires_linux"));
    }
    let value: NativeWslDeviceSetupRequest = native_ipc::read_bounded(&mut tokio::io::stdin(), 512)
        .await
        .map_err(|_| Error("wsl_device_setup_request"))?;
    request(&value)?;
    let root = source_root(
        Path::new("/opt/aiphone/bundle"),
        Path::new("/var/lib/aiphone/installation"),
    )?;
    if root.join("data/restore-quarantine.json").exists() {
        return Err(Error("device_setup_restore_quarantined"));
    }
    if cancelled.is_cancelled() {
        return Err(Error("device_setup_cancelled_remote_may_continue"));
    }
    let phone = paired_phone(&value.bridge_id)
        .await
        .map_err(|_| Error("wsl_device_setup_pair_unavailable"))?;
    let (end, stop) = watch::channel(false);
    let signal = cancelled.clone();
    let watcher = tokio::spawn(async move {
        signal.cancelled().await;
        let _ = end.send(true);
    });
    let initialized = input::setup(
        NativeAndroidInputSetup {
            version: 1,
            project_root: root
                .to_str()
                .ok_or(Error("wsl_device_setup_source"))?
                .into(),
            adb_program: "/opt/aiphone/native-phone-bridge/platform-tools/adb".into(),
            serial: phone.serial,
            apk_path: root
                .join("runtime/phone-apps/input.apk")
                .to_string_lossy()
                .into(),
            notification_probe_path: root
                .join("runtime/phone-apps/notification-access.jar")
                .to_string_lossy()
                .into(),
            offline_apps: true,
            observe_only: false,
        },
        stop,
    )
    .await;
    watcher.abort();
    let _ = watcher.await;
    initialized.map_err(|_| Error("wsl_device_setup_phone_unavailable"))?;
    let desktop = super::run(&root, cancelled.clone(), true).await?;
    // A lost/replaced bridge after provisioning cannot publish readiness. A retry
    // re-observes the durable provisioning request instead of recreating devices.
    paired_phone(&value.bridge_id)
        .await
        .map_err(|_| Error("wsl_device_setup_pair_unavailable"))?;
    if cancelled.is_cancelled() {
        return Err(Error("device_setup_cancelled_remote_may_continue"));
    }
    native_ipc::write_bounded(
        &mut tokio::io::stdout(),
        &NativeWslDevicesReady {
            version: 1,
            bridge_id: value.bridge_id,
            desktop_id: desktop,
        },
        512,
    )
    .await
    .map_err(|_| Error("wsl_device_setup_receipt"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_or_foreign_requests_and_source_ids_never_select_an_installation() {
        for value in [
            NativeWslDeviceSetupRequest::default(),
            NativeWslDeviceSetupRequest {
                version: 1,
                bridge_id: "../foreign".into(),
            },
        ] {
            assert!(request(&value).is_err());
        }
        request(&NativeWslDeviceSetupRequest {
            version: 1,
            bridge_id: "a".repeat(32),
        })
        .unwrap();
        let root =
            std::env::temp_dir().join(format!("aiphone-wsl-source-{}", uuid::Uuid::new_v4()));
        let bundle = root.join("bundle");
        std::fs::create_dir_all(&bundle).unwrap();
        for identity in ["../foreign", "z".repeat(64).as_str()] {
            std::fs::write(bundle.join("source.sha256"), identity).unwrap();
            assert!(source_root(&bundle, &root).is_err());
        }
        let release = root.join("releases").join("a".repeat(64));
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(bundle.join("source.sha256"), "a".repeat(64) + "\n").unwrap();
        assert_eq!(source_root(&bundle, &root).unwrap(), release);
        std::fs::remove_dir_all(root).unwrap();
    }
}
