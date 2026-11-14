//! Fixed native SDK lifecycle, independent of device focus and model execution.
mod adb;
#[cfg(target_os = "macos")]
mod avo;
mod cli;
mod link;
mod plan;
mod process;
mod readiness;
#[cfg(windows)]
mod windows_cuttlefish;
#[cfg(target_os = "macos")]
pub use avo::runtime as avo_runtime;
pub use cli::run;
pub const CONTRACT: &str = "ai-phone-native-android-session-v1";
pub(crate) async fn wait_serial(
    serial: String,
    observe_only: bool,
    stop: &mut tokio::sync::watch::Receiver<bool>,
) -> crate::Result<ai_phone_api::wire::NativeAndroidBooted> {
    let adb = adb::Adb {
        port: 15037,
        serial,
    };
    if observe_only {
        return readiness::existing(&adb, stop).await;
    }
    let until = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
    let mut root_requested = false;
    loop {
        readiness::cancelled(stop)?;
        if let Some(value) = process::boot_probe(&adb, &mut root_requested).await? {
            return Ok(value);
        }
        if tokio::time::Instant::now() >= until {
            return Err(crate::Error::Invalid("android_session_unavailable"));
        }
        readiness::pause(stop).await?;
    }
}

pub(crate) async fn backend_observation(
    serial: &str,
) -> crate::Result<ai_phone_api::wire::NativeAndroidBooted> {
    readiness::observe(
        &adb::Adb {
            port: 15037,
            serial: serial.into(),
        },
        true,
    )
    .await
}

#[cfg(test)]
mod tests;
