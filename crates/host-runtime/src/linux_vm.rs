#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
use crate::native_ipc;
use crate::{Error, Result};
#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
use ai_phone_api::wire::{LinuxVmBootstrapReply, LinuxVmBootstrapRequest};
use ring::hmac;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::ffi::OsString;
#[cfg(windows)]
use std::time::Duration;
use std::{fs, path::Path};
#[cfg(windows)]
use tokio::net::TcpListener;

#[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
const FRAME_LIMIT: usize = 8192 + 80;

pub(crate) fn read_key(path: &Path) -> Result<hmac::Key> {
    let bytes: [u8; 32] = fs::read(path)?
        .try_into()
        .map_err(|_| Error::Invalid("vm_bootstrap_key"))?;
    Ok(hmac::Key::new(hmac::HMAC_SHA256, &bytes))
}

pub(crate) fn proof(key: &hmac::Key, stage: &[u8], nonce: &[u8], data: &[u8]) -> Vec<u8> {
    hmac::sign(key, &[stage, nonce, data].concat())
        .as_ref()
        .to_vec()
}

pub(crate) fn verify(
    key: &hmac::Key,
    stage: &[u8],
    nonce: &[u8],
    data: &[u8],
    proof: &[u8],
) -> Result<()> {
    hmac::verify(key, &[stage, nonce, data].concat(), proof)
        .map_err(|_| Error::Invalid("vm_bootstrap_authentication"))
}

#[cfg(windows)]
pub(crate) async fn authenticate(
    listener: TcpListener,
    key: &hmac::Key,
    profile: Vec<u8>,
) -> Result<()> {
    let (mut stream, _) = tokio::time::timeout(Duration::from_secs(120), listener.accept())
        .await
        .map_err(|_| Error::Invalid("vm_bootstrap_connect_timeout"))??;
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|_| Error::Invalid("vm_bootstrap_nonce"))?;
    tokio::time::timeout(Duration::from_secs(1800), async {
        native_ipc::write(
            &mut stream,
            &LinuxVmBootstrapRequest {
                nonce: nonce.to_vec(),
                ..Default::default()
            },
        )
        .await?;
        let reply: LinuxVmBootstrapReply = native_ipc::read(&mut stream).await?;
        verify(key, b"guest", &nonce, b"", &reply.proof)?;
        let proof = proof(key, b"host", &nonce, &profile);
        native_ipc::write_bounded(
            &mut stream,
            &LinuxVmBootstrapRequest {
                nonce: Vec::new(),
                profile,
                proof,
            },
            FRAME_LIMIT,
        )
        .await?;
        let reply: LinuxVmBootstrapReply = native_ipc::read(&mut stream).await?;
        verify(key, b"result", &nonce, reply.error.as_bytes(), &reply.proof)?;
        if !reply.error.is_empty() {
            eprintln!("AI_PHONE_QEMU_GUEST_ERROR {}", reply.error);
            return Err(Error::Invalid("vm_bootstrap_guest_failed"));
        }
        Ok(())
    })
    .await
    .map_err(|_| Error::Invalid("vm_bootstrap_initialization_timeout"))?
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn prepare_guest() -> Result<()> {
    let busybox = "/opt/aiphone/engine/bin/busybox";
    let commands: &[&[&str]] = &[
        &["mount", "-t", "proc", "proc", "/proc"],
        &["mount", "-t", "sysfs", "sysfs", "/sys"],
        &["mount", "-t", "tmpfs", "-o", "mode=0755", "tmpfs", "/run"],
        &["mount", "-t", "cgroup2", "cgroup2", "/sys/fs/cgroup"],
        &["ip", "link", "set", "lo", "up"],
        &["ip", "link", "set", "eth0", "up"],
        &["ip", "addr", "add", "10.0.2.15/24", "dev", "eth0"],
        &["ip", "route", "add", "default", "via", "10.0.2.2"],
    ];
    for command in commands {
        if !std::process::Command::new(busybox)
            .args(*command)
            .status()?
            .success()
        {
            return Err(Error::Invalid("vm_guest_boot_setup"));
        }
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) async fn guest(arguments: &[OsString]) -> Result<()> {
    if !arguments.is_empty() {
        return Err(Error::Invalid("vm_bootstrap_arguments"));
    }
    prepare_guest()?;
    let key = read_key(Path::new(
        "/sys/firmware/qemu_fw_cfg/by_name/opt/aiphone/control-key/raw",
    ))?;
    let mut port = None;
    for device in fs::read_dir("/sys/class/virtio-ports")? {
        let device = device?;
        if fs::read_to_string(device.path().join("name"))?.trim() == "org.aiphone.bootstrap" {
            port = Some(Path::new("/dev").join(device.file_name()));
            break;
        }
    }
    let mut stream = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(port.ok_or(Error::Invalid("vm_bootstrap_port"))?)
        .await?;
    let challenge: LinuxVmBootstrapRequest = native_ipc::read(&mut stream).await?;
    let nonce: [u8; 32] = challenge
        .nonce
        .try_into()
        .map_err(|_| Error::Invalid("vm_bootstrap_nonce"))?;
    native_ipc::write(
        &mut stream,
        &LinuxVmBootstrapReply {
            proof: proof(&key, b"guest", &nonce, b""),
            error: String::new(),
        },
    )
    .await?;
    let request: LinuxVmBootstrapRequest =
        native_ipc::read_bounded(&mut stream, FRAME_LIMIT).await?;
    verify(&key, b"host", &nonce, &request.profile, &request.proof)?;
    let initialization = async {
        crate::wsl::backend::initialize_request(&request.profile, "0.0.0.0").await?;
        crate::vm_control::guest::listen().await
    }
    .await;
    let error = initialization
        .as_ref()
        .err()
        .map(|error| error.code().to_owned())
        .unwrap_or_default();
    native_ipc::write(
        &mut stream,
        &LinuxVmBootstrapReply {
            proof: proof(&key, b"result", &nonce, error.as_bytes()),
            error,
        },
    )
    .await?;
    drop(stream);
    crate::vm_control::guest::run(initialization?, key).await
}
