//! Startup waits for actual local service responses, not successful process spawning.
use crate::{Error, Result};
use ai_phone_api::wire::DeviceRequest;
use prost::Message;
use std::{fs, path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::watch,
    time::Instant,
};

async fn probe(
    port: u16,
    path: &str,
    token: &str,
    body: Option<&[u8]>,
    content: &str,
) -> Result<()> {
    let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let method = if body.is_some() { "POST" } else { "GET" };
    let body = body.unwrap_or_default();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/x-protobuf\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    let mut bytes = vec![];
    loop {
        let mut chunk = [0; 1024];
        let count = stream.read(&mut chunk).await?;
        bytes.extend_from_slice(&chunk[..count]);
        if count == 0 || bytes.windows(4).any(|value| value == b"\r\n\r\n") {
            break;
        }
        if bytes.len() > 8192 {
            return Err(Error::Invalid("unix_host_service_response"));
        }
    }
    let header = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    if !header.starts_with("http/1.1 200 ") || !header.contains(&format!("content-type: {content}"))
    {
        return Err(Error::Invalid("unix_host_service_not_ready"));
    }
    Ok(())
}
pub(super) async fn wait(root: &Path, mut stop: watch::Receiver<bool>) -> Result<()> {
    let token = fs::read_to_string(root.join("data/assistant/token"))?;
    let runner = fs::read_to_string(root.join("data/phone-runner/token"))?;
    let cursor = DeviceRequest {
        phone_id: "phone-local-01".into(),
        owner_id: "local-user".into(),
        ..Default::default()
    }
    .encode_to_vec();
    let deadline = Instant::now() + Duration::from_secs(30);
    for (port, path, auth, body, content) in [
        (
            17863,
            "/v1/setup/status",
            token.trim(),
            Some(&b""[..]),
            "application/x-protobuf",
        ),
        (17860, "/phone", token.trim(), None, "text/html"),
        (
            17864,
            "/v1/list",
            token.trim(),
            Some(&b""[..]),
            "application/x-protobuf",
        ),
        (
            17862,
            "/v1/cursor",
            runner.trim(),
            Some(cursor.as_slice()),
            "application/x-protobuf",
        ),
    ] {
        loop {
            if *stop.borrow() {
                return Err(Error::Cancelled);
            }
            let result = tokio::select! {
                result=tokio::time::timeout_at(deadline,probe(port,path,auth,body,content))=>result,
                _=stop.changed()=>return Err(Error::Cancelled),
            };
            match result {
                Ok(Ok(())) => break,
                Err(_) => return Err(Error::Invalid("unix_host_service_not_ready")),
                Ok(Err(_)) => {}
            }
            if Instant::now() >= deadline {
                return Err(Error::Invalid("unix_host_service_not_ready"));
            }
            tokio::select! {_=stop.changed()=>return Err(Error::Cancelled),_=tokio::time::sleep(Duration::from_millis(250))=>{}}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn response(header: &'static [u8]) -> u16 {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let count = stream.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..count]);
            assert!(request.starts_with("POST /v1/setup/status HTTP/1.1"));
            assert!(request.contains("Authorization: Bearer fixture-token"));
            stream.write_all(header).await.unwrap();
        });
        port
    }
    #[tokio::test]
    async fn a_process_or_unrelated_response_cannot_claim_service_readiness() {
        let port =
            response(b"HTTP/1.1 503 Unavailable\r\nContent-Type: application/x-protobuf\r\n\r\n")
                .await;
        assert!(
            probe(
                port,
                "/v1/setup/status",
                "fixture-token",
                Some(b""),
                "application/x-protobuf"
            )
            .await
            .is_err()
        );
        let port = response(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n").await;
        assert!(
            probe(
                port,
                "/v1/setup/status",
                "fixture-token",
                Some(b""),
                "application/x-protobuf"
            )
            .await
            .is_err()
        );
        let port =
            response(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\n\r\n").await;
        probe(
            port,
            "/v1/setup/status",
            "fixture-token",
            Some(b""),
            "application/x-protobuf",
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn cancelled_start_does_not_probe_live_application_ports() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["assistant", "phone-runner"] {
            fs::create_dir_all(temp.path().join(format!("data/{name}"))).unwrap();
            fs::write(
                temp.path().join(format!("data/{name}/token")),
                "fixture-token",
            )
            .unwrap();
        }
        let (_send, stop) = watch::channel(true);
        assert_eq!(
            wait(temp.path(), stop).await.unwrap_err().code(),
            "installation_cancelled"
        );
    }
}
