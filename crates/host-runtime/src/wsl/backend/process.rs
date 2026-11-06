//! A startup command owns its pipes and cancellation; committed services outlive it.
use crate::{Error, Result};
use rustix::process::{Pid, Signal, kill_process, kill_process_group};
use std::{
    process::{Output, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::watch,
};

pub(super) trait Execute {
    async fn run(&mut self, command: Command, input: Vec<u8>) -> Result<Output>;
}
pub(super) struct Process(pub watch::Receiver<bool>);

async fn cancelled(stop: &mut watch::Receiver<bool>) {
    while !*stop.borrow_and_update() {
        if stop.changed().await.is_err() {
            return;
        }
    }
}
impl Execute for Process {
    async fn run(&mut self, mut command: Command, input: Vec<u8>) -> Result<Output> {
        if *self.0.borrow() {
            return Err(Error::Cancelled);
        }
        let mut child = command
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let pid = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .and_then(Pid::from_raw)
            .ok_or(Error::Invalid("wsl_initializer_process"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or(Error::Invalid("wsl_initializer_pipe"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(Error::Invalid("wsl_initializer_pipe"))?;
        let outcome = {
            let exchange = async {
                let read = async {
                    let mut bytes = Vec::new();
                    stdout.take(1024 * 1024 + 1).read_to_end(&mut bytes).await?;
                    if bytes.len() > 1024 * 1024 {
                        return Err(std::io::Error::other("initializer output limit"));
                    }
                    Ok(bytes)
                };
                let write = async move {
                    stdin.write_all(&input).await?;
                    stdin.shutdown().await?;
                    drop(stdin);
                    Ok::<_, std::io::Error>(())
                };
                let (status, bytes, ()) = tokio::try_join!(child.wait(), read, write)?;
                Ok::<_, Error>(Output {
                    status,
                    stdout: bytes,
                    stderr: Vec::new(),
                })
            };
            tokio::pin!(exchange);
            tokio::select! {result=&mut exchange=>result,_=cancelled(&mut self.0)=>Err(Error::Cancelled)}
        };
        if outcome.is_err() && child.try_wait()?.is_none() {
            let termination = kill_process(pid, Signal::TERM);
            if let Err(error) = termination
                && error != rustix::io::Errno::SRCH
            {
                return Err(std::io::Error::from(error).into());
            }
            if tokio::time::timeout(Duration::from_secs(10), child.wait())
                .await
                .is_err()
            {
                match kill_process_group(pid, Signal::KILL) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                    Err(error) => return Err(std::io::Error::from(error).into()),
                }
                child.wait().await?;
            }
        }
        outcome
    }
}
