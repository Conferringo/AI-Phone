//! Complete sealed WSL backend initialization; native phone readiness belongs to the host.
mod process;
use crate::{Error, Result, filesystem, profile};
use process::{Execute, Process};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};
use tokio::{process::Command, sync::watch};

const FOREIGN_ENGINE: [&str; 9] = [
    "DOCKER_HOST",
    "DOCKER_CONFIG",
    "DOCKER_CONTEXT",
    "DOCKER_TLS",
    "DOCKER_TLS_VERIFY",
    "DOCKER_CERT_PATH",
    "DOCKER_API_VERSION",
    "LIMA_HOME",
    "AI_PHONE_MANAGED_BIN",
];
const NAMES: [&str; 4] = [
    "ai-phone-postgres-1",
    "ai-phone-redis-1",
    "ai-phone-desktop",
    "ai-phone-ocr",
];
const PATH: &str = "/usr/local/bin:/usr/bin:/bin";

fn phase(name: &str) {
    eprintln!("AI_PHONE_INITIALIZE {name}");
}
fn text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or(Error::Invalid("wsl_initializer_path"))
}
fn environment(home: &Path) -> Result<BTreeMap<String, String>> {
    let mut env: BTreeMap<_, _> = std::env::vars()
        .filter(|(key, _)| !FOREIGN_ENGINE.contains(&key.as_str()))
        .collect();
    env.insert(
        "DOCKER_HOST".into(),
        "unix:///run/aiphone/docker.sock".into(),
    );
    env.insert(
        "DOCKER_CONFIG".into(),
        text(
            &home
                .parent()
                .ok_or(Error::Invalid("wsl_initializer_path"))?
                .join("docker-cli"),
        )?,
    );
    env.insert("PATH".into(), PATH.into());
    env.insert("NEXT_TELEMETRY_DISABLED".into(), "1".into());
    Ok(env)
}
struct Backend<'a, E> {
    root: &'a Path,
    program: &'a OsStr,
    env: BTreeMap<String, String>,
    execute: E,
}
impl<E: Execute> Backend<'_, E> {
    fn command(
        &self,
        program: impl AsRef<OsStr>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Command {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(self.root)
            .env_clear()
            .envs(&self.env);
        command
    }
    async fn checked(&mut self, command: Command, input: Vec<u8>) -> Result<Vec<u8>> {
        let output = self.execute.run(command, input).await?;
        if !output.status.success() {
            return Err(Error::Invalid("wsl_initializer_command"));
        }
        Ok(output.stdout)
    }
    async fn docker(&mut self, args: &[&str]) -> Result<Vec<u8>> {
        self.checked(self.command("docker", args), Vec::new()).await
    }
    async fn native(&mut self, args: &[OsString], input: Vec<u8>) -> Result<Vec<u8>> {
        self.checked(self.command(self.program, args), input).await
    }
    async fn unit(
        &mut self,
        name: &str,
        operation: &str,
        command: Vec<String>,
        extra: &[(&str, String)],
    ) -> Result<Value> {
        let mut env = self.env.clone();
        env.extend(
            extra
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone())),
        );
        let input = if operation == "start" {
            serde_json::to_vec(&json!({"command":command,"environment":env}))
                .map_err(|_| Error::Invalid("service_spec"))?
        } else {
            Vec::new()
        };
        let bytes = self
            .native(
                &[
                    "--service-control".into(),
                    self.root.as_os_str().into(),
                    name.into(),
                    operation.into(),
                ],
                input,
            )
            .await?;
        serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("service_receipt"))
    }
    async fn containers(&mut self) -> Result<BTreeMap<String, Container>> {
        let mut found = BTreeMap::new();
        for name in NAMES {
            let filter = format!("name=^/{name}$");
            let bytes = self
                .docker(&[
                    "container",
                    "ls",
                    "--all",
                    "--filter",
                    &filter,
                    "--no-trunc",
                    "--format",
                    "{{.ID}}",
                ])
                .await?;
            let id = std::str::from_utf8(&bytes)
                .map_err(|_| Error::Invalid("container_ownership"))?
                .trim();
            if id.is_empty() {
                continue;
            }
            if !identity(id) {
                return Err(Error::Invalid("container_ownership"));
            }
            let bytes=self.docker(&["container","inspect","--format","{\"id\":{{json .Id}},\"running\":{{json .State.Running}},\"labels\":{{json .Config.Labels}}}",id]).await?;
            let value: Container = serde_json::from_slice(&bytes)
                .map_err(|_| Error::Invalid("container_ownership"))?;
            value.verify(self.root, id)?;
            found.insert(name.into(), value);
        }
        Ok(found)
    }
    async fn start_engine(&mut self) -> Result<()> {
        phase("engine_supervisor");
        filesystem::directory(Path::new("/run/aiphone"), true)?;
        self.unit(
            "wsl-engine",
            "start",
            [
                "/usr/local/bin/dockerd",
                "--host",
                "unix:///run/aiphone/docker.sock",
                "--data-root",
                "/var/lib/aiphone/docker",
                "--exec-root",
                "/run/aiphone/engine",
                "--pidfile",
                "/run/aiphone/dockerd.pid",
            ]
            .map(str::to_owned)
            .into(),
            &[],
        )
        .await?;
        phase("engine_ready");
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let output = self
                .execute
                .run(self.command("docker", ["info"]), Vec::new())
                .await?;
            if output.status.success() {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err(Error::Invalid("wsl_engine_not_ready"));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    async fn start(&mut self, images: &Path) -> Result<()> {
        phase("container_ownership");
        let existing = self.containers().await?;
        phase("images");
        let bytes = self
            .native(
                &[
                    "--activate-images".into(),
                    self.root.as_os_str().into(),
                    images.as_os_str().into(),
                ],
                Vec::new(),
            )
            .await?;
        let selected: BTreeMap<String, String> =
            serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("image_manifest"))?;
        self.env.extend(selected);
        phase("environment");
        self.env.extend(profile_environment(self.root)?);
        for name in ["assistant", "phone-runner", "channels"] {
            token(self.root, name)?;
        }
        phase("database_start");
        self.docker(&[
            "compose",
            "-f",
            "compose.yaml",
            "-f",
            "compose.dev.yaml",
            "up",
            "-d",
            "--wait",
            "--no-build",
            "--pull",
            "never",
            "postgres",
            "redis",
            "ocr",
        ])
        .await?;
        let assistant = text(&self.root.join("runtime/services/bin/assistant"))?;
        phase("database_prepare");
        if self
            .unit("assistant", "status", Vec::new(), &[])
            .await?
            .is_null()
        {
            for flag in ["--prepare-database", "--migrate"] {
                self.checked(self.command(&assistant, [flag]), Vec::new())
                    .await?;
            }
        }
        phase("services_start");
        let ocr = text(&self.root.join("runtime/services/bin/vision"))?;
        self.unit(
            "runner",
            "start",
            vec![text(&self.root.join("runtime/services/bin/runner"))?],
            &[("AI_PHONE_OCR", ocr)],
        )
        .await?;
        self.unit("assistant", "start", vec![assistant], &[])
            .await?;
        let bun = text(&self.root.join("runtime/bin/bun"))?;
        self.unit(
            "web",
            "start",
            vec![
                bun.clone(),
                text(&self.root.join("runtime/web/apps/web/server.js"))?,
            ],
            &[("HOSTNAME", "127.0.0.1".into()), ("PORT", "17860".into())],
        )
        .await?;
        for (name, entry) in [("channels", "channels"), ("media", "device-stream")] {
            self.unit(
                name,
                "start",
                vec![
                    bun.clone(),
                    "--no-install".into(),
                    text(
                        &self
                            .root
                            .join(format!("runtime/js-services/apps/{entry}/server.js")),
                    )?,
                ],
                &[],
            )
            .await?;
        }
        if let Some(desktop) = existing.get("ai-phone-desktop")
            && !desktop.running
        {
            self.docker(&["start", &desktop.id]).await?;
        }
        Ok(())
    }
}
fn identity(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
#[derive(Deserialize)]
struct Container {
    id: String,
    running: bool,
    labels: BTreeMap<String, String>,
}
impl Container {
    fn verify(&self, root: &Path, id: &str) -> Result<()> {
        if self.id != id
            || self
                .labels
                .get("com.docker.compose.project")
                .map(String::as_str)
                != Some("ai-phone")
            || self
                .labels
                .get("com.docker.compose.project.working_dir")
                .and_then(|path| Path::new(path).canonicalize().ok())
                .as_deref()
                != Some(root)
        {
            return Err(Error::Invalid("foreign_installation"));
        }
        Ok(())
    }
}
fn token(root: &Path, section: &str) -> Result<()> {
    let folder = root.join("data").join(section);
    filesystem::directory(&folder, true)?;
    if folder.join("token").try_exists()? {
        return Ok(());
    }
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::Invalid("host_profile_random"))?;
    let value = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    filesystem::publish_private(&folder, "token", value.as_bytes(), true)
}
fn profile_environment(root: &Path) -> Result<BTreeMap<String, String>> {
    let value: Value =
        serde_json::from_slice(&fs::read(root.join("data/deployment/profile.json"))?)
            .map_err(|_| Error::Invalid("host_profile"))?;
    let mut env = BTreeMap::from([
        ("AI_PHONE_PROJECT_ROOT".into(), text(root)?),
        (
            "AI_PHONE_DATA_ROOT".into(),
            text(&root.join("data").canonicalize()?)?,
        ),
        ("AI_PHONE_ANDROID_ABI".into(), "x86_64".into()),
        ("AI_PHONE_CLASH_DESKTOP_DIR".into(), "linux-amd64".into()),
        ("AI_PHONE_LOCAL_ENTRY".into(), "1".into()),
    ]);
    for (field, key) in [
        ("sdk_root", "AI_PHONE_SDK_ROOT"),
        ("locale", "AI_PHONE_DEFAULT_LOCALE"),
        ("java_home", "JAVA_HOME"),
        ("java_home", "AI_PHONE_JAVA_HOME"),
        ("host_proxy", "AI_PHONE_HOST_PROXY"),
        ("container_proxy", "AI_PHONE_CONTAINER_PROXY"),
    ] {
        env.insert(
            key.into(),
            value[field]
                .as_str()
                .ok_or(Error::Invalid("host_profile"))?
                .into(),
        );
    }
    env.insert(
        "AI_PHONE_EMULATOR_PROXY".into(),
        value
            .get("emulator_proxy")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
    );
    for (key, folder) in [
        ("ANDROID_USER_HOME", "user"),
        ("ANDROID_EMULATOR_HOME", "user"),
        ("ANDROID_AVD_HOME", "avd"),
    ] {
        env.insert(key.into(), text(&root.join("data/android").join(folder))?);
    }
    for (section, field, key) in [
        ("phone", "cpus", "AI_PHONE_PHONE_CPUS"),
        ("phone", "memory_mb", "AI_PHONE_PHONE_MEMORY"),
        ("phone", "storage_gb", "AI_PHONE_PHONE_STORAGE"),
        ("desktop", "cpus", "AI_PHONE_DESKTOP_CPUS"),
        ("desktop", "memory_mb", "AI_PHONE_DESKTOP_MEMORY"),
    ] {
        let number = value[section][field]
            .as_u64()
            .ok_or(Error::Invalid("host_profile"))?;
        env.insert(
            key.into(),
            format!(
                "{number}{}",
                if key == "AI_PHONE_DESKTOP_MEMORY" {
                    "m"
                } else {
                    ""
                }
            ),
        );
    }
    let apk = root.join("runtime/guest-apps/clash-x86_64.apk");
    if apk.is_file() {
        env.insert("AI_PHONE_CLASH_ANDROID_APK".into(), text(&apk)?);
    }
    Ok(env)
}

pub(crate) async fn run(arguments: &[OsString]) -> Result<()> {
    if !arguments.is_empty() {
        return Err(Error::Invalid("wsl_initializer_arguments"));
    }
    if std::env::consts::OS != "linux" || std::env::consts::ARCH != "x86_64" {
        return Err(Error::Invalid("wsl_initializer_host"));
    }
    let mut request = Vec::new();
    std::io::stdin().take(8193).read_to_end(&mut request)?;
    super::InitializeRequest::from_json(request.clone())?;
    let (send, stop) = watch::channel(false);
    let signal = tokio::spawn(async move {
        let result = crate::component::interrupt().await;
        if result.is_ok() {
            send.send_replace(true);
        }
        result
    });
    let result = initialize(
        Path::new("/opt/aiphone/bundle"),
        Path::new("/var/lib/aiphone/installation"),
        &request,
        Process(stop),
    )
    .await;
    signal.abort();
    let _ = signal.await;
    result?;
    println!(
        "{}",
        json!({"backend_started":true,"phone_ready":false,"complete_device_bundle":false,"windows_boot_verified":false})
    );
    Ok(())
}
async fn initialize<E: Execute>(
    bundle: &Path,
    home: &Path,
    request: &[u8],
    execute: E,
) -> Result<()> {
    phase("preflight");
    let parent = home
        .parent()
        .ok_or(Error::Invalid("wsl_initializer_path"))?;
    filesystem::directory(parent, true)?;
    let _lock = filesystem::lease(&parent.join("initialize.lock"))?.ok_or(Error::Busy)?;
    if fs2::available_space(parent)? < 4 * 1024 * 1024 * 1024 {
        return Err(Error::LowSpace);
    }
    let id = fs::read_to_string(bundle.join("source.sha256"))?;
    let id = id.trim();
    if !identity(id) {
        return Err(Error::Invalid("source_resource_identity"));
    }
    let program = std::env::current_exe()?;
    let mut backend = Backend {
        root: parent,
        program: program.as_os_str(),
        env: environment(home)?,
        execute,
    };
    phase("source");
    let receipt = backend
        .native(
            &[
                "--stage-source".into(),
                bundle.join("source.payload").into_os_string(),
                home.as_os_str().into(),
                id.into(),
            ],
            Vec::new(),
        )
        .await?;
    let root = home.join("releases").join(id).canonicalize()?;
    if receipt != format!("{id} {}\n", root.display()).as_bytes() {
        return Err(Error::Invalid("source_resource_identity"));
    }
    backend.root = &root;
    if root.join("data/restore-quarantine.json").try_exists()? {
        return Err(Error::Invalid("restored_state_quarantined"));
    }
    if fs::symlink_metadata(root.join("data/deployment/container-host.json")).is_ok() {
        return Err(Error::Invalid("foreign_installation"));
    }
    phase("profile");
    profile::prepare(
        &root,
        Path::new("/opt/aiphone/native-phone-bridge"),
        "android35-x86_64",
        request,
        PATH,
        "",
    )?;
    backend.start_engine().await?;
    backend.start(&bundle.join("images.payload")).await
}

#[cfg(test)]
mod tests;
