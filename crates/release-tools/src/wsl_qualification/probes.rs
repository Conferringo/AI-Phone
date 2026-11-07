#[cfg(test)]
use super::atomic;
use super::{
    Receipt,
    process::{Context, Failure, Result},
};
use ai_phone_api::wire;
use image::GenericImageView;
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Cursor,
    path::Path,
    time::{Duration, Instant},
};

struct Rpc<'a> {
    client: &'a reqwest::Client,
    base: String,
    token: String,
}
impl Rpc<'_> {
    async fn call<I: Message, O: Message + Default>(
        &self,
        operation: &str,
        value: &I,
    ) -> Result<O> {
        let response = self
            .client
            .post(format!("{}/v1/{operation}", self.base))
            .bearer_auth(&self.token)
            .header("content-type", "application/x-protobuf")
            .body(value.encode_to_vec())
            .send()
            .await?;
        if response.status() != 200
            || response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                != Some("application/x-protobuf")
        {
            return Err(Failure::invalid(format!(
                "qualification_rpc_{operation}_{}",
                response.status().as_u16()
            )));
        }
        O::decode(response.bytes().await?.as_ref())
            .map_err(|_| Failure::invalid("qualification_failed"))
    }
}
async fn health(
    context: &Context,
    url: &str,
    body: Option<Vec<u8>>,
    content: &str,
    token: &str,
) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let mut request = context
            .client
            .request(
                if body.is_some() {
                    reqwest::Method::POST
                } else {
                    reqwest::Method::GET
                },
                url,
            )
            .timeout(Duration::from_secs(5));
        if let Some(body) = &body {
            request = request
                .bearer_auth(token)
                .header("content-type", "application/x-protobuf")
                .body(body.clone());
        }
        match request.send().await {
            Ok(mut response) if response.status() == 200 => {
                if !response
                    .headers()
                    .get("content-type")
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.contains(content))
                {
                    return Err(Failure::invalid("qualification_failed"));
                }
                // The protobuf default message is legitimately empty; HTML must have a body.
                if content != "application/x-protobuf"
                    && response.chunk().await?.is_none_or(|bytes| bytes.is_empty())
                {
                    return Err(Failure::invalid("qualification_failed"));
                }
                return Ok(());
            }
            Ok(_) | Err(_) if Instant::now() < until => {
                tokio::time::sleep(Duration::from_millis(250)).await
            }
            _ => return Err(Failure::invalid("qualification_failed")),
        }
    }
}
pub(super) async fn ready(context: &Context, receipt: &mut Receipt) -> Result<()> {
    let token = context.token("assistant")?;
    let runner = context.token("phone-runner")?;
    let cursor = wire::DeviceRequest {
        phone_id: "phone-local-01".into(),
        owner_id: "local-user".into(),
        ..Default::default()
    }
    .encode_to_vec();
    for (name, url, body, content, credential) in [
        (
            "assistant",
            format!("{}/v1/setup/status", context.endpoints.assistant),
            Some(vec![]),
            "application/x-protobuf",
            token.as_str(),
        ),
        (
            "web",
            format!("{}/phone", context.endpoints.web),
            None,
            "text/html",
            token.as_str(),
        ),
        (
            "channels",
            format!("{}/v1/list", context.endpoints.channels),
            Some(vec![]),
            "application/x-protobuf",
            token.as_str(),
        ),
        (
            "runner_endpoint",
            format!("{}/v1/cursor", context.endpoints.runner),
            Some(cursor),
            "application/x-protobuf",
            runner.as_str(),
        ),
    ] {
        health(context, &url, body, content, credential).await?;
        receipt.checked(name);
    }
    Ok(())
}
#[derive(Deserialize)]
struct DatabaseRole {
    runtime: String,
    privileged: bool,
    ddl: bool,
}
pub(super) async fn database_roles(context: &Context, receipt: &mut Receipt) -> Result<()> {
    let assistant = context.root.join("runtime/services/bin/assistant");
    let role: DatabaseRole = serde_json::from_slice(
        &context
            .execute(
                [assistant.as_os_str(), "--database-check".as_ref()],
                &[],
                120,
            )
            .await?,
    )?;
    if role.runtime != "ai_phone_runtime" || role.privileged || role.ddl {
        return Err(Failure::invalid("qualification_failed"));
    }
    receipt.checked("database_roles");
    let response = context
        .execute(
            ["docker", "exec", "ai-phone-redis-1", "redis-cli", "ping"],
            &[],
            120,
        )
        .await?;
    if response.as_slice() != b"PONG\n" {
        return Err(Failure::invalid("qualification_failed"));
    }
    receipt.checked("redis");
    Ok(())
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DeviceMarker {
    nonce: String,
    device_id: String,
}
#[derive(Deserialize)]
struct Profile {
    desktop: Resources,
}
#[derive(Deserialize)]
struct Resources {
    cpus: u32,
    memory_mb: u32,
}
pub(super) async fn desktop_registration(context: &Context) -> Result<String> {
    let marker = context
        .root
        .join("data/deployment/wsl-desktop-qualification.json");
    let rpc = Rpc {
        client: &context.client,
        base: context.endpoints.assistant.clone(),
        token: context.token("assistant")?,
    };
    let device = if context.phase == 1 {
        if marker.try_exists()? {
            return Err(Failure::invalid("qualification_marker_exists"));
        }
        let state: wire::RegistryState = rpc
            .call("registry/state", &wire::RegistryRequest::default())
            .await?;
        if state.devices.iter().any(|device| device.kind == "desktop") {
            return Err(Failure::invalid("qualification_desktop_exists"));
        }
        let agent = state
            .agents
            .first()
            .ok_or_else(|| Failure::invalid("qualification_agent_missing"))?;
        let device = uuid::Uuid::new_v4().to_string();
        let record = DeviceMarker {
            nonce: context.nonce.clone(),
            device_id: device.clone(),
        };
        // Exclusive ownership is recorded before the API can create the actual device.
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&marker)?;
        serde_json::to_writer(&mut file, &record)?;
        file.flush()?;
        let _: wire::RegistryState = rpc
            .call(
                "registry/device-save",
                &wire::DeviceSaveRequest {
                    id: device.clone(),
                    name: "Qualification desktop".into(),
                    kind: "desktop".into(),
                    default_agent_id: agent.id.clone(),
                    ..Default::default()
                },
            )
            .await?;
        let profile: Profile = serde_json::from_slice(&fs::read(
            context.root.join("data/deployment/profile.json"),
        )?)?;
        let job: wire::DesktopProvisionJob = rpc
            .call(
                "desktop/provision",
                &wire::DesktopProvisionRequest {
                    device_id: device.clone(),
                    request_id: uuid::Uuid::new_v4().to_string(),
                    cpus: profile.desktop.cpus,
                    memory_mb: profile.desktop.memory_mb,
                    template_id: "debian13-openbox".into(),
                },
            )
            .await?;
        if job.id.is_empty() || job.device_id != device {
            return Err(Failure::invalid("qualification_provision_identity"));
        }
        let until = Instant::now() + Duration::from_secs(180);
        loop {
            let state: wire::DeviceLinkState = rpc
                .call("desktop/state", &wire::RegistryRequest::default())
                .await?;
            let current = state
                .jobs
                .iter()
                .find(|current| current.id == job.id && current.device_id == device);
            if current.is_some_and(|job| job.status == "completed") {
                break;
            }
            if current.is_some_and(|job| job.status == "failed") || Instant::now() >= until {
                return Err(Failure::invalid("qualification_provision_failed"));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        device
    } else {
        recorded_device(&marker, &context.nonce)?
    };
    let state: wire::RegistryState = rpc
        .call("registry/state", &wire::RegistryRequest::default())
        .await?;
    if !state
        .devices
        .iter()
        .any(|row| row.id == device && row.kind == "desktop" && row.runtime_state == "managed")
    {
        return Err(Failure::invalid("qualification_desktop_unmanaged"));
    }
    let frame: wire::DesktopObservation = rpc
        .call(
            "desktop/observe",
            &wire::DesktopRequest {
                device_id: device.clone(),
            },
        )
        .await?;
    let actual = frame_evidence(&frame.png)?;
    if frame.device_id != device
        || frame.width != 1280
        || frame.height != 800
        || actual["width"] != frame.width
        || actual["height"] != frame.height
    {
        return Err(Failure::invalid("qualification_observation"));
    }
    Ok(device)
}
fn recorded_device(path: &Path, nonce: &str) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > 4096 {
        return Err(Failure::invalid("qualification_marker_invalid"));
    }
    let marker: DeviceMarker = serde_json::from_slice(&fs::read(path)?)
        .map_err(|_| Failure::invalid("qualification_marker_invalid"))?;
    if marker.nonce != nonce
        || !uuid::Uuid::parse_str(&marker.device_id)
            .is_ok_and(|id| id.to_string() == marker.device_id)
    {
        return Err(Failure::invalid("qualification_marker_owner"));
    }
    Ok(marker.device_id)
}
fn decoded(data: &[u8], format: image::ImageFormat) -> Result<image::DynamicImage> {
    let mut reader = image::ImageReader::with_format(Cursor::new(data), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| Failure::invalid("qualification_observation"))
}
pub(super) fn frame_evidence(data: &[u8]) -> Result<Value> {
    if !data.starts_with(b"\x89PNG\r\n\x1a\n") || data.len() > 16 * 1024 * 1024 {
        return Err(Failure::invalid("qualification_observation"));
    }
    let mut offset = 8;
    let mut first = true;
    let mut complete = false;
    while offset + 12 <= data.len() {
        let length =
            u32::from_be_bytes(data[offset..offset + 4].try_into().expect("four bytes")) as usize;
        let end = offset
            .checked_add(12 + length)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| Failure::invalid("qualification_observation"))?;
        let kind = &data[offset + 4..offset + 8];
        let body = &data[offset + 8..end - 4];
        if crc32fast::hash(&data[offset + 4..end - 4])
            != u32::from_be_bytes(data[end - 4..end].try_into().expect("four bytes"))
        {
            return Err(Failure::invalid("qualification_observation"));
        }
        if first
            && (kind != b"IHDR"
                || body.len() != 13
                || ![2, 6].contains(&body[9])
                || body[8] != 8
                || body[10..] != [0, 0, 0])
        {
            return Err(Failure::invalid("qualification_observation"));
        }
        first = false;
        complete = kind == b"IEND" && body.is_empty() && end == data.len();
        offset = end;
    }
    if offset != data.len() || !complete {
        return Err(Failure::invalid("qualification_observation"));
    }
    let image = decoded(data, image::ImageFormat::Png)?;
    Ok(
        json!({"width":image.width(),"height":image.height(),"sha256":format!("{:x}",Sha256::digest(data))}),
    )
}
pub(super) async fn desktop_frame(context: &Context) -> Result<Value> {
    let data = context
        .execute(
            [
                "docker",
                "exec",
                "-u",
                "ai",
                "ai-phone-desktop",
                "/usr/local/bin/desktop-bridge",
                "snapshot",
            ],
            &[],
            120,
        )
        .await?;
    let value = frame_evidence(&data)?;
    if value["width"] != 1280 || value["height"] != 800 {
        return Err(Failure::invalid("qualification_observation"));
    }
    Ok(value)
}
async fn first_jpeg(mut response: reqwest::Response) -> Result<Vec<u8>> {
    let mut data = vec![];
    let mut start = None;
    while data.len() < 4 * 1024 * 1024 {
        let Some(chunk) = response.chunk().await? else {
            break;
        };
        let previous = data.len();
        data.extend_from_slice(&chunk[..chunk.len().min(4 * 1024 * 1024 - previous)]);
        let scan = previous.saturating_sub(1);
        if start.is_none() {
            start = data[scan..]
                .windows(2)
                .position(|part| part == b"\xff\xd8")
                .map(|index| index + scan);
        }
        if let Some(start) = start {
            let from = scan.max(start + 2);
            if let Some(end) = data[from..].windows(2).position(|part| part == b"\xff\xd9") {
                return Ok(data[start..from + end + 2].to_vec());
            }
        }
    }
    Err(Failure::invalid("qualification_observation"))
}
pub(super) async fn desktop_media(context: &Context, device: &str) -> Result<()> {
    let web = &context.endpoints.web;
    let entry = context
        .client
        .post(format!("{web}/api/auth/enter"))
        .header("origin", web.as_str())
        .header("content-type", "application/x-protobuf")
        .body(vec![])
        .timeout(Duration::from_secs(5))
        .send()
        .await?;
    if entry.status() != 200 {
        return Err(Failure::invalid("qualification_observation"));
    }
    let cookies = entry
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .collect::<Vec<_>>()
        .join("; ");
    let response = context
        .client
        .get(format!("{web}/api/desktop/media?device_id={device}&raw=1"))
        .header("cookie", cookies)
        .timeout(Duration::from_secs(5))
        .send()
        .await?;
    if response.status() != 200
        || response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim)
            != Some("application/octet-stream")
    {
        return Err(Failure::invalid("qualification_observation"));
    }
    let data = first_jpeg(response).await?;
    let image = decoded(&data, image::ImageFormat::Jpeg)?;
    let (width, height) = image.dimensions();
    if u64::from(width) * u64::from(height) * 3 + u64::from(height) > 32 * 1024 * 1024 {
        return Err(Failure::invalid("qualification_observation"));
    }
    Ok(())
}
const CREATE_WORK_FILE: &str = "set -C; printf '%s' \"$1\" > \"$2\"";
fn check_work_file(path: &Path, expected: &[u8], guest: &[u8]) -> Result<()> {
    if guest != expected || fs::read(path)? != expected {
        return Err(Failure::invalid("qualification_failed"));
    }
    Ok(())
}
pub(super) async fn work_file(context: &Context) -> Result<()> {
    let name = format!("qualification-{}.txt", context.nonce);
    let guest = format!("/home/ai/Files/{name}");
    let expected = format!("AI Phone offline qualification\n{}", context.nonce);
    if context.phase == 1 {
        context
            .execute(
                [
                    "docker",
                    "exec",
                    "-u",
                    "ai",
                    "ai-phone-desktop",
                    "sh",
                    "-c",
                    CREATE_WORK_FILE,
                    "qualification",
                    &expected,
                    &guest,
                ],
                &[],
                120,
            )
            .await?;
    }
    let actual = context
        .execute(
            [
                "docker",
                "exec",
                "-u",
                "ai",
                "ai-phone-desktop",
                "cat",
                &guest,
            ],
            &[],
            120,
        )
        .await?;
    check_work_file(
        &context.root.join("data/desktop/home/Files").join(name),
        expected.as_bytes(),
        &actual,
    )
}
pub(super) async fn database_marker(context: &Context) -> Result<()> {
    let mut sql = if context.phase == 1 {
        format!(
            "CREATE TABLE qualification_receipt(value text PRIMARY KEY); INSERT INTO qualification_receipt VALUES ('{}'); ",
            context.nonce
        )
    } else {
        String::new()
    };
    sql.push_str("SELECT value FROM qualification_receipt;");
    let output = context
        .execute(
            [
                "docker",
                "exec",
                "-i",
                "ai-phone-postgres-1",
                "sh",
                "-c",
                r#"exec psql -X -qAt -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB""#,
            ],
            sql.as_bytes(),
            120,
        )
        .await?;
    if String::from_utf8_lossy(&output).trim() != context.nonce {
        return Err(Failure::invalid("qualification_failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    struct Received {
        head: String,
        body: Vec<u8>,
    }
    impl Received {
        fn path(&self) -> &str {
            self.head.split_whitespace().nth(1).unwrap()
        }
    }
    struct Reply {
        status: u16,
        content: &'static str,
        extra: &'static str,
        chunks: Vec<Vec<u8>>,
    }
    fn reply(content: &'static str, body: Vec<u8>) -> Reply {
        Reply {
            status: 200,
            content,
            extra: "",
            chunks: vec![body],
        }
    }
    fn pb(value: impl Message) -> Reply {
        reply("application/x-protobuf", value.encode_to_vec())
    }
    async fn http(
        count: usize,
        mut respond: impl FnMut(usize, &Received) -> Reply + Send + 'static,
    ) -> (String, tokio::task::JoinHandle<Vec<Received>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut received = vec![];
            for index in 0..count {
                let (socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
                let mut reader = BufReader::new(socket);
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).await.unwrap();
                    head.push_str(&line);
                    if line == "\r\n" {
                        break;
                    }
                }
                let size = head
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; size];
                reader.read_exact(&mut body).await.unwrap();
                let request = Received { head, body };
                let response = respond(index, &request);
                let mut socket = reader.into_inner();
                let size: usize = response.chunks.iter().map(Vec::len).sum();
                socket.write_all(format!("HTTP/1.1 {} fixture\r\ncontent-type: {}\r\ncontent-length: {size}\r\nconnection: close\r\n{}\r\n",response.status,response.content,response.extra).as_bytes()).await.unwrap();
                for chunk in response.chunks {
                    socket.write_all(&chunk).await.unwrap();
                    tokio::task::yield_now().await;
                }
                received.push(request);
            }
            received
        });
        (url, task)
    }
    fn context(root: &Path) -> Context {
        fs::create_dir_all(root.join("data/assistant")).unwrap();
        fs::create_dir_all(root.join("data/phone-runner")).unwrap();
        fs::create_dir_all(root.join("data/deployment")).unwrap();
        fs::write(root.join("data/assistant/token"), "assistant-fixture").unwrap();
        fs::write(root.join("data/phone-runner/token"), "runner-fixture").unwrap();
        fs::write(
            root.join("data/deployment/profile.json"),
            r#"{"desktop":{"cpus":2,"memory_mb":2048}}"#,
        )
        .unwrap();
        Context::new(root.into(), "a".repeat(32), 1).unwrap()
    }
    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]));
        let mut out = Cursor::new(vec![]);
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }
    #[tokio::test]
    async fn health_uses_the_runner_token_and_scoped_protobuf_without_claiming_phone_readiness() {
        let root = tempfile::tempdir().unwrap();
        let mut context = context(root.path());
        let (url, server) = http(4, |index, request| {
            let headers = request.head.to_ascii_lowercase();
            if index == 3 {
                assert!(headers.contains("authorization: bearer runner-fixture"));
                let message = wire::DeviceRequest::decode(request.body.as_slice()).unwrap();
                assert_eq!(message.phone_id, "phone-local-01");
                assert_eq!(message.owner_id, "local-user");
            } else if index != 1 {
                assert!(headers.contains("authorization: bearer assistant-fixture"));
            }
            if index == 1 {
                reply("text/html", b"<main>workbench</main>".to_vec())
            } else {
                reply("application/x-protobuf", vec![])
            }
        })
        .await;
        context.endpoints.assistant = url.clone();
        context.endpoints.web = url.clone();
        context.endpoints.channels = url.clone();
        context.endpoints.runner = url;
        let mut receipt = Receipt::new(context.nonce.clone(), 1, root.path());
        ready(&context, &mut receipt).await.unwrap();
        let requests = server.await.unwrap();
        assert_eq!(requests[0].path(), "/v1/setup/status");
        assert_eq!(requests[3].path(), "/v1/cursor");
        assert!(!receipt.phone_ready);
        assert_eq!(receipt.checks.values().filter(|value| **value).count(), 4);
    }
    #[tokio::test]
    async fn html_health_does_not_accept_empty_or_wrong_type_bodies() {
        for (content, body) in [("text/html", vec![]), ("application/json", b"{}".to_vec())] {
            let root = tempfile::tempdir().unwrap();
            let context = context(root.path());
            let (url, server) = http(1, move |_, _| reply(content, body.clone())).await;
            assert!(
                health(&context, &url, None, "text/html", "unused")
                    .await
                    .is_err()
            );
            server.await.unwrap();
        }
    }
    #[tokio::test]
    async fn desktop_creation_and_restart_reuse_one_owned_device_through_real_protobuf_http() {
        let root = tempfile::tempdir().unwrap();
        let mut context = context(root.path());
        let image = png(1280, 800);
        let mut device = String::new();
        let (url, server) = http(8, move |_, request| {
            assert!(
                request
                    .head
                    .to_ascii_lowercase()
                    .contains("authorization: bearer assistant-fixture")
            );
            match request.path() {
                "/v1/registry/state" => {
                    let mut value = wire::RegistryState::default();
                    value.agents.push(wire::RegisteredAgent {
                        id: "fixture-agent".into(),
                        ..Default::default()
                    });
                    if !device.is_empty() {
                        value.devices.push(wire::RegisteredDevice {
                            id: device.clone(),
                            kind: "desktop".into(),
                            runtime_state: "managed".into(),
                            ..Default::default()
                        });
                    }
                    pb(value)
                }
                "/v1/registry/device-save" => {
                    let value = wire::DeviceSaveRequest::decode(request.body.as_slice()).unwrap();
                    assert_eq!(value.default_agent_id, "fixture-agent");
                    assert_eq!(value.kind, "desktop");
                    assert!(uuid::Uuid::parse_str(&value.id).is_ok());
                    device = value.id;
                    pb(wire::RegistryState::default())
                }
                "/v1/desktop/provision" => {
                    let value =
                        wire::DesktopProvisionRequest::decode(request.body.as_slice()).unwrap();
                    assert_eq!(value.device_id, device);
                    assert_eq!((value.cpus, value.memory_mb), (2, 2048));
                    assert_eq!(value.template_id, "debian13-openbox");
                    pb(wire::DesktopProvisionJob {
                        id: "fixture-job".into(),
                        device_id: device.clone(),
                        ..Default::default()
                    })
                }
                "/v1/desktop/state" => pb(wire::DeviceLinkState {
                    jobs: vec![wire::DesktopProvisionJob {
                        id: "fixture-job".into(),
                        device_id: device.clone(),
                        status: "completed".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                "/v1/desktop/observe" => {
                    assert_eq!(
                        wire::DesktopRequest::decode(request.body.as_slice())
                            .unwrap()
                            .device_id,
                        device
                    );
                    pb(wire::DesktopObservation {
                        device_id: device.clone(),
                        width: 1280,
                        height: 800,
                        png: image.clone(),
                        ..Default::default()
                    })
                }
                _ => panic!("unexpected RPC"),
            }
        })
        .await;
        context.endpoints.assistant = url;
        let first = desktop_registration(&context).await.unwrap();
        context.phase = 2;
        assert_eq!(desktop_registration(&context).await.unwrap(), first);
        let requests = server.await.unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.path() == "/v1/registry/device-save")
                .count(),
            1
        );
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.path() == "/v1/desktop/provision")
                .count(),
            1
        );
    }
    #[tokio::test]
    async fn desktop_api_header_only_png_is_not_a_usable_observation() {
        let root = tempfile::tempdir().unwrap();
        let mut context = context(root.path());
        context.phase = 2;
        let device = uuid::Uuid::new_v4().to_string();
        atomic(
            root.path()
                .join("data/deployment/wsl-desktop-qualification.json"),
            &DeviceMarker {
                nonce: context.nonce.clone(),
                device_id: device.clone(),
            },
        )
        .unwrap();
        let (url, server) = http(2, move |index, _| {
            if index == 0 {
                pb(wire::RegistryState {
                    devices: vec![wire::RegisteredDevice {
                        id: device.clone(),
                        kind: "desktop".into(),
                        runtime_state: "managed".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                })
            } else {
                pb(wire::DesktopObservation {
                    device_id: device.clone(),
                    width: 1280,
                    height: 800,
                    png: b"\x89PNG\r\n\x1a\n".to_vec(),
                    ..Default::default()
                })
            }
        })
        .await;
        context.endpoints.assistant = url;
        assert_eq!(
            desktop_registration(&context).await.unwrap_err().probe,
            "qualification_observation"
        );
        server.await.unwrap();
    }
    #[tokio::test]
    async fn raw_media_requires_web_auth_and_decodes_the_actual_jpeg_without_guest_python() {
        let image = image::RgbImage::from_pixel(3, 2, image::Rgb([30, 40, 50]));
        let mut encoded = Cursor::new(vec![]);
        image
            .write_to(&mut encoded, image::ImageFormat::Jpeg)
            .unwrap();
        let encoded = encoded.into_inner();
        for (content, image, passed) in [
            ("application/octet-stream", encoded, true),
            ("multipart/x-mixed-replace", vec![], false),
            (
                "application/octet-stream",
                b"\xff\xd8\xff\xd9".to_vec(),
                false,
            ),
        ] {
            let root = tempfile::tempdir().unwrap();
            let mut context = context(root.path());
            let (url, server) = http(2, move |index, request| {
                if index == 0 {
                    assert_eq!(request.path(), "/api/auth/enter");
                    assert!(request.head.contains("origin:"));
                    let mut response = reply("application/x-protobuf", vec![]);
                    response.extra = "set-cookie: fixture-session=accepted; HttpOnly\r\n";
                    response
                } else {
                    assert_eq!(
                        request.path(),
                        "/api/desktop/media?device_id=fixture-device&raw=1"
                    );
                    assert!(request.head.contains("cookie: fixture-session=accepted"));
                    Reply {
                        status: 200,
                        content,
                        extra: "",
                        chunks: vec![vec![b' ', 0xff], image.iter().copied().skip(1).collect()],
                    }
                }
            })
            .await;
            context.endpoints.web = url;
            assert_eq!(
                desktop_media(&context, "fixture-device").await.is_ok(),
                passed
            );
            server.await.unwrap();
        }
    }
    #[test]
    fn actual_pixels_are_required_and_crc_and_truncation_are_not_accepted() {
        let image = image::RgbaImage::from_pixel(2, 3, image::Rgba([20, 30, 40, 255]));
        let mut bytes = Cursor::new(vec![]);
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let mut bytes = bytes.into_inner();
        let value = frame_evidence(&bytes).unwrap();
        assert_eq!(value["width"], 2);
        assert_eq!(value["height"], 3);
        assert!(frame_evidence(&bytes[..33]).is_err());
        bytes[29] ^= 1;
        assert!(frame_evidence(&bytes).is_err());
    }
    #[test]
    fn recorded_device_never_adopts_a_different_owner_or_partial_marker() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("marker");
        let id = uuid::Uuid::new_v4().to_string();
        atomic(
            &path,
            &DeviceMarker {
                nonce: "owner".into(),
                device_id: id.clone(),
            },
        )
        .unwrap();
        assert_eq!(recorded_device(&path, "owner").unwrap(), id);
        assert!(recorded_device(&path, "other").is_err());
        fs::write(&path, b"{\"nonce\":\"owner\"}").unwrap();
        assert!(recorded_device(&path, "owner").is_err());
    }
    #[test]
    fn work_file_creation_and_both_readbacks_preserve_existing_work() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("work file.txt");
        let expected = b"AI Phone offline qualification\nnon-secret-identity";
        let create = || {
            std::process::Command::new("sh")
                .args(["-c", CREATE_WORK_FILE, "qualification"])
                .arg(std::str::from_utf8(expected).unwrap())
                .arg(&path)
                .output()
                .unwrap()
        };
        assert!(create().status.success());
        let actual = std::process::Command::new("cat")
            .arg(&path)
            .output()
            .unwrap();
        assert!(actual.status.success());
        check_work_file(&path, expected, &actual.stdout).unwrap();
        assert!(!create().status.success());
        assert_eq!(fs::read(&path).unwrap(), expected);
        assert!(check_work_file(&path, expected, b"wrong guest data").is_err());
        fs::write(&path, b"changed host file").unwrap();
        assert!(check_work_file(&path, expected, expected).is_err());
    }
    #[tokio::test]
    async fn database_role_requires_the_actual_restricted_native_result() {
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("runtime/services/bin/assistant");
        fs::create_dir_all(program.parent().unwrap()).unwrap();
        for body in [
            r#"{"runtime":"ai_phone_runtime","privileged":true,"ddl":false}"#,
            r#"{"runtime":"ai_phone_runtime","privileged":false,"ddl":true}"#,
            r#"{"runtime":"postgres","privileged":false,"ddl":false}"#,
        ] {
            super::super::tests::script(&program, &format!("#!/bin/sh\nprintf '%s' '{body}'\n"));
            let context = Context::new(temp.path().into(), "a".repeat(32), 1).unwrap();
            let mut receipt = Receipt::new(context.nonce.clone(), 1, temp.path());
            assert_eq!(
                database_roles(&context, &mut receipt)
                    .await
                    .unwrap_err()
                    .probe,
                "qualification_failed"
            );
            assert!(!receipt.checks["database_roles"]);
            assert!(!receipt.checks["redis"]);
        }
    }
}
