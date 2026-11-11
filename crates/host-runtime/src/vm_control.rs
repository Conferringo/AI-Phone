//! Authenticated, loopback-forwarded control of the owned Linux VM.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) mod guest;

use crate::{Error, Result, linux_vm, native_ipc};
use ai_phone_api::wire::{
    LinuxVmControlReply, LinuxVmControlRequest, LinuxVmOperation, NativeWslDeviceSetupRequest,
    NativeWslDevicesReady,
};
use prost::Message;
use std::{net::Ipv4Addr, path::Path, time::Duration};
use tokio::net::TcpStream;

pub const GUEST_PORT: u16 = 17861;
const LIMIT: usize = 16448;

pub(crate) async fn open(
    port: u16,
    key_file: &Path,
    operation: LinuxVmOperation,
    payload: Vec<u8>,
) -> Result<(TcpStream, Vec<u8>)> {
    let key = linux_vm::read_key(key_file)?;
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await?;
    let mut client_nonce = [0; 32];
    getrandom::fill(&mut client_nonce).map_err(|_| Error::Invalid("vm_control_nonce"))?;
    native_ipc::write(
        &mut stream,
        &LinuxVmControlRequest {
            payload: client_nonce.to_vec(),
            ..Default::default()
        },
    )
    .await?;
    let challenge: LinuxVmControlReply = native_ipc::read(&mut stream).await?;
    let nonce: [u8; 32] = challenge
        .payload
        .try_into()
        .map_err(|_| Error::Invalid("vm_control_nonce"))?;
    linux_vm::verify(
        &key,
        b"control-challenge",
        &client_nonce,
        &nonce,
        &challenge.proof,
    )?;
    let proof = linux_vm::proof(&key, &(operation as i32).to_be_bytes(), &nonce, &payload);
    native_ipc::write_bounded(
        &mut stream,
        &LinuxVmControlRequest {
            operation: operation as i32,
            payload,
            proof,
        },
        LIMIT,
    )
    .await?;
    let reply: LinuxVmControlReply = native_ipc::read(&mut stream).await?;
    if reply.error.is_empty() {
        linux_vm::verify(&key, b"control-reply", &nonce, &reply.payload, &reply.proof)?;
        Ok((stream, reply.payload))
    } else {
        linux_vm::verify(
            &key,
            b"control-error",
            &nonce,
            reply.error.as_bytes(),
            &reply.proof,
        )?;
        eprintln!("AI_PHONE_QEMU_CONTROL_ERROR {}", reply.error);
        Err(Error::Invalid("vm_control_guest_failed"))
    }
}

async fn empty(port: u16, key: &Path, operation: LinuxVmOperation) -> Result<()> {
    tokio::time::timeout(
        Duration::from_secs(15),
        open(port, key, operation, Vec::new()),
    )
    .await
    .map_err(|_| Error::Invalid("vm_control_timeout"))??;
    Ok(())
}

#[cfg(windows)]
pub(crate) async fn version(port: u16, key: &Path) -> Result<Vec<u8>> {
    let (_, payload) = tokio::time::timeout(
        Duration::from_secs(15),
        open(port, key, LinuxVmOperation::Ping, Vec::new()),
    )
    .await
    .map_err(|_| Error::Invalid("vm_control_timeout"))??;
    Ok(payload)
}

pub async fn shutdown(port: u16, key: &Path) -> Result<()> {
    empty(port, key, LinuxVmOperation::Shutdown).await
}

pub async fn complete_devices(
    port: u16,
    key: &Path,
    bridge_id: &str,
) -> Result<NativeWslDevicesReady> {
    let request = NativeWslDeviceSetupRequest {
        version: 1,
        bridge_id: bridge_id.into(),
    };
    let (_, bytes) = tokio::time::timeout(
        Duration::from_secs(600),
        open(
            port,
            key,
            LinuxVmOperation::SetupDevices,
            request.encode_to_vec(),
        ),
    )
    .await
    .map_err(|_| Error::Invalid("vm_device_setup_unknown"))??;
    crate::wsl::devices::decode_receipt(&bytes, bridge_id)
}
