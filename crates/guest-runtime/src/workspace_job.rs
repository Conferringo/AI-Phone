//! One guest task owns its process group; files outlive the bounded execution.
use crate::Result;
use rustix::process::{Pid, Signal, kill_process_group};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    process::Command,
    signal::unix::{SignalKind, signal},
};
use uuid::Uuid;

pub struct Job {
    folder: PathBuf,
    pid: PathBuf,
    canceled: PathBuf,
}
impl Job {
    pub fn new(workspace: &Path, id: Uuid) -> Self {
        let folder = workspace.join(".jobs");
        Self {
            pid: folder.join(format!("{id}.pid")),
            canceled: folder.join(format!("{id}.cancel")),
            folder,
        }
    }
    pub fn cancel(&self) -> Result<()> {
        fs::create_dir_all(&self.folder)?;
        fs::write(&self.canceled, [])?;
        let text = match fs::read_to_string(&self.pid) {
            Ok(value) => value,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let id = Pid::from_raw(text.trim().parse()?).ok_or("invalid job pid")?;
        stop(id)
    }
    fn publish(&self, id: Pid) -> Result<()> {
        let mut output = tempfile::NamedTempFile::new_in(&self.folder)?;
        write!(output, "{}", id.as_raw_nonzero())?;
        output.persist(&self.pid).map_err(|e| e.error)?;
        Ok(())
    }
    pub async fn run(&self, workspace: &Path, script: &str, limit: Duration) -> Result<u8> {
        let mut term = signal(SignalKind::terminate())?;
        let mut interrupt = signal(SignalKind::interrupt())?;
        fs::create_dir_all(&self.folder)?;
        if self.canceled.try_exists()? {
            remove(&self.canceled)?;
            return Ok(130);
        }
        let mut child = Command::new("/bin/sh")
            .args(["-lc", script])
            .current_dir(workspace)
            .stdin(Stdio::null())
            .process_group(0)
            .kill_on_drop(true)
            .spawn()?;
        let id = child
            .id()
            .and_then(|id| Pid::from_raw(id as i32))
            .ok_or("job pid missing")?;
        let result: Result<u8> = async {
            self.publish(id)?;
            if self.canceled.try_exists()? {
                return Ok(130);
            }
            tokio::select! {
                status = child.wait() => {
                    let status = status?;
                    Ok(if self.canceled.try_exists()? {
                        130
                    } else {
                        status.code().unwrap_or(1) as u8
                    })
                },
                _ = tokio::time::sleep(limit) => Ok(124),
                _ = term.recv() => Ok(130),
                _ = interrupt.recv() => Ok(130),
            }
        }
        .await;
        let killed = stop(id);
        let reaped = child.wait().await;
        let cleaned = remove(&self.pid).and_then(|_| remove(&self.canceled));
        let code = result?;
        killed?;
        reaped?;
        cleaned?;
        Ok(code)
    }
}
fn stop(id: Pid) -> Result<()> {
    match kill_process_group(id, Signal::KILL) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(e) => Err(io::Error::from(e).into()),
    }
}
fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub async fn cli(args: &[OsString]) -> Result<u8> {
    let operation = args
        .first()
        .and_then(|v| v.to_str())
        .ok_or("job operation missing")?;
    let id = Uuid::parse_str(
        args.get(1)
            .and_then(|v| v.to_str())
            .ok_or("job id missing")?,
    )?;
    let root = std::env::current_dir()?;
    let job = Job::new(&root, id);
    match operation {
        "workspace-run" if args.len() == 3 => {
            job.run(
                &root,
                args[2].to_str().ok_or("job script is not UTF-8")?,
                Duration::from_secs(90),
            )
            .await
        }
        "workspace-cancel" if args.len() == 2 => {
            job.cancel()?;
            Ok(0)
        }
        _ => Err("invalid job arguments".into()),
    }
}
