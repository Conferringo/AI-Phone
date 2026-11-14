use super::plan::Plan;
use crate::{Error, Result, filesystem};
use rustix::process::{Pid, Signal, getuid, kill_process};
use std::{
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::PathBuf,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, copy_bidirectional},
    net::{UnixListener, UnixStream},
    process::{Child, Command},
    task::{JoinHandle, JoinSet},
};

pub fn runtime(console: u16) -> PathBuf {
    PathBuf::from(format!(
        "/private/tmp/aimanity-android-{}-{console}",
        getuid().as_raw()
    ))
}

pub(super) struct Relay {
    // The lock outlives the listener and its per-connection tasks.
    task: JoinHandle<std::io::Result<()>>,
    socket: PathBuf,
    _lock: filesystem::Lease,
}
impl Relay {
    pub(super) fn start(console: u16) -> Result<Self> {
        let directory = runtime(console);
        filesystem::directory(&directory, false)?;
        let metadata = fs::symlink_metadata(&directory)?;
        if metadata.uid() != getuid().as_raw() || metadata.mode() & 0o077 != 0 {
            return Err(Error::Invalid("android_avo_runtime_directory"));
        }
        let lock = filesystem::lease(&directory.join("runtime.lock"))?
            .ok_or(Error::Invalid("android_avo_runtime_busy"))?;
        let socket = directory.join("adb.sock");
        match fs::symlink_metadata(&socket) {
            Ok(meta) if meta.file_type().is_socket() => fs::remove_file(&socket)?,
            Ok(_) => return Err(Error::Invalid("android_avo_adb_socket")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let listener = UnixListener::bind(&socket)?;
        let vsock = directory.join("vsock.sock");
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    connection = listener.accept() => {
                        let (mut client, _) = connection?;
                        let vsock = vsock.clone();
                        connections.spawn(async move {
                            let mut guest = connect(vsock).await?;
                            copy_bidirectional(&mut client, &mut guest).await?;
                            Ok::<(), std::io::Error>(())
                        });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Ok(Self {
            task,
            socket,
            _lock: lock,
        })
    }
    pub(super) async fn exited(&mut self) -> Result<()> {
        (&mut self.task)
            .await
            .map_err(|_| Error::Invalid("android_avo_adb_relay"))??;
        Err(Error::Invalid("android_avo_adb_relay"))
    }
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
        let _ = fs::remove_file(&self.socket);
    }
}

async fn connect(path: PathBuf) -> std::io::Result<BufReader<UnixStream>> {
    let mut guest = BufReader::new(UnixStream::connect(path).await?);
    guest.get_mut().write_all(b"CONNECT 6520\n").await?;
    let mut ok = String::new();
    tokio::time::timeout(Duration::from_secs(1), guest.read_line(&mut ok)).await??;
    if !ok.starts_with("OK ") {
        return Err(std::io::Error::other("Avo guest ADB not ready"));
    }
    Ok(guest)
}

pub(super) async fn ready(console: u16) -> bool {
    connect(runtime(console).join("vsock.sock")).await.is_ok()
}

pub(super) fn command(plan: &Plan) -> Result<Command> {
    let instance = plan.avd.join("cuttlefish");
    filesystem::directory(&instance, false)?;
    let userdata = instance.join("userdata.img");
    if !userdata.try_exists()? {
        // As in Avo's DataDiskStore, Android formats the sparse data disk at first boot.
        let disk = tempfile::NamedTempFile::new_in(&instance)?;
        disk.as_file()
            .set_len(u64::from(plan.resources.storage_gb) << 30)?;
        disk.as_file().sync_all()?;
        disk.persist_noclobber(&userdata)
            .map_err(|error| Error::Io(error.error))?;
        filesystem::sync_directory(&instance)?;
    }
    filesystem::regular(&userdata)?;
    let mut command = plan.command("avo", "avo-vmm")?;
    command
        .arg("cuttlefish")
        .arg(plan.sdk.join("avo/images"))
        .arg("--instance")
        .arg(&instance)
        .arg("--runtime")
        .arg(runtime(plan.ports.console))
        .arg("--userdata")
        .arg(userdata)
        .args([
            "--cpus",
            &plan.resources.cpus.to_string(),
            "--mem",
            &plan.resources.memory_mb.to_string(),
            "--display",
            "720x1440",
            "--gpu",
            "2d",
            "--cameras",
            "none",
            "--no-microphone",
        ])
        .env("AVO_VMM_MUTE", "1");
    Ok(command)
}

pub(super) async fn shutdown(child: &mut Child) -> Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    let pid = child
        .id()
        .and_then(|pid| Pid::from_raw(pid as i32))
        .ok_or(Error::Invalid("android_emulator_stop_unknown"))?;
    kill_process(pid, Signal::TERM).map_err(std::io::Error::from)?;
    tokio::time::timeout(Duration::from_secs(15), child.wait())
        .await
        .map_err(|_| Error::Invalid("android_emulator_stop_unknown"))??;
    Ok(())
}
