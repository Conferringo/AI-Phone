use crate::Result;
use std::{
    fs::File,
    io,
    os::{fd::OwnedFd, unix::fs::FileTypeExt},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, copy},
    net::{
        UnixStream,
        unix::pipe::{Receiver, Sender},
    },
    process::Child,
};
type Input = Box<dyn AsyncRead + Unpin>;
type Output = Box<dyn AsyncWrite + Unpin>;
fn socket(file: File) -> io::Result<UnixStream> {
    let stream = std::os::unix::net::UnixStream::from(OwnedFd::from(file));
    stream.set_nonblocking(true)?;
    UnixStream::from_std(stream)
}
// The executable adapter maps Puppeteer's fd 3/4 onto stdin/stdout before exec.
// Bun uses socket pairs; the native process boundary also supports Unix pipes.
pub(super) fn descriptors() -> io::Result<(Input, Output)> {
    let input = File::from(rustix::io::dup(std::io::stdin())?);
    let output = File::from(rustix::io::dup(std::io::stdout())?);
    let input: Input = if input.metadata()?.file_type().is_socket() {
        Box::new(socket(input)?)
    } else {
        Box::new(Receiver::from_file(input)?)
    };
    let output: Output = if output.metadata()?.file_type().is_socket() {
        Box::new(socket(output)?)
    } else {
        Box::new(Sender::from_file(output)?)
    };
    Ok((input, output))
}
pub(super) async fn relay(mut child: Child, mut input: Input, mut output: Output) -> Result<()> {
    let mut stdin = child.stdin.take().ok_or_else(super::invalid)?;
    let mut stdout = child.stdout.take().ok_or_else(super::invalid)?;
    let outgoing = copy(&mut stdout, &mut output);
    tokio::pin!(outgoing);
    let drained = tokio::select! {_=copy(&mut input,&mut stdin)=>false,_=&mut outgoing=>true,_=crate::component::interrupt()=>false,_=child.wait()=>false};
    drop(stdin);
    let flush = async {
        if !drained {
            let _ = outgoing.await;
        }
        child.wait().await
    };
    if tokio::time::timeout(Duration::from_secs(3), flush)
        .await
        .is_err()
    {
        if let Some(pid) = child
            .id()
            .and_then(|v| rustix::process::Pid::from_raw(v as i32))
        {
            rustix::process::kill_process(pid, rustix::process::Signal::TERM)
                .map_err(io::Error::from)?;
        }
        if tokio::time::timeout(Duration::from_secs(1), child.wait())
            .await
            .is_err()
        {
            child.kill().await?;
        }
    }
    Ok(())
}
