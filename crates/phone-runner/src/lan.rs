use axum::{
    body::Body,
    http::{HeaderValue, Method, Request, Response, StatusCode, Uri, header, uri::Authority},
};
use hyper::{body::Incoming, server::conn::http1, service::service_fn, upgrade};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::{TokioExecutor, TokioIo},
};
use std::{
    convert::Infallible,
    fs::{self, OpenOptions},
    io::{self, Write},
    net::{IpAddr, Ipv4Addr},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};
use tokio::{
    io::{AsyncWriteExt, copy_bidirectional},
    net::{TcpListener, TcpStream},
    task::JoinSet,
};

const UPSTREAM: &str = "127.0.0.1:17860";
const LAN_HEADER: &str = "x-aiphone-lan";
type WebClient = Client<HttpConnector, Incoming>;

fn address() -> Result<Ipv4Addr, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(configured) = std::env::var_os("AI_PHONE_LAN_IP") {
        let address: Ipv4Addr = configured.to_str().ok_or("invalid LAN address")?.parse()?;
        if !address.is_private() {
            return Err("LAN address must be private".into());
        }
        return Ok(address);
    }
    let mut interfaces = if_addrs::get_if_addrs()?;
    interfaces.sort_by_key(|interface| match interface.name.as_str() {
        "en1" => 0,
        "en0" => 1,
        _ => 2,
    });
    interfaces
        .into_iter()
        .filter(|interface| {
            interface.name.starts_with("en")
                || interface.name.starts_with("eth")
                || interface.name.starts_with("wl")
        })
        .find_map(|interface| match interface.ip() {
            IpAddr::V4(ip) if ip.is_private() => Some(ip),
            _ => None,
        })
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::AddrNotAvailable, "No private LAN interface").into()
        })
}

fn empty_response(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    response
}

async fn forward(
    mut request: Request<Incoming>,
    upstream: WebClient,
    authority: Authority,
) -> Result<Response<Body>, Infallible> {
    let Some(path) = request
        .uri()
        .path_and_query()
        .filter(|path| path.as_str().starts_with('/') && request.method() != Method::CONNECT)
        .cloned()
    else {
        return Ok(empty_response(StatusCode::BAD_REQUEST));
    };
    *request.uri_mut() = Uri::builder()
        .scheme("http")
        .authority(authority)
        .path_and_query(path)
        .build()
        .expect("fixed upstream and validated request path");
    request.headers_mut().remove(header::CONNECTION);
    let downstream_upgrade = if request
        .headers()
        .get(header::UPGRADE)
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"))
    {
        request
            .headers_mut()
            .insert(header::CONNECTION, HeaderValue::from_static("upgrade"));
        Some(upgrade::on(&mut request))
    } else {
        request.headers_mut().remove(header::UPGRADE);
        None
    };
    request
        .headers_mut()
        .insert(LAN_HEADER, HeaderValue::from_static("1"));
    let Ok(mut response) = upstream.request(request).await else {
        return Ok(empty_response(StatusCode::SERVICE_UNAVAILABLE));
    };
    if response.status() == StatusCode::SWITCHING_PROTOCOLS
        && let Some(downstream) = downstream_upgrade
    {
        let upstream = upgrade::on(&mut response);
        tokio::spawn(async move {
            if let Ok((downstream, upstream)) = tokio::try_join!(downstream, upstream) {
                let _ =
                    copy_bidirectional(&mut TokioIo::new(downstream), &mut TokioIo::new(upstream))
                        .await;
            }
        });
    }
    Ok(response.map(Body::new))
}

async fn relay(
    mut downstream: TcpStream,
    upstream: WebClient,
    authority: Authority,
) -> io::Result<()> {
    let peer = downstream.peer_addr()?.ip();
    if !matches!(peer, IpAddr::V4(ip) if ip.is_private() || ip.is_loopback()) {
        downstream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await?;
        return Ok(());
    }
    downstream.set_nodelay(true)?;
    http1::Builder::new()
        .serve_connection(
            TokioIo::new(downstream),
            service_fn(move |request| forward(request, upstream.clone(), authority.clone())),
        )
        .with_upgrades()
        .await
        .map_err(io::Error::other)
}

pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let address = address()?;
    let listener = TcpListener::bind((address, 17860)).await?;
    let root = std::env::var_os("AI_PHONE_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    fs::create_dir_all(root.join("data"))?;
    let origin = format!("http://{address}:17860");
    let mut deployment = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(root.join("data/deployment.json"))?;
    serde_json::to_writer_pretty(
        &mut deployment,
        &serde_json::json!({"origin": origin, "lan_ip": address.to_string(), "port": 17860}),
    )?;
    deployment.write_all(b"\n")?;
    println!("AI Phone LAN: {origin}");
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut connector = HttpConnector::new();
    connector.set_nodelay(true);
    let upstream = Client::builder(TokioExecutor::new()).build(connector);
    let authority = Authority::from_static(UPSTREAM);
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            stream = listener.accept() => {
                let (stream, _) = stream?;
                connections.spawn(relay(stream, upstream.clone(), authority.clone()));
            }
            Some(_) = connections.join_next() => {}
            _ = tokio::signal::ctrl_c() => break,
            _ = terminate.recv() => break,
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

    const CLIENT_FRAME: [u8; 9] = [0x82, 0x83, 1, 2, 3, 4, 1, 253, 124];
    const SERVER_FRAME: [u8; 5] = [0x82, 3, 0, 255, 127];

    async fn backend(mut request: Request<Incoming>) -> Result<Response<Body>, Infallible> {
        assert_eq!(request.headers()[LAN_HEADER], "1");
        assert_eq!(request.headers().get_all(LAN_HEADER).iter().count(), 1);
        if request.uri().path() == "/socket" {
            let socket = upgrade::on(&mut request);
            tokio::spawn(async move {
                let mut socket = TokioIo::new(socket.await.unwrap());
                let mut received = [0; CLIENT_FRAME.len()];
                socket.read_exact(&mut received).await.unwrap();
                assert_eq!(received, CLIENT_FRAME);
                socket.write_all(&SERVER_FRAME).await.unwrap();
            });
            return Ok(Response::builder()
                .status(StatusCode::SWITCHING_PROTOCOLS)
                .header(header::CONNECTION, "upgrade")
                .header(header::UPGRADE, "websocket")
                .header("sec-websocket-accept", "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
                .body(Body::empty())
                .unwrap());
        }
        assert_eq!(request.method(), Method::POST);
        assert_eq!(request.headers()[header::ORIGIN], "http://127.0.0.1:17860");
        assert!(!request.headers().contains_key(header::CONNECTION));
        let deny = request.uri().path() == "/api/auth/enter";
        let body = axum::body::to_bytes(Body::new(request.into_body()), 16)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), &[0, 255, 127]);
        Ok(if deny {
            empty_response(StatusCode::FORBIDDEN)
        } else {
            Response::new(Body::from(body))
        })
    }

    #[tokio::test]
    async fn relay_stamps_requests_and_passes_binary_websockets() {
        let backend_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let authority: Authority = backend_listener
            .local_addr()
            .unwrap()
            .to_string()
            .parse()
            .unwrap();
        let relay_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = relay_listener.local_addr().unwrap();
        let mut listeners = JoinSet::new();
        listeners.spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                let (socket, _) = backend_listener.accept().await.unwrap();
                connections.spawn(async move {
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(socket), service_fn(backend))
                        .with_upgrades()
                        .await;
                });
            }
        });
        listeners.spawn(async move {
            let client = Client::builder(TokioExecutor::new()).build_http();
            let mut connections = JoinSet::new();
            loop {
                let (socket, _) = relay_listener.accept().await.unwrap();
                connections.spawn(relay(socket, client.clone(), authority.clone()));
            }
        });

        tokio::time::timeout(Duration::from_secs(10), async {
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            let post = |operation| {
                client
                    .post(format!("http://{address}/api/auth/{operation}"))
                    .header(header::ORIGIN, "http://127.0.0.1:17860")
                    .header(header::CONNECTION, LAN_HEADER)
                    .header(LAN_HEADER, "0")
                    .header(LAN_HEADER, "")
                    .body(vec![0, 255, 127])
                    .send()
            };
            assert_eq!(post("enter").await.unwrap().status(), StatusCode::FORBIDDEN);
            let login = post("login").await.unwrap();
            assert_eq!(login.status(), StatusCode::OK);
            assert_eq!(login.bytes().await.unwrap().as_ref(), &[0, 255, 127]);

            let mut socket = BufReader::new(TcpStream::connect(address).await.unwrap());
            socket.write_all(b"GET /socket HTTP/1.1\r\nHost: lan.test\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").await.unwrap();
            let mut line = String::new();
            socket.read_line(&mut line).await.unwrap();
            assert!(line.starts_with("HTTP/1.1 101 "), "{line}");
            loop {
                line.clear();
                assert_ne!(socket.read_line(&mut line).await.unwrap(), 0);
                if line == "\r\n" { break; }
            }
            socket.write_all(&CLIENT_FRAME).await.unwrap();
            let mut received = [0; SERVER_FRAME.len()];
            socket.read_exact(&mut received).await.unwrap();
            assert_eq!(received, SERVER_FRAME);
        }).await.unwrap();
    }
}
