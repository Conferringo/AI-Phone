//! Private component protobuf framing and cancellable parent input.
use prost::Message;
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
#[cfg(unix)]
pub(crate) fn owner_input() -> crate::Result<OwnerInput> {
    use std::os::{
        fd::{AsFd, OwnedFd},
        unix::fs::FileTypeExt,
    };
    // Unlike blocking stdin, this lease releases immediately when its owner exits.
    let file = std::fs::File::from(std::io::stdin().as_fd().try_clone_to_owned()?);
    let kind = file.metadata()?.file_type();
    let input: OwnedFd = file.into();
    if kind.is_fifo() {
        Ok(OwnerInput::Pipe(
            tokio::net::unix::pipe::Receiver::from_owned_fd(input)?,
        ))
    } else if kind.is_socket() {
        let stream = std::os::unix::net::UnixStream::from(input);
        stream.set_nonblocking(true)?;
        Ok(OwnerInput::Socket(tokio::net::UnixStream::from_std(
            stream,
        )?))
    } else {
        Err(crate::Error::Invalid("native_owner_input"))
    }
}
#[cfg(unix)]
pub(crate) enum OwnerInput {
    Pipe(tokio::net::unix::pipe::Receiver),
    Socket(tokio::net::UnixStream),
}
#[cfg(unix)]
pub(crate) async fn while_owned<T>(
    work: impl std::future::Future<Output = crate::Result<T>>,
) -> crate::Result<T> {
    let mut input = owner_input()?;
    let mut byte = [0];
    tokio::select! {biased;
        _=input.read(&mut byte)=>Err(crate::Error::Cancelled),
        result=work=>result,
    }
}
#[cfg(unix)]
impl AsyncRead for OwnerInput {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Pipe(input) => std::pin::Pin::new(input).poll_read(context, buffer),
            Self::Socket(input) => std::pin::Pin::new(input).poll_read(context, buffer),
        }
    }
}
pub(crate) const LIMIT: usize = 512;
pub(crate) fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "device_bridge_frame")
}
pub(crate) async fn read<T: Message + Default>(
    stream: &mut (impl AsyncRead + Unpin),
) -> io::Result<T> {
    read_bounded(stream, LIMIT).await
}
pub async fn read_bounded<T: Message + Default>(
    stream: &mut (impl AsyncRead + Unpin),
    limit: usize,
) -> io::Result<T> {
    let size = stream.read_u32().await? as usize;
    if size > limit {
        return Err(invalid());
    }
    let mut data = vec![0; size];
    stream.read_exact(&mut data).await?;
    T::decode(data.as_slice()).map_err(|_| invalid())
}
pub(crate) async fn write<T: Message>(
    stream: &mut (impl AsyncWrite + Unpin),
    message: &T,
) -> io::Result<()> {
    write_bounded(stream, message, LIMIT).await
}
pub async fn write_bounded<T: Message>(
    stream: &mut (impl AsyncWrite + Unpin),
    message: &T,
    limit: usize,
) -> io::Result<()> {
    let bytes = message.encode_to_vec();
    if bytes.len() > limit {
        return Err(invalid());
    }
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await
}
