use crate::{Error, Result};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, sync::watch};

pub(super) trait Device {
    async fn execute(&mut self, args: &[&str], seconds: u64) -> Result<String>;
    fn cancelled(&self) -> bool;
}
pub(super) struct Adb {
    pub program: PathBuf,
    pub client: ai_phone_adb::Client,
    pub serial: String,
    pub stop: watch::Receiver<bool>,
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
impl Device for Adb {
    fn cancelled(&self) -> bool {
        *self.stop.borrow()
    }
    async fn execute(&mut self, args: &[&str], seconds: u64) -> Result<String> {
        if self.cancelled() {
            return Err(Error::Cancelled);
        }
        let result = if args.first() == Some(&"shell") {
            let text = args[1..]
                .iter()
                .map(|a| quote(a))
                .collect::<Vec<_>>()
                .join(" ");
            let output = tokio::select! {biased;
                _=self.stop.changed()=>return Err(Error::Cancelled),
                result=self.client.shell(&text,ai_phone_adb::Limits{stdout:65536,stderr:4096},Duration::from_secs(seconds))=>result.map_err(|_|Error::Invalid("phone_input_command_unknown"))?,
            };
            if output.code != 0 {
                return Err(Error::Invalid("phone_input_command_unknown"));
            }
            output.stdout
        } else {
            let mut child = Command::new(&self.program)
                .args(["-P", "15037", "-s", &self.serial])
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()?;
            let mut reader = child
                .stdout
                .take()
                .ok_or(Error::Invalid("phone_input_command_unknown"))?;
            let collect = async {
                let mut body = Vec::new();
                (&mut reader).take(65537).read_to_end(&mut body).await?;
                if body.len() > 65536 || !child.wait().await?.success() {
                    return Err(Error::Invalid("phone_input_command_unknown"));
                }
                Ok(body)
            };
            let result = tokio::select! {biased;
                _=self.stop.changed()=>Err(Error::Cancelled),
                result=tokio::time::timeout(Duration::from_secs(seconds),collect)=>result.unwrap_or(Err(Error::Invalid("phone_input_command_unknown"))),
            };
            if result.is_err() {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
            result?
        };
        String::from_utf8(result)
            .map(|s| s.trim().to_owned())
            .map_err(|_| Error::Invalid("phone_input_observation"))
    }
}
