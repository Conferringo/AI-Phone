use super::{
    plan::Plan,
    process::{self, Started},
};
use crate::{Error, Result, native_ipc as transport};
use ai_phone_api::wire::{
    NativeAndroidSessionControl, NativeAndroidSessionFailure, NativeAndroidSessionReply,
    NativeAndroidStart, native_android_session_reply::Result as Reply,
};
use tokio::sync::watch;

async fn serve(
    request: NativeAndroidStart,
    stop: &mut watch::Receiver<bool>,
    published: &mut bool,
) -> Result<()> {
    match process::start(Plan::parse(request)?, stop).await? {
        Started::Existing(receipt) => {
            transport::write(
                &mut tokio::io::stdout(),
                &NativeAndroidSessionReply {
                    result: Some(Reply::Booted(receipt)),
                },
            )
            .await?;
            *published = true;
            Ok(())
        }
        Started::Owned(mut phone) => {
            let result = async {
                let receipt = phone
                    .receipt
                    .as_ref()
                    .ok_or(Error::Invalid("android_not_booted"))?;
                transport::write(
                    &mut tokio::io::stdout(),
                    &NativeAndroidSessionReply {
                        result: Some(Reply::Booted(receipt.clone())),
                    },
                )
                .await?;
                *published = true;
                phone.wait(stop).await
            }
            .await;
            let cleanup = phone.shutdown().await;
            cleanup.and(result)
        }
    }
}
pub async fn run(arguments: &[std::ffi::OsString]) -> Result<()> {
    if !arguments.is_empty() {
        return Err(Error::Invalid("android_session_arguments"));
    }
    let request: NativeAndroidStart =
        transport::read_bounded(&mut tokio::io::stdin(), 16384).await?;
    let (end, mut stop) = watch::channel(false);
    let control_end = end.clone();
    let control = tokio::spawn(async move {
        if let Ok(value) =
            transport::read::<NativeAndroidSessionControl>(&mut tokio::io::stdin()).await
            && value.cancel_startup
        {
            let _ = control_end.send(true);
        }
    });
    let signal_end = end.clone();
    let signal = tokio::spawn(async move {
        let result = crate::component::interrupt().await;
        if result.is_ok() {
            let _ = signal_end.send(true);
        }
        result
    });
    let mut published = false;
    let result = serve(request, &mut stop, &mut published).await;
    if let Err(error) = &result
        && !published
    {
        let reply = NativeAndroidSessionReply {
            result: Some(Reply::Failure(NativeAndroidSessionFailure {
                code: error.code().into(),
                outcome_unknown: error.code().ends_with("unknown"),
            })),
        };
        let _ = transport::write(&mut tokio::io::stdout(), &reply).await;
    }
    signal.abort();
    let _ = signal.await;
    control.abort();
    let _ = control.await;
    result
}
