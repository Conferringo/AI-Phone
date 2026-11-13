use super::adb::Adb;
use crate::{Error, Result};
use ai_phone_api::wire::NativeAndroidBooted;
use std::time::Duration;
use tokio::{sync::watch, time::Instant};

pub(super) fn frame(bytes: &[u8]) -> Result<(u32, u32)> {
    if bytes.len() < 33 || bytes.get(..16) != Some(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR") {
        return Err(Error::Invalid("android_frame_unavailable"));
    }
    let width = u32::from_be_bytes(
        bytes[16..20]
            .try_into()
            .map_err(|_| Error::Invalid("android_frame_unavailable"))?,
    );
    let height = u32::from_be_bytes(
        bytes[20..24]
            .try_into()
            .map_err(|_| Error::Invalid("android_frame_unavailable"))?,
    );
    if ![width, height]
        .into_iter()
        .all(|value| (1..=8192).contains(&value))
    {
        return Err(Error::Invalid("android_frame_unavailable"));
    }
    Ok((width, height))
}
pub(super) fn cancelled(stop: &watch::Receiver<bool>) -> Result<()> {
    if *stop.borrow() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
pub(super) async fn pause(stop: &mut watch::Receiver<bool>) -> Result<()> {
    tokio::select! {
        _=stop.changed()=>Err(Error::Cancelled),
        _=tokio::time::sleep(Duration::from_millis(200))=>cancelled(stop),
    }
}
pub(super) async fn observe(adb: &Adb, reused: bool) -> Result<NativeAndroidBooted> {
    if !adb.boot_completed().await? || adb.uid().await? != 0 {
        return Err(Error::Invalid("android_not_booted"));
    }
    snapshot(adb, reused).await
}
pub(super) async fn snapshot(adb: &Adb, reused: bool) -> Result<NativeAndroidBooted> {
    let (width, height) = frame(&adb.screenshot().await?)?;
    Ok(NativeAndroidBooted {
        version: 1,
        serial: adb.serial.clone(),
        root_verified: true,
        width,
        height,
        reused_session: reused,
        backend_paired: false,
        backend_bridge_id: String::new(),
    })
}
pub(super) async fn existing(
    adb: &Adb,
    stop: &mut watch::Receiver<bool>,
) -> Result<NativeAndroidBooted> {
    let until = Instant::now() + Duration::from_secs(120);
    loop {
        cancelled(stop)?;
        if let Ok(value) = observe(adb, true).await {
            return Ok(value);
        }
        if Instant::now() >= until {
            return Err(Error::Invalid("android_session_unavailable"));
        }
        pause(stop).await?;
    }
}
