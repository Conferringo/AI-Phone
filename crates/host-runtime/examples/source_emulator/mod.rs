//! Independent upstream SDK oracle, not another product session controller.
use ai_phone_adb::{Client, Limits};
use image::GenericImageView;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    time::{Instant, sleep},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const ADB_PORT: u16 = 15049;
const SERIAL: &str = "emulator-5586";
#[derive(Deserialize)]
struct Pin {
    size_bytes: u64,
    sha256: String,
}
fn admit(path: &Path, pin: &Pin) -> Result<()> {
    let mut file = File::open(path)?;
    if file.metadata()?.len() != pin.size_bytes {
        return Err("archive size differs".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if format!("{:x}", hash.finalize()) != pin.sha256 {
        return Err("archive digest differs".into());
    }
    Ok(())
}
fn pixels(bytes: &[u8]) -> Result<(u32, u32)> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)?;
    let (width, height) = image.dimensions();
    if width < 64 || height < 64 {
        return Err("device frame too small".into());
    }
    let rgb = image.into_rgb8();
    let first = rgb.get_pixel(0, 0);
    if rgb.pixels().all(|pixel| pixel == first) {
        return Err("device frame is blank".into());
    }
    Ok((width, height))
}
fn property(path: &Path, key: &str, expected: &str) -> Result<()> {
    let text = fs::read_to_string(path)?;
    if text
        .lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim())
        != Some(expected)
    {
        return Err("SDK property differs".into());
    }
    Ok(())
}
async fn command(program: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let output = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output()).await??;
    if !output.status.success() {
        return Err(format!(
            "qualification command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output.stdout)
}
struct Owned {
    child: Child,
    pid: rustix::process::Pid,
    log: PathBuf,
    closed: bool,
}
impl Owned {
    fn spawn(mut command: Command, log: &Path) -> Result<Self> {
        let file = File::create(log)?;
        let child = command
            .process_group(0)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(file)
            .spawn()?;
        let pid = rustix::process::Pid::from_raw(child.id().ok_or("child missing")? as i32)
            .ok_or("child pid")?;
        Ok(Self {
            child,
            pid,
            log: log.into(),
            closed: false,
        })
    }
    fn alive(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            let mut file = File::open(&self.log)?;
            file.seek(SeekFrom::Start(file.metadata()?.len().saturating_sub(4096)))?;
            let mut tail = Vec::new();
            file.take(4096).read_to_end(&mut tail)?;
            let tail = String::from_utf8_lossy(&tail)
                .replace(self.log.parent().unwrap().to_str().unwrap(), "<evidence>");
            return Err(format!(
                "owned SDK process {} exited {status}: {tail}",
                self.log.file_name().unwrap().to_string_lossy()
            )
            .into());
        }
        Ok(())
    }
    async fn wait_exit(&mut self) -> Result<()> {
        let status = tokio::time::timeout(Duration::from_secs(15), self.child.wait()).await??;
        if !status.success() {
            return Err("emulator shutdown failed".into());
        }
        Ok(())
    }
    async fn stop(mut self) -> Result<()> {
        match rustix::process::kill_process_group(self.pid, rustix::process::Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(e) => return Err(e.into()),
        }
        self.child.wait().await?;
        self.closed = true;
        Ok(())
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.closed {
            let _ = rustix::process::kill_process_group(self.pid, rustix::process::Signal::KILL);
        }
    }
}
struct Context {
    sdk: PathBuf,
    image: PathBuf,
    root: PathBuf,
}
impl Context {
    fn sdk_command(&self, role: &str, program: &str) -> Command {
        let mut command = Command::new(self.sdk.join(role).join(program));
        command
            .current_dir(&self.root)
            .env("ANDROID_HOME", &self.sdk)
            .env("ANDROID_SDK_ROOT", &self.sdk)
            .env("ANDROID_AVD_HOME", self.root.join("avd"))
            .env("ANDROID_USER_HOME", self.root.join("android"))
            .env("ANDROID_EMULATOR_HOME", self.root.join("android"))
            .env("HOME", self.root.join("profile"))
            .env("ANDROID_ADB_SERVER_PORT", ADB_PORT.to_string())
            .env("ADB_SERVER_SOCKET", format!("tcp:127.0.0.1:{ADB_PORT}"))
            .env("ADB_MDNS_AUTO_CONNECT", "0")
            .env_remove("ADB_VENDOR_KEYS")
            .env_remove("ANDROID_SERIAL");
        for name in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            command.env_remove(name);
        }
        command
    }
    fn avd(&self) -> Result<()> {
        fs::create_dir_all(self.sdk.join("platforms"))?;
        fs::create_dir_all(self.root.join("profile"))?;
        let folder = self.root.join("avd/source-sdk.avd");
        fs::create_dir_all(&folder)?;
        let mut values = format!(
            "avd.ini.encoding=UTF-8\nabi.type=x86_64\nhw.cpu.arch=x86_64\nimage.sysdir.1={}/\ntag.id=default\nhw.cpu.ncore=2\nhw.ramSize=2048\ndisk.dataPartition.size=16G\nhw.lcd.width=720\nhw.lcd.height=1440\nhw.lcd.density=320\nhw.keyboard=yes\nhw.gpu.enabled=yes\nhw.gpu.mode=swiftshader_indirect\nshowDeviceFrame=no\nfastboot.forceColdBoot=yes\nfastboot.forceFastBoot=no\n",
            self.image.display()
        );
        values.push_str("hw.camera.back=none\nhw.camera.front=none\n");
        fs::write(folder.join("config.ini"), values)?;
        fs::write(
            self.root.join("avd/source-sdk.ini"),
            format!(
                "avd.ini.encoding=UTF-8\npath={}\ntarget=android-35\n",
                folder.display()
            ),
        )?;
        Ok(())
    }
    fn adb(&self) -> Result<Owned> {
        let mut command = self.sdk_command("platform-tools", "adb");
        command.args([
            "--one-device",
            SERIAL,
            "-L",
            &format!("tcp:{ADB_PORT}"),
            "server",
            "nodaemon",
        ]);
        Owned::spawn(command, &self.root.join("adb.log"))
    }
    fn emulator(&self) -> Result<Owned> {
        let mut command = self.sdk_command("emulator", "emulator");
        command.args([
            "-avd",
            "source-sdk",
            "-port",
            "5586",
            "-no-window",
            "-audio",
            "none",
            "-no-boot-anim",
            "-no-snapshot",
            "-writable-system",
            "-no-metrics",
            "-gpu",
            "swiftshader_indirect",
            "-memory",
            "2048",
            "-cores",
            "2",
            "-accel",
            "on",
        ]);
        Owned::spawn(command, &self.root.join("emulator.log"))
    }
    async fn stop_emulator(&self, mut emulator: Owned) -> Result<()> {
        let mut command = self.sdk_command("platform-tools", "adb");
        command.args(["-P", &ADB_PORT.to_string(), "-s", SERIAL, "emu", "kill"]);
        let output = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output();
        if !tokio::time::timeout(Duration::from_secs(15), output)
            .await??
            .status
            .success()
        {
            return Err("emulator console stop failed".into());
        }
        emulator.wait_exit().await?;
        emulator.stop().await
    }
}
async fn shell(client: &Client, text: &str) -> Result<String> {
    let value = client
        .shell(
            text,
            Limits {
                stdout: 65536,
                stderr: 4096,
            },
            Duration::from_secs(10),
        )
        .await?;
    if value.code != 0 {
        return Err("guest command failed".into());
    }
    Ok(String::from_utf8(value.stdout)?.trim().into())
}
async fn boot(client: &Client, adb: &mut Owned, emulator: &mut Owned) -> Result<()> {
    let startup = async {
        let mut root = false;
        loop {
            adb.alive()?;
            emulator.alive()?;
            if shell(client, "getprop sys.boot_completed")
                .await
                .is_ok_and(|v| v == "1")
            {
                let uid = shell(client, "id -u")
                    .await
                    .map_err(|e| format!("boot uid observation: {e}"))?;
                if uid == "0" {
                    return Ok::<(), Box<dyn std::error::Error>>(());
                }
                if !root {
                    root = true;
                    client
                        .raw("root:", 4096, Duration::from_secs(10))
                        .await
                        .map_err(|e| format!("root transition request: {e}"))?;
                }
            }
            sleep(Duration::from_millis(200)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(120), startup)
        .await
        .map_err(|_| "root boot deadline exceeded")?
}

async fn frame(client: &Client, path: &Path) -> Result<(u32, u32)> {
    let bytes = client
        .raw(
            "exec:screencap -p",
            16 * 1024 * 1024,
            Duration::from_secs(15),
        )
        .await?;
    let size = pixels(&bytes)?;
    fs::write(path, bytes)?;
    Ok(size)
}
async fn focus(client: &Client, settings: bool) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let value = shell(client, "dumpsys window | grep mCurrentFocus").await?;
        if !value.is_empty() && value.contains("com.android.settings") == settings {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err("window focus did not change".into());
        }
        sleep(Duration::from_millis(50)).await;
    }
}
async fn show_settings(client: &Client) -> Result<()> {
    shell(client, "input keyevent 224; wm dismiss-keyguard").await?;
    shell(client, "am start -W -a android.settings.SETTINGS").await?;
    focus(client, true).await
}
fn checkpoint(root: &Path, receipt: &mut Value, stage: &str) -> Result<()> {
    receipt["stage"] = stage.into();
    fs::write(
        root.join("receipt.json"),
        serde_json::to_vec_pretty(receipt)?,
    )?;
    eprintln!("SOURCE_SDK_QUALIFY {stage}");
    Ok(())
}
pub(super) fn guard(confirmed: bool, architecture: &str) -> Result<()> {
    if !confirmed || architecture != "x86_64" {
        return Err("requires explicit disposable Linux x64 qualification".into());
    }
    Ok(())
}
pub(super) async fn run(args: Vec<OsString>) -> Result<()> {
    let [project, inputs, evidence] = args.as_slice() else {
        return Err(
            "usage: qualify_source_emulator <project> <archives> <new evidence directory>".into(),
        );
    };
    let project = Path::new(project);
    let inputs = Path::new(inputs);
    let root = Path::new(evidence);
    fs::create_dir(root)?;
    let policy: Value = serde_json::from_slice(&fs::read(
        project.join("config/emulator-source-qualification.json"),
    )?)?;
    let factory: Value =
        serde_json::from_slice(&fs::read(project.join("config/android-factory.json"))?)?;
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| "qualification nonce unavailable")?;
    let nonce = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut receipt = json!({"format":1,"host":"linux-x64","nonce":nonce,"qualification_source_commit":std::env::var("GITHUB_SHA").ok(),"qualification_run_id":std::env::var("GITHUB_RUN_ID").ok(),"source_commit":policy["source_commit"],"build_id":policy["build_id"],"sdk_sha256":policy["sdk"]["sha256"],"api":35,"abi":"x86_64","status":"failed","stage":"admission","checks":{"archives":false,"versions":false,"root_boot":false,"decoded_frame":false,"key_input":false,"restart_data":false,"cleanup":false},"source_dependency_closure":"unverified","redistribution":"review-pending","binary_publication":false,"user_state_bundled":false});
    checkpoint(root, &mut receipt, "admission")?;
    let mut adb = None;
    let mut emulator = None;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let validation = async {
        for (name, pin) in [
            ("source-sdk.zip", &policy["sdk"]),
            (
                "platform-tools.zip",
                &factory["hosts"]["linux-x64"]["archives"]["platform-tools"],
            ),
            ("system-image.zip", &factory["images"]["x86_64"]),
        ] {
            admit(&inputs.join(name), &serde_json::from_value(pin.clone())?)?;
        }
        receipt["checks"]["archives"] = true.into();
        checkpoint(root, &mut receipt, "extract")?;
        let sdk = root.join("sdk");
        let image = root.join("image");
        fs::create_dir(&sdk)?;
        fs::create_dir(&image)?;
        for (name, dest) in [
            ("source-sdk.zip", &sdk),
            ("platform-tools.zip", &sdk),
            ("system-image.zip", &image),
        ] {
            command(
                Path::new("unzip"),
                &[
                    "-q",
                    inputs.join(name).to_str().ok_or("archive path")?,
                    "-d",
                    dest.to_str().ok_or("SDK path")?,
                ],
            )
            .await?;
        }
        let expected = policy["sdk"]["version"].as_str().ok_or("source version")?;
        property(
            &sdk.join("emulator/source.properties"),
            "Pkg.Revision",
            expected,
        )?;
        property(
            &sdk.join("platform-tools/source.properties"),
            "Pkg.Revision",
            factory["hosts"]["linux-x64"]["archives"]["platform-tools"]["version"]
                .as_str()
                .ok_or("ADB version")?,
        )?;
        let context = Context {
            sdk,
            image: image.join("x86_64"),
            root: root.into(),
        };
        let version = command(&context.sdk.join("emulator/emulator"), &["-version"]).await?;
        if !String::from_utf8_lossy(&version).contains(expected) {
            return Err("source emulator program version differs".into());
        }
        receipt["checks"]["versions"] = true.into();
        context.avd()?;
        checkpoint(root, &mut receipt, "boot")?;
        adb = Some(context.adb()?);
        emulator = Some(context.emulator()?);
        let client = Client::new(ADB_PORT, SERIAL.into())?;
        boot(&client, adb.as_mut().unwrap(), emulator.as_mut().unwrap()).await?;
        let identity = shell(
            &client,
            "getprop ro.build.version.sdk; getprop ro.product.cpu.abi",
        )
        .await
        .map_err(|e| format!("boot API/ABI observation: {e}"))?;
        if identity != "35\nx86_64" {
            return Err("guest API or ABI differs".into());
        }
        receipt["checks"]["root_boot"] = true.into();
        checkpoint(root, &mut receipt, "input")?;
        show_settings(&client).await?;
        let (width, height) = frame(&client, &root.join("frame.png")).await?;
        receipt["frame"] = json!({"width":width,"height":height});
        receipt["checks"]["decoded_frame"] = true.into();
        shell(&client, "input keyevent 3").await?;
        focus(&client, false).await?;
        receipt["checks"]["key_input"] = true.into();
        shell(
            &client,
            &format!("printf '%s' '{nonce}' > /data/local/tmp/aiphone-source-qualification"),
        )
        .await?;
        checkpoint(root, &mut receipt, "restart")?;
        context.stop_emulator(emulator.take().unwrap()).await?;
        emulator = Some(context.emulator()?);
        boot(&client, adb.as_mut().unwrap(), emulator.as_mut().unwrap()).await?;
        if shell(&client, "cat /data/local/tmp/aiphone-source-qualification").await? != nonce {
            return Err("owned guest data changed across restart".into());
        }
        show_settings(&client).await?;
        frame(&client, &root.join("restarted-frame.png")).await?;
        receipt["checks"]["restart_data"] = true.into();
        context.stop_emulator(emulator.take().unwrap()).await?;
        Ok::<_, Box<dyn std::error::Error>>(())
    };
    let validation = tokio::select! {
        result=validation=>result,
        _=terminate.recv()=>Err("qualification interrupted".into()),
        _=tokio::signal::ctrl_c()=>Err("qualification interrupted".into()),
    };
    let mut cleanup: Result<()> = Ok(());
    for child in [emulator.take(), adb.take()].into_iter().flatten() {
        cleanup = cleanup.and(child.stop().await);
    }
    receipt["checks"]["cleanup"] = cleanup.is_ok().into();
    let result = validation.and(cleanup);
    if result.is_ok() {
        receipt["status"] = "passed".into();
        checkpoint(root, &mut receipt, "complete")?;
    } else {
        fs::write(
            root.join("receipt.json"),
            serde_json::to_vec_pretty(&receipt)?,
        )?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_png_header_is_not_a_rendered_device_frame() {
        let mut fake = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        fake.extend([0, 0, 2, 208, 0, 0, 5, 160]);
        fake.resize(33, 0);
        assert!(pixels(&fake).is_err());
    }
    #[test]
    fn decoded_pixels_are_required_and_a_solid_or_truncated_frame_fails() {
        let mut image = image::RgbImage::from_pixel(64, 128, image::Rgb([10, 20, 30]));
        let encode = |image: &image::RgbImage| {
            let mut out = std::io::Cursor::new(Vec::new());
            image.write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        assert!(pixels(&encode(&image)).is_err());
        image.put_pixel(32, 64, image::Rgb([30, 20, 10]));
        let data = encode(&image);
        assert_eq!(pixels(&data).unwrap(), (64, 128));
        assert!(pixels(&data[..data.len() / 2]).is_err());
    }
    #[test]
    fn archive_admission_never_modifies_inputs_and_rejects_size_or_digest_changes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("input");
        fs::write(&path, b"source").unwrap();
        let mut pin = Pin {
            size_bytes: 6,
            sha256: format!("{:x}", Sha256::digest(b"source")),
        };
        admit(&path, &pin).unwrap();
        pin.size_bytes = 5;
        assert!(admit(&path, &pin).is_err());
        pin.size_bytes = 6;
        pin.sha256 = "0".repeat(64);
        assert!(admit(&path, &pin).is_err());
        assert_eq!(fs::read(path).unwrap(), b"source");
    }
    #[test]
    fn avd_preparation_creates_the_source_sdks_required_runtime_layout() {
        let root = tempfile::tempdir().unwrap();
        let context = Context {
            sdk: root.path().join("sdk"),
            image: root.path().join("image/x86_64"),
            root: root.path().into(),
        };
        context.avd().unwrap();
        assert!(context.sdk.join("platforms").is_dir());
        let config = fs::read_to_string(root.path().join("avd/source-sdk.avd/config.ini")).unwrap();
        assert!(config.contains(&format!("image.sysdir.1={}/", context.image.display())));
        assert!(config.contains("disk.dataPartition.size=16G"));
    }
    #[tokio::test]
    async fn existing_evidence_and_non_disposable_or_wrong_hosts_are_not_adopted() {
        assert!(guard(false, "x86_64").is_err());
        assert!(guard(true, "aarch64").is_err());
        assert!(guard(true, "x86_64").is_ok());
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("keep"), b"owned state").unwrap();
        let error = run(vec![
            root.path().into(),
            root.path().into(),
            root.path().into(),
        ])
        .await
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(root.path().join("keep")).unwrap(), b"owned state");
    }
    #[tokio::test]
    async fn owned_process_cleanup_reaps_the_child_on_explicit_stop_and_drop() {
        let root = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec sleep 60"]);
        let child = Owned::spawn(command, &root.path().join("child.log")).unwrap();
        let pid = child.pid;
        child.stop().await.unwrap();
        assert!(rustix::process::test_kill_process(pid).is_err());
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec sleep 60"]);
        let child = Owned::spawn(command, &root.path().join("dropped.log")).unwrap();
        let pid = child.pid;
        drop(child);
        let until = Instant::now() + Duration::from_secs(1);
        while rustix::process::test_kill_process(pid).is_ok() {
            assert!(Instant::now() < until);
            sleep(Duration::from_millis(5)).await;
        }
    }
    #[tokio::test]
    async fn failed_child_reports_its_status_and_bounded_path_redacted_output() {
        let root = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf '%s' \"$1\" >&2; exit 23", "fixture"])
            .arg(format!(
                "{}\nactual SDK failure in {}",
                "x".repeat(10000),
                root.path().display()
            ));
        let mut child = Owned::spawn(command, &root.path().join("emulator.log")).unwrap();
        let error = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Err(error) = child.alive() {
                    break error.to_string();
                }
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(error.contains("23"));
        assert!(error.contains("actual SDK failure in <evidence>"));
        assert!(!error.contains(root.path().to_str().unwrap()));
        assert!(error.len() < 4300);
        child.stop().await.unwrap();
    }
}
