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
    let actual = owner(home)?;
    if expected.version != 1 || expected != &actual {
        return Err(Error::Invalid("phone_peer_identity"));
    }
    let serial = advertised_serial(home, expected)?;
    let mut value = crate::android::session::backend_observation(&serial).await?;
    value.backend_paired = true;
    value.backend_bridge_id = expected.bridge_id.clone();
    Ok(value)
}
fn owner(home: &Path) -> Result<DeviceBridgePeerOwner> {
    DeviceBridgePeerOwner::decode(
        filesystem::bounded_file_with_limit(&home.join(".phone-peer.pb"), 512)?.as_slice(),
    )
    .map_err(|_| Error::Invalid("phone_peer_identity"))
}
fn advertised_serial(home: &Path, expected: &DeviceBridgePeerOwner) -> Result<String> {
    let directory = home.join("avd/running");
    filesystem::existing_directory(&directory)?;
    let rows = super::discovery::values(filesystem::bounded_file_with_limit(
        &directory.join("pid_0.ini"),
        16384,
    )?)?;
    match rows.get("aiphone.serial") {
        Some(serial)
            if serial == super::peer::WHPX_SERIAL
                && rows.get("aiphone.bridge") == Some(&expected.bridge_id) =>
        {
            Ok(serial.clone())
        }
        None if super::discovery::advertised_token(
            &rows,
            5564,
            8558,
            Some(&expected.bridge_id),
        )?
        .is_some() =>
        {
            Ok("emulator-5564".into())
        }
        _ => Err(Error::Invalid("phone_peer_unavailable")),
    }
}

/// Returns the serial advertised by the expected bridge owner.
pub fn paired_phone_serial(bridge_id: &str) -> Result<String> {
    let home = Path::new(RUNTIME);
    let owner = owner(home)?;
    if owner.version != 1 || owner.bridge_id != bridge_id || owner.secret.len() != 32 {
        return Err(Error::Invalid("phone_peer_identity"));
    }
    advertised_serial(home, &owner)
}
