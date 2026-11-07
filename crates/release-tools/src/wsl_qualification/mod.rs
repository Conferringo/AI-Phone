//! Maintainer PID-1 worker, copied into an owned disposable container; never packaged.
mod diagnostics;
mod probes;
mod process;

use diagnostics::Initialization;
use process::{Context, Failure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const STATE: &str = "/var/lib/aiphone";
const BOOTSTRAP: &str = "/usr/local/libexec/aiphone-bootstrap";
const BUNDLE: &str = "/opt/aiphone/bundle";
const CHECKS: [&str; 11] = [
    "assistant",
    "web",
    "channels",
    "runner_endpoint",
    "database_roles",
    "redis",
    "desktop_frame",
    "work_file",
    "database_marker",
    "desktop_registered",
    "desktop_media",
];

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    nonce: String,
    phase: u8,
}

#[derive(Serialize)]
struct Receipt {
    format: u32,
    nonce: String,
    phase: u8,
    status: &'static str,
    stage: &'static str,
    checks: BTreeMap<&'static str, bool>,
    frame: Value,
    windows_boot_verified: bool,
    phone_ready: bool,
    initialization: Initialization,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<Failure>,
    #[serde(skip)]
    result_path: PathBuf,
}
impl Receipt {
    fn new(nonce: String, phase: u8, state: &Path) -> Self {
        Self {
            format: 1,
            nonce,
            phase,
            status: "failed",
            stage: "guard",
            checks: CHECKS.into_iter().map(|name| (name, false)).collect(),
            frame: json!({}),
            windows_boot_verified: false,
            phone_ready: false,
            initialization: Initialization::new(),
            failure: None,
            result_path: state.join("qualification-result.json"),
        }
    }
    fn checkpoint(&mut self, stage: &'static str) -> Result<()> {
        self.stage = stage;
        atomic(&self.result_path, &self)
    }
    fn checked(&mut self, name: &'static str) {
        *self.checks.get_mut(name).expect("declared check") = true;
    }
}
fn atomic(path: impl AsRef<Path>, value: &impl Serialize) -> Result<()> {
    let path = path.as_ref();
    let mut file = tempfile::NamedTempFile::new_in(path.parent().expect("state directory"))?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|error| Failure::from(error.error))?;
    Ok(())
}
fn hexadecimal(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn fixture_identity() -> Result<String> {
    let nonce = std::env::var("AI_PHONE_WSL_QUALIFICATION_NONCE").unwrap_or_default();
    if !cfg!(target_os = "linux")
        || std::process::id() != 1
        || std::env::var("container").as_deref() != Ok("docker")
        || !Path::new("/.dockerenv").is_file()
        || !hexadecimal(&nonce, 32)
    {
        return Err(Failure::invalid("qualification_environment"));
    }
    Ok(nonce)
}
fn next_phase(path: &Path, nonce: &str) -> Result<u8> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(1),
        Err(error) => return Err(error.into()),
    };
    let marker: Marker = serde_json::from_slice(&bytes)?;
    if marker.nonce != nonce || marker.phase != 1 {
        return Err(Failure::invalid("qualification_marker_owner"));
    }
    Ok(2)
}
async fn nested_cgroups() -> Result<()> {
    prepare_cgroups(Path::new("/sys/fs/cgroup"))?;
    process::command(
        Path::new("/"),
        ["/bin/mount", "--make-rshared", "/"],
        &[],
        15,
    )
    .await?;
    Ok(())
}
fn prepare_cgroups(root: &Path) -> Result<()> {
    if root.join("cgroup.controllers").try_exists()? {
        let controllers = fs::read_to_string(root.join("cgroup.controllers"))?;
        let group = root.join("aiphone-qualification");
        fs::create_dir_all(&group)?;
        for pid in fs::read_to_string(root.join("cgroup.procs"))?.split_whitespace() {
            match fs::write(group.join("cgroup.procs"), pid) {
                Ok(()) => {}
                Err(error) if error.raw_os_error() == Some(3) => {}
                Err(error) => return Err(error.into()),
            }
        }
        fs::write(
            root.join("cgroup.subtree_control"),
            controllers
                .split_whitespace()
                .map(|name| format!("+{name}"))
                .collect::<Vec<_>>()
                .join(" "),
        )?;
    }
    Ok(())
}
async fn shutdown(context: &Context) -> Result<()> {
    for name in ["channels", "media", "web", "assistant", "runner"] {
        context
            .execute(
                [
                    context.bootstrap_str(),
                    "--service-control",
                    context.root_str(),
                    name,
                    "stop",
                ],
                &[],
                120,
            )
            .await?;
    }
    context
        .execute(
            [
                "docker",
                "compose",
                "-f",
                "compose.desktop.yaml",
                "stop",
                "--timeout",
                "15",
                "desktop",
            ],
            &[],
            120,
        )
        .await?;
    context
        .execute(
            [
                "docker",
                "compose",
                "-f",
                "compose.yaml",
                "-f",
                "compose.dev.yaml",
                "stop",
                "--timeout",
                "15",
                "postgres",
                "redis",
            ],
            &[],
            120,
        )
        .await?;
    context
        .execute(
            [
                context.bootstrap_str(),
                "--service-control",
                context.root_str(),
                "wsl-engine",
                "stop",
            ],
            &[],
            120,
        )
        .await?;
    Ok(())
}
async fn perform(receipt: &mut Receipt, context: &Context) -> Result<()> {
    receipt.checkpoint("nesting")?;
    nested_cgroups().await?;
    receipt.checkpoint("initialize")?;
    process::initialize(receipt, context).await?;
    let result: Result<()> = async {
        receipt.checkpoint("readiness")?;
        probes::ready(context, receipt).await?;
        probes::database_roles(context, receipt).await?;
        receipt.checkpoint("desktop")?;
        let device = probes::desktop_registration(context).await?;
        receipt.checked("desktop_registered");
        receipt.frame = probes::desktop_frame(context).await?;
        receipt.checked("desktop_frame");
        probes::desktop_media(context, &device).await?;
        receipt.checked("desktop_media");
        receipt.checkpoint("persistence")?;
        probes::work_file(context).await?;
        receipt.checked("work_file");
        probes::database_marker(context).await?;
        receipt.checked("database_marker");
        Ok(())
    }
    .await;
    if result.is_ok() {
        receipt.checkpoint("stopping")?;
    }
    shutdown(context).await?;
    result?;
    receipt.checkpoint("stopped")
}
pub async fn run() -> crate::Result<Value> {
    let nonce = fixture_identity()?;
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(STATE)?;
    let marker = Path::new(STATE).join("qualification-marker.json");
    let phase = next_phase(&marker, &nonce)?;
    let source = fs::read_to_string(Path::new(BUNDLE).join("source.sha256"))?;
    let source = source.trim();
    if !hexadecimal(source, 64) {
        return Err(Failure::invalid("qualification_identity").into());
    }
    let context = Context::new(
        PathBuf::from(STATE)
            .join("installation/releases")
            .join(source),
        nonce.clone(),
        phase,
    )?;
    let mut receipt = Receipt::new(nonce.clone(), phase, Path::new(STATE));
    receipt.checkpoint("guard")?;
    let result = perform(&mut receipt, &context).await.and_then(|()| {
        if receipt.checks.values().all(|value| *value) {
            Ok(())
        } else {
            Err(Failure::invalid("qualification_failed"))
        }
    });
    match result {
        Ok(()) => {
            atomic(marker, &Marker { nonce, phase })?;
            receipt.status = "passed";
            receipt.checkpoint("stopped")?;
            Ok(json!({"qualification":"passed","phase":phase}))
        }
        Err(error) => {
            receipt.failure = Some(error.clone());
            let stage = receipt.stage;
            receipt.checkpoint(stage)?;
            Err(error.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    pub(super) fn script(program: &Path, content: &str) {
        // The writer exits before exec. Parallel test forks cannot inherit a
        // writable script descriptor and cause ETXTBSY in another test.
        let status = std::process::Command::new("/bin/sh")
            .args(["-c", "printf '%s' \"$2\" > \"$1\"", "fixture-writer"])
            .arg(program)
            .arg(content)
            .status()
            .unwrap();
        assert!(status.success());
        fs::set_permissions(program, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn phase_marker_requires_the_same_owner_and_exact_first_phase() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("marker");
        assert_eq!(next_phase(&marker, "owner").unwrap(), 1);
        atomic(
            &marker,
            &Marker {
                nonce: "owner".into(),
                phase: 1,
            },
        )
        .unwrap();
        assert_eq!(next_phase(&marker, "owner").unwrap(), 2);
        assert!(next_phase(&marker, "other").is_err());
        atomic(
            &marker,
            &Marker {
                nonce: "owner".into(),
                phase: 2,
            },
        )
        .unwrap();
        assert!(next_phase(&marker, "owner").is_err());
    }
    #[test]
    fn initial_receipt_has_no_false_execution_or_completion() {
        let state = tempfile::tempdir().unwrap();
        let value = serde_json::to_value(Receipt::new("a".repeat(32), 1, state.path())).unwrap();
        assert_eq!(value["status"], "failed");
        assert_eq!(value["initialization"]["step"], "not_started");
        assert_eq!(value["checks"].as_object().unwrap().len(), 11);
        assert!(
            value["checks"]
                .as_object()
                .unwrap()
                .values()
                .all(|value| value == false)
        );
        assert_eq!(value["phone_ready"], false);
        assert_eq!(value["windows_boot_verified"], false);
        assert!(value.get("failure").is_none());
    }
    #[test]
    fn private_cgroup_preparation_enables_the_actual_controllers() {
        let temp = tempfile::tempdir().unwrap();
        prepare_cgroups(temp.path()).unwrap();
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
        fs::write(temp.path().join("cgroup.controllers"), b"cpu memory pids\n").unwrap();
        fs::write(temp.path().join("cgroup.procs"), b"123\n456\n").unwrap();
        prepare_cgroups(temp.path()).unwrap();
        assert_eq!(
            fs::read_to_string(temp.path().join("cgroup.subtree_control")).unwrap(),
            "+cpu +memory +pids"
        );
        assert_eq!(
            fs::read_to_string(temp.path().join("aiphone-qualification/cgroup.procs")).unwrap(),
            "456"
        );
    }
}
