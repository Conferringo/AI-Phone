//! The workspace container and its native guest worker; no SQL or model policy.
use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    process::Stdio,
    time::Duration,
};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
type Result<T> = std::result::Result<T, String>;
pub(crate) struct Runtime<'a> {
    pub container: &'a str,
    pub image: &'a str,
    pub directory: PathBuf,
    pub skills: PathBuf,
}

#[cfg(debug_assertions)]
pub(crate) async fn qualify(image: &str) -> Result<()> {
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = temp.path().canonicalize().map_err(|e| e.to_string())?;
    let container = format!("aiphone-workspace-qa-{}", Uuid::now_v7());
    let directory = root.join("workspace");
    let skills = root.join("skills");
    std::fs::create_dir_all(directory.join(".home")).map_err(|e| e.to_string())?;
    std::fs::create_dir(&skills).map_err(|e| e.to_string())?;
    let runtime = Runtime {
        container: &container,
        image,
        directory,
        skills,
    };
    let mut ready = None;
    let result = async {
        runtime.prepare(&mut ready).await?;
        let original = ready.clone();
        std::fs::write(runtime.directory.join("user-file"), "retain")
            .map_err(|e| e.to_string())?;
        runtime.require(&["exec", "-u", "0", &container, "rm", "/usr/local/bin/desktop-bridge"])
            .await?;
        let legacy_arguments = ["exec", "-w", "/workspace", &container, "/bin/sh", "-c",
            "echo started > legacy-ready; while [ ! -f release-legacy ]; do sleep 0.02; done; printf retained > legacy-result"];
        let legacy = runtime.command(&legacy_arguments, 4096, 20);
        let upgrade = async {
            wait_file(&runtime.directory.join("legacy-ready")).await?;
            ready = None;
            runtime.prepare(&mut ready).await?;
            if ready != original {
                return Err("workspace upgrade restarted its container".into());
            }
            std::fs::write(runtime.directory.join("release-legacy"), "continue")
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(())
        };
        let (old, upgraded) = tokio::join!(legacy, upgrade);
        upgraded?;
        if !old?.0 {
            return Err("legacy job was interrupted".into());
        }
        if std::fs::read_to_string(runtime.directory.join("legacy-result"))
            .map_err(|e| e.to_string())? != "retained" {
            return Err("legacy artifact mismatch".into());
        }
        let token = CancellationToken::new();
        let (success, output) = runtime.execute("printf native; printf result > output.txt", &token).await?;
        if !success || output != "native" || std::fs::read_to_string(runtime.directory.join("output.txt"))
            .map_err(|e| e.to_string())? != "result" {
            return Err("native task output mismatch".into());
        }
        let stop = CancellationToken::new();
        let cancel = async {
            wait_file(&runtime.directory.join("running")).await?;
            stop.cancel();
            Ok::<_, String>(())
        };
        let (task, canceled) = tokio::join!(
            runtime.execute("printf started > running; sleep 20; printf wrong > late-output", &stop),
            cancel
        );
        canceled?;
        if task.as_ref().err().map(String::as_str) != Some("canceled") {
            return Err("native task did not cancel".into());
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while std::fs::read_dir(runtime.directory.join(".jobs"))
            .map_err(|e| e.to_string())?.next().is_some() {
            if tokio::time::Instant::now() >= deadline {
                return Err("canceled job did not clean up".into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if runtime.directory.join("late-output").exists()
            || std::fs::read_to_string(runtime.directory.join("user-file"))
                .map_err(|e| e.to_string())? != "retain" {
            return Err("workspace data changed".into());
        }
        let unknown = CancellationToken::new();
        let break_control = async {
            wait_file(&runtime.directory.join("failure-ready")).await?;
            runtime.require(&["exec", "-u", "0", &container, "rm", "/usr/local/bin/desktop-bridge"])
                .await?;
            unknown.cancel();
            Ok::<_, String>(())
        };
        let (task, broken) = tokio::join!(
            runtime.execute("printf started > failure-ready; sleep 20", &unknown),
            break_control
        );
        broken?;
        if !task.as_ref().err().is_some_and(|e| e.starts_with("取消尚未确认")) {
            return Err(format!("failed cancellation was reported as {task:?}"));
        }
        println!("workspace runtime: fresh start, in-place upgrade, running legacy task, native output, cancellation and unknown-outcome receipts passed");
        Ok::<_, String>(())
    }.await;
    let cleanup = runtime.require(&["rm", "-f", &container]).await;
    result?;
    cleanup?;
    Ok(())
}
#[cfg(debug_assertions)]
async fn wait_file(path: &std::path::Path) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        if tokio::time::Instant::now() >= deadline {
            return Err("workspace task did not start".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(())
}
async fn drain(
    mut reader: impl tokio::io::AsyncRead + Unpin,
    bytes: &mut Vec<u8>,
    limit: usize,
) -> std::io::Result<()> {
    let mut chunk = [0; 8192];
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        let keep = n.min(limit.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&chunk[..keep]);
    }
}
impl Runtime<'_> {
    async fn command(&self, args: &[&str], limit: usize, seconds: u64) -> Result<(bool, String)> {
        let mut child = crate::desktop::docker()
            .args(args)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("无法启动容器命令：{e}"))?;
        let out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();
        let (mut a, mut b) = (vec![], vec![]);
        let (_, _, status) = tokio::time::timeout(Duration::from_secs(seconds), async {
            tokio::try_join!(
                drain(out, &mut a, limit),
                drain(err, &mut b, limit),
                child.wait()
            )
        })
        .await
        .map_err(|_| "工作指令超时，请先检查结果，勿直接重放".to_string())?
        .map_err(|e| e.to_string())?;
        a.extend(b);
        a.truncate(limit);
        Ok((status.success(), String::from_utf8_lossy(&a).into_owned()))
    }
    async fn require(&self, args: &[&str]) -> Result<String> {
        let (ok, text) = self.command(args, 4096, 20).await?;
        if !ok {
            return Err(text);
        }
        Ok(text)
    }
    async fn worker(&self) -> Result<()> {
        let args = [
            "exec",
            self.container,
            "/usr/local/bin/desktop-bridge",
            "--workspace-job-contract",
        ];
        let (ok, value) = self.command(&args, 4096, 10).await?;
        if ok && value.trim() == ai_phone_api::WORKSPACE_JOB_CONTRACT {
            return Ok(());
        }
        // Existing tools containers keep their installed tools and running jobs.
        // Replace only this reserved program, by rename, so executing inodes survive.
        let binary = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        let status = tokio::time::timeout(
            Duration::from_secs(20),
            crate::desktop::docker()
                .args([
                    "run",
                    "--rm",
                    "--pull=never",
                    "--network=none",
                    "--read-only",
                    "--cap-drop=ALL",
                    "--entrypoint",
                    "/bin/cat",
                    self.image,
                    "/usr/local/bin/desktop-bridge",
                ])
                .stdout(binary.reopen().map_err(|e| e.to_string())?)
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .status(),
        )
        .await
        .map_err(|_| "读取工作区原生组件超时".to_string())?
        .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("无法读取工作区原生任务组件".into());
        }
        binary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        let staged = format!("/usr/local/bin/.desktop-bridge-{}", Uuid::now_v7());
        let destination = format!("{}:{staged}", self.container);
        let installation = async {
            self.require(&[
                "cp",
                binary.path().to_str().ok_or("组件路径不是UTF-8")?,
                &destination,
            ])
            .await?;
            self.require(&[
                "exec",
                "-u",
                "0",
                self.container,
                "mv",
                "-f",
                &staged,
                "/usr/local/bin/desktop-bridge",
            ])
            .await?;
            let value = self.require(&args).await?;
            if value.trim() != ai_phone_api::WORKSPACE_JOB_CONTRACT {
                return Err("工作区原生组件版本不匹配".into());
            }
            Ok(())
        }
        .await;
        if installation.is_err() {
            let _ = self
                .command(
                    &["exec", "-u", "0", self.container, "rm", "-f", &staged],
                    4096,
                    10,
                )
                .await;
        }
        installation
    }
    pub async fn prepare(&self, ready: &mut Option<String>) -> Result<()> {
        let (found, state) = self
            .command(
                &[
                    "inspect",
                    "--format",
                    "{{.Id}} {{.State.Running}}",
                    self.container,
                ],
                4096,
                10,
            )
            .await?;
        let id = if found {
            let (id, running) = state.trim().split_once(' ').ok_or("工作环境状态无效")?;
            if running != "true" {
                self.require(&["start", self.container]).await?;
            }
            id.to_owned()
        } else {
            let owner = std::fs::metadata(&self.directory).map_err(|e| e.to_string())?;
            let user = format!("{}:{}", owner.uid(), owner.gid());
            let work = format!("type=bind,src={},dst=/workspace", self.directory.display());
            let skills = format!(
                "type=bind,src={},dst=/skills,readonly",
                self.skills.display()
            );
            let proxy = std::env::var("AI_PHONE_CONTAINER_PROXY").unwrap_or_default();
            let http = format!("http_proxy={proxy}");
            let https = format!("https_proxy={proxy}");
            let upper_http = format!("HTTP_PROXY={proxy}");
            let upper_https = format!("HTTPS_PROXY={proxy}");
            self.require(&[
                "run",
                "--pull=never",
                "-d",
                "--name",
                self.container,
                "--label",
                "dev.aiphone.managed=true",
                "--user",
                &user,
                "--cap-drop=ALL",
                "--security-opt=no-new-privileges",
                "--pids-limit=256",
                "--memory=2g",
                "--cpus=2",
                "--mount",
                &work,
                "--mount",
                &skills,
                "--env",
                "HOME=/workspace/.home",
                "--env",
                &http,
                "--env",
                &https,
                "--env",
                &upper_http,
                "--env",
                &upper_https,
                "--env",
                "NO_PROXY=localhost,127.0.0.1",
                "--env",
                "PIP_USER=1",
                "--env",
                "PYTHONUNBUFFERED=1",
                self.image,
            ])
            .await?
            .trim()
            .to_owned()
        };
        if ready.as_ref() != Some(&id) {
            self.worker().await?;
            *ready = Some(id);
        }
        Ok(())
    }
    pub async fn execute(
        &self,
        script: &str,
        cancel: &CancellationToken,
    ) -> Result<(bool, String)> {
        let job = Uuid::now_v7().to_string();
        let arguments = [
            "exec",
            "-w",
            "/workspace",
            self.container,
            "/usr/local/bin/desktop-bridge",
            "workspace-run",
            &job,
            script,
        ];
        tokio::select! {
          result=self.command(&arguments,65536,100)=>result,
          _=cancel.cancelled()=>{
            match self.command(&["exec","-w","/workspace",self.container,"/usr/local/bin/desktop-bridge","workspace-cancel",&job],4096,10).await {
              Ok((true,_))=>Err("canceled".into()),
              Ok((false,error))|Err(error)=>Err(format!("取消尚未确认；结果可能未知：{error}")),
            }
          }
        }
    }
}
