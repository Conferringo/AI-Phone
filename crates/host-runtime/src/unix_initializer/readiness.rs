//! Startup waits for actual local service responses, not successful process spawning.
use crate::{Error, Result};
use ai_phone_api::wire::{DeviceObservation, DeviceRequest};
use prost::Message;
use reqwest::{Client, header::CONTENT_TYPE};
use std::{fs, path::Path, time::Duration};
use tokio::{sync::watch, time::Instant};

async fn probe(
    client: &Client,
    port: u16,
    path: &str,
    token: &str,
    body: Option<&[u8]>,
    content: &str,
) -> Result<()> {
    let url = format!("http://127.0.0.1:{port}{path}");
    let request = match body {
        Some(bytes) => client
            .post(url)
            .header(CONTENT_TYPE, "application/x-protobuf")
            .body(bytes.to_vec()),
        None => client.get(url),
    };
    let pending = || Error::Invalid("unix_host_service_not_ready");
    let response = request
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| pending())?;
    if response.status() != 200
        || response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            != Some(content)
    {
        return Err(pending());
    }
    if path == "/v1/observe" {
        let body = response.bytes().await.map_err(|_| pending())?;
        if DeviceObservation::decode(body)
            .map_err(|_| pending())?
            .root_identity
            != "0"
        {
            return Err(pending());
        }
    }
    Ok(())
}
pub(super) async fn wait(root: &Path, mut stop: watch::Receiver<bool>) -> Result<()> {
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| Error::Invalid("unix_host_service_not_ready"))?;
    let token = fs::read_to_string(root.join("data/assistant/token"))?;
    let runner = fs::read_to_string(root.join("data/phone-runner/token"))?;
    let observation = DeviceRequest {
        phone_id: "phone-local-01".into(),
        owner_id: "local-user".into(),
        metadata_only: true,
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
            "/v1/observe",
            runner.trim(),
            Some(observation.as_slice()),
            "application/x-protobuf",
        ),
    ] {
        loop {
            if *stop.borrow() {
                return Err(Error::Cancelled);
            }
            let result = tokio::select! {
                result=tokio::time::timeout_at(deadline,probe(&client,port,path,auth,body,content))=>result,
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
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    async fn response(path: &'static str, reply: &'static [u8]) -> u16 {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let count = stream.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..count]);
            assert!(request.starts_with(&format!("POST {path} HTTP/1.1")));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer fixture-token")
            );
            stream.write_all(reply).await.unwrap();
        });
        port
    }
    #[tokio::test]
    async fn a_process_or_unrelated_response_cannot_claim_service_readiness() {
        let client = Client::builder().no_proxy().build().unwrap();
        let path = "/v1/setup/status";
        let port = response(
            path,
            b"HTTP/1.1 503 Unavailable\r\nContent-Type: application/x-protobuf\r\n\r\n",
        )
        .await;
        assert!(
            probe(
                &client,
                port,
                "/v1/setup/status",
                "fixture-token",
                Some(b""),
                "application/x-protobuf"
            )
            .await
            .is_err()
        );
        let port = response(path, b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n").await;
        assert!(
            probe(
                &client,
                port,
                "/v1/setup/status",
                "fixture-token",
                Some(b""),
                "application/x-protobuf"
            )
            .await
            .is_err()
        );
        let port = response(
            path,
            b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\n\r\n",
        )
        .await;
        probe(
            &client,
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
    async fn phone_observation_must_report_root() {
        let client = Client::builder().no_proxy().build().unwrap();
        let path = "/v1/observe";
        let pending = response(
            path,
            b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n",
        )
        .await;
        let rooted = response(path, b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 3\r\n\r\n\x4a\x010").await;
        for (port, expected) in [(pending, false), (rooted, true)] {
            assert_eq!(
                probe(
                    &client,
                    port,
                    path,
                    "fixture-token",
                    Some(&[]),
                    "application/x-protobuf"
                )
                .await
                .is_ok(),
                expected
            );
        }
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
