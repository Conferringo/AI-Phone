use super::{Context, Record, hex, invalid};
use crate::Result;
use serde::Deserialize;
use std::{collections::HashMap, env, ffi::OsString, path::Path, process::Stdio, time::Duration};
use tokio::process::{Child, Command};
pub(super) struct Engine {
    program: OsString,
    network: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct Container {
    pub id: String,
    name: String,
    image: String,
    config: Config,
    state: State,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Config {
    labels: HashMap<String, String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct State {
    running: bool,
}
impl Container {
    pub fn owned(&self, record: &Record, key: &str) -> Result<()> {
        if self.image != record.image_id
            || !record.container_id.is_empty() && self.id != record.container_id
            || [
                ("dev.aiphone.component", "channel-browser"),
                ("dev.aiphone.profile", key),
                ("dev.aiphone.launch", record.token.as_str()),
            ]
            .iter()
            .any(|(k, v)| self.config.labels.get(*k).map(String::as_str) != Some(*v))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn mount(profile: &Path, readonly: bool) -> Result<String> {
    let value = format!("source={}", profile.to_str().ok_or_else(invalid)?);
    let field = if value.contains([',', '"']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    };
    Ok(format!(
        "type=bind,{field},target=/profile{}",
        if readonly { ",readonly" } else { "" }
    ))
}
impl Engine {
    pub fn from_env() -> Result<Self> {
        let network = env::var("AI_PHONE_BROWSER_NETWORK").unwrap_or_else(|_| "bridge".into());
        if network != "bridge" && network != "none" {
            return Err(invalid());
        }
        Ok(Self {
            program: env::var_os("AI_PHONE_DOCKER").unwrap_or_else(|| "docker".into()),
            network,
        })
    }
    async fn execute(&self, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new(&self.program);
        cmd.args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(15), cmd.output())
            .await
            .map_err(|_| invalid())??;
        if !output.status.success() || output.stdout.len() > 1024 * 1024 {
            return Err(invalid());
        }
        Ok(String::from_utf8(output.stdout)
            .map_err(|_| invalid())?
            .trim()
            .to_owned())
    }
    pub async fn inspect(&self, selector: &str, name: Option<&str>) -> Result<Option<Container>> {
        let id = self
            .execute(&[
                "container",
                "ls",
                "--all",
                "--no-trunc",
                "--filter",
                selector,
                "--format",
                "{{.ID}}",
            ])
            .await?;
        if id.is_empty() {
            return Ok(None);
        }
        if !hex(&id, 64) {
            return Err(invalid());
        }
        let mut rows: Vec<Container> =
            serde_json::from_str(&self.execute(&["inspect", &id]).await?).map_err(|_| invalid())?;
        if rows.len() != 1 {
            return Err(invalid());
        }
        let row = rows.remove(0);
        if row.id != id || name.is_some_and(|v| row.name != format!("/{v}")) {
            return Err(invalid());
        }
        Ok(Some(row))
    }
    pub async fn check(&self, image: &str) -> Result<String> {
        let version = self
            .execute(&[
                "run",
                "--rm",
                "--network",
                "none",
                "--pull",
                "never",
                "--cap-drop",
                "ALL",
                "--entrypoint",
                "/usr/bin/chromium",
                image,
                "--version",
            ])
            .await?;
        let number = version
            .strip_prefix("Chromium ")
            .and_then(|v| v.split_whitespace().next())
            .ok_or_else(invalid)?;
        let parts: Vec<_> = number.split('.').collect();
        if parts.len() != 4
            || parts
                .iter()
                .any(|v| v.is_empty() || !v.bytes().all(|v| v.is_ascii_digit()))
        {
            return Err(invalid());
        }
        Ok(version)
    }
    pub async fn create(
        &self,
        context: &Context,
        record: &Record,
        args: &[String],
    ) -> Result<String> {
        let readonly = mount(&context.profile, true)?;
        let user = self
            .execute(&[
                "run",
                "--rm",
                "--network",
                "none",
                "--pull",
                "never",
                "--cap-drop",
                "ALL",
                "--mount",
                &readonly,
                "--entrypoint",
                "/usr/bin/stat",
                &context.image,
                "-c",
                "%u:%g",
                "/profile",
            ])
            .await?;
        let ids: Vec<_> = user.split(':').collect();
        if ids.len() != 2
            || ids
                .iter()
                .any(|v| v.parse::<u32>().map_or(true, |n| n == u32::MAX))
        {
            return Err(invalid());
        }
        let mount = mount(&context.profile, false)?;
        let profile = format!("dev.aiphone.profile={}", context.key);
        let token = format!("dev.aiphone.launch={}", record.token);
        // Fixed descriptor wiring only: CDP bytes use fd 3/4, never stdout logging or a TCP port.
        const LAUNCH: &str = "exec 3<&0 4>&1; exec /usr/bin/chromium --headless=new --no-sandbox --remote-debugging-pipe --user-data-dir=/profile --disable-background-networking \"$@\" about:blank >/dev/null 2>/dev/null";
        let mut command = vec![
            "create",
            "--rm",
            "--init",
            "--interactive",
            "--name",
            &context.name,
            "--hostname",
            &context.name,
            "--pull",
            "never",
            "--network",
            &self.network,
            "--memory",
            "768m",
            "--cpus",
            "1",
            "--shm-size",
            "128m",
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "--mount",
            &mount,
            "--user",
            &user,
            "--env",
            "XDG_CONFIG_HOME=/profile/.config",
            "--env",
            "XDG_CACHE_HOME=/profile/.cache",
            "--env",
            "XDG_DATA_HOME=/profile/.local/share",
            "--label",
            "dev.aiphone.component=channel-browser",
            "--label",
            &profile,
            "--label",
            &token,
            "--entrypoint",
            "/bin/sh",
            &context.image,
            "-c",
            LAUNCH,
            "--",
        ];
        command.extend(args.iter().map(String::as_str));
        let id = self.execute(&command).await?;
        if !hex(&id, 64) {
            return Err(invalid());
        }
        Ok(id)
    }
    pub fn attach(&self, id: &str) -> Result<Child> {
        Ok(Command::new(&self.program)
            .args(["start", "--attach", "--interactive", id])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?)
    }
    pub async fn remove(&self, container: &Container) -> Result<()> {
        if container.state.running {
            self.execute(&["stop", "--time", "5", &container.id])
                .await?;
        }
        if let Err(error) = self.execute(&["rm", "--force", &container.id]).await
            && self
                .inspect(&format!("id={}", container.id), None)
                .await?
                .is_some()
        {
            return Err(error);
        }
        Ok(())
    }
}
