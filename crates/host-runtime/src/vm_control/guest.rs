use super::{GUEST_PORT, LIMIT};
use crate::{Error, Result, linux_vm, native_ipc};
use ai_phone_api::wire::{LinuxVmControlReply, LinuxVmControlRequest, LinuxVmOperation};
use ring::hmac;
use std::{net::Ipv4Addr, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    process::Command,
};

const BOOTSTRAP: &str = "/usr/local/libexec/aiphone-bootstrap";
const BUSYBOX: &str = "/opt/aiphone/engine/bin/busybox";

pub(crate) async fn listen() -> Result<TcpListener> {
    Ok(TcpListener::bind((Ipv4Addr::new(10, 0, 2, 15), GUEST_PORT)).await?)
}

pub(crate) async fn run(listener: TcpListener, key: hmac::Key) -> Result<()> {
    let key = Arc::new(key);
    loop {
        let (stream, _) = listener.accept().await?;
        let key = key.clone();
        tokio::spawn(async move {
            if let Err(error) = handle(stream, &key).await {
                eprintln!("AI_PHONE_QEMU_CONTROL {}", error.code());
            }
        });
    }
}

async fn admit(
    stream: &mut TcpStream,
    key: &hmac::Key,
    nonce: &[u8],
) -> Result<LinuxVmControlRequest> {
    let hello: LinuxVmControlRequest = native_ipc::read(stream).await?;
    let client_nonce: [u8; 32] = hello
        .payload
        .try_into()
        .map_err(|_| Error::Invalid("vm_control_nonce"))?;
    native_ipc::write(
        stream,
        &LinuxVmControlReply {
            payload: nonce.to_vec(),
            proof: linux_vm::proof(key, b"control-challenge", &client_nonce, nonce),
            error: String::new(),
        },
    )
    .await?;
    let request: LinuxVmControlRequest = native_ipc::read_bounded(stream, LIMIT).await?;
    linux_vm::verify(
        key,
        &request.operation.to_be_bytes(),
        nonce,
        &request.payload,
        &request.proof,
    )?;
    Ok(request)
}

async fn reply(
    stream: &mut TcpStream,
    key: &hmac::Key,
    nonce: &[u8],
    result: Result<Vec<u8>>,
) -> Result<()> {
    let response = match result {
        Ok(payload) => LinuxVmControlReply {
            proof: linux_vm::proof(key, b"control-reply", nonce, &payload),
            payload,
            error: String::new(),
        },
        Err(error) => LinuxVmControlReply {
            proof: linux_vm::proof(key, b"control-error", nonce, error.code().as_bytes()),
            payload: Vec::new(),
            error: error.code().into(),
        },
    };
    native_ipc::write(stream, &response).await?;
    Ok(())
}

async fn handle(mut stream: TcpStream, key: &hmac::Key) -> Result<()> {
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|_| Error::Invalid("vm_control_nonce"))?;
    let request = tokio::time::timeout(Duration::from_secs(10), admit(&mut stream, key, &nonce))
        .await
        .map_err(|_| Error::Invalid("vm_control_timeout"))??;
    let operation = LinuxVmOperation::try_from(request.operation)
        .map_err(|_| Error::Invalid("vm_control_operation"))?;
    let flag = match operation {
        LinuxVmOperation::Ping => {
            let release = tokio::fs::read_to_string("/opt/aiphone/bundle/source.sha256")
                .await
                .map(|value| value.trim().as_bytes().to_vec())
                .map_err(Error::from);
            return reply(&mut stream, key, &nonce, release).await;
        }
        LinuxVmOperation::Shutdown => {
            // BusyBox asks PID 1 to shut down and flush the persistent guest disk.
            reply(&mut stream, key, &nonce, Ok(Vec::new())).await?;
            let status = Command::new(BUSYBOX).arg("poweroff").status().await?;
            return status
                .success()
                .then_some(())
                .ok_or(Error::Invalid("vm_guest_shutdown"));
        }
        LinuxVmOperation::PhonePeer => "--phone-peer",
        LinuxVmOperation::PhoneStatus => "--phone-peer-status",
        LinuxVmOperation::SetupDevices => "--setup-wsl-devices",
        LinuxVmOperation::Unspecified => return Err(Error::Invalid("vm_control_operation")),
    };
    let peer = operation == LinuxVmOperation::PhonePeer;
    let mut child = Command::new(BOOTSTRAP)
        .arg(flag)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut input = child
        .stdin
        .take()
        .ok_or(Error::Invalid("vm_control_child"))?;
    let mut output = child
        .stdout
        .take()
        .ok_or(Error::Invalid("vm_control_child"))?;
    input.write_u32(request.payload.len() as u32).await?;
    input.write_all(&request.payload).await?;
    let mut owned_input = peer.then_some(input);
    let duration = if operation == LinuxVmOperation::SetupDevices {
        Duration::from_secs(600)
    } else {
        Duration::from_secs(15)
    };
    let result = tokio::time::timeout(duration, async {
        let length = output.read_u32().await? as usize;
        if length > 384 {
            return Err(Error::Invalid("vm_control_child"));
        }
        let mut payload = vec![0; length];
        output.read_exact(&mut payload).await?;
        if !peer && !child.wait().await?.success() {
            return Err(Error::Invalid("vm_control_child"));
        }
        Ok(payload)
    })
    .await
    .unwrap_or(Err(Error::Invalid("vm_control_timeout")));
    let connected = peer && result.is_ok();
    if connected {
        let result = reply(&mut stream, key, &nonce, result).await;
        if result.is_ok() {
            tokio::select! {
                _ = stream.read_u8() => {},
                _ = child.wait() => return Ok(()),
            }
        }
        drop(owned_input.take());
        let _ = tokio::time::timeout(Duration::from_secs(15), child.wait()).await;
        return result;
    }
    reply(&mut stream, key, &nonce, result).await
}
