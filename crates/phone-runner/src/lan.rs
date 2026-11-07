use std::{
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

async fn relay(mut downstream: TcpStream) -> io::Result<()> {
    let peer = downstream.peer_addr()?.ip();
    if !matches!(peer, IpAddr::V4(ip) if ip.is_private() || ip.is_loopback()) {
        downstream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await?;
        return Ok(());
    }
    downstream.set_nodelay(true)?;
    let mut upstream = match TcpStream::connect((Ipv4Addr::LOCALHOST, 17860)).await {
        Ok(stream) => stream,
        Err(_) => {
            downstream
                .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await?;
            return Ok(());
        }
    };
    upstream.set_nodelay(true)?;
    copy_bidirectional(&mut downstream, &mut upstream).await?;
    Ok(())
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
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            stream = listener.accept() => {
                let (stream, _) = stream?;
                connections.spawn(relay(stream));
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
