//! Complete sealed WSL backend initialization; native phone readiness belongs to the host.
pub(crate) mod process;
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
fn environment(
    home: &Path,
    run: &Path,
    inherited: impl IntoIterator<Item = (String, String)>,
) -> Result<BTreeMap<String, String>> {
    let mut env: BTreeMap<_, _> = inherited
        .into_iter()
        .filter(|(key, _)| !FOREIGN_ENGINE.contains(&key.as_str()))
        .collect();
    env.insert(
        "DOCKER_HOST".into(),
        format!("unix://{}", text(&run.join("docker.sock"))?),
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
pub(crate) struct Backend<'a, E> {
    pub(crate) root: &'a Path,
    pub(crate) program: &'a OsStr,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) execute: E,
}
impl<E: Execute> Backend<'_, E> {
    pub(crate) fn command(
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
    pub(crate) async fn checked(&mut self, command: Command, input: Vec<u8>) -> Result<Vec<u8>> {
        let output = self.execute.run(command, input).await?;
        if !output.status.success() {
            return Err(Error::Invalid("wsl_initializer_command"));
        }
        Ok(output.stdout)
    }
    async fn docker(&mut self, args: &[&str]) -> Result<Vec<u8>> {
        self.checked(self.command("docker", args), Vec::new()).await
    }
    pub(crate) async fn native(&mut self, args: &[OsString], input: Vec<u8>) -> Result<Vec<u8>> {
        self.checked(self.command(self.program, args), input).await
    }
    pub(crate) async fn unit(
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
    pub(crate) async fn containers(&mut self) -> Result<BTreeMap<String, Container>> {
        let filter = format!("name=^/({})$", NAMES.join("|"));
        let listed = self
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
        let ids: Vec<_> = std::str::from_utf8(&listed)
            .map_err(|_| Error::Invalid("container_ownership"))?
            .lines()
            .collect();
        let mut found = BTreeMap::new();
        if ids.is_empty() {
            return Ok(found);
        }
        if !ids.iter().all(|id| identity(id)) {
            return Err(Error::Invalid("container_ownership"));
        }
        let mut command = vec![
            "container",
            "inspect",
            "--format",
            r#"{"id":{{json .Id}},"name":{{json .Name}},"running":{{json .State.Running}},"labels":{{json .Config.Labels}}}"#,
        ];
        command.extend(ids.iter().copied());
        let inspected = self.docker(&command).await?;
        let rows = serde_json::Deserializer::from_slice(&inspected)
            .into_iter::<Container>()
            .collect::<serde_json::Result<Vec<_>>>()
            .map_err(|_| Error::Invalid("container_ownership"))?;
        if rows.len() != ids.len() {
            return Err(Error::Invalid("container_ownership"));
        }
        for (value, id) in rows.into_iter().zip(ids) {
            value.verify(self.root, id)?;
            let name = value
                .name
                .strip_prefix('/')
                .filter(|name| NAMES.contains(name))
                .ok_or(Error::Invalid("container_ownership"))?
                .to_owned();
            if found.insert(name, value).is_some() {
                return Err(Error::Invalid("container_ownership"));
            }
        }
        Ok(found)
    }
    async fn start_engine(&mut self, home: &Path, run: &Path) -> Result<()> {
        phase("engine_supervisor");
        filesystem::directory(run, true)?;
        self.unit(
            "wsl-engine",
            "start",
            [
                "/usr/local/bin/dockerd",
                "--host",
                &self.env["DOCKER_HOST"],
                "--data-root",
                &text(
                    &home
                        .parent()
                        .ok_or(Error::Invalid("wsl_initializer_path"))?
                        .join("docker"),
                )?,
                "--exec-root",
                &text(&run.join("engine"))?,
                "--pidfile",
                &text(&run.join("dockerd.pid"))?,
            ]
            .map(str::to_owned)
            .into(),
            &[],
        )
        .await?;
        phase("engine_ready");
        self.wait_engine(Instant::now() + Duration::from_secs(30))
            .await
    }
    async fn wait_engine(&mut self, until: Instant) -> Result<()> {
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
    pub(crate) async fn start(&mut self, images: &Path, user: &Path, web_bind: &str) -> Result<()> {
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
        self.env
            .extend(crate::runtime_environment::profile_environment(
                self.root, user,
            )?);
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
        self.unit("assistant", "start", vec![assistant.clone()], &[])
            .await?;
        let bun = text(&self.root.join("runtime/bin/bun"))?;
        self.unit(
            "web",
            "start",
            vec![
                bun.clone(),
                text(&self.root.join("runtime/web/apps/web/server.js"))?,
            ],
            &[("HOSTNAME", web_bind.into()), ("PORT", "17860".into())],
        )
        .await?;
        self.unit(
            "channels",
            "start",
            vec![assistant, "--channels".into()],
            &[],
        )
        .await?;
        self.unit(
            "media",
            "start",
            vec![
                text(&self.root.join("runtime/services/bin/runner"))?,
                "--media".into(),
            ],
            &[],
        )
        .await?;
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
pub(crate) struct Container {
    id: String,
    name: String,
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
        {
            return Err(Error::Invalid("foreign_installation"));
        }
        let directory = self
            .labels
            .get("com.docker.compose.project.working_dir")
            .and_then(|path| Path::new(path).canonicalize().ok())
            .ok_or(Error::Invalid("foreign_installation"))?;
        if directory == root {
            return Ok(());
        }
        crate::staging::owns_release(root, &directory)
            .map_err(|_| Error::Invalid("foreign_installation"))
    }
}
pub(crate) fn token(root: &Path, section: &str) -> Result<()> {
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
pub(crate) async fn run(arguments: &[OsString]) -> Result<()> {
    if !arguments.is_empty() {
        return Err(Error::Invalid("wsl_initializer_arguments"));
    }
    if std::env::consts::OS != "linux" || std::env::consts::ARCH != "x86_64" {
        return Err(Error::Invalid("wsl_initializer_host"));
    }
    let mut request = Vec::new();
    std::io::stdin().take(8193).read_to_end(&mut request)?;
    initialize_request(&request, "127.0.0.1").await?;
    println!(
        "{}",
        json!({"backend_started":true,"phone_ready":false,"complete_device_bundle":false,"windows_boot_verified":false})
    );
    Ok(())
}

pub(crate) async fn initialize_request(request: &[u8], web_bind: &str) -> Result<()> {
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
        Path::new("/run/aiphone"),
        request,
        web_bind,
        Process(stop),
        &|path| Ok(fs2::available_space(path)?),
    )
    .await;
    signal.abort();
    let _ = signal.await;
    result
}
async fn initialize<E: Execute>(
    bundle: &Path,
    home: &Path,
    run: &Path,
    request: &[u8],
    web_bind: &str,
    execute: E,
    available: &dyn Fn(&Path) -> Result<u64>,
) -> Result<()> {
    let request = profile::parse_request(request)
        .map_err(|_| Error::Invalid("wsl_initialization_request"))?;
    phase("preflight");
    let parent = home
        .parent()
        .ok_or(Error::Invalid("wsl_initializer_path"))?;
    filesystem::directory(parent, true)?;
    let _lock = filesystem::lease(&parent.join("initialize.lock"))?.ok_or(Error::Busy)?;
    if available(parent)? < 4 * 1024 * 1024 * 1024 {
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
        env: environment(home, run, std::env::vars())?,
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
    profile::prepare_request(
        &root,
        Path::new("/opt/aiphone/native-phone-bridge"),
        "android35-x86_64",
        request,
        PATH,
        "",
    )?;
    backend.start_engine(home, run).await?;
    backend
        .start(&bundle.join("images.payload"), Path::new("/root"), web_bind)
        .await
}

#[cfg(test)]
mod tests;
