use crate::{Error, Result, filesystem, native_ipc};
use ai_phone_api::wire::{DeviceBridgeGuestAddress, DeviceBridgePeerOwner};
use prost::Message;
use std::{
    ffi::OsString,
    net::{Ipv4Addr, UdpSocket},
    path::Path,
};

pub const RUNTIME: &str = "/var/lib/aiphone/installation/user/data/android/user";

pub async fn address(arguments: &[OsString]) -> Result<()> {
    if !arguments.is_empty() {
        return Err(Error::Invalid("phone_guest_arguments"));
    }
    // UDP connect selects the default route without sending a packet or DNS query.
    let route = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    route.connect((Ipv4Addr::new(1, 1, 1, 1), 9))?;
    let std::net::IpAddr::V4(ip) = route.local_addr()?.ip() else {
        return Err(Error::Invalid("phone_guest_address"));
    };
    if !ip.is_private() {
        return Err(Error::Invalid("phone_guest_address"));
    }
    native_ipc::write_bounded(
        &mut tokio::io::stdout(),
        &DeviceBridgeGuestAddress {
            ipv4: ip.to_string(),
        },
        512,
    )
    .await?;
    Ok(())
}
pub async fn status(arguments: &[OsString]) -> Result<()> {
    if !arguments.is_empty() {
        return Err(Error::Invalid("phone_guest_arguments"));
    }
    let expected: DeviceBridgePeerOwner =
        native_ipc::read_bounded(&mut tokio::io::stdin(), 512).await?;
    let value = observe(Path::new(RUNTIME), &expected).await?;
    native_ipc::write_bounded(&mut tokio::io::stdout(), &value, 512).await?;
    Ok(())
}
async fn observe(
    home: &Path,
    expected: &DeviceBridgePeerOwner,
) -> Result<ai_phone_api::wire::NativeAndroidBooted> {
    let actual = DeviceBridgePeerOwner::decode(
        filesystem::bounded_file_with_limit(&home.join(".phone-peer.pb"), 512)?.as_slice(),
    )
    .map_err(|_| Error::Invalid("phone_peer_identity"))?;
    if expected.version != 1 || expected != &actual {
        return Err(Error::Invalid("phone_peer_identity"));
    }
    if super::discovery::token(
        &home.join("avd/running"),
        5564,
        8558,
        Some(&expected.bridge_id),
    )?
    .is_none()
    {
        return Err(Error::Invalid("phone_peer_unavailable"));
    }
    let mut value = crate::android::session::backend_observation().await?;
    value.backend_paired = true;
    Ok(value)
}
