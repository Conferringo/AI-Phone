//! Native local deployment lifecycle. Remote workbenches never receive this authority.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{ChildStdin, Command},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::Manager;
mod checkpoint;
mod download;
mod process;
mod toolchain;
use download::{hash, mise as download_mise};
use process::execute;

const INTEGRITY: &str = include_str!("../gen/host/integrity.json");
const POLICY: &str = include_str!("../../../../config/host-bootstrap.json");

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Phone {
    cpus: u8,
    memory_mb: u32,
    storage_gb: u8,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Desktop {
    cpus: u8,
    memory_mb: u32,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub locale: String,
    phone: Phone,
    desktop: Desktop,
}
impl Request {
    fn validate(&self) -> Result<(), String> {
        super::checked_locale(Some(self.locale.clone()))?;
        if ![1, 2, 4].contains(&self.phone.cpus)
            || ![1, 2, 4].contains(&self.desktop.cpus)
            || ![1536, 2048, 3072, 4096].contains(&self.phone.memory_mb)
            || ![2048, 2560, 3072, 4096].contains(&self.desktop.memory_mb)
            || ![16, 24, 32].contains(&self.phone.storage_gb)
        {
            return Err("invalid_device_resources".into());
        }
        Ok(())
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Progress {
    pub phase: String,
    pub state: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub license: String,
}
#[derive(Serialize)]
pub struct Capability {
    pub platform: String,
    pub arch: String,
    pub accelerator: String,
    pub implemented: bool,
}
#[derive(Serialize)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub value: u64,
    pub required: u64,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub host: Capability,
    pub checks: Vec<Check>,
    pub progress: Progress,
    pub data_directory: String,
    pub previous_request: Option<Request>,
}

pub fn capability(os: &str, arch: &str) -> Capability {
    Capability {
        platform: os.into(),
        arch: arch.into(),
        accelerator: match os {
            "macos" => "HVF / Virtualization.framework",
            "windows" => "WHPX / WSL2",
            "linux" => "KVM",
            "android" => "privileged AVF / KVM; TCG experimental",
            _ => "unsupported",
        }
        .into(),
        implemented: os == "macos" && arch == "aarch64",
    }
}

#[derive(Default)]
pub struct ManagerState {
    running: AtomicBool,
    cancelled: AtomicBool,
    progress: Mutex<Progress>,
    input: Mutex<Option<ChildStdin>>,
}
#[derive(Default)]
pub struct Bootstrap(pub Arc<ManagerState>);

impl ManagerState {
    fn publish(&self, folder: &Path, next: Progress) -> Result<(), String> {
        checkpoint::save(folder, &next)?;
        *self.progress.lock().map_err(|_| "installer_state")? = next;
        Ok(())
    }
    fn set(&self, folder: &Path, phase: &str, state: &str, error: &str) -> Result<(), String> {
        self.publish(
            folder,
            Progress {
                phase: phase.into(),
                state: state.into(),
                error: error.into(),
                license: String::new(),
            },
        )
    }
    fn recover(&self, folder: &Path) -> Result<(), String> {
        let mut progress = self.progress.lock().map_err(|_| "installer_state")?;
        if progress.state.is_empty() && !self.running.load(Ordering::SeqCst) {
            *progress = checkpoint::restore(folder)?;
        }
        Ok(())
    }
    fn failed(&self, folder: &Path, error: &str) {
        let state = if error == "installation_cancelled" {
            "cancelled"
        } else {
            "failed"
        };
        if self.set(folder, "failed", state, error).is_err()
            && let Ok(mut progress) = self.progress.lock()
        {
            *progress = Progress {
                phase: "failed".into(),
                state: state.into(),
                error: "checkpoint_write_failed".into(),
                license: String::new(),
            };
        }
    }
    fn check_cancelled(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::SeqCst) {
            Err("installation_cancelled".into())
        } else {
            Ok(())
        }
    }
    fn process_error(&self) -> String {
        self.progress
            .lock()
            .ok()
            .filter(|progress| progress.state == "failed" && !progress.error.is_empty())
            .map(|progress| progress.error.clone())
            .unwrap_or_else(|| "installation_failed".into())
    }
}

fn home(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|p| p.join("host"))
        .map_err(|e| e.to_string())
}

fn verify_resources(folder: &Path) -> Result<(), String> {
    let expected: std::collections::BTreeMap<String, String> =
        serde_json::from_str(INTEGRITY).map_err(|e| e.to_string())?;
    for name in ["source.tar.gz", "entry.py"] {
        if hash(&folder.join(name))? != *expected.get(name).ok_or("invalid_payload")? {
            return Err("invalid_payload".into());
        }
    }
    Ok(())
}

fn checks(path: &Path) -> Vec<Check> {
    #[cfg(target_os = "macos")]
    {
        let ram = Command::new("/usr/sbin/sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0)
            / 1024_u64.pow(3);
        let mut existing = path;
        while !existing.exists() {
            if let Some(parent) = existing.parent() {
                existing = parent;
            } else {
                break;
            }
        }
        let disk = Command::new("/bin/df")
            .arg("-k")
            .arg(existing)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                s.lines()
                    .nth(1)
                    .and_then(|l| l.split_whitespace().nth(3))
                    .and_then(|v| v.parse::<u64>().ok())
            })
            .unwrap_or(0)
            / 1024_u64.pow(2);
        let policy: serde_json::Value = serde_json::from_str(POLICY).expect("bundled host policy");
        let disk_required = policy[if path.join("user/data/deployment/profile.json").is_file()
            || checkpoint::admitted(path)
        {
            "minimum_resume_free_gb"
        } else {
            "minimum_free_gb"
        }]
        .as_u64()
        .unwrap();
        vec![
            Check {
                name: "memory".into(),
                passed: ram >= policy["minimum_ram_gb"].as_u64().unwrap(),
                value: ram,
                required: policy["minimum_ram_gb"].as_u64().unwrap(),
            },
            Check {
                name: "storage".into(),
                passed: disk >= disk_required,
                value: disk,
                required: disk_required,
            },
        ]
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Vec::new()
    }
}

pub fn snapshot(app: &tauri::AppHandle) -> Result<Snapshot, String> {
    let manager = app.state::<Bootstrap>();
    let path = home(app)?;
    manager.0.recover(&path)?;
    Ok(Snapshot {
        host: capability(std::env::consts::OS, std::env::consts::ARCH),
        checks: checks(&path),
        progress: manager
            .0
            .progress
            .lock()
            .map_err(|_| "installer_state")?
            .clone(),
        data_directory: path.join("user").display().to_string(),
        previous_request: fs::read(path.join("request.json"))
            .ok()
            .and_then(|v| serde_json::from_slice::<Request>(&v).ok())
            .filter(|r| r.validate().is_ok()),
    })
}

fn prepare_folder(folder: &Path) -> Result<(), String> {
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(folder, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn prepare_runtime(manager: &ManagerState, folder: &Path) -> Result<PathBuf, String> {
    let layout = toolchain::Toolchain::new(folder);
    layout.prepare()?;
    let mise = layout.binary;
    let policy: serde_json::Value = serde_json::from_str(POLICY).map_err(|e| e.to_string())?;
    manager.set(folder, "runtime", "running", "")?;
    tauri::async_runtime::block_on(download_mise(
        manager,
        &mise,
        &policy["mise"]["macos-arm64"],
    ))?;
    Ok(mise)
}

fn run(
    app: &tauri::AppHandle,
    manager: &Arc<ManagerState>,
    request: Request,
) -> Result<(), String> {
    let folder = home(app)?;
    prepare_folder(&folder)?;
    let resource = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("host");
    verify_resources(&resource)?;
    let request_path = folder.join("request.json");
    fs::write(
        &request_path,
        serde_json::to_vec(&request).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mise = prepare_runtime(manager, &folder)?;
    let layout = toolchain::Toolchain::new(&folder);
    let mut command = Command::new(&mise);
    command
        .current_dir(&folder)
        .args(["exec", "-y", "python@3.13.5", "--", "python"])
        .arg(resource.join("entry.py"))
        .arg("--payload")
        .arg(resource.join("source.tar.gz"))
        .arg("--home")
        .arg(&folder)
        .arg("--request")
        .arg(request_path)
        .arg("--mise")
        .arg(&mise);
    layout.apply(&mut command);
    command
        .env("MISE_YES", "1")
        .env("MISE_SELF_UPDATE_AVAILABLE", "false")
        .env("MISE_PARANOID", "1");
    execute(manager, command, &folder)
}

pub fn start(app: tauri::AppHandle, request: Request) -> Result<(), String> {
    request.validate()?;
    if !capability(std::env::consts::OS, std::env::consts::ARCH).implemented {
        return Err("host_backend_pending".into());
    }
    let path = home(&app)?;
    if checks(&path).iter().any(|c| !c.passed) {
        return Err("host_resources".into());
    }
    prepare_folder(&path)?;
    let manager = app.state::<Bootstrap>().0.clone();
    if manager.running.swap(true, Ordering::SeqCst) {
        return Err("installation_running".into());
    }
    manager.cancelled.store(false, Ordering::SeqCst);
    if let Err(error) = manager.set(&path, "checking", "running", "") {
        manager.running.store(false, Ordering::SeqCst);
        return Err(error);
    }
    std::thread::spawn(move || {
        if let Err(error) = run(&app, &manager, request) {
            manager.failed(&path, &error);
        }
        manager.running.store(false, Ordering::SeqCst);
    });
    Ok(())
}

// Only a completed checkpoint for this exact payload may resume automatically.
pub fn resume_installed(app: &tauri::AppHandle) -> Result<bool, String> {
    let folder = home(app)?;
    if !checkpoint::completed(&folder)? {
        return Ok(false);
    }
    let request: Request = serde_json::from_slice(
        &fs::read(folder.join("request.json")).map_err(|_| "invalid_install_checkpoint")?,
    )
    .map_err(|_| "invalid_install_checkpoint")?;
    if let Err(error) = start(app.clone(), request) {
        app.state::<Bootstrap>().0.failed(&folder, &error);
        return Err(error);
    }
    Ok(true)
}

pub fn ready(app: &tauri::AppHandle) -> bool {
    app.state::<Bootstrap>()
        .0
        .progress
        .lock()
        .is_ok_and(|progress| progress.state == "ready")
}

pub fn cancel(app: &tauri::AppHandle) {
    app.state::<Bootstrap>()
        .0
        .cancelled
        .store(true, Ordering::SeqCst);
}
pub fn is_running(app: &tauri::AppHandle) -> bool {
    app.state::<Bootstrap>().0.running.load(Ordering::SeqCst)
}
pub fn wait_until_idle(app: &tauri::AppHandle) {
    while is_running(app) {
        std::thread::sleep(Duration::from_millis(150));
    }
}

pub fn accept_license(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<Bootstrap>();
    let mut progress = state.0.progress.lock().map_err(|_| "installer_state")?;
    if progress.state != "license" {
        return Err("no_pending_license".into());
    }
    let mut input = state.0.input.lock().map_err(|_| "installer_state")?;
    input
        .as_mut()
        .ok_or("no_pending_license")?
        .write_all(b"{\"accept\":true}\n")
        .and_then(|_| input.as_mut().unwrap().flush())
        .map_err(|_| "license_response_failed")?;
    progress.state = "running".into();
    progress.license.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::Read;
    #[test]
    #[cfg(unix)]
    fn installer_failure_reason_survives_the_process_boundary() {
        let folder = std::env::temp_dir().join(format!("ai-phone-reason-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let manager = Arc::new(ManagerState::default());
        let mut command = Command::new("/bin/sh");
        command.args(["-c","printf '%s\\n' 'AI_PHONE_SETUP {\"phase\":\"failed\",\"state\":\"failed\",\"error\":\"foreign_installation\"}'; exit 1"]);
        let result = execute(&manager, command, &folder);
        fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            result,
            Err("foreign_installation".into()),
            "the UI needs the actual recovery reason"
        );
    }
    #[test]
    fn stopping_a_download_interrupts_both_headers_and_a_stalled_body() {
        for headers in [false, true] {
            let folder = std::env::temp_dir().join(format!(
                "ai-phone-download-{}-{headers}",
                std::process::id()
            ));
            fs::create_dir_all(&folder).unwrap();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/fixture", listener.local_addr().unwrap());
            let (accepted, waiting) = std::sync::mpsc::channel();
            let (release, released) = std::sync::mpsc::channel();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = [0; 4096];
                let _ = stream.read(&mut bytes);
                if headers {
                    stream
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\n")
                        .unwrap();
                }
                accepted.send(()).unwrap();
                let _ = released.recv_timeout(Duration::from_secs(5));
                let _ = stream.write_all(if headers {
                    b"abc"
                } else {
                    b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc"
                });
            });
            let manager = Arc::new(ManagerState::default());
            let state = manager.clone();
            let destination = folder.join("mise");
            let (finished, result) = std::sync::mpsc::channel();
            let download = std::thread::spawn(move || {
                let record =
                    serde_json::json!({"url":url,"sha256":format!("{:x}",Sha256::digest(b"abc"))});
                let outcome =
                    tauri::async_runtime::block_on(download_mise(&state, &destination, &record));
                finished.send(outcome).unwrap();
            });
            waiting.recv_timeout(Duration::from_secs(3)).unwrap();
            manager.cancelled.store(true, Ordering::SeqCst);
            let outcome = result.recv_timeout(Duration::from_secs(1));
            let _ = release.send(());
            server.join().unwrap();
            download.join().unwrap();
            fs::remove_dir_all(folder).unwrap();
            assert_eq!(
                outcome.unwrap(),
                Err("installation_cancelled".into()),
                "Stop must not wait for a stalled network peer"
            );
        }
    }
    #[test]
    #[cfg(unix)]
    fn installer_events_are_durable_before_the_client_can_restart() {
        let folder = std::env::temp_dir().join(format!("ai-phone-progress-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let manager = Arc::new(ManagerState::default());
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "printf '%s\\n' 'AI_PHONE_SETUP {\"phase\":\"dependencies\",\"state\":\"completed\"}'",
        ]);
        execute(&manager, command, &folder).unwrap();
        let checkpoint = folder.join("deployment.json");
        assert!(
            checkpoint.is_file(),
            "closing the app must not erase installation progress"
        );
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(&checkpoint).unwrap()).unwrap();
        assert_eq!(saved["progress"]["phase"], "dependencies");
        assert_eq!(saved["progress"]["state"], "running");
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn ready_event_from_a_failed_installer_cannot_authorize_auto_resume() {
        let folder =
            std::env::temp_dir().join(format!("ai-phone-failed-ready-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let manager = Arc::new(ManagerState::default());
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "printf '%s\\n' 'AI_PHONE_SETUP {\"phase\":\"ready\",\"state\":\"ready\"}'; exit 1",
        ]);
        assert!(execute(&manager, command, &folder).is_err());
        assert!(
            !checkpoint::completed(&folder).unwrap(),
            "the process must succeed before installation is complete"
        );
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn arbitrary_os_images_and_unimplemented_hosts_are_not_claimed_available() {
        assert!(capability("macos", "aarch64").implemented);
        for (os, arch) in [
            ("macos", "x86_64"),
            ("windows", "x86_64"),
            ("linux", "x86_64"),
            ("android", "aarch64"),
        ] {
            assert!(!capability(os, arch).implemented);
        }
    }
    #[test]
    fn deployment_contract_rejects_unknown_keys_and_unbounded_hardware() {
        let value = r#"{"locale":"en","phone":{"cpus":2,"memory_mb":2048,"storage_gb":16},"desktop":{"cpus":2,"memory_mb":2560}}"#;
        assert!(
            serde_json::from_str::<Request>(value)
                .unwrap()
                .validate()
                .is_ok()
        );
        for invalid in [
            value.replace("2048", "9999999"),
            value.replace("\"en\"", "\"../../en\""),
            value.replace("\"cpus\":2", "\"cpus\":0"),
        ] {
            assert!(
                serde_json::from_str::<Request>(&invalid)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        assert!(
            serde_json::from_str::<Request>(&value.replace("\"locale\"", "\"command\"")).is_err()
        );
    }

    #[test]
    #[cfg(unix)]
    fn cancellation_stops_the_installer_process_group_before_returning() {
        let folder = std::env::temp_dir().join(format!(
            "ai-phone-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&folder).unwrap();
        let manager = Arc::new(ManagerState::default());
        let state = manager.clone();
        let mut command = Command::new("/bin/sh");
        command.current_dir(&folder).args([
            "-c",
            "(sleep 3; touch unsafe-replayed) & echo started; wait",
        ]);
        let reader_log = folder.join("install.log");
        let worker_folder = folder.clone();
        let worker = std::thread::spawn(move || execute(&state, command, &worker_folder));
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !fs::read_to_string(&reader_log)
            .unwrap_or_default()
            .contains("started")
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        manager.cancelled.store(true, Ordering::SeqCst);
        assert_eq!(
            worker.join().unwrap().unwrap_err(),
            "installation_cancelled"
        );
        assert!(!folder.join("unsafe-replayed").exists());
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn retry_budget_is_not_the_initial_download_reserve() {
        let folder = std::env::temp_dir().join(format!("ai-phone-space-{}", std::process::id()));
        fs::create_dir(&folder).unwrap();
        let initial = checks(&folder)
            .into_iter()
            .find(|c| c.name == "storage")
            .unwrap()
            .required;
        fs::create_dir_all(folder.join("user/data/deployment")).unwrap();
        fs::write(folder.join("user/data/deployment/profile.json"), b"{}").unwrap();
        let resumed = checks(&folder)
            .into_iter()
            .find(|c| c.name == "storage")
            .unwrap()
            .required;
        assert!(resumed < initial);
        fs::remove_file(folder.join("user/data/deployment/profile.json")).unwrap();
        checkpoint::save(
            &folder,
            &Progress {
                phase: "runtime".into(),
                state: "running".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            checks(&folder)
                .into_iter()
                .find(|check| check.name == "storage")
                .unwrap()
                .required,
            resumed,
            "an admitted install interrupted before profile creation must retain its retry reserve"
        );
        fs::remove_dir_all(folder).unwrap();
    }
}
