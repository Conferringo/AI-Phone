use crate::{Error, Result};
use std::{
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub(super) struct Adb {
    pub port: u16,
    pub serial: String,
}
impl Adb {
    async fn request(&self, service: &str, limit: usize) -> Result<Vec<u8>> {
        bounded(async {
            let mut socket =
                TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))).await?;
            socket.set_nodelay(true)?;
            command(&mut socket, &format!("host:transport:{}", self.serial)).await?;
            command(&mut socket, service).await?;
            let mut bytes = Vec::new();
            socket
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .await?;
            if bytes.len() > limit {
                return Err(Error::Invalid("android_adb_output"));
            }
            Ok(bytes)
        })
        .await
    }
    pub async fn uid(&self) -> Result<u32> {
        let bytes = self.request("shell:id -u", 128).await?;
        std::str::from_utf8(&bytes)
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .ok_or(Error::Invalid("android_root_identity"))
    }
    pub async fn boot_completed(&self) -> Result<bool> {
        Ok(self
            .request("shell:getprop sys.boot_completed", 128)
            .await?
            .trim_ascii()
            == b"1")
    }
    pub async fn root(&self) -> Result<()> {
        let bytes = self.request("root:", 4096).await?;
        if bytes
            .windows(b"cannot run as root".len())
            .any(|v| v == b"cannot run as root")
        {
            return Err(Error::Invalid("android_root_unavailable"));
        }
        Ok(())
    }
    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        self.request("exec:screencap -p", 8 * 1024 * 1024).await
    }
    pub async fn server(&self) -> Result<()> {
        bounded(async {
            let mut socket =
                TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))).await?;
            command(&mut socket, "host:version").await?;
            let mut size = [0; 4];
            socket.read_exact(&mut size).await?;
            let count = std::str::from_utf8(&size)
                .ok()
                .and_then(|s| usize::from_str_radix(s, 16).ok())
                .filter(|n| *n <= 16)
                .ok_or(Error::Invalid("android_adb_frame"))?;
            let mut version = vec![0; count];
            socket.read_exact(&mut version).await?;
            if version.is_empty() || !version.iter().all(u8::is_ascii_hexdigit) {
                return Err(Error::Invalid("android_adb_frame"));
            }
            Ok(())
        })
        .await
    }
}

async fn bounded<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(3), future)
        .await
        .map_err(|_| Error::Invalid("android_adb_unknown"))?
}
async fn command(socket: &mut TcpStream, service: &str) -> Result<()> {
    socket
        .write_all(format!("{:04x}{service}", service.len()).as_bytes())
        .await?;
    let mut status = [0; 4];
    socket.read_exact(&mut status).await?;
    match &status {
        b"OKAY" => Ok(()),
        b"FAIL" => Err(Error::Invalid("android_adb_rejected")),
        _ => Err(Error::Invalid("android_adb_frame")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn selects_the_exact_serial_before_a_bounded_raw_uid_request() {
        let server = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = server.local_addr().unwrap().port();
        let peer = tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            for expected in ["host:transport:emulator-5582", "shell:id -u"] {
                let mut length = [0; 4];
                socket.read_exact(&mut length).await.unwrap();
                let size =
                    usize::from_str_radix(std::str::from_utf8(&length).unwrap(), 16).unwrap();
                let mut bytes = vec![0; size];
                socket.read_exact(&mut bytes).await.unwrap();
                assert_eq!(bytes, expected.as_bytes());
                socket.write_all(b"OKAY").await.unwrap();
            }
            socket.write_all(b"0\r\n").await.unwrap();
        });
        let result = Adb {
            port,
            serial: "emulator-5582".into(),
        }
        .uid()
        .await;
        if result.is_err() {
            peer.abort();
            let _ = peer.await;
        } else {
            peer.await.unwrap();
        }
        assert_eq!(result.unwrap(), 0);
    }
}
