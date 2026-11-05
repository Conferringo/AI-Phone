//! The native phone worker holds the link even after the UI detaches.
use super::plan::Plan;
use crate::{
    Error, Result,
    device_bridge::{self, Connection},
    filesystem, native_ipc,
    wsl::{self, PhoneOperation},
};
use ai_phone_api::wire::{
    DeviceBridgeConfig, DeviceBridgeGuestAddress, DeviceBridgePeerOwner, DeviceBridgePhone,
    DeviceBridgeRole, NativeAndroidBooted,
};
use prost::Message;
use std::{
    io::Write,
    net::{Ipv4Addr, UdpSocket},
    process::Stdio,
    time::Duration,
};
use tokio::io::AsyncReadExt;
use tokio::{process::Command, sync::watch};

fn identity(plan: &Plan) -> Result<DeviceBridgePeerOwner> {
    let path = plan.home.join("phone-link.pb");
    if path.try_exists()? {
        return DeviceBridgePeerOwner::decode(
            filesystem::bounded_file_with_limit(&path, 512)?.as_slice(),
        )
        .map_err(|_| Error::Invalid("phone_link_identity"));
    }
    let mut random = [0u8; 48];
    getrandom::fill(&mut random).map_err(|_| Error::Invalid("phone_link_entropy"))?;
    let value = DeviceBridgePeerOwner {
        version: 1,
        bridge_id: random[..16].iter().map(|b| format!("{b:02x}")).collect(),
        secret: random[16..].to_vec(),
    };
    let mut output = tempfile::NamedTempFile::new_in(&plan.home)?;
    output.write_all(&value.encode_to_vec())?;
    output.as_file().sync_all()?;
    output
        .persist_noclobber(path)
        .map_err(|e| Error::Io(e.error))?;
    filesystem::sync_directory(&plan.home)?;
    Ok(value)
}
fn token(plan: &Plan) -> Result<String> {
    let mut result = None;
    for root in [
        plan.home.join("profile/local/Temp"),
        plan.home.join("profile/Library/Caches/TemporaryItems"),
        plan.home.join("android"),
    ] {
        if let Some(value) = device_bridge::discovery::token(
            &root.join("avd/running"),
            plan.ports.console,
            plan.ports.grpc,
            None,
        )? && result.replace(value).is_some()
        {
            return Err(Error::Invalid("emulator_discovery_invalid"));
        }
    }
    result.ok_or(Error::Invalid("emulator_auth_unavailable"))
}

fn host_address(guest: Ipv4Addr) -> Result<Ipv4Addr> {
    if !guest.is_private() {
        return Err(Error::Invalid("phone_guest_address"));
    }
    let route = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    route.connect((guest, 9))?;
    let std::net::IpAddr::V4(local) = route.local_addr()?.ip() else {
        return Err(Error::Invalid("phone_guest_address"));
    };
    selected_address(guest, local)
}
fn selected_address(guest: Ipv4Addr, local: Ipv4Addr) -> Result<Ipv4Addr> {
    if !local.is_private() && !local.is_loopback() {
        return Err(Error::Invalid("phone_guest_address"));
    }
    Ok(if guest == local {
        Ipv4Addr::LOCALHOST
    } else {
        local
    })
}

async fn request(
    plan: &Plan,
    operation: PhoneOperation,
    input: Option<&DeviceBridgePeerOwner>,
    stop: &mut watch::Receiver<bool>,
) -> Result<Vec<u8>> {
    if *stop.borrow() {
        return Err(Error::Cancelled);
    }
    let mut command = Command::from(wsl::phone_command(&plan.wsl_distribution, operation)?);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let mut reader = child
        .stdout
        .take()
        .ok_or(Error::Invalid("phone_link_observation"))?;
    let exchange = async {
        if let Some(input) = input {
            let mut writer = child
                .stdin
                .take()
                .ok_or(Error::Invalid("phone_link_observation"))?;
            native_ipc::write_bounded(&mut writer, input, 512).await?;
        }
        drop(child.stdin.take());
        let mut value = Vec::new();
        (&mut reader).take(517).read_to_end(&mut value).await?;
        if value.len() > 516 {
            return Err(Error::Invalid("phone_link_observation"));
        }
        if !child.wait().await?.success() {
            return Err(Error::Invalid("phone_link_observation"));
        }
        Ok(value)
    };
    let result = tokio::select! {biased;
        _=stop.changed()=>Err(Error::Cancelled),
        result=tokio::time::timeout(Duration::from_secs(15),exchange)=>result.unwrap_or(Err(Error::Invalid("phone_link_observation"))),
    };
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

async fn query<T: Message + Default>(
    plan: &Plan,
    operation: PhoneOperation,
    input: Option<&DeviceBridgePeerOwner>,
    stop: &mut watch::Receiver<bool>,
) -> Result<T> {
    let bytes = request(plan, operation, input, stop).await?;
    let mut source = bytes.as_slice();
    let value = native_ipc::read_bounded(&mut source, 512).await?;
    if !source.is_empty() {
        return Err(Error::Invalid("phone_link_observation"));
    }
    Ok(value)
}
fn contract(bytes: &[u8]) -> Result<()> {
    if bytes != format!("{}\n", device_bridge::PEER_CONTRACT).as_bytes() {
        return Err(Error::Invalid("phone_link_upgrade_required"));
    }
    Ok(())
}
pub(super) async fn ensure_backend(plan: &Plan, stop: &mut watch::Receiver<bool>) -> Result<()> {
    // This query exists in v1 too. Refuse an old installed bootstrap before
    // sending it newer commands or a pipe-owned peer request.
    contract(&request(plan, PhoneOperation::Contract, None, stop).await?)?;
    if request(plan, PhoneOperation::DeviceSetupContract, None, stop).await?
        != format!("{}\n", wsl::DEVICE_SETUP_CONTRACT).as_bytes()
    {
        return Err(Error::Invalid("phone_link_upgrade_required"));
    }
    Ok(())
}

pub(super) async fn observe(
    plan: &Plan,
    stop: &mut watch::Receiver<bool>,
) -> Result<NativeAndroidBooted> {
    let owner = identity(plan)?;
    let value: NativeAndroidBooted =
        query(plan, PhoneOperation::Status, Some(&owner), stop).await?;
    if value.version != 1
        || value.serial != plan.serial()
        || !value.root_verified
        || !value.backend_paired
        || ![value.width, value.height]
            .into_iter()
            .all(|n| (1..=8192).contains(&n))
        || value.frame_sha256.len() != 64
        || !value.frame_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(Error::Invalid("phone_link_observation"));
    }
    Ok(value)
}
pub(super) async fn start(plan: &Plan, stop: &mut watch::Receiver<bool>) -> Result<Connection> {
    let token = token(plan)?;
    let address: DeviceBridgeGuestAddress =
        query(plan, PhoneOperation::Address, None, stop).await?;
    let guest = address
        .ipv4
        .parse()
        .map_err(|_| Error::Invalid("phone_guest_address"))?;
    let address = host_address(guest)?;
    let owner = identity(plan)?;
    let request = DeviceBridgePhone {
        transport: Some(DeviceBridgeConfig {
            version: owner.version,
            role: DeviceBridgeRole::Guest as i32,
            bridge_id: owner.bridge_id,
            secret: owner.secret,
            endpoint: String::new(),
            adb_port: 0,
            emulator_port: 0,
        }),
        runtime_directory: device_bridge::guest::RUNTIME.into(),
        serial: plan.serial(),
        grpc_token: token,
        owner_pipe: true,
    };
    let command = Command::from(wsl::phone_command(
        &plan.wsl_distribution,
        PhoneOperation::Peer,
    )?);
    let mut connection = Connection::start(request, address, command, stop).await?;
    if let Err(error) = observe(plan, stop).await {
        connection.shutdown().await?;
        return Err(error);
    }
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nat_uses_its_selected_host_interface_and_mirrored_uses_loopback() {
        assert_eq!(
            selected_address(
                "172.22.68.2".parse().unwrap(),
                "172.22.64.1".parse().unwrap()
            )
            .unwrap(),
            "172.22.64.1".parse::<Ipv4Addr>().unwrap()
        );
        assert_eq!(
            selected_address(
                "192.168.1.2".parse().unwrap(),
                "192.168.1.2".parse().unwrap()
            )
            .unwrap(),
            Ipv4Addr::LOCALHOST
        );
        assert!(
            selected_address("172.22.68.2".parse().unwrap(), "8.8.8.8".parse().unwrap()).is_err()
        );
    }
    #[test]
    fn old_or_partial_peer_contracts_cannot_enable_managed_pairing() {
        for bytes in [
            b"ai-phone-phone-peer-v1\n".as_slice(),
            b"",
            b"ai-phone-phone-peer-v2\nextra",
        ] {
            assert!(contract(bytes).is_err());
        }
        contract(b"ai-phone-phone-peer-v2\n").unwrap();
    }
}
