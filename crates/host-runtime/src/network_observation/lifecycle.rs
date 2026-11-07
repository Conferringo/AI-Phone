//! Existing core lifecycle. GUI application ownership and saved subscriptions remain intact.
use super::{Client, LIMIT, Observer, Result, adb, invalid, metadata, now, phone_shell};
use serde_json::{Value, json};
use std::{ffi::OsString, fs, os::unix::fs::DirBuilderExt, path::Path, time::Duration};

pub enum Operation {
    Start,
    Stop,
    Resume,
    Handoff,
}
impl std::str::FromStr for Operation {
    type Err = crate::Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "start" => Ok(Self::Start),
            "stop" => Ok(Self::Stop),
            "resume" => Ok(Self::Resume),
            "handoff" => Ok(Self::Handoff),
            _ => Err(invalid("device_network_operation")),
        }
    }
}

pub struct Controller(Observer);
impl Controller {
    pub fn new(root: &Path, kind: &str, id: &str) -> Result<Self> {
        Ok(Self(Observer::new(root, kind, id)?))
    }
    pub async fn execute(&self, operation: Operation) -> Result<Value> {
        // Preserve the former backend invocation's total operation deadline.
        tokio::time::timeout(Duration::from_secs(210), async {
            match operation {
                Operation::Start => {
                    self.start().await?;
                    Ok(json!({"started":true}))
                }
                Operation::Stop => {
                    self.stop().await?;
                    Ok(json!({"stopped":true}))
                }
                Operation::Resume => self.resume().await,
                Operation::Handoff => self.handoff().await,
            }
        })
        .await
        .map_err(|_| invalid("device_network_timeout"))?
    }
    fn directory(&self) -> &'static str {
        if self.0.phone {
            "/data/local/ai-phone-network"
        } else {
            "/etc/ai-phone-network"
        }
    }
    async fn stop(&self) -> Result<()> {
        let dir = self.directory();
        self.0.shell(&format!("rm -f {dir}/enabled; if [ -f {dir}/supervisor.pid ]; then p=$(cat {dir}/supervisor.pid); case \"$p\" in ''|*[!0-9]*) exit 1;; esac; if [ -r /proc/$p/cmdline ] && tr '\\000' ' ' </proc/$p/cmdline | grep -q '{dir}'; then kill -TERM \"$p\"; fi; fi")).await?;
        for _ in 0..40 {
            if self.0.processes().await == "stopped" {
                let route = if self.0.phone {
                    "ip rule del priority 8995 uidrange 10000-99999 table 2022 2>/dev/null || true"
                } else {
                    "ip rule del priority 8990 fwmark 9488 table main 2>/dev/null || true"
                };
                self.0.shell(route).await?;
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        Err(invalid("device_network_stop_unconfirmed"))
    }
    async fn start(&self) -> Result<()> {
        if self.0.application_owned()? {
            return Err(invalid("device_network_application_owned"));
        }
        let dir = self.directory();
        let core = if self.0.phone {
            "/data/local/ai-phone-network/mihomo"
        } else {
            "/opt/ai-phone/network-core/mihomo"
        };
        let current = self.0.processes().await;
        if current == "unknown" {
            return Err(invalid("device_network_state_unknown"));
        }
        if current == "stopped" {
            let mark = if self.0.phone {
                "ip rule show | grep -q '8995:.*uidrange 10000-99999 lookup 2022' || ip rule add priority 8995 uidrange 10000-99999 table 2022; "
            } else {
                "ip rule show | grep -q 'fwmark 0x2510 lookup main' || ip rule add priority 8990 fwmark 9488 table main; "
            };
            self.0.shell(&format!("mkdir -p /dev/net; if [ ! -c /dev/net/tun ]; then mknod /dev/net/tun c 10 200; fi; chmod 600 /dev/net/tun; {mark}touch {dir}/enabled; chmod 600 {dir}/config.json {dir}/enabled; nohup sh {dir}/supervisor.sh {dir} {core} >/dev/null 2>&1 </dev/null &")).await?;
        }
        for _ in 0..30 {
            let health = self.0.health().await;
            if health.running && health.tun {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        Err(invalid("device_network_tun_unconfirmed"))
    }
    async fn resume(&self) -> Result<Value> {
        if self.0.application_owned()? {
            return Ok(json!({"managedBy":"application"}));
        }
        if metadata(&self.0.folder.join("source.json"))?["enabled"] == true {
            if self.0.phone {
                ensure_root(&adb()?).await?;
            }
            let health = self.0.health().await;
            if health.confirmed && !health.running {
                self.start().await?;
            }
        }
        self.0.state().await
    }
    async fn handoff(&self) -> Result<Value> {
        if self.0.application_owned()? {
            return Ok(json!({"managedBy":"application"}));
        }
        let marker = self.0.folder.join("app-managed.json");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.0.folder)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.0.folder)?;
        serde_json::to_writer(
            file.as_file_mut(),
            &json!({"application":if self.0.phone{"Clash Meta"}else{"Clash Party"},"at":now()}),
        )
        .map_err(|_| invalid("device_network_metadata"))?;
        file.persist(&marker).map_err(|e| e.error)?;
        if let Err(error) = self.stop().await {
            fs::remove_file(marker)?;
            return Err(error);
        }
        Ok(json!({"managedBy":"application"}))
    }
}
async fn ensure_root(client: &Client) -> Result<()> {
    if phone_shell(client, "id -u").await? == "0" {
        return Ok(());
    }
    client
        .raw("root:", LIMIT, Duration::from_secs(20))
        .await
        .map_err(|_| invalid("device_network_adb"))?;
    for _ in 0..15 {
        if phone_shell(client, "id -u")
            .await
            .is_ok_and(|value| value == "0")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok(())
}
pub async fn run(args: &[OsString]) -> Result<()> {
    let (fields, owned) = if args.len() == 5 && args[4] == "--stdin-lease" {
        (&args[..4], true)
    } else {
        (args, false)
    };
    if fields.len() != 4 {
        return Err(invalid("device_network_arguments"));
    }
    let text = |i: usize| {
        fields[i]
            .to_str()
            .ok_or_else(|| invalid("device_network_arguments"))
    };
    let controller = Controller::new(Path::new(&fields[0]), text(1)?, text(2)?)?;
    let operation = text(3)?.parse()?;
    let result = if owned {
        crate::native_ipc::while_owned(controller.execute(operation)).await?
    } else {
        controller.execute(operation).await?
    };
    println!("{result}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    async fn request(stream: &mut TcpStream, expected: &str) {
        let mut size = [0; 4];
        stream.read_exact(&mut size).await.unwrap();
        let mut bytes =
            vec![0; usize::from_str_radix(std::str::from_utf8(&size).unwrap(), 16).unwrap()];
        stream.read_exact(&mut bytes).await.unwrap();
        assert_eq!(bytes, expected.as_bytes());
        stream.write_all(b"OKAY").await.unwrap();
    }
    async fn server(replies: &'static [&'static str]) -> (Client, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = Client::new(
            listener.local_addr().unwrap().port(),
            "emulator-5564".into(),
        )
        .unwrap();
        let task = tokio::spawn(async move {
            for reply in replies {
                let (mut stream, _) = listener.accept().await.unwrap();
                request(&mut stream, "host:transport:emulator-5564").await;
                if *reply == "root" {
                    request(&mut stream, "root:").await;
                    stream
                        .write_all(b"restarting adbd as root\n")
                        .await
                        .unwrap();
                } else {
                    request(&mut stream, "shell,v2,raw:id -u").await;
                    let mut eof = [0; 5];
                    stream.read_exact(&mut eof).await.unwrap();
                    assert_eq!(eof, [4, 0, 0, 0, 0]);
                    stream.write_u8(1).await.unwrap();
                    stream.write_u32_le(reply.len() as u32).await.unwrap();
                    stream.write_all(reply.as_bytes()).await.unwrap();
                    stream.write_all(&[3, 1, 0, 0, 0, 0]).await.unwrap();
                }
            }
        });
        (client, task)
    }
    #[tokio::test]
    async fn existing_root_is_observed_without_restarting_adbd() {
        let (client, task) = server(&["0"]).await;
        ensure_root(&client).await.unwrap();
        task.await.unwrap();
    }
    #[tokio::test]
    async fn adb_restart_is_sent_once_then_observed_before_network_recovery() {
        let (client, task) = server(&["2000", "root", "0"]).await;
        ensure_root(&client).await.unwrap();
        task.await.unwrap();
    }
}
