//! The existing native phone owner outlives the installer, just as it outlives a viewport.
use crate::{Error, Result, native_ipc};
use ai_phone_api::wire::{
    NativeAndroidBooted, NativeAndroidSessionControl, NativeAndroidSessionReply,
    NativeAndroidStart, native_android_session_reply::Result as Reply,
};
use std::{process::Stdio, time::Duration};
use tokio::{process::Command, sync::watch};

pub(super) async fn start(
    request: NativeAndroidStart,
    mut stop: watch::Receiver<bool>,
) -> Result<NativeAndroidBooted> {
    if *stop.borrow() {
        return Err(Error::Cancelled);
    }
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--android-session")
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut input = child
        .stdin
        .take()
        .ok_or(Error::Invalid("android_session_outcome_unknown"))?;
    let mut output = child
        .stdout
        .take()
        .ok_or(Error::Invalid("android_session_outcome_unknown"))?;
    native_ipc::write_bounded(&mut input, &request, 16384).await?;
    let receipt = tokio::select! {
        value=tokio::time::timeout(Duration::from_secs(150),native_ipc::read_bounded::<NativeAndroidSessionReply>(&mut output,512))=>value.map_err(|_|Error::Invalid("android_session_outcome_unknown"))?,
        _=stop.changed()=>{
            native_ipc::write_bounded(&mut input,&NativeAndroidSessionControl{cancel_startup:true},512).await?;
            tokio::time::timeout(Duration::from_secs(20),native_ipc::read_bounded::<NativeAndroidSessionReply>(&mut output,512)).await.map_err(|_|Error::Invalid("android_session_outcome_unknown"))?
        }
    }?;
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    match receipt.result {
        Some(Reply::Booted(value))
            if value.version == 1
                && value.root_verified
                && value.serial == "emulator-5564"
                && value.width > 0
                && value.height > 0 =>
        {
            if *stop.borrow() {
                Err(Error::Cancelled)
            } else {
                Ok(value)
            }
        }
        Some(Reply::Failure(_)) => Err(Error::Invalid("android_session_start_failed")),
        _ => Err(Error::Invalid("android_session_outcome_unknown")),
    }
}
