//! Native local deployment lifecycle. Remote workbenches never receive this authority.
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::process::Command;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::ChildStdin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::Manager;
mod checkpoint;
#[cfg(any(windows, test))]
mod native_phone;
#[cfg(unix)]
mod process;
mod resources;
#[cfg(unix)]
use process::execute;
use resources::{available_bytes, hash};

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

fn platform_key(os: &str, arch: &str) -> String {
    let arch = match arch {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    format!("{os}-{arch}")
}
pub fn capability(os: &str, arch: &str) -> Capability {
    let resources = serde_json::from_str(INTEGRITY).expect("bundled resource inventory");
    capability_from_resources(os, arch, &resources)
}

fn capability_from_resources(
    os: &str,
    arch: &str,
    resources: &std::collections::BTreeMap<String, String>,
) -> Capability {
    let key = platform_key(os, arch);
    let policy: serde_json::Value = serde_json::from_str(POLICY).expect("bundled host policy");
    // Native WSL owns its runtime; it never uses the source installer's SDK/Lima
    // host table. Resource bytes and actual device readiness are checked on start.
    let native_windows = os == "windows"
        && arch == "x86_64"
        && resources.contains_key("android.payload")
        && resources.contains_key("wsl/manifest.json");
    Capability {
        platform: os.into(),
        arch: arch.into(),
        accelerator: match os {
            "macos" => "HVF / Virtualization.framework",
            "windows" => "WHPX / WSL2",
            "linux" => "KVM",
            _ => "unsupported",
        }
        .into(),
        implemented: policy["hosts"][&key].is_object() || native_windows,
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
    #[cfg(any(target_os = "windows", test))]
    fn complete(&self, folder: &Path) -> Result<(), String> {
        self.check_cancelled()?;
        self.set(folder, "ready", "ready", "")
    }
    #[cfg(unix)]
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

struct Components {
    #[cfg(unix)]
    source: String,
    android: Option<String>,
    #[cfg(unix)]
    images: bool,
    wsl: Option<String>,
}

fn verify_resources(folder: &Path) -> Result<Components, String> {
    let expected: std::collections::BTreeMap<String, String> =
        serde_json::from_str(INTEGRITY).map_err(|e| e.to_string())?;
    if !expected.contains_key("source.payload") {
        return Err("invalid_payload".into());
    }
    for (name, digest) in &expected {
        if ![
            "source.payload",
            "entry.py",
            "android.payload",
            "images.payload",
            "tools/manifest.json",
            "wsl/manifest.json",
        ]
        .contains(&name.as_str())
        {
            return Err("invalid_payload".into());
        }
        // Native staging authenticates source and Android while extracting them.
        if matches!(name.as_str(), "source.payload" | "android.payload") {
            continue;
        }
        if hash(&folder.join(name))? != *digest {
            return Err("invalid_payload".into());
        }
    }
    Ok(Components {
        #[cfg(unix)]
        source: expected["source.payload"].clone(),
        android: expected.get("android.payload").cloned(),
        #[cfg(unix)]
        images: expected.contains_key("images.payload"),
        wsl: expected.get("wsl/manifest.json").cloned(),
    })
}

fn checks(path: &Path) -> Vec<Check> {
    let ram = resources::total_memory_bytes() / 1024_u64.pow(3);
    let disk = available_bytes(path) / 1024_u64.pow(3);
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
    let components = verify_resources(&resource)?;
    let request_path = folder.join("request.json");
    fs::write(
        &request_path,
        serde_json::to_vec(&request).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    if components.wsl.is_some() {
        return register_windows_backend(
            manager,
            &folder,
            &resource,
            components.wsl.as_deref(),
            components.android.as_deref(),
            &request,
        );
    }
    #[cfg(not(target_os = "windows"))]
    if components.wsl.is_some() {
        return Err("wsl_backend_host".into());
    }
    #[cfg(unix)]
    return run_native_installer(manager, &folder, &resource, &request_path, components);
    #[cfg(not(unix))]
    Err("wsl_backend_missing".into())
}

#[cfg(target_os = "windows")]
fn register_windows_backend(
    manager: &Arc<ManagerState>,
    folder: &Path,
    resource: &Path,
    identity: Option<&str>,
    android_identity: Option<&str>,
    request: &Request,
) -> Result<(), String> {
    let identity = identity.ok_or("wsl_backend_missing")?;
    manager.set(folder, "backend", "running", "")?;
    let cancelled = || manager.cancelled.load(Ordering::SeqCst);
    let mut native = ai_phone_host_runtime::wsl::Native::new(&cancelled);
    let mut installation = ai_phone_host_runtime::Installation::lock(folder)
        .map_err(|error| error.code().to_string())?;
    let phone = if let Some(identity) = android_identity {
        manager.set(folder, "android", "running", "")?;
        let phone = installation
            .prepare_android(
                &resource.join("android.payload"),
                identity,
                ai_phone_host_runtime::android::Resources {
                    cpus: request.phone.cpus.into(),
                    memory_mb: request.phone.memory_mb,
                    storage_gb: request.phone.storage_gb.into(),
                },
                &available_bytes,
                &cancelled,
            )
            .map_err(|error| error.code().to_string())?;
        Some(phone)
    } else {
        None
    };
    let request = ai_phone_host_runtime::wsl::InitializeRequest::from_json(
        serde_json::to_vec(request).map_err(|_| "invalid_device_resources")?,
    )
    .map_err(|error| error.code().to_string())?;
    let registered = installation
        .start_wsl_bundle(
            &resource.join("wsl"),
            identity,
            &request,
            &mut native,
            &available_bytes,
            &cancelled,
        )
        .map_err(|error| error.code().to_string())?;
    if !registered.backend_initialized {
        return Err("host_backend_pending".into());
    }
    let phone = phone.ok_or("android_runtime_missing")?;
    manager.set(folder, "android", "running", "")?;
    // The detached worker owns the phone and bridge. Completion observes the
    // same pair and durable device setup; registration alone is never ready.
    let boot = native_phone::start(&phone, &registered.distribution, &cancelled)?;
    manager.set(folder, "devices", "running", "")?;
    native
        .complete_devices(&registered.distribution, &boot.backend_bridge_id)
        .map_err(|error| error.code().to_string())?;
    manager.complete(folder)
}

#[cfg(unix)]
fn run_native_installer(
    manager: &Arc<ManagerState>,
    folder: &Path,
    resource: &Path,
    request: &Path,
    components: Components,
) -> Result<(), String> {
    let android = components.android.ok_or("android_runtime_missing")?;
    if !components.images {
        return Err("image_runtime_missing".into());
    }
    let mut command =
        Command::new(std::env::current_exe().map_err(|_| "native_initializer_program")?);
    command
        .arg("--initialize-unix-host")
        .arg(resource)
        .arg(folder)
        .arg(request)
        .arg(components.source)
        .arg(android)
        .current_dir(folder);
    execute(manager, command, folder)
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
        assert!(capability("linux", "x86_64").implemented);
        for (os, arch) in [
            ("macos", "x86_64"),
            ("windows", "x86_64"),
            ("linux", "aarch64"),
        ] {
            assert!(!capability_from_resources(os, arch, &Default::default()).implemented);
        }
    }
    #[test]
    fn only_the_complete_native_windows_pair_can_enter_device_configuration() {
        use std::collections::BTreeMap;
        let mut resources = BTreeMap::new();
        assert!(!capability_from_resources("windows", "x86_64", &resources).implemented);
        resources.insert("wsl/manifest.json".into(), "a".repeat(64));
        assert!(!capability_from_resources("windows", "x86_64", &resources).implemented);
        resources.insert("android.payload".into(), "b".repeat(64));
        assert!(capability_from_resources("windows", "x86_64", &resources).implemented);
        for (os, arch) in [("windows", "aarch64"), ("linux", "aarch64")] {
            assert!(!capability_from_resources(os, arch, &resources).implemented);
        }
        resources.remove("wsl/manifest.json");
        assert!(!capability_from_resources("windows", "x86_64", &resources).implemented);
    }
    #[test]
    fn native_completion_is_durable_and_cancellation_cannot_publish_ready() {
        let folder = std::env::temp_dir().join(format!(
            "aiphone-native-completion-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&folder).unwrap();
        let manager = ManagerState::default();
        manager.set(&folder, "devices", "running", "").unwrap();
        assert!(!checkpoint::completed(&folder).unwrap());
        manager.cancelled.store(true, Ordering::SeqCst);
        assert_eq!(
            manager.complete(&folder),
            Err("installation_cancelled".into())
        );
        assert!(!checkpoint::completed(&folder).unwrap());
        manager.cancelled.store(false, Ordering::SeqCst);
        manager.complete(&folder).unwrap();
        assert!(checkpoint::completed(&folder).unwrap());
        let restarted = ManagerState::default();
        restarted.recover(&folder).unwrap();
        assert_eq!(restarted.progress.lock().unwrap().state, "installed");
        fs::remove_dir_all(folder).unwrap();
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
