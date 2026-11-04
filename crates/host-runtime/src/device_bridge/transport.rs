use prost::Message;
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
pub(super) const LIMIT: usize = 512;
pub(super) fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "device_bridge_frame")
}
pub(super) async fn read<T: Message + Default>(
    stream: &mut (impl AsyncRead + Unpin),
) -> io::Result<T> {
    let size = stream.read_u32().await? as usize;
    if size > LIMIT {
        return Err(invalid());
    }
    let mut data = vec![0; size];
    stream.read_exact(&mut data).await?;
    T::decode(data.as_slice()).map_err(|_| invalid())
}
pub(super) async fn write<T: Message>(
    stream: &mut (impl AsyncWrite + Unpin),
    message: &T,
) -> io::Result<()> {
    let bytes = message.encode_to_vec();
    if bytes.len() > LIMIT {
        return Err(invalid());
    }
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await
}
#[cfg(test)]
mod tests {
    use super::*;
    use ai_phone_api::wire::DeviceBridgeOpened;
    use tokio::io::AsyncWriteExt;
    #[tokio::test]
    async fn frames_are_bounded_complete_and_leave_following_bytes_untouched() {
        let (mut writer, mut reader) = tokio::io::duplex(1024);
        write(&mut writer, &DeviceBridgeOpened { accepted: true })
            .await
            .unwrap();
        writer.write_all(b"unchanged raw payload").await.unwrap();
        assert!(
            read::<DeviceBridgeOpened>(&mut reader)
                .await
                .unwrap()
                .accepted
        );
        use tokio::io::AsyncReadExt;
        let mut rest = [0; 21];
        reader.read_exact(&mut rest).await.unwrap();
        assert_eq!(&rest, b"unchanged raw payload");
    }
    #[tokio::test]
    async fn oversized_outbound_frame_does_not_publish_a_partial_header() {
        use ai_phone_api::wire::DeviceBridgeOpen;
        let (mut output, mut input) = tokio::io::duplex(1024);
        let oversized = DeviceBridgeOpen {
            secret: vec![7; LIMIT + 1],
            ..Default::default()
        };
        assert!(write(&mut output, &oversized).await.is_err());
        drop(output);
        let mut bytes = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut input, &mut bytes)
            .await
            .unwrap();
        assert!(bytes.is_empty());
    }
    #[tokio::test]
    async fn rejects_oversized_truncated_and_invalid_frames() {
        for data in [
            ((LIMIT + 1) as u32).to_be_bytes().to_vec(),
            vec![0, 0, 0, 2, 255],
            vec![0, 0, 0, 1, 255],
        ] {
            let mut input = data.as_slice();
            assert!(read::<DeviceBridgeOpened>(&mut input).await.is_err());
        }
    }
}
