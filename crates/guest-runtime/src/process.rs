//! Bounded OS tool execution. No retry; cancellation kills the owned process group.
use crate::{Error, Result};
use std::{ffi::OsString, future::Future, path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

#[derive(Clone, Copy, Debug)]
pub enum Capture {
    Bounded(usize),
    Tail(usize),
    Discard,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub input: Vec<u8>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
    pub timeout: Duration,
    pub capture: Capture,
}

impl Request {
    pub fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(OsString::from).collect(),
            input: vec![],
            cwd: None,
            env: vec![],
            timeout: Duration::from_secs(8),
            capture: Capture::Bounded(65536),
        }
    }
}

#[derive(Debug)]
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait Runner: Send + Sync {
    fn execute(&self, request: Request) -> impl Future<Output = Result<Output>> + Send;
}

#[derive(Clone, Copy, Default)]
pub struct OsRunner;

pub struct GroupGuard(Option<rustix::process::Pid>);
impl GroupGuard {
    pub fn for_child(child: &tokio::process::Child) -> Result<Self> {
        Ok(Self(child_pid(child)?))
    }
    pub fn disarm(&mut self) {
        self.0 = None;
    }

    fn kill(&mut self) -> Result<()> {
        let Some(pid) = self.0.take() else {
            return Ok(());
        };
        match rustix::process::kill_process_group(pid, rustix::process::Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
impl Drop for GroupGuard {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

fn command(request: &Request) -> Command {
    let mut command = Command::new(&request.program);
    command
        .args(&request.args)
        .envs(request.env.iter().cloned())
        .stdin(Stdio::piped())
        .kill_on_drop(true);
    command.stdout(if matches!(request.capture, Capture::Discard) {
        Stdio::null()
    } else {
        Stdio::piped()
    });
    command.stderr(if matches!(request.capture, Capture::Discard) {
        Stdio::null()
    } else {
        Stdio::piped()
    });
    if let Some(path) = &request.cwd {
        command.current_dir(path);
    }
    command.process_group(0);
    command
}

async fn capture(mut source: impl AsyncRead + Unpin, policy: Capture) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let count = source.read(&mut chunk).await?;
        if count == 0 {
            return Ok(result);
        }
        append(&mut result, &chunk[..count], policy)?;
    }
}

fn append(result: &mut Vec<u8>, chunk: &[u8], policy: Capture) -> Result<()> {
    match policy {
        Capture::Discard => (),
        Capture::Bounded(limit) => {
            if result.len().saturating_add(chunk.len()) > limit {
                return Err("结果超出大小限制".into());
            }
            result.extend_from_slice(chunk);
        }
        Capture::Tail(limit) => {
            result.extend_from_slice(chunk);
            if result.len() > limit {
                result.drain(..result.len() - limit);
            }
        }
    }
    Ok(())
}

async fn input(mut pipe: tokio::process::ChildStdin, bytes: Vec<u8>) -> Result<()> {
    pipe.write_all(&bytes).await?;
    pipe.shutdown().await?;
    Ok(())
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(1))
}

async fn collect(child: &mut tokio::process::Child, request: &Request) -> Result<Output> {
    let stdin = child.stdin.take().ok_or("命令没有输入管道")?;
    let stderr = child.stderr.take();
    let stdout = child.stdout.take();
    let read = async {
        match stdout {
            Some(pipe) => capture(pipe, request.capture).await,
            None => Ok(vec![]),
        }
    };
    let errors = async {
        match stderr {
            Some(pipe) => capture(pipe, Capture::Tail(65536)).await,
            None => Ok(vec![]),
        }
    };
    let (_, stdout, stderr) = tokio::try_join!(input(stdin, request.input.clone()), read, errors)?;
    let code = exit_code(child.wait().await?);
    Ok(Output {
        code,
        stdout,
        stderr,
    })
}

fn outcome(
    result: std::result::Result<Result<Output>, tokio::time::error::Elapsed>,
) -> Result<Output> {
    result.map_err(|_| Error::from("操作超时，结果可能未知，请先观察或核对文件"))?
}

impl Runner for OsRunner {
    async fn execute(&self, request: Request) -> Result<Output> {
        let mut child = command(&request).spawn()?;
        let id = child.id().ok_or("命令没有进程编号")?;
        let mut group = GroupGuard(rustix::process::Pid::from_raw(i32::try_from(id)?));
        let result =
            outcome(tokio::time::timeout(request.timeout, collect(&mut child, &request)).await);
        if result.is_ok() {
            // Successful tools may deliberately leave a clipboard owner or requested GUI application.
            group.0 = None;
            return result;
        }
        let cleanup = stop_group(&mut child, &mut group, Duration::ZERO).await;
        preserve(result, cleanup)
    }
}

pub fn checked(output: Output) -> Result<Vec<u8>> {
    if output.code == 0 {
        return Ok(output.stdout);
    }
    let error = if output.stderr.is_empty() {
        output.stdout
    } else {
        output.stderr
    };
    Err(String::from_utf8_lossy(&error)
        .chars()
        .take(2000)
        .collect::<String>()
        .into())
}

fn child_pid(child: &tokio::process::Child) -> Result<Option<rustix::process::Pid>> {
    child
        .id()
        .map(|id| {
            rustix::process::Pid::from_raw(i32::try_from(id)?)
                .ok_or_else(|| "无效子进程编号".into())
        })
        .transpose()
}

/// Observe without reaping: the leader continues to pin the process-group ID.
pub fn exit_observed(child: &tokio::process::Child) -> Result<bool> {
    let Some(pid) = child_pid(child)? else {
        return Ok(true);
    };
    let flags = rustix::process::WaitIdOptions::EXITED
        | rustix::process::WaitIdOptions::NOHANG
        | rustix::process::WaitIdOptions::NOWAIT;
    Ok(
        rustix::process::waitid(rustix::process::WaitId::Pid(pid), flags)
            .map_err(|error| format!("process.observe: {error}"))?
            .is_some(),
    )
}

pub async fn wait_observed(child: &tokio::process::Child, interval: Duration) -> Result<()> {
    while !exit_observed(child)? {
        tokio::time::sleep(interval).await;
    }
    Ok(())
}

fn signal_error(group: bool, value: rustix::process::Signal, error: rustix::io::Errno) -> Error {
    format!(
        "process.signal.{scope}.{value:?}: {error}",
        scope = if group { "group" } else { "child" }
    )
    .into()
}

async fn signal(
    child: &tokio::process::Child,
    pid: rustix::process::Pid,
    group: bool,
    value: rustix::process::Signal,
) -> Result<()> {
    let result = if group {
        rustix::process::kill_process_group(pid, value)
    } else {
        rustix::process::kill_process(pid, value)
    };
    match result {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(rustix::io::Errno::PERM) => {
            let primary = signal_error(group, value, rustix::io::Errno::PERM);
            match quiescent_after_permission_error(child, pid, group).await {
                Ok(true) => Ok(()),
                Ok(false) => Err(primary),
                Err(probe) => preserve(Err(primary), Err(probe)),
            }
        }
        Err(error) => Err(signal_error(group, value, error)),
    }
}

#[cfg(not(target_os = "macos"))]
async fn quiescent_after_permission_error(
    _child: &tokio::process::Child,
    _pid: rustix::process::Pid,
    _group: bool,
) -> Result<bool> {
    Ok(false)
}

#[cfg(target_os = "macos")]
async fn quiescent_after_permission_error(
    child: &tokio::process::Child,
    pid: rustix::process::Pid,
    group: bool,
) -> Result<bool> {
    if !group || !exit_observed(child)? {
        return Ok(false);
    }
    if child_pid(child)? != Some(pid) {
        return Err("process.quiescence.identity_changed".into());
    }
    // NOWAIT keeps this owned leader unreaped while the fixed cold-path query runs.
    group_snapshot(pid).await
}

#[cfg(target_os = "macos")]
async fn group_snapshot(pid: rustix::process::Pid) -> Result<bool> {
    let mut child = Command::new("/bin/ps")
        .args(["-axo", "pid=,pgid=,stat="])
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let result = tokio::time::timeout(Duration::from_secs(2), snapshot_bytes(&mut child))
        .await
        .map_err(|_| Error::from("process.quiescence.query_timeout"))
        .and_then(|result| result);
    match result {
        Ok(bytes) => quiet_table(&bytes, pid.as_raw_nonzero().get()),
        Err(primary) => preserve(Err(primary), stop_probe(&mut child).await),
    }
}

#[cfg(target_os = "macos")]
async fn snapshot_bytes(child: &mut tokio::process::Child) -> Result<Vec<u8>> {
    let pipe = child
        .stdout
        .take()
        .ok_or("process.quiescence.missing_stdout")?;
    let bytes = capture(pipe, Capture::Bounded(1024 * 1024)).await?;
    if !child.wait().await?.success() {
        return Err("process.quiescence.query_failed".into());
    }
    Ok(bytes)
}

#[cfg(target_os = "macos")]
async fn stop_probe(child: &mut tokio::process::Child) -> Result<()> {
    if child.id().is_none() {
        return Ok(());
    }
    let killed = child.start_kill().map_err(Error::from);
    let waited = child.wait().await.map(|_| ()).map_err(Error::from);
    preserve(killed, waited)
}

#[cfg(any(target_os = "macos", test))]
fn quiet_table(bytes: &[u8], group: i32) -> Result<bool> {
    for line in std::str::from_utf8(bytes)?
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let (_, pgid, state) = process_row(line)?;
        if pgid == group && !state.starts_with('Z') {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(any(target_os = "macos", test))]
fn process_row(line: &str) -> Result<(i32, i32, &str)> {
    let fields: Vec<_> = line.split_whitespace().take(4).collect();
    let [pid, group, state] = fields.as_slice() else {
        return Err("process.quiescence.invalid_row".into());
    };
    let pid = pid.parse::<i32>()?;
    let group = group.parse::<i32>()?;
    if pid < 0 || group < 0 || !state.is_ascii() {
        return Err("process.quiescence.invalid_row".into());
    }
    Ok((pid, group, *state))
}

async fn wait_grace(child: &tokio::process::Child, grace: Duration) -> Result<bool> {
    let mut exited = exit_observed(child)?;
    let deadline = tokio::time::Instant::now() + grace;
    while !exited && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
        exited = exit_observed(child)?;
    }
    Ok(exited)
}

pub async fn stop_owned(
    child: &mut tokio::process::Child,
    group: bool,
    grace: Duration,
) -> Result<()> {
    let Some(pid) = child_pid(child)? else {
        return Ok(());
    };
    // Reject an already reaped/unowned PID before any process-group signal.
    exit_observed(child)?;
    let term = signal(child, pid, group, rustix::process::Signal::TERM).await;
    let observed = wait_grace(child, grace).await;
    let kill = match &observed {
        Ok(exited) if group || !exited => {
            signal(child, pid, group, rustix::process::Signal::KILL).await
        }
        Ok(_) => Ok(()),
        Err(_) => Err("无法保持子进程身份；未向未知进程组发送信号".into()),
    };
    let waited = child.wait().await.map(|_| ()).map_err(Error::from);
    preserve(preserve(term, observed.map(|_| ())), preserve(kill, waited))
}

/// Disarm even on cleanup error: a reaped leader must never be signalled again.
pub async fn stop_group(
    child: &mut tokio::process::Child,
    guard: &mut GroupGuard,
    grace: Duration,
) -> Result<()> {
    let result = stop_owned(child, true, grace).await;
    guard.disarm();
    result
}

pub fn preserve<T>(primary: Result<T>, cleanup: Result<()>) -> Result<T> {
    match (primary, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(primary), Err(cleanup)) => Err(format!("{primary}；清理失败：{cleanup}").into()),
    }
}

#[cfg(test)]
mod quiescence_tests {
    use super::*;

    #[test]
    fn fixed_membership_table_never_calls_a_live_or_unknown_group_quiet() {
        assert!(quiet_table(b"", 42).unwrap());
        assert!(quiet_table(b"42 42 Z\n43 42 Z+\n99 90 R\n", 42).unwrap());
        assert!(!quiet_table(b"42 42 Z\n43 42 S\n", 42).unwrap());
        assert!(!quiet_table(b"42 42 Z\n43 42 T\n", 42).unwrap());
        for bytes in [
            b"42 nope Z".as_slice(),
            b"42 42",
            b"42 42 Z extra",
            b"-1 42 Z",
            &[255],
        ] {
            assert!(quiet_table(bytes, 42).is_err());
        }
    }

    #[test]
    fn zero_work_fixture() {}

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn real_darwin_exited_leader_is_reaped_without_swallowing_live_groups() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process::quiescence_tests::zero_work_fixture"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut guard = GroupGuard::for_child(&child).unwrap();
        tokio::time::timeout(
            Duration::from_secs(2),
            wait_observed(&child, Duration::from_millis(10)),
        )
        .await
        .unwrap()
        .unwrap();
        stop_group(&mut child, &mut guard, Duration::from_millis(50))
            .await
            .unwrap();
        assert!(child.id().is_none());
    }
}
