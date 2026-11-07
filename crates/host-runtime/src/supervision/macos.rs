//! User launchd owns committed processes; setup never restarts a running job.
use crate::{Error, Result, filesystem};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, io::Read, path::Path};
use tokio::process::Command;

const SERVICES: [(&str, &str); 7] = [
    ("emulator", "dev.aiphone.emulator.p2"),
    ("runner", "dev.aiphone.runner.p2"),
    ("assistant", "dev.aiphone.assistant"),
    ("channels", "dev.aiphone.channels"),
    ("media", "dev.aiphone.device-stream"),
    ("web", "dev.aiphone.web.p2"),
    ("lan", "dev.aiphone.lan"),
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    command: Vec<String>,
    environment: BTreeMap<String, String>,
}
fn label(name: &str) -> Result<&'static str> {
    SERVICES
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
        .ok_or(Error::Invalid("service_control_arguments"))
}
fn domain() -> String {
    format!("gui/{}", rustix::process::getuid().as_raw())
}
fn job_state(root: &Path, body: &str) -> Result<bool> {
    let field = |key: &str| body.lines().find_map(|line| line.trim().strip_prefix(key));
    let directory = field("working directory = ").ok_or(Error::Invalid("foreign_installation"))?;
    let directory = Path::new(directory).canonicalize()?;
    if directory != root && directory != root.join("apps/web") {
        return Err(Error::Invalid("foreign_installation"));
    }
    Ok(field("state = ") == Some("running"))
}
async fn status(root: &Path, label: &str) -> Result<Option<bool>> {
    let output = Command::new("/bin/launchctl")
        .args(["print", &format!("{}/{label}", domain())])
        .output()
        .await?;
    if output.status.code() == Some(113) {
        return Ok(None);
    }
    if !output.status.success() {
        return Err(Error::Invalid("service_observation_failed"));
    }
    Ok(Some(job_state(
        root,
        &String::from_utf8_lossy(&output.stdout),
    )?))
}
pub(crate) async fn preflight(root: &Path) -> Result<()> {
    for (_, label) in SERVICES {
        status(root, label).await?;
    }
    Ok(())
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn plist(root: &Path, label: &str, name: &str, spec: &Spec) -> Result<Vec<u8>> {
    if spec.command.first().is_none_or(String::is_empty)
        || spec
            .command
            .iter()
            .any(|value| value.chars().any(|ch| ch < ' '))
        || spec
            .environment
            .iter()
            .any(|(key, value)| key.contains(['=', '\0']) || value.contains('\0'))
    {
        return Err(Error::Invalid("service_spec"));
    }
    let args = spec
        .command
        .iter()
        .map(|value| format!("<string>{}</string>", xml(value)))
        .collect::<String>();
    let environment = spec
        .environment
        .iter()
        .map(|(key, value)| format!("<key>{}</key><string>{}</string>", xml(key), xml(value)))
        .collect::<String>();
    let directory = xml(root.to_str().ok_or(Error::Invalid("service_path"))?);
    let log = xml(root
        .join(format!("data/services/{name}.log"))
        .to_str()
        .ok_or(Error::Invalid("service_path"))?);
    Ok(format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>{label}</string><key>ProgramArguments</key><array>{args}</array><key>WorkingDirectory</key><string>{directory}</string><key>EnvironmentVariables</key><dict>{environment}</dict><key>RunAtLoad</key><true/><key>KeepAlive</key><true/><key>ThrottleInterval</key><integer>5</integer><key>StandardOutPath</key><string>{log}</string><key>StandardErrorPath</key><string>{log}</string></dict></plist>").into_bytes())
}
async fn launch(args: &[&std::ffi::OsStr]) -> Result<()> {
    let status = Command::new("/bin/launchctl").args(args).status().await?;
    if !status.success() {
        return Err(Error::Invalid("service_start_failed"));
    }
    Ok(())
}
pub(super) async fn run(arguments: &[OsString]) -> Result<()> {
    let [root, name, operation] = arguments else {
        return Err(Error::Invalid("service_control_arguments"));
    };
    let root = Path::new(root).canonicalize()?;
    let name = name
        .to_str()
        .ok_or(Error::Invalid("service_control_arguments"))?;
    let label = label(name)?;
    let observed = status(&root, label).await?;
    let value: Value = match operation.to_str() {
        Some("status") => observed
            .map(|running| json!({"running":running}))
            .unwrap_or(Value::Null),
        Some("start") => {
            let mut bytes = vec![];
            std::io::stdin()
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024 {
                return Err(Error::Invalid("service_spec"));
            }
            let spec: Spec =
                serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("service_spec"))?;
            let body = plist(&root, label, name, &spec)?;
            if observed != Some(true) {
                let folder = root.join("data/services");
                filesystem::directory(&folder, true)?;
                let _lock =
                    filesystem::lease(&folder.join(format!("{name}.lock")))?.ok_or(Error::Busy)?;
                if observed == Some(false) {
                    launch(&[
                        std::ffi::OsStr::new("kickstart"),
                        std::ffi::OsStr::new(&format!("{}/{label}", domain())),
                    ])
                    .await?;
                } else {
                    let file = format!("{name}.plist");
                    filesystem::publish_private(&folder, &file, &body, false)?;
                    launch(&[
                        std::ffi::OsStr::new("bootstrap"),
                        std::ffi::OsStr::new(&domain()),
                        folder.join(file).as_os_str(),
                    ])
                    .await?;
                }
            }
            json!({"started":observed!=Some(true)})
        }
        Some("stop") => {
            if observed.is_some() {
                launch(&[
                    std::ffi::OsStr::new("bootout"),
                    std::ffi::OsStr::new(&format!("{}/{label}", domain())),
                ])
                .await?;
            }
            Value::Null
        }
        _ => return Err(Error::Invalid("service_control_arguments")),
    };
    println!("{}", value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn foreign_job_is_rejected_and_running_owned_job_is_observed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        assert!(
            job_state(
                &root,
                &format!("working directory = {}\nstate = running", root.display())
            )
            .unwrap()
        );
        assert!(
            !job_state(
                &root,
                &format!("working directory = {}\nstate = waiting", root.display())
            )
            .unwrap()
        );
        assert!(job_state(&root, "working directory = /\nstate = running").is_err());
        assert!(job_state(&root, "state = running").is_err());
    }
    #[test]
    fn launchd_spec_preserves_argv_and_escapes_xml() {
        let spec = Spec {
            command: vec!["/Application Support/bun".into(), "a<&\".js".into()],
            environment: BTreeMap::from([("PATH".into(), "/only/owned/bin".into())]),
        };
        let body = String::from_utf8(
            plist(
                Path::new("/Application Support/app"),
                "dev.aiphone.web.p2",
                "web",
                &spec,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(body.contains("<string>a&lt;&amp;&quot;.js</string>"));
        assert!(body.contains("<key>KeepAlive</key><true/>"));
        assert!(!body.contains("python"));
        assert!(label("unknown").is_err());
        use std::io::Write;
        let mut check = std::process::Command::new("/usr/bin/plutil")
            .args(["-lint", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        check
            .stdin
            .take()
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
        assert!(check.wait().unwrap().success());
    }
}
