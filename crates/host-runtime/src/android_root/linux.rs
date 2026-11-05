use crate::{Error, Installation, Result};
use ai_phone_api::wire::{
    NativeAndroidRootExited, NativeAndroidRootInstall, NativeAndroidRootPrepared,
    NativeAndroidRootStart,
};
use flate2::read::GzDecoder;
use rustix::{
    mount::{UnmountFlags, mount_bind, unmount},
    process::getuid,
};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::process::Command;

const TOYBOX: &str = "/system/bin/toybox";
const QEMU: &str = "/usr/bin/qemu-system-aarch64";
const KERNEL: &str = "/opt/ai-phone/guest/kernel";
const INITRD: &str = "/opt/ai-phone/guest/initramfs.gz";

struct Cancellable<R> {
    source: R,
    cancelled: Arc<AtomicBool>,
}

impl<R: Read> Read for Cancellable<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(std::io::Error::other(Error::Cancelled));
        }
        self.source.read(bytes)
    }
}

fn extract(request: NativeAndroidRootInstall, cancelled: Arc<AtomicBool>) -> Result<PathBuf> {
    if request.version != 1
        || !Path::new(&request.payload_path).is_absolute()
        || !Path::new(&request.installation).is_absolute()
    {
        return Err(Error::Invalid("android_root_install_request"));
    }
    let installation = Installation::lock(Path::new(&request.installation))?;
    let home = &installation.home;
    let destination = home.join("rootfs");
    if destination.try_exists()? {
        return Err(Error::Invalid("android_root_already_installed"));
    }
    let pending = tempfile::Builder::new()
        .prefix(".rootfs-")
        .tempdir_in(home)?;
    let source = Cancellable {
        source: File::open(&request.payload_path)?,
        cancelled: cancelled.clone(),
    };
    tar::Archive::new(GzDecoder::new(source))
        .unpack(pending.path())
        .map_err(|error| {
            if cancelled.load(Ordering::Relaxed) {
                Error::Cancelled
            } else {
                error.into()
            }
        })?;
    fs::set_permissions(pending.path(), fs::Permissions::from_mode(0o700))?;
    for name in [
        "usr/bin/qemu-system-aarch64",
        "opt/ai-phone/guest/kernel",
        "opt/ai-phone/guest/initramfs.gz",
    ] {
        if !pending.path().join(name).is_file() {
            return Err(Error::Invalid("android_root_runtime_incomplete"));
        }
    }
    rustix::fs::syncfs(File::open(pending.path())?).map_err(std::io::Error::from)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    fs::rename(pending.path(), &destination)?;
    File::open(home)?.sync_all()?;
    Ok(destination)
}

async fn install(request: NativeAndroidRootInstall) -> Result<NativeAndroidRootPrepared> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut work = tokio::task::spawn_blocking({
        let cancelled = cancelled.clone();
        move || extract(request, cancelled)
    });
    let result = tokio::select! {
        result = &mut work => result,
        signal = crate::component::interrupt() => {
            signal?;
            cancelled.store(true, Ordering::Relaxed);
            work.await
        }
    }
    .map_err(|_| Error::Invalid("android_root_install_worker"))??;
    Ok(NativeAndroidRootPrepared {
        version: 1,
        runtime_root: result.display().to_string(),
    })
}

fn qemu_arguments(request: &NativeAndroidRootStart) -> Result<Vec<OsString>> {
    if request.version != 1
        || ![1, 2, 4].contains(&request.cpus)
        || ![256, 512, 1024, 1536, 2048, 3072, 4096].contains(&request.memory_mb)
        || !Path::new(&request.runtime_root).is_absolute()
    {
        return Err(Error::Invalid("android_root_guest_request"));
    }
    let accelerator = if request.diagnostic_tcg { "tcg" } else { "kvm" };
    let cpu = if request.diagnostic_tcg {
        "cortex-a72"
    } else {
        "host"
    };
    Ok([
        "chroot",
        &request.runtime_root,
        QEMU,
        "-machine",
        "virt",
        "-cpu",
        cpu,
        "-smp",
        &request.cpus.to_string(),
        "-m",
        &format!("{}M", request.memory_mb),
        "-accel",
        accelerator,
        "-display",
        "none",
        "-monitor",
        "none",
        "-serial",
        "stdio",
        "-nic",
        "none",
        "-kernel",
        KERNEL,
        "-initrd",
        INITRD,
        "-append",
        "console=ttyAMA0 panic=-1",
        "-no-reboot",
    ]
    .into_iter()
    .map(OsString::from)
    .collect())
}

#[derive(Default)]
struct Mounts(Vec<PathBuf>);

impl Mounts {
    fn bind(&mut self, source: &Path, target: PathBuf) -> Result<()> {
        mount_bind(source, &target).map_err(std::io::Error::from)?;
        self.0.push(target);
        Ok(())
    }

    fn close(&mut self) -> std::io::Result<()> {
        while let Some(path) = self.0.last() {
            unmount(path, UnmountFlags::empty()).map_err(std::io::Error::from)?;
            self.0.pop();
        }
        Ok(())
    }
}

async fn guest(request: NativeAndroidRootStart) -> Result<NativeAndroidRootExited> {
    let arguments = qemu_arguments(&request)?;
    let root = Path::new(&request.runtime_root).canonicalize()?;
    let _lease = crate::filesystem::lease(&root.join(".guest.lock"))?.ok_or(Error::Busy)?;
    if !request.diagnostic_tcg {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")?;
    }
    let mut mounts = Mounts::default();
    let result = async {
        for name in ["dev", "proc"] {
            mounts.bind(&Path::new("/").join(name), root.join(name))?;
        }
        let mut child = Command::new(TOYBOX)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::from(std::io::stderr()))
            .stderr(Stdio::inherit())
            .process_group(0)
            .kill_on_drop(true)
            .spawn()?;
        let (status, cancelled) = tokio::select! {
            status = child.wait() => (status?, false),
            signal = crate::component::interrupt() => {
                signal?;
                crate::supervision::stop_child(&mut child).await?;
                (child.wait().await?, true)
            }
        };
        Ok(NativeAndroidRootExited {
            version: 1,
            exit_code: status.code().unwrap_or(-1),
            kvm: !request.diagnostic_tcg,
            cancelled,
        })
    }
    .await;
    match (result, mounts.close()) {
        (result, Ok(())) => result,
        (Ok(_), Err(error)) => Err(error.into()),
        (Err(primary), Err(cleanup)) => Err(Error::Cleanup {
            primary: Box::new(primary),
            cleanup,
        }),
    }
}

async fn platform() -> Result<()> {
    if getuid().as_raw() != 0 {
        return Err(Error::Invalid("android_root_required"));
    }
    let output = Command::new("/system/bin/getprop")
        .arg("ro.build.version.sdk")
        .output()
        .await?;
    if !output.status.success() || output.stdout.iter().all(u8::is_ascii_whitespace) {
        return Err(Error::Invalid("android_root_requires_android"));
    }
    Ok(())
}

pub(super) async fn run(arguments: &[OsString]) -> Result<()> {
    let [operation] = arguments else {
        return Err(Error::Invalid("android_root_arguments"));
    };
    platform().await?;
    let mut input = tokio::io::stdin();
    let mut output = tokio::io::stdout();
    match operation.to_str() {
        Some("install") => {
            let request = crate::native_ipc::read_bounded(&mut input, 65_536).await?;
            let reply = install(request).await?;
            crate::native_ipc::write_bounded(&mut output, &reply, 65_536).await?;
        }
        Some("run") => {
            let request = crate::native_ipc::read_bounded(&mut input, 65_536).await?;
            let reply = guest(request).await?;
            crate::native_ipc::write_bounded(&mut output, &reply, 65_536).await?;
        }
        _ => return Err(Error::Invalid("android_root_arguments")),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
