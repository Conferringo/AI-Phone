use super::{
    BOOTSTRAP, Receipt,
    diagnostics::{self, Cursor},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

pub(super) type Result<T> = std::result::Result<T, Failure>;
#[derive(Clone, Debug, Serialize)]
pub(super) struct Failure {
    pub error: &'static str,
    pub returncode: Option<i32>,
    pub probe: String,
}
impl Failure {
    pub fn invalid(probe: impl Into<String>) -> Self {
        Self {
            error: "validation_failed",
            returncode: None,
            probe: probe.into(),
        }
    }
    pub fn timeout() -> Self {
        Self {
            error: "subprocess_timeout",
            returncode: None,
            probe: "unclassified".into(),
        }
    }
    fn command(status: std::process::ExitStatus) -> Self {
        use std::os::unix::process::ExitStatusExt;
        Self {
            error: "command_failed",
            returncode: status
                .code()
                .or_else(|| status.signal().map(|value| -value)),
            probe: "unclassified".into(),
        }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.probe)
    }
}
impl std::error::Error for Failure {}
impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        let error = match error.kind() {
            std::io::ErrorKind::NotFound => "resource_missing",
            std::io::ErrorKind::PermissionDenied => "permission_denied",
            std::io::ErrorKind::WouldBlock => "lock_busy",
            _ => "os_error",
        };
        Self {
            error,
            returncode: None,
            probe: "unclassified".into(),
        }
    }
}
impl From<serde_json::Error> for Failure {
    fn from(_: serde_json::Error) -> Self {
        Self::invalid("unclassified")
    }
}
impl From<reqwest::Error> for Failure {
    fn from(_: reqwest::Error) -> Self {
        Self {
            error: "os_error",
            returncode: None,
            probe: "unclassified".into(),
        }
    }
}

pub(super) struct Context {
    pub root: PathBuf,
    pub nonce: String,
    pub phase: u8,
    pub client: reqwest::Client,
    pub bootstrap: PathBuf,
    pub endpoints: Endpoints,
}
pub(super) struct Endpoints {
    pub assistant: String,
    pub web: String,
    pub channels: String,
    pub runner: String,
}
impl Context {
    pub fn new(root: PathBuf, nonce: String, phase: u8) -> Result<Self> {
        Ok(Self {
            root,
            nonce,
            phase,
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(120))
                .build()?,
            bootstrap: BOOTSTRAP.into(),
            endpoints: Endpoints {
                assistant: "http://127.0.0.1:17863".into(),
                web: "http://127.0.0.1:17860".into(),
                channels: "http://127.0.0.1:17864".into(),
                runner: "http://127.0.0.1:17862".into(),
            },
        })
    }
    pub fn root_str(&self) -> &str {
        self.root.to_str().expect("fixed ASCII installation path")
    }
    pub fn bootstrap_str(&self) -> &str {
        self.bootstrap.to_str().expect("native program path")
    }
    pub async fn execute<I, S>(&self, args: I, input: &[u8], seconds: u64) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        command(&self.root, args, input, seconds).await
    }
    pub fn token(&self, role: &str) -> Result<String> {
        Ok(
            fs::read_to_string(self.root.join("data").join(role).join("token"))?
                .trim()
                .into(),
        )
    }
}
pub(super) async fn command<I, S>(
    root: &Path,
    args: I,
    input: &[u8],
    seconds: u64,
) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut args = args.into_iter();
    let program = args.next().expect("explicit program");
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(root)
        .env("AI_PHONE_PROJECT_ROOT", root)
        .env("DOCKER_HOST", "unix:///run/aiphone/docker.sock")
        .env("DOCKER_CONFIG", "/var/lib/aiphone/docker-cli")
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    stdin.write_all(input).await?;
    drop(stdin);
    let output = tokio::time::timeout(Duration::from_secs(seconds), child.wait_with_output())
        .await
        .map_err(|_| Failure::timeout())??;
    if !output.status.success() {
        return Err(Failure::command(output.status));
    }
    Ok(output.stdout)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Acknowledgement {
    backend_started: bool,
    phone_ready: bool,
    complete_device_bundle: bool,
    windows_boot_verified: bool,
}

pub(super) async fn initialize(receipt: &mut Receipt, context: &Context) -> Result<()> {
    let result = native_initialize(receipt, context).await;
    if let Err(error) = &result {
        receipt.initialization.error = error.error;
        receipt.initialization.returncode = error.returncode;
    }
    result
}
async fn native_initialize(receipt: &mut Receipt, context: &Context) -> Result<()> {
    let request = json!({"locale":"en","phone":{"cpus":2,"memory_mb":2048,"storage_gb":16},"desktop":{"cpus":2,"memory_mb":2048}});
    let mut child = Command::new(&context.bootstrap)
        .arg("--initialize-wsl-backend")
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("piped input");
    stdin.write_all(&serde_json::to_vec(&request)?).await?;
    drop(stdin);
    let stdout = child.stdout.take().expect("piped output");
    let output = tokio::spawn(async move {
        let mut bytes = vec![];
        stdout.take(8193).read_to_end(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    });
    let mut stderr = BufReader::new(child.stderr.take().expect("piped errors"));
    let mut line = vec![];
    let mut cursor = Cursor::Unverified;
    while stderr.read_until(b'\n', &mut line).await? != 0 {
        tokio::io::stderr().write_all(&line).await?;
        if let Some(step) = line
            .strip_prefix(b"AI_PHONE_INITIALIZE ")
            .and_then(|value| std::str::from_utf8(value).ok())
            .map(str::trim)
            .filter(|step| diagnostics::STEPS.contains(step))
        {
            receipt.initialization.step = step.into();
            if step == "engine_supervisor" {
                cursor = diagnostics::engine_cursor(&context.root);
            }
            receipt.checkpoint("initialize")?;
        }
        line.clear();
    }
    let bytes = output
        .await
        .map_err(|_| Failure::invalid("unclassified"))??;
    let status = child.wait().await?;
    if !status.success() {
        if ["engine_supervisor", "engine_ready"].contains(&receipt.initialization.step.as_str()) {
            receipt.initialization.engine = diagnostics::engine_report(context, cursor).await;
        }
        return Err(Failure::command(status));
    }
    let ack: Acknowledgement = serde_json::from_slice(&bytes)?;
    if !ack.backend_started
        || ack.phone_ready
        || ack.complete_device_bundle
        || ack.windows_boot_verified
    {
        return Err(Failure::invalid("unclassified"));
    }
    receipt.initialization.step = "complete".into();
    receipt.checkpoint("initialize")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fixture(root: &Path, body: &str, progress: &str, code: u8) -> Context {
        let program = root.join("bootstrap");
        fs::write(&program,format!("#!/bin/sh\nif [ \"$1\" = --service-control ]; then exit 23; fi\ncat > \"$0.request\"\nprintf '%s\\n' '{progress}' >&2\nprintf '%s' '{body}'\nexit {code}\n")).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let mut context = Context::new(root.into(), "a".repeat(32), 1).unwrap();
        context.bootstrap = program;
        context
    }
    #[tokio::test]
    async fn native_initialization_uses_stdin_and_requires_its_complete_acknowledgement() {
        let root = tempfile::tempdir().unwrap();
        let body = r#"{"backend_started":true,"phone_ready":false,"complete_device_bundle":false,"windows_boot_verified":false}"#;
        let context = fixture(root.path(), body, "AI_PHONE_INITIALIZE services_start", 0);
        let mut receipt = Receipt::new(context.nonce.clone(), 1, root.path());
        initialize(&mut receipt, &context).await.unwrap();
        let request: serde_json::Value =
            serde_json::from_slice(&fs::read(root.path().join("bootstrap.request")).unwrap())
                .unwrap();
        assert_eq!(request["phone"]["storage_gb"], 16);
        assert_eq!(receipt.initialization.step, "complete");
        assert_eq!(receipt.status, "failed");
        assert!(receipt.checks.values().all(|checked| !*checked));
        for body in [
            r#"{"backend_started":false,"phone_ready":false,"complete_device_bundle":false,"windows_boot_verified":false}"#,
            "{\"backend_started\":true}",
        ] {
            let context = fixture(root.path(), body, "AI_PHONE_INITIALIZE services_start", 0);
            let mut receipt = Receipt::new(context.nonce.clone(), 1, root.path());
            assert!(initialize(&mut receipt, &context).await.is_err());
            assert_ne!(receipt.initialization.step, "complete");
        }
    }
    #[tokio::test]
    async fn diagnostic_failure_does_not_replace_the_failed_native_command() {
        let root = tempfile::tempdir().unwrap();
        let context = fixture(
            root.path(),
            "",
            "AI_PHONE_INITIALIZE engine_supervisor\nAI_PHONE_INITIALIZE engine_ready",
            17,
        );
        let mut receipt = Receipt::new(context.nonce.clone(), 1, root.path());
        let error = initialize(&mut receipt, &context).await.unwrap_err();
        assert_eq!(error.returncode, Some(17));
        assert_eq!(receipt.initialization.returncode, Some(17));
        assert_eq!(receipt.initialization.step, "engine_ready");
        assert_eq!(
            serde_json::to_value(&receipt.initialization.engine).unwrap()["state"],
            "unavailable"
        );
    }
    #[tokio::test]
    async fn checkpoint_failure_reaps_the_owned_initializer() {
        let root = tempfile::tempdir().unwrap();
        let program = root.path().join("bootstrap");
        fs::write(&program,"#!/bin/sh\necho $$ > \"$0.pid\"\ncat >/dev/null\necho 'AI_PHONE_INITIALIZE preflight' >&2\nexec sleep 30\n").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let mut context = Context::new(root.path().into(), "a".repeat(32), 1).unwrap();
        context.bootstrap = program;
        let blocked = root.path().join("not-a-directory");
        fs::write(&blocked, b"preserved").unwrap();
        let mut receipt = Receipt::new(context.nonce.clone(), 1, &blocked);
        assert!(initialize(&mut receipt, &context).await.is_err());
        let pid = fs::read_to_string(root.path().join("bootstrap.pid")).unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let alive = Command::new("sh")
                .args(["-c", "kill -0 \"$1\"", "qualification", pid.trim()])
                .stderr(Stdio::null())
                .status()
                .await
                .unwrap()
                .success();
            if !alive {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "native child was not reaped"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(fs::read(&blocked).unwrap(), b"preserved");
    }
    #[tokio::test]
    async fn command_failure_and_deadline_are_real_process_outcomes() {
        let root = tempfile::tempdir().unwrap();
        let error = command(root.path(), ["/bin/sh", "-c", "exit 7"], &[], 2)
            .await
            .unwrap_err();
        assert_eq!(error.error, "command_failed");
        assert_eq!(error.returncode, Some(7));
        let error = command(root.path(), ["/bin/sh", "-c", "exec sleep 5"], &[], 0)
            .await
            .unwrap_err();
        assert_eq!(error.error, "subprocess_timeout");
    }
    #[test]
    fn receipt_does_not_serialize_underlying_io_or_private_error_text() {
        let error = Failure::from(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private-token-and-path",
        ));
        assert_eq!(error.error, "permission_denied");
        assert!(
            !serde_json::to_string(&error)
                .unwrap()
                .contains("private-token-and-path")
        );
    }
}
