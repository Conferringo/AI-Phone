use super::plan::Plan;
use crate::{Error, Result, android::device, filesystem};
use serde_json::{Value, json};
use std::{
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    process::{Child, Command},
};

async fn userdata(plan: &Plan, directory: &Path, sdk: &Path) -> Result<PathBuf> {
    let disk = directory.join("android.qcow2");
    if !disk.try_exists()? {
        let base = sdk.join(format!("userdata-{}.qcow2", plan.resources.storage_gb));
        filesystem::regular(&base)?;
        let scratch = tempfile::tempdir_in(directory)?;
        let path = scratch.path().join("userdata.qcow2");
        let status = plan
            .command("cuttlefish", "qemu-img")?
            .args(["create", "-f", "qcow2", "-F", "qcow2", "-b"])
            .arg(device::text(&base)?)
            .arg(device::text(&path)?)
            .stdout(Stdio::null())
            .kill_on_drop(true)
            .status()
            .await?;
        if !status.success() {
            return Err(Error::Invalid("android_disk_create"));
        }
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)?
            .sync_all()?;
        std::fs::rename(path, &disk)?;
        filesystem::sync_directory(directory)?;
    }
    filesystem::regular(&disk)?;
    Ok(disk)
}

pub(super) async fn command(plan: &Plan, console: u16) -> Result<Command> {
    let sdk = plan.sdk.join("cuttlefish");
    let directory = plan.avd.join("cuttlefish");
    filesystem::directory(&directory, false)?;
    let disk = sdk.join("disk.qcow2");
    let kernel = sdk.join("bzImage");
    filesystem::regular(&disk)?;
    filesystem::regular(&kernel)?;
    let data = userdata(plan, &directory, &sdk).await?;
    let initrd = sdk.join("initrd.img");
    filesystem::regular(&initrd)?;
    let block = |name: &str, path: &Path, read_only: bool| -> Result<String> {
        Ok(json!({
            "driver": "qcow2", "node-name": name, "read-only": read_only,
            "file": {"driver": "file", "filename": device::text(path)?}
        })
        .to_string())
    };
    let mut command = plan.command("cuttlefish", "qemu-system-x86_64")?;
    command
        .current_dir(&directory)
        .args(["-machine", "q35,accel=whpx", "-nodefaults", "-display", "none", "-no-reboot"])
        .args(["-cpu", "Nehalem"])
        .arg("-L").arg(device::text(&sdk.join("share"))?)
        .arg("-kernel").arg(device::text(&kernel)?)
        .arg("-initrd").arg(device::text(&initrd)?)
        .args(["-append", "console=ttyS0 printk.devkmsg=on audit=1 panic=-1 8250.nr_uarts=1 loop.max_part=7 init=/init bootconfig"])
        .args(["-smp", &plan.resources.cpus.to_string(), "-m", &plan.resources.memory_mb.to_string()])
        .args(["-blockdev", &block("os", &disk, true)?, "-device", "virtio-blk-pci,drive=os,addr=0x5"])
        .args(["-blockdev", &block("data", &data, false)?, "-device", "virtio-blk-pci,drive=data,addr=0x6"])
        .args(["-netdev", &format!("user,id=network,hostfwd=tcp:127.0.0.1:{}-10.0.2.15:5555", plan.ports.adb_endpoint())])
        .args(["-device", "virtio-net-pci,netdev=network,romfile=,addr=0x7"])
        .args(["-device", "virtio-gpu-pci,xres=720,yres=1440,addr=0x8"])
        .args(["-device", "virtio-serial-pci,max_ports=16,addr=0x9"]);
    for port in 0..16 {
        let backend = if port == 1 {
            format!("socket,id=hvc1,host=127.0.0.1,port={console},server=off")
        } else {
            format!("null,id=hvc{port}")
        };
        command.args([
            "-chardev",
            &backend,
            "-device",
            &format!("virtconsole,chardev=hvc{port}"),
        ]);
    }
    command
        .args([
            "-device",
            "virtio-rng-pci",
            "-audiodev",
            "none,id=audio",
            "-device",
            "virtio-sound-pci,audiodev=audio",
        ])
        .args([
            "-qmp",
            &format!("tcp:127.0.0.1:{},server=on,wait=off", plan.ports.console),
        ])
        .args(["-serial", "file:kernel.log"]);
    Ok(command)
}

pub(super) async fn provision_adb(plan: &Plan, listener: TcpListener) -> Result<()> {
    let public_key = filesystem::bounded_file_with_limit(&plan.adb_public_key(), 4096)?;
    let key = std::str::from_utf8(&public_key)
        .ok()
        .and_then(|value| value.split_ascii_whitespace().next())
        .filter(|value| {
            value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b))
        })
        .ok_or(Error::Invalid("android_adb_public_key"))?;
    let (stream, _) = listener.accept().await?;
    drop(listener);
    eprintln!("android_console_transport");
    let mut console = BufReader::new(stream);
    let mut prompt = [0; 4];
    let mut recorded = 0;
    loop {
        let bytes = console.fill_buf().await?;
        if bytes.is_empty() {
            return Err(Error::Invalid("android_console_unavailable"));
        }
        let mut consumed = bytes.len();
        let found = bytes.iter().position(|byte| {
            prompt.rotate_left(1);
            prompt[3] = *byte;
            &prompt == b":/ $"
        });
        if let Some(index) = found {
            consumed = index + 1;
        }
        let count = consumed.min(2048 - recorded);
        if count > 0 {
            eprint!("{}", String::from_utf8_lossy(&bytes[..count]));
            recorded += count;
        }
        console.consume(consumed);
        if found.is_some() {
            break;
        }
    }
    eprintln!("\nandroid_console_connected");
    console.get_mut().write_all(format!(
        "su 0 sh -c 'ip link set eth0 up && ip address replace 10.0.2.15/24 dev eth0 || exit; \
         while [ ! -d /data/misc ]; do sleep 1; done; \
         d=/data/misc/adb; p=\"$d/adb_keys\"; \
         if [ ! -d \"$d\" ]; then mkdir \"$d\" && chown system:shell \"$d\" && chmod 2750 \"$d\" && restorecon \"$d\" || exit; fi; \
         if ! grep -qxF \"{key} aimanity\" \"$p\" 2>/dev/null; then \
         echo \"{key} aimanity\" >> \"$p\" && chown system:shell \"$p\" && chmod 640 \"$p\" && restorecon \"$p\" || exit; fi; \
         echo AIMANITY_ADB_READY'\r"
    ).as_bytes()).await?;
    let mut line = String::new();
    while console.read_line(&mut line).await? > 0 {
        if line.trim() == "AIMANITY_ADB_READY" {
            return Ok(());
        }
        line.clear();
    }
    Err(Error::Invalid("android_console_provision"))
}

pub(super) async fn stage_audio(plan: &Plan) -> Result<()> {
    let source = plan.sdk.join("cuttlefish/aimanity-audio.jar");
    filesystem::regular(&source)?;
    let adb = || -> Result<Command> {
        let mut command = plan.command("platform-tools", "adb")?;
        command.args(["-P", &plan.ports.adb.to_string(), "-s", &plan.serial()]);
        command.stdout(Stdio::null()).kill_on_drop(true);
        Ok(command)
    };
    let destination = "/data/local/tmp/aimanity-audio.jar";
    let setup = async {
        if !adb()?
            .arg("push")
            .arg(source)
            .arg(destination)
            .status()
            .await?
            .success()
            || !adb()?
                .args(["shell", "chmod", "0444", destination])
                .status()
                .await?
                .success()
        {
            return Err(Error::Invalid("android_audio_stage"));
        }
        Ok(())
    };
    tokio::time::timeout(Duration::from_secs(10), setup)
        .await
        .map_err(|_| Error::Invalid("android_audio_stage"))?
}

async fn quit(port: u16) -> Result<()> {
    let mut qmp = BufReader::new(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await?);
    let mut line = String::new();
    qmp.read_line(&mut line).await?;
    if !serde_json::from_str::<Value>(&line).is_ok_and(|value| value.get("QMP").is_some()) {
        return Err(Error::Invalid("android_emulator_stop_unknown"));
    }
    qmp.get_mut()
        .write_all(b"{\"execute\":\"qmp_capabilities\"}\r\n{\"execute\":\"quit\"}\r\n")
        .await?;
    Ok(())
}

pub(super) async fn shutdown(plan: &Plan, child: &mut Child) -> Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        quit(plan.ports.console).await?;
        child.wait().await?;
        Ok::<_, Error>(())
    })
    .await;
    if !matches!(result, Ok(Ok(()))) {
        child.kill().await?;
        child.wait().await?;
    }
    Ok(())
}
