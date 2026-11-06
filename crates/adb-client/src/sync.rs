//! ADB SYNC streaming: no host SDK subprocess and no whole-file buffering.
use super::*;
use tokio::io::{AsyncRead, AsyncWrite};

#[derive(Debug)]
pub struct Entry {
    pub name: String,
    pub mode: u32,
    pub bytes: u64,
    pub modified: u64,
}
async fn request(socket: &mut TcpStream, id: &[u8; 4], path: &str) -> Result<()> {
    if path.contains('\0') || path.len() > 4096 {
        return Err(Error::Protocol);
    }
    frame(socket, id, path.len() as u32).await?;
    socket.write_all(path.as_bytes()).await?;
    Ok(())
}
async fn frame(socket: &mut TcpStream, id: &[u8; 4], count: u32) -> Result<()> {
    socket.write_all(id).await?;
    socket.write_all(&count.to_le_bytes()).await?;
    Ok(())
}
async fn header(socket: &mut TcpStream) -> Result<([u8; 4], u32)> {
    let mut id = [0; 4];
    socket.read_exact(&mut id).await?;
    let count = socket.read_u32_le().await?;
    if &id == b"FAIL" {
        if count > 65536 {
            return Err(Error::Limit);
        }
        let mut bytes = vec![0; count as usize];
        socket.read_exact(&mut bytes).await?;
        return Err(Error::Rejected(
            String::from_utf8_lossy(&bytes).into_owned(),
        ));
    }
    Ok((id, count))
}
impl Client {
    pub async fn list(&self, path: &str, deadline: Duration) -> Result<Vec<Entry>> {
        bounded(deadline, async {
            let mut socket = self.device("sync:").await?;
            request(&mut socket, b"LIST", path).await?;
            let mut entries = Vec::new();
            loop {
                let (id, mode) = header(&mut socket).await?;
                if &id == b"DONE" {
                    break;
                }
                if &id != b"DENT" {
                    return Err(Error::Protocol);
                }
                let bytes = socket.read_u32_le().await? as u64;
                let modified = socket.read_u32_le().await? as u64;
                let length = socket.read_u32_le().await? as usize;
                if length > 4096 {
                    return Err(Error::Limit);
                }
                let mut name = vec![0; length];
                socket.read_exact(&mut name).await?;
                let name = String::from_utf8(name).map_err(|_| Error::Protocol)?;
                if name != "." && name != ".." {
                    entries.push(Entry {
                        name,
                        mode,
                        bytes,
                        modified,
                    });
                }
            }
            Ok(entries)
        })
        .await
    }
    pub async fn pull(
        &self,
        path: &str,
        output: &mut (impl AsyncWrite + Unpin),
        deadline: Duration,
    ) -> Result<u64> {
        bounded(deadline, async {
            let mut socket = self.device("sync:").await?;
            request(&mut socket, b"RECV", path).await?;
            let mut buffer = vec![0; 65536];
            let mut total = 0;
            loop {
                let (id, count) = header(&mut socket).await?;
                if &id == b"DONE" {
                    output.flush().await?;
                    return Ok(total);
                }
                if &id != b"DATA" {
                    return Err(Error::Protocol);
                }
                if count > 65536 {
                    return Err(Error::Limit);
                }
                socket.read_exact(&mut buffer[..count as usize]).await?;
                output.write_all(&buffer[..count as usize]).await?;
                total += count as u64;
            }
        })
        .await
    }
    pub async fn push(
        &self,
        path: &str,
        input: &mut (impl AsyncRead + Unpin),
        modified: u32,
        deadline: Duration,
    ) -> Result<u64> {
        bounded(deadline, async {
            let mut socket = self.device("sync:").await?;
            request(&mut socket, b"SEND", &format!("{path},{}", 0o100644)).await?;
            let mut buffer = vec![0; 65536];
            let mut total = 0;
            loop {
                let count = input.read(&mut buffer).await?;
                if count == 0 {
                    break;
                }
                frame(&mut socket, b"DATA", count as u32).await?;
                socket.write_all(&buffer[..count]).await?;
                total += count as u64;
            }
            frame(&mut socket, b"DONE", modified).await?;
            let (id, _) = header(&mut socket).await?;
            if &id != b"OKAY" {
                return Err(Error::Protocol);
            }
            Ok(total)
        })
        .await
    }
}
