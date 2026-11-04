use super::*;
use ai_phone_api::wire::{DeviceBridgeOpen, DeviceBridgeOpened};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

fn request(role: DeviceBridgeRole, endpoint: SocketAddr) -> DeviceBridgeConfig {
    DeviceBridgeConfig {
        version: 1,
        role: role as i32,
        bridge_id: "a".repeat(32),
        secret: vec![7; 32],
        endpoint: endpoint.to_string(),
        ..Default::default()
    }
}
async fn host() -> (Bridge, [TcpListener; 2]) {
    let a = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut host = Bridge::bind(request(
        DeviceBridgeRole::Host,
        "127.0.0.1:0".parse().unwrap(),
    ))
    .await
    .unwrap();
    let relay::Route::Host { targets, .. } = &mut host.routes[0].1 else {
        panic!("host route")
    };
    *targets = [a.local_addr().unwrap(), b.local_addr().unwrap()];
    (host, [a, b])
}
async fn stop_task(end: watch::Sender<bool>, task: tokio::task::JoinHandle<Result<()>>) {
    end.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn both_routes_preserve_large_streams_half_close_and_stop_without_late_connections() {
    let (host, targets) = host().await;
    let address = host.receipt.endpoint.parse().unwrap();
    let (end, stop) = watch::channel(false);
    let host_task = tokio::spawn(host.serve(stop.clone()));
    let config =
        Arc::new(config::Config::try_from(request(DeviceBridgeRole::Guest, address)).unwrap());
    let routes = bind_guest(config, [0, 0]).await.unwrap();
    let ports: Vec<_> = routes
        .iter()
        .map(|(l, _)| l.local_addr().unwrap())
        .collect();
    let guest = Bridge {
        receipt: DeviceBridgeStarted::default(),
        routes,
    };
    let guest_task = tokio::spawn(guest.serve(stop));
    let mut echoes = Vec::new();
    for (index, listener) in targets.into_iter().enumerate() {
        echoes.push(tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).await.unwrap();
            stream.write_all(&[index as u8]).await.unwrap();
            stream.write_all(&bytes).await.unwrap();
        }));
    }
    for (index, port) in ports.iter().enumerate() {
        let payload = vec![index as u8 + 8; 1024 * 1024];
        let mut stream = TcpStream::connect(port).await.unwrap();
        let sent = payload.clone();
        let writer = tokio::spawn(async move {
            stream.write_all(&sent).await.unwrap();
            stream.shutdown().await.unwrap();
            let mut out = Vec::new();
            stream.read_to_end(&mut out).await.unwrap();
            out
        });
        let out = tokio::time::timeout(Duration::from_secs(2), writer)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out[0], index as u8);
        assert_eq!(&out[1..], payload);
    }
    end.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        host_task.await.unwrap().unwrap();
        guest_task.await.unwrap().unwrap();
    })
    .await
    .unwrap();
    for echo in echoes {
        echo.await.unwrap();
    }
    for port in ports.into_iter().chain([address]) {
        assert!(TcpStream::connect(port).await.is_err());
    }
}
#[tokio::test]
async fn wrong_key_identity_version_and_unknown_service_never_open_targets() {
    let (host, targets) = host().await;
    let address: SocketAddr = host.receipt.endpoint.parse().unwrap();
    let (end, stop) = watch::channel(false);
    let task = tokio::spawn(host.serve(stop));
    for field in ["secret", "identity", "version", "service"] {
        let mut request = DeviceBridgeOpen {
            version: 1,
            bridge_id: "a".repeat(32),
            secret: vec![7; 32],
            service: DeviceBridgeService::Adb as i32,
        };
        match field {
            "secret" => request.secret[0] = 8,
            "identity" => request.bridge_id = "b".repeat(32),
            "version" => request.version = 2,
            "service" => request.service = 300,
            _ => unreachable!(),
        };
        let mut stream = TcpStream::connect(address).await.unwrap();
        transport::write(&mut stream, &request).await.unwrap();
        assert!(
            !transport::read::<DeviceBridgeOpened>(&mut stream)
                .await
                .unwrap()
                .accepted
        );
    }
    for target in targets {
        assert!(
            tokio::time::timeout(Duration::from_millis(30), target.accept())
                .await
                .is_err()
        );
    }
    stop_task(end, task).await;
}
#[tokio::test]
async fn cancellation_reaps_partial_handshakes_and_active_raw_connections() {
    let (host, targets) = host().await;
    let address = host.receipt.endpoint.parse().unwrap();
    let (end, stop) = watch::channel(false);
    let task = tokio::spawn(host.serve(stop));
    let config = config::Config::try_from(request(DeviceBridgeRole::Guest, address)).unwrap();
    let mut raw = relay::connect(&config, DeviceBridgeService::Adb)
        .await
        .unwrap();
    let (mut target, _) = targets[0].accept().await.unwrap();
    let mut partial = TcpStream::connect(address).await.unwrap();
    partial.write_u32(32).await.unwrap();
    raw.write_all(b"active").await.unwrap();
    let mut before = [0; 6];
    target.read_exact(&mut before).await.unwrap();
    assert_eq!(&before, b"active");
    stop_task(end, task).await;
    for stream in [&mut raw, &mut target, &mut partial] {
        let result = tokio::time::timeout(Duration::from_secs(1), stream.read_u8()).await;
        assert!(
            result.unwrap().is_err(),
            "all sockets must close before the owner returns"
        );
    }
}
#[tokio::test]
async fn guest_does_not_publish_when_paired_target_is_unavailable() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    assert!(
        Bridge::bind(request(DeviceBridgeRole::Guest, address))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn stalled_peers_are_bounded_and_cancel_before_the_admission_deadline() {
    let (host, _) = host().await;
    let address: SocketAddr = host.receipt.endpoint.parse().unwrap();
    let (end, stop) = watch::channel(false);
    let task = tokio::spawn(host.serve(stop));
    let mut pending = Vec::new();
    for _ in 0..32 {
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_u32(32).await.unwrap();
        pending.push(stream);
    }
    let mut excess = TcpStream::connect(address).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), excess.read_u8())
            .await
            .unwrap()
            .is_err()
    );
    stop_task(end, task).await;
    for mut stream in pending {
        assert!(
            tokio::time::timeout(Duration::from_secs(1), stream.read_u8())
                .await
                .unwrap()
                .is_err()
        );
    }
}
