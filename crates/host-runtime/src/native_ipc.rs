//! Bounded length-prefixed protobuf for private native component pipes.
use prost::Message;
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
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
