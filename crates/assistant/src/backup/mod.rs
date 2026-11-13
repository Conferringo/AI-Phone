mod archive;
mod database;
mod phone;

use flate2::{Compression, write::GzEncoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| "Invalid filesystem path".into())
}
fn command(program: &str, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .ok_or("Command stdin missing")?
            .write_all(input)?;
    }
    let result = child.wait_with_output()?;
    if !result.status.success() {
        return Err(format!("{program} failed ({})", result.status).into());
    }
    Ok(result.stdout)
}
fn docker(args: &[&str]) -> Result<Vec<u8>> {
    command("docker", args, None)
}
fn docker_json(args: &[&str]) -> Result<Value> {
    Ok(serde_json::from_slice(&docker(args)?)?)
}
fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn private(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().ok_or("Private file parent missing")?)?;
    crate::private_file::atomic_private(path, bytes)?;
    Ok(())
}
fn save(path: &Path, value: &Value) -> Result<()> {
    private(path, &serde_json::to_vec(value)?)
}
fn new_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?)
}
const BACKUP_FORMAT: u64 = 2;

#[derive(Clone, Copy)]
enum BackupDigest {
    Legacy,
    Current,
}
impl BackupDigest {
    fn from_manifest(manifest: &Value) -> Result<Self> {
        match manifest["format"].as_u64() {
            Some(1) => Ok(Self::Legacy),
            Some(BACKUP_FORMAT) => Ok(Self::Current),
            _ => Err("Unsupported backup format".into()),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Legacy => "sha256",
            Self::Current => "sha512",
        }
    }

    fn hasher(self) -> BackupHasher {
        match self {
            Self::Legacy => BackupHasher::Legacy(Sha256::new()),
            Self::Current => BackupHasher::Current(Sha512::new()),
        }
    }

    fn file(self, path: &Path) -> Result<String> {
        let mut input = File::open(path)?;
        let mut hash = self.hasher();
        io::copy(&mut input, &mut hash)?;
        Ok(hash.finish())
    }
}

enum BackupHasher {
    Legacy(Sha256),
    Current(Sha512),
}
impl BackupHasher {
    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Legacy(hash) => hash.update(bytes),
            Self::Current(hash) => hash.update(bytes),
        }
    }

    fn finish(self) -> String {
        match self {
            Self::Legacy(hash) => format!("{:x}", hash.finalize()),
            Self::Current(hash) => format!("{:x}", hash.finalize()),
        }
    }
}
impl Write for BackupHasher {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn receipt(path: &Path) -> Result<Value> {
    Ok(
        json!({"path":path.file_name().ok_or("Payload name missing")?,"size":fs::metadata(path)?.len(),"sha512":BackupDigest::Current.file(path)?}),
    )
}
fn environment(root: &Path) -> Result<BTreeMap<String, String>> {
    let bytes = command(
        text(&std::env::current_exe()?)?,
        &[
            "--runtime-environment",
            text(root)?,
            text(&PathBuf::from(
                std::env::var_os("HOME").ok_or("HOME missing")?,
            ))?,
        ],
        None,
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn service(root: &Path, role: &str, operation: &str, spec: Option<&Value>) -> Result<Value> {
    Ok(serde_json::from_slice(&command(
        text(&std::env::current_exe()?)?,
        &["--service-control", text(root)?, role, operation],
        spec.map(serde_json::to_vec).transpose()?.as_deref(),
    )?)?)
}
fn service_spec(_root: &Path, role: &str) -> Result<Value> {
    #[cfg(target_os = "linux")]
    {
        read_json(
            &_root
                .join("data/services")
                .join(format!("{role}.spec.json")),
        )
    }
    #[cfg(target_os = "macos")]
    {
        let labels = [
            ("lan", "dev.aiphone.lan"),
            ("channels", "dev.aiphone.channels"),
            ("web", "dev.aiphone.web.p2"),
            ("assistant", "dev.aiphone.assistant"),
            ("runner", "dev.aiphone.runner.p2"),
            ("media", "dev.aiphone.device-stream"),
            ("emulator", "dev.aiphone.emulator.p2"),
        ];
        let label = labels
            .iter()
            .find(|(name, _)| *name == role)
            .ok_or("Unknown service")?
            .1;
        let uid = String::from_utf8(command("/usr/bin/id", &["-u"], None)?)?;
        let job = String::from_utf8(command(
            "/bin/launchctl",
            &["print", &format!("gui/{}/{label}", uid.trim())],
            None,
        )?)?;
        let path = job
            .lines()
            .find_map(|line| line.trim().strip_prefix("path = "))
            .ok_or("Loaded service plist missing")?;
        let plist: Value = serde_json::from_slice(&command(
            "/usr/bin/plutil",
            &["-convert", "json", "-o", "-", path],
            None,
        )?)?;
        Ok(json!({"command":plist["ProgramArguments"],"environment":plist["EnvironmentVariables"]}))
    }
}
fn freeze<T>(
    root: &Path,
    desktop: &str,
    enabled: bool,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if !enabled {
        return action();
    }
    let mut units = vec![];
    for role in [
        "lan",
        "channels",
        "web",
        "assistant",
        "runner",
        "media",
        "emulator",
    ] {
        let status = service(root, role, "status", None)?;
        if !status.is_null() {
            units.push((role, service_spec(root, role)?));
        }
    }
    let was_running = docker_json(&["inspect", desktop])?[0]["State"]["Running"]
        .as_bool()
        .ok_or("Desktop state missing")?;
    let mut stopped = vec![];
    let mut desktop_stopped = false;
    let result = (|| {
        for (role, spec) in units {
            service(root, role, "stop", None)?;
            stopped.push((role, spec));
        }
        if was_running {
            docker(&["stop", "--time", "20", desktop])?;
            desktop_stopped = true;
        }
        action()
    })();
    if desktop_stopped {
        docker(&["start", desktop])?;
    }
    for (role, spec) in stopped.into_iter().rev() {
        service(root, role, "start", Some(&spec))?;
    }
    result
}
fn export_desktop(container: &str, path: &Path) -> Result<()> {
    let mut child = Command::new("docker")
        .args(["export", container])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut compressed = GzEncoder::new(new_file(path)?, Compression::fast());
    io::copy(
        &mut child.stdout.take().ok_or("Desktop export stdout missing")?,
        &mut compressed,
    )?;
    compressed.finish()?.sync_all()?;
    if !child.wait()?.success() {
        return Err("Desktop export failed".into());
    }
    Ok(())
}
fn absolute(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            _ => result.push(part.as_os_str()),
        }
    }
    Ok(result)
}
fn create(
    root: &Path,
    destination: &Path,
    container: &str,
    desktop: &str,
    quiesce: bool,
) -> Result<Value> {
    let root = root.canonicalize()?;
    let destination = absolute(destination)?;
    if destination.try_exists()? {
        return Err("Backup destination exists".into());
    }
    let data = root.join("data").canonicalize()?;
    if destination.starts_with(&data) && !destination.starts_with(data.join("backups")) {
        return Err("Save backups under data/backups or outside user state".into());
    }
    let parent = destination.parent().ok_or("Backup parent missing")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".aiphone-backup-")
        .tempdir_in(parent)?;
    let mut manifest = freeze(&root, desktop, quiesce, || {
        let env = database::environment(&root)?;
        let (dependencies, chains) = phone::dependencies(&root)?;
        let dump = new_file(&temporary.path().join("database.dump"))?;
        let status = Command::new("docker")
            .args([
                "exec",
                container,
                "pg_dump",
                "-U",
                &env["POSTGRES_USER"],
                "-d",
                &env["POSTGRES_DB"],
                "-Fc",
                "--no-owner",
                "--no-acl",
            ])
            .stdout(Stdio::from(dump))
            .stderr(Stdio::inherit())
            .status()?;
        if !status.success() {
            return Err("PostgreSQL backup failed".into());
        }
        let database =
            database::fingerprints(container, &env["POSTGRES_USER"], &env["POSTGRES_DB"])?;
        let info = docker_json(&["inspect", desktop])?;
        let info = &info[0];
        if info["State"]["Running"] != false {
            return Err("Stop desktop before a consistent backup".into());
        }
        let desktop_metadata = json!({"config":info["Config"],"memory":info["HostConfig"]["Memory"],"cpus":info["HostConfig"]["NanoCpus"],"shm":info["HostConfig"]["ShmSize"],"architecture":docker_json(&["image","inspect",info["Image"].as_str().ok_or("Desktop image missing")?])?[0]["Architecture"]});
        export_desktop(desktop, &temporary.path().join("desktop-rootfs.tar.gz"))?;
        let (records, excluded) =
            archive::write_state(&root, &temporary.path().join("state.tar.gz"), &dependencies)?;
        Ok(
            json!({"format":BACKUP_FORMAT,"id":uuid::Uuid::new_v4().simple().to_string(),"created_at":time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?,"consistency":if quiesce{"quiesced"}else{"offline-fixture"},"source_root":root,"source_data":data,"records":records,"excluded":excluded,"database":database,"desktop":desktop_metadata,"disk_dependencies":dependencies,"disk_chains":chains,"files":[]}),
        )
    })?;
    let mut entries = fs::read_dir(temporary.path())?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    manifest["files"] = entries
        .iter()
        .map(|entry| receipt(entry))
        .collect::<Result<Vec<_>>>()?
        .into();
    save(&temporary.path().join("manifest.json"), &manifest)?;
    let directory = destination
        .extension()
        .is_some_and(|extension| extension == "backup");
    let size = if directory {
        let size = entries
            .iter()
            .map(|entry| fs::metadata(entry).map(|info| info.len()))
            .collect::<std::io::Result<Vec<_>>>()?
            .iter()
            .sum::<u64>()
            + fs::metadata(temporary.path().join("manifest.json"))?.len();
        fs::rename(temporary.path(), &destination)?;
        size
    } else {
        archive::write_envelope(temporary.path(), &destination)?;
        fs::metadata(&destination)?.len()
    };
    Ok(
        json!({"id":manifest["id"],"path":destination,"size":size,"records":manifest["records"].as_object().ok_or("Backup records missing")?.len(),"consistent":quiesce}),
    )
}
fn restore(
    root: &Path,
    archive: &Path,
    destination: &Path,
    container: &str,
    admin: Option<&str>,
    database: Option<&str>,
    sdk: Option<&Path>,
) -> Result<Value> {
    let destination = absolute(destination)?;
    if destination.try_exists()? {
        for entry in fs::read_dir(&destination)? {
            let entry = entry?;
            if entry.file_name() != "data"
                || !entry.file_type()?.is_dir()
                || fs::read_dir(entry.path())?.next().is_some()
            {
                return Err("Restore destination must be empty".into());
            }
        }
    }
    let parent = destination.parent().ok_or("Restore parent missing")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".aiphone-restore-")
        .tempdir_in(parent)?;
    let (folder, manifest) = archive::read_backup(archive, temporary.path())?;
    let state = temporary.path().join("restored");
    fs::create_dir(&state)?;
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700))?;
    archive::restore_state(&folder.join("state.tar.gz"), &state, &manifest)?;
    if let Some(database) = database {
        let env = fs::read_to_string(state.join(".env"))?;
        let next = env
            .lines()
            .map(|line| {
                if line.starts_with("POSTGRES_DB=") {
                    format!("POSTGRES_DB={database}")
                } else {
                    line.into()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        private(&state.join(".env"), next.as_bytes())?;
        let file = state.join("data/database/credentials.json");
        let mut value = read_json(&file)?;
        value["database"] = database.into();
        save(&file, &value)?;
    }
    let env = database::environment(&state)?;
    let admin = admin.unwrap_or(&env["POSTGRES_USER"]);
    let sdk = match sdk {
        Some(path) => path.to_path_buf(),
        None => PathBuf::from(
            environment(root)?
                .remove("AI_PHONE_SDK_ROOT")
                .ok_or("Android SDK missing")?,
        ),
    };
    let changes = phone::relocate(&state, &manifest, &destination, &sdk)?;
    database::restore(
        &state,
        container,
        admin,
        &env["POSTGRES_DB"],
        &folder.join("database.dump"),
    )?;
    let restored = database::fingerprints(container, admin, &env["POSTGRES_DB"])?;
    if restored != manifest["database"] {
        return Err("Restored table fingerprints differ".into());
    }
    database::sql(
        container,
        admin,
        &env["POSTGRES_DB"],
        "UPDATE runs SET status='paused',next_wake_at=NULL WHERE status IN ('queued','running'); UPDATE proactive_state SET exploring=false,notifications_enabled=false;",
    )?;
    save(
        &state.join("data/restore-quarantine.json"),
        &json!({"backup_id":manifest["id"],"reason":"Restore verified; original instance must be offline before activation"}),
    )?;
    let has_desktop = !manifest["desktop"].is_null();
    if has_desktop {
        let recovery = state.join("data/desktop/recovery");
        fs::create_dir_all(&recovery)?;
        fs::copy(
            folder.join("desktop-rootfs.tar.gz"),
            recovery.join("rootfs.tar.gz"),
        )?;
        save(&recovery.join("container.json"), &manifest["desktop"])?;
    }
    let mut report = json!({"backup_id":manifest["id"],"database_verified":true,"tables":restored.as_object().ok_or("Table fingerprints missing")?.len(),"state_entries":manifest["records"].as_object().ok_or("State records missing")?.len(),"relocations":changes,"quarantined":true,"desktop_rootfs_saved":has_desktop});
    if has_desktop {
        let digest = BackupDigest::from_manifest(&manifest)?;
        report[format!("linux_rootfs_{}", digest.key())] = manifest["files"]
            .as_array()
            .ok_or("Backup files missing")?
            .iter()
            .find(|row| row["path"] == "desktop-rootfs.tar.gz")
            .ok_or("Desktop payload missing")?[digest.key()]
        .clone();
    }
    save(&state.join("data/restore-report.json"), &report)?;
    if destination.try_exists()? {
        if destination.join("data").try_exists()? {
            fs::remove_dir(destination.join("data"))?;
        }
        fs::remove_dir(&destination)?;
    }
    fs::rename(state, destination)?;
    Ok(report)
}
fn recover_desktop(destination: &Path, name: &str) -> Result<Value> {
    let suffix = name
        .strip_prefix("aiphone-recovery-")
        .ok_or("Use an aiphone-recovery- container name")?;
    if suffix.is_empty()
        || suffix.len() > 40
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("Invalid recovery container name".into());
    }
    let root = destination.canonicalize()?;
    let recovery = root.join("data/desktop/recovery");
    if !root.join("data/restore-quarantine.json").try_exists()? {
        return Err("Only quarantined restored state can boot offline".into());
    }
    let report = read_json(&root.join("data/restore-report.json"))?;
    let metadata = read_json(&recovery.join("container.json"))?;
    let config = &metadata["config"];
    let archive = recovery.join("rootfs.tar.gz");
    let (digest, expected) = match (
        report["linux_rootfs_sha512"].as_str(),
        report["linux_rootfs_sha256"].as_str(),
    ) {
        (Some(expected), None) => (BackupDigest::Current, expected),
        (None, Some(expected)) => (BackupDigest::Legacy, expected),
        _ => return Err("Recovered desktop rootfs checksum missing or ambiguous".into()),
    };
    if digest.file(&archive)? != expected {
        return Err("Recovered desktop rootfs changed".into());
    }
    let image = format!("{name}:verified");
    for args in [
        vec!["inspect", name],
        vec!["image", "inspect", image.as_str()],
    ] {
        if Command::new("docker")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success()
        {
            return Err("Recovery container or image already exists".into());
        }
    }
    let architecture = metadata["architecture"].as_str().unwrap_or("arm64");
    let mut import = Command::new("docker");
    import.args(["import", "--platform", &format!("linux/{architecture}")]);
    for (key, change) in [("Entrypoint", "ENTRYPOINT"), ("Cmd", "CMD")] {
        if config[key]
            .as_array()
            .is_some_and(|array| !array.is_empty())
        {
            import.args(["--change", &format!("{change} {}", config[key])]);
        }
    }
    let status = import
        .args(["-", &image])
        .stdin(Stdio::from(File::open(&archive)?))
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()?;
    if !status.success() {
        return Err("Desktop image import failed".into());
    }
    let temporary = tempfile::Builder::new()
        .prefix(".environment-")
        .tempdir_in(&recovery)?;
    let env_file = temporary.path().join("environment");
    let values = config["Env"]
        .as_array()
        .ok_or("Desktop environment missing")?
        .iter()
        .map(|value| value.as_str().ok_or("Desktop environment invalid"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    private(&env_file, (values.join("\n") + "\n").as_bytes())?;
    let mut created = false;
    let result = (|| {
        let mut create = Command::new("docker");
        create.args([
            "create",
            "--name",
            name,
            "--label",
            &format!(
                "dev.aiphone.recovery={}",
                report["backup_id"]
                    .as_str()
                    .ok_or("Backup identity missing")?
            ),
            "--init",
            "--network",
            "none",
            "--cap-add",
            "NET_ADMIN",
            "--env-file",
            text(&env_file)?,
        ]);
        for (key, option) in [("memory", "--memory"), ("shm", "--shm-size")] {
            if let Some(size) = metadata[key].as_u64().filter(|size| *size > 0) {
                create.args([option, &size.to_string()]);
            }
        }
        if let Some(cpus) = metadata["cpus"].as_u64().filter(|cpus| *cpus > 0) {
            create.args(["--cpus", &(cpus as f64 / 1_000_000_000.0).to_string()]);
        }
        for (source, target) in [
            (root.join("data/desktop/home"), "/home/ai"),
            (
                root.join("data/device-network/desktop"),
                "/etc/ai-phone-network",
            ),
        ] {
            let actual = source.canonicalize()?;
            if !actual.starts_with(root.join("data")) || !actual.is_dir() {
                return Err("Recovery mount leaves restored state".into());
            }
            create.args([
                "--mount",
                &format!("type=bind,source={},target={target}", actual.display()),
            ]);
        }
        if !create
            .arg(&image)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()?
            .success()
        {
            return Err("Recovery container creation failed".into());
        }
        created = true;
        docker(&["start", name])?;
        Ok(json!({"container":name,"image":image,"network":"none","original_device_changed":false}))
    })();
    if result.is_err() {
        if created {
            docker(&["rm", "--force", name])?;
        }
        docker(&["image", "rm", &image])?;
    }
    result
}
pub(crate) fn run(arguments: &[OsString]) -> Result<Value> {
    let operation = arguments
        .first()
        .and_then(|argument| argument.to_str())
        .ok_or("Backup operation required")?;
    let option = |name: &str| {
        arguments
            .iter()
            .position(|value| value == name)
            .and_then(|index| arguments.get(index + 1))
            .map(|value| value.as_os_str())
    };
    let path = |name: &str| {
        option(name)
            .map(Path::new)
            .ok_or_else(|| format!("{name} required"))
    };
    let root = option("--root")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let container = option("--pg-container")
        .and_then(|value| value.to_str())
        .unwrap_or("ai-phone-postgres-1");
    match operation {
        "create" => create(
            &root,
            path("--output")?,
            container,
            option("--desktop-container")
                .and_then(|value| value.to_str())
                .unwrap_or("ai-phone-desktop"),
            !arguments.iter().any(|value| value == "--offline"),
        ),
        "restore" => restore(
            &root,
            path("--archive")?,
            path("--destination")?,
            container,
            option("--pg-admin").and_then(|value| value.to_str()),
            option("--database").and_then(|value| value.to_str()),
            option("--sdk-root").map(Path::new),
        ),
        "recover-desktop" => recover_desktop(
            path("--destination")?,
            option("--desktop-container")
                .and_then(|value| value.to_str())
                .ok_or("Recovery container required")?,
        ),
        _ => Err("Expected backup create|restore|recover-desktop".into()),
    }
}
