//! Start the fixed native component; viewport changes never own its lifetime.
use ai_phone_api::wire::{
    NativeAndroidBooted, NativeAndroidSessionControl, NativeAndroidSessionReply,
    NativeAndroidStart, native_android_session_reply::Result as Reply,
};
use ai_phone_host_runtime::android::Prepared;
use ai_phone_host_runtime::native_ipc;
use std::{process::Stdio, time::Duration};
use tokio::process::{Child, Command};

async fn cancelled(test: &dyn Fn() -> bool) {
    while !test() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
async fn launch(
    request: NativeAndroidStart,
    cancel: &dyn Fn() -> bool,
) -> Result<NativeAndroidBooted, String> {
    if cancel() {
        return Err("installation_cancelled".into());
    }
    let mut command = Command::new(std::env::current_exe().map_err(|_| "android_session_program")?);
    command
        .arg("--android-session")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|_| "android_session_start")?;
    // Always close both pipes before reaping: EOF unblocks the worker's stdin.
    // A missing/truncated reply cannot prove whether the phone already started.
    let receipt = exchange(&mut child, &request, cancel).await;
    tauri::async_runtime::spawn(async move {
        let _ = child.wait().await;
    });
    interpret(receipt?, cancel(), !request.wsl_distribution.is_empty())
}

async fn exchange(
    child: &mut Child,
    request: &NativeAndroidStart,
    cancel: &dyn Fn() -> bool,
) -> Result<NativeAndroidSessionReply, &'static str> {
    let mut input = child
        .stdin
        .take()
        .ok_or("android_session_outcome_unknown")?;
    let mut output = child
        .stdout
        .take()
        .ok_or("android_session_outcome_unknown")?;
    native_ipc::write_bounded(&mut input, request, 16384)
        .await
        .map_err(|_| "android_session_outcome_unknown")?;
    tokio::select! {
            result=tokio::time::timeout(Duration::from_secs(150),native_ipc::read_bounded::<NativeAndroidSessionReply>(&mut output,512))=>result.map_err(|_|"android_session_outcome_unknown")?,
            _=cancelled(cancel)=>{
                native_ipc::write_bounded(&mut input,&NativeAndroidSessionControl{cancel_startup:true},512).await.map_err(|_|"android_session_outcome_unknown")?;
                tokio::time::timeout(Duration::from_secs(20),native_ipc::read_bounded::<NativeAndroidSessionReply>(&mut output,512)).await.map_err(|_|"android_session_outcome_unknown")?
            }
        }.map_err(|_| "android_session_outcome_unknown")
}

fn valid_boot(boot: &NativeAndroidBooted) -> bool {
    boot.version == 1
        && boot.root_verified
        && boot.serial == "emulator-5564"
        && [boot.width, boot.height]
            .into_iter()
            .all(|value| (1..=8192).contains(&value))
        && boot.frame_sha256.len() == 64
        && boot
            .frame_sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
}

fn interpret(
    receipt: NativeAndroidSessionReply,
    cancelled: bool,
    paired: bool,
) -> Result<NativeAndroidBooted, String> {
    match receipt.result {
        Some(Reply::Booted(boot))
            if valid_boot(&boot)
                && (!paired
                    || (boot.backend_paired
                        && boot.backend_bridge_id.len() == 32
                        && boot
                            .backend_bridge_id
                            .bytes()
                            .all(|v| v.is_ascii_hexdigit()))) =>
        {
            if cancelled {
                Err("installation_cancelled".into())
            } else {
                Ok(boot)
            }
        }
        Some(Reply::Failure(failure)) => Err(if failure.outcome_unknown {
            "android_session_outcome_unknown".into()
        } else {
            failure.code
        }),
        _ => Err("android_session_outcome_unknown".into()),
    }
}

pub(super) fn start(
    phone: &Prepared,
    distribution: &str,
    cancel: &dyn Fn() -> bool,
) -> Result<NativeAndroidBooted, String> {
    let request = NativeAndroidStart {
        phone: Some(phone.wire().map_err(|e| e.code().to_string())?),
        adb_port: 0,
        console_port: 0,
        grpc_port: 0,
        wsl_distribution: distribution.into(),
    };
    tauri::async_runtime::block_on(launch(request, cancel))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_initialization_never_launches_a_worker() {
        let phone = Prepared {
            sdk_root: "unused-sdk".into(),
            avd_home: "unused-avd".into(),
            reused_sdk: false,
            reused_device: false,
        };
        assert_eq!(
            start(&phone, "", &|| true),
            Err("installation_cancelled".into())
        );
    }
    #[test]
    fn missing_unrooted_or_other_device_receipts_never_publish_ready() {
        for boot in [
            NativeAndroidBooted::default(),
            NativeAndroidBooted {
                version: 1,
                serial: "emulator-5582".into(),
                root_verified: true,
                width: 720,
                height: 1440,
                frame_sha256: "a".repeat(64),
                reused_session: false,
                backend_paired: false,
                backend_bridge_id: String::new(),
            },
        ] {
            assert_eq!(
                interpret(
                    NativeAndroidSessionReply {
                        result: Some(Reply::Booted(boot))
                    },
                    false,
                    false
                ),
                Err("android_session_outcome_unknown".into())
            );
        }
        let unknown = NativeAndroidSessionReply {
            result: Some(Reply::Failure(
                ai_phone_api::wire::NativeAndroidSessionFailure {
                    code: "primary_failure".into(),
                    outcome_unknown: true,
                },
            )),
        };
        assert_eq!(
            interpret(unknown, false, false),
            Err("android_session_outcome_unknown".into())
        );
    }
    #[test]
    fn a_local_boot_does_not_satisfy_a_requested_backend_pair() {
        let mut boot = NativeAndroidBooted {
            version: 1,
            serial: "emulator-5564".into(),
            root_verified: true,
            width: 720,
            height: 1440,
            frame_sha256: "a".repeat(64),
            ..Default::default()
        };
        let reply = |boot| NativeAndroidSessionReply {
            result: Some(Reply::Booted(boot)),
        };
        assert!(interpret(reply(boot.clone()), false, false).is_ok());
        assert_eq!(
            interpret(reply(boot.clone()), false, true),
            Err("android_session_outcome_unknown".into())
        );
        boot.backend_paired = true;
        boot.backend_bridge_id = "a".repeat(32);
        assert_eq!(interpret(reply(boot.clone()), false, true).unwrap(), boot);
        assert_eq!(
            interpret(reply(boot), true, true),
            Err("installation_cancelled".into())
        );
    }
}
