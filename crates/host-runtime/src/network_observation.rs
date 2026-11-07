//! Read-side device networking. GUI applications and existing configuration retain ownership.
use crate::{Error, Result};
use ai_phone_adb::{Client, Limits};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
const LIMIT: usize = 2 * 1024 * 1024;
pub struct Observer {
    root: PathBuf,
    folder: PathBuf,
    phone: bool,
    id: String,
}
fn invalid(code: &'static str) -> Error {
    Error::Invalid(code)
}
fn metadata(path: &Path) -> Result<Value> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(json!({})),
        Err(e) => return Err(e.into()),
    };
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| invalid("device_network_metadata"))?;
    if !value.is_object() {
        return Err(invalid("device_network_metadata"));
    }
    Ok(value)
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
async fn output(mut command: Command, input: Option<&[u8]>, seconds: u64) -> Result<String> {
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    tokio::time::timeout(Duration::from_secs(seconds), async {
        if let Some(bytes) = input {
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(bytes).await?;
        }
        let mut stdout = child.stdout.take().unwrap();
        let collect = async {
            let mut bytes = vec![];
            let mut part = [0; 8192];
            loop {
                let n = stdout.read(&mut part).await?;
                if n == 0 {
                    break;
                }
                if bytes.len() + n > LIMIT {
                    return Err(io::Error::other(invalid("device_network_output_limit")));
                }
                bytes.extend_from_slice(&part[..n]);
            }
            Ok::<_, io::Error>(bytes)
        };
        let (bytes, status) = tokio::try_join!(collect, child.wait())?;
        if !status.success() {
            return Err(invalid("device_network_command"));
        }
        String::from_utf8(bytes)
            .map(|v| v.trim().to_owned())
            .map_err(|_| invalid("device_network_response"))
    })
    .await
    .map_err(|_| invalid("device_network_timeout"))?
}
impl Observer {
    pub fn new(root: &Path, kind: &str, id: &str) -> Result<Self> {
        crate::network_paths::validate(kind, id)?;
        let root = root.canonicalize()?;
        let folder = root.join("data/device-network").join(id);
        Ok(Self {
            root,
            folder,
            phone: kind == "phone",
            id: id.into(),
        })
    }
    pub fn application_owned(&self) -> Result<bool> {
        self.folder
            .join("app-managed.json")
            .try_exists()
            .map_err(Into::into)
    }
    async fn docker(&self, args: &[&str], input: Option<&[u8]>, seconds: u64) -> Result<String> {
        let mut command =
            Command::new(std::env::var_os("AI_PHONE_DOCKER").unwrap_or_else(|| "docker".into()));
        command.args(args).current_dir(&self.root);
        output(command, input, seconds).await
    }
    async fn shell(&self, text: &str) -> Result<String> {
        if self.phone {
            let value = Client::new(15037, "emulator-5564".into())
                .map_err(|_| invalid("device_network_adb"))?
                .shell(
                    text,
                    Limits {
                        stdout: LIMIT,
                        stderr: 4096,
                    },
                    Duration::from_secs(20),
                )
                .await
                .map_err(|_| invalid("device_network_adb"))?;
            if value.code != 0 {
                return Err(invalid("device_network_command"));
            }
            String::from_utf8(value.stdout)
                .map(|v| v.trim().to_owned())
                .map_err(|_| invalid("device_network_response"))
        } else {
            self.docker(
                &["exec", "-i", "ai-phone-desktop", "sh", "-c", text],
                None,
                20,
            )
            .await
        }
    }
    async fn controller(&self, path: &str) -> Result<Value> {
        let secret = if self.phone {
            fs::read_to_string(self.folder.join("token"))?
        } else {
            metadata(&self.folder.join("config.json"))?["secret"]
                .as_str()
                .ok_or_else(|| invalid("device_network_secret"))?
                .to_owned()
        };
        let header =
            serde_json::to_string(&format!("Authorization: Bearer {}", secret.trim())).unwrap();
        let url = serde_json::to_string(&format!("http://127.0.0.1:19090{path}")).unwrap();
        let config = format!(
            "noproxy = \"*\"\nfail\nsilent\nshow-error\nmax-time = 2\nheader = {header}\nurl = {url}\n"
        );
        let response = if self.phone {
            self.shell(&format!("printf '%s' {} | curl --config -", quote(&config)))
                .await?
        } else {
            self.docker(
                &["exec", "-i", "ai-phone-desktop", "curl", "--config", "-"],
                Some(config.as_bytes()),
                5,
            )
            .await?
        };
        serde_json::from_str(&response).map_err(|_| invalid("device_network_response"))
    }
    async fn processes(&self) -> String {
        let dir = if self.phone {
            "/data/local/ai-phone-network"
        } else {
            "/etc/ai-phone-network"
        };
        let command = format!(
            "for f in supervisor core; do if [ -r {dir}/$f.pid ]; then p=$(cat {dir}/$f.pid); case \"$p\" in ''|*[!0-9]*) continue;; esac; if [ -r /proc/$p/cmdline ] && tr '\\000' ' ' </proc/$p/cmdline | grep -q '{dir}'; then echo running; exit 0; fi; fi; done; echo stopped"
        );
        match self.shell(&command).await {
            Ok(value) if value == "running" || value == "stopped" => value,
            _ => "unknown".into(),
        }
    }
    async fn health(&self) -> Health {
        let observed: Result<Health> = async {
            let (version, config) =
                tokio::try_join!(self.controller("/version"), self.controller("/configs"))?;
            let link = self
                .shell("ip link show aiphone0 >/dev/null 2>&1 && echo active")
                .await?;
            Ok(Health {
                running: true,
                confirmed: true,
                version: version["version"].as_str().unwrap_or("").into(),
                tun: config["tun"]["enable"] == true && link == "active",
            })
        }
        .await;
        match observed {
            Ok(value) => value,
            Err(_) => {
                let actual = self.processes().await;
                Health {
                    running: actual == "running",
                    confirmed: actual == "stopped",
                    version: String::new(),
                    tun: false,
                }
            }
        }
    }
    async fn state(&self) -> Result<Value> {
        let source = metadata(&self.folder.join("source.json"))?;
        let job = metadata(&self.folder.join("job.json"))?;
        let checks = metadata(&self.folder.join("checks.json"))?;
        if self.application_owned()? {
            let command = if self.phone {
                "pidof com.github.metacubex.clash.meta:background >/dev/null && echo app; ip link show tun0 >/dev/null 2>&1 && echo tun; true"
            } else {
                "pgrep -x mihomo >/dev/null && echo app; ip link show Mihomo >/dev/null 2>&1 && echo tun; true"
            };
            let result = self.shell(command).await;
            let confirmed = result.is_ok();
            let content = result.unwrap_or_default();
            let running = content.lines().any(|v| v == "app");
            let tun = content.lines().any(|v| v == "tun");
            return Ok(
                json!({"deviceId":self.id,"managedBy":"application","revision":source["revision"].as_u64().unwrap_or(0),"enabled":running,"hasSubscription":source["subscriptionUrl"].as_str().is_some_and(|v|!v.is_empty()),"status":if !confirmed{"unknown"}else if running{if tun{"running"}else{"application"}}else{"off"},"error":if confirmed{""}else{"设备状态暂未确认"},"coreVersion":"","tunActive":tun,"nodeCount":source["nodeCount"].as_u64().unwrap_or(0),"checkedAtMs":checks["at"].as_u64().unwrap_or(0),"checks":checks["items"].as_array().cloned().unwrap_or_default()}),
            );
        }
        let health = self.health().await;
        let committed = job["requestId"]
            .as_str()
            .is_some_and(|id| !id.is_empty() && source["requestId"].as_str() == Some(id));
        let alive = job["pid"]
            .as_i64()
            .and_then(|p| i32::try_from(p).ok())
            .and_then(rustix::process::Pid::from_raw)
            .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok());
        let interrupted = job["status"] == "applying" && !committed && !alive;
        let status = if interrupted {
            "interrupted"
        } else if job["status"] == "applying" && !committed {
            "applying"
        } else if !health.confirmed {
            "unknown"
        } else if health.running {
            if health.tun { "running" } else { "degraded" }
        } else if source["enabled"] == true {
            "failed"
        } else {
            "off"
        };
        let job_error = job["error"].as_str().unwrap_or("");
        let error = if interrupted {
            "网络应用中断，请检查设备状态后重试"
        } else if !committed && !job_error.is_empty() {
            job_error
        } else if !health.confirmed {
            "设备网络状态暂未确认，请稍后重试"
        } else if !health.running && source["enabled"] == true {
            "设备内 Clash 未运行"
        } else if health.running && !health.tun {
            "TUN 未生效"
        } else {
            ""
        };
        Ok(
            json!({"deviceId":self.id,"revision":source["revision"].as_u64().unwrap_or(0),"enabled":source["enabled"].as_bool().unwrap_or(false),"hasSubscription":source["subscriptionUrl"].as_str().is_some_and(|v|!v.is_empty()),"status":status,"error":error,"coreVersion":health.version,"tunActive":health.tun,"nodeCount":source["nodeCount"].as_u64().unwrap_or(0),"checkedAtMs":checks["at"].as_u64().unwrap_or(0),"checks":if checks["revision"]==source["revision"]{checks["items"].as_array().cloned().unwrap_or_default()}else{vec![]}}),
        )
    }
    async fn probe(&self) -> Result<Value> {
        let source = metadata(&self.folder.join("source.json"))?;
        let pin = metadata(&self.root.join("config/mihomo-release.json"))?;
        let mut items = vec![];
        for (name, url) in [
            ("国内 HTTPS", "https://www.baidu.com/"),
            ("境外 HTTPS", "https://example.com/"),
            (
                "下载",
                "https://raw.githubusercontent.com/MetaCubeX/mihomo/v1.19.33/LICENSE",
            ),
        ] {
            let start = tokio::time::Instant::now();
            let checked: Result<u64> = async {
                let mut bytes = [0; 8];
                getrandom::fill(&mut bytes).map_err(|_| invalid("device_network_random"))?;
                let suffix = u64::from_ne_bytes(bytes);
                let path = if self.phone {
                    format!("/data/local/tmp/aiphone-netprobe-{suffix:016x}")
                } else {
                    format!("/etc/ai-phone-network/probe-{suffix:016x}.part")
                };
                let publish = if self.phone { "" } else {
                    "mv \"$f\" /etc/ai-phone-network/probe-download && f=/etc/ai-phone-network/probe-download && "
                };
                let umask = if self.phone { "umask 077; " } else { "" };
                let bound = if self.phone { "" } else { "--max-filesize 1048576 " };
                let command = format!("{umask}f={}; curl --noproxy '*' -fsSL --max-time 15 {bound}--url {} -o \"$f\" && {publish}{{ wc -c < \"$f\"; sha256sum \"$f\"; }}; status=$?; rm -f {}; exit $status", quote(&path), quote(url), quote(&path));
                let command = if self.phone { format!("su 2000 sh -c {}", quote(&command)) } else { command };
                let result = self.shell(&command).await?;
                let mut parts = result.split_whitespace();
                let count: u64 = parts.next().ok_or_else(|| invalid("device_network_response"))?
                    .parse().map_err(|_| invalid("device_network_response"))?;
                let checksum = parts.next().ok_or_else(|| invalid("device_network_response"))?;
                if count < 30 || (name == "下载" && (pin["verification_download"]["sha256"] != checksum
                    || pin["verification_download"]["size_bytes"] != count)) {
                    return Err(invalid("device_network_response"));
                }
                Ok(count)
            }.await;
            let (passed, detail) = match checked {
                Ok(bytes) => (true, format!("{bytes} bytes")),
                Err(_) => (false, "设备内直连请求失败；未使用宿主代理".into()),
            };
            items.push(json!({"name":name,"passed":passed,"detail":detail,"durationMs":start.elapsed().as_millis() as u64}));
        }
        fs::create_dir_all(&self.folder)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.folder)?;
        let mut checks = json!({"at":now(),"items":items});
        if let Some(revision) = source.get("revision") {
            checks["revision"] = revision.clone();
        }
        serde_json::to_writer(file.as_file_mut(), &checks)
            .map_err(|_| invalid("device_network_metadata"))?;
        file.persist(self.folder.join("checks.json"))
            .map_err(|e| e.error)?;
        self.state().await
    }
    pub async fn read(&self, operation: &str) -> Result<Value> {
        match operation {
            "state" => self.state().await,
            "probe" => self.probe().await,
            "credential" => {
                let source = metadata(&self.folder.join("source.json"))?;
                Ok(
                    json!({"subscriptionUrl":source["subscriptionUrl"].as_str().unwrap_or(""),"revision":source["revision"].as_u64().unwrap_or(0)}),
                )
            }
            "health" => serde_json::to_value(self.health().await)
                .map_err(|_| invalid("device_network_response")),
            "processes" => Ok(Value::String(self.processes().await)),
            _ => Err(invalid("device_network_operation")),
        }
    }
}
#[derive(serde::Serialize)]
struct Health {
    running: bool,
    confirmed: bool,
    version: String,
    tun: bool,
}
pub async fn run(args: &[OsString]) -> Result<()> {
    if args.len() != 4 {
        return Err(invalid("device_network_arguments"));
    }
    let arg = |i: usize| {
        args[i]
            .to_str()
            .ok_or_else(|| invalid("device_network_arguments"))
    };
    let value = Observer::new(Path::new(&args[0]), arg(1)?, arg(2)?)?
        .read(arg(3)?)
        .await?;
    println!("{value}");
    Ok(())
}
