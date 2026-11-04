use super::{Bridge, transport};
use crate::{Error, Result};
use ai_phone_api::wire::DeviceBridgeConfig;
use std::ffi::OsString;
use tokio::sync::watch;

pub async fn run(arguments: &[OsString]) -> Result<()> {
    if !arguments.is_empty() {
        return Err(Error::Invalid("device_bridge_arguments"));
    }
    let request: DeviceBridgeConfig = transport::read(&mut tokio::io::stdin()).await?;
    let bridge = Bridge::bind(request).await?;
    transport::write(&mut tokio::io::stdout(), bridge.receipt()).await?;
    let (end, stop) = watch::channel(false);
    let signal = tokio::spawn(async move {
        let result = crate::component::interrupt().await;
        let _ = end.send(true);
        result
    });
    let result = bridge.serve(stop).await;
    signal.abort();
    let _ = signal.await;
    result
}
