use super::*;
use crate::Error;
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

#[tokio::test]
async fn phone_peer_publishes_the_existing_discovery_contract_after_pairing() {
    let (host, _targets) = host().await;
    let address = host.receipt.endpoint.parse().unwrap();
    let (end, stop) = watch::channel(false);
    let task = tokio::spawn(host.serve(stop));
    let root = tempfile::tempdir().unwrap();
    let value = phone_peer_request(root.path(), address);
    let peer = peer::PhonePeer::bind(value.clone()).await.unwrap();
    let advertisement = root.path().join("avd/running/pid_0.ini");
    let text = std::fs::read_to_string(&advertisement).unwrap();
    assert!(
        text.contains("port.serial=5564\n") && text.contains("grpc.token=private-test-token\n")
    );
    assert!(text.contains(&format!("grpc.port={}\n", peer.receipt().emulator_port)));
    assert!(matches!(
        peer::PhonePeer::bind(value.clone()).await,
        Err(Error::Busy)
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&advertisement)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let ports = [peer.receipt().adb_port, peer.receipt().emulator_port];
    let (peer_end, stop) = watch::channel(false);
    let peer_task = tokio::spawn(peer.serve(stop));
    stop_task(peer_end, peer_task).await;
    assert!(!advertisement.exists());
    assert!(root.path().join(".phone-peer.pb").exists());
    for port in ports {
        assert!(
            TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port as u16))
                .await
                .is_err()
        );
    }
    let mut resumed = value;
    resumed.grpc_token = "updated-private-token".into();
    let peer = peer::PhonePeer::bind(resumed).await.unwrap();
    assert!(
        std::fs::read_to_string(advertisement)
            .unwrap()
            .contains("grpc.token=updated-private-token\n")
    );
    let (_stop, stop) = watch::channel(true);
    peer.serve(stop).await.unwrap();
    stop_task(end, task).await;
}

fn phone_peer_request(
    root: &std::path::Path,
    address: SocketAddr,
) -> ai_phone_api::wire::DeviceBridgePhone {
    let mut transport = request(DeviceBridgeRole::Guest, address);
    let listeners = [
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
    ];
    transport.adb_port = listeners[0].local_addr().unwrap().port().into();
    transport.emulator_port = listeners[1].local_addr().unwrap().port().into();
    ai_phone_api::wire::DeviceBridgePhone {
        transport: Some(transport),
        runtime_directory: root.to_str().unwrap().into(),
        serial: "emulator-5564".into(),
        grpc_token: "private-test-token".into(),
    }
}

#[tokio::test]
async fn unauthenticated_phone_peer_never_claims_the_runtime_directory() {
    let (host, _targets) = host().await;
    let address = host.receipt.endpoint.parse().unwrap();
    let (end, stop) = watch::channel(false);
    let task = tokio::spawn(host.serve(stop));
    let root = tempfile::tempdir().unwrap();
    let mut value = phone_peer_request(root.path(), address);
    value.transport.as_mut().unwrap().secret = vec![9; 32];
    assert!(peer::PhonePeer::bind(value).await.is_err());
    stop_task(end, task).await;
    assert!(
        !root.path().join(".phone-peer.pb").exists(),
        "a rejected pair cannot reserve persistent ownership"
    );
    assert!(!root.path().join("avd").exists());
}

#[tokio::test]
async fn phone_peer_preserves_foreign_metadata_and_rejects_input_injection() {
    let root = tempfile::tempdir().unwrap();
    let value = phone_peer_request(root.path(), "127.0.0.1:23000".parse().unwrap());
    for (serial, token) in [
        ("emulator-05564", "ok"),
        ("emulator-5565", "ok"),
        ("emulator-5564", "line\ngrpc.port=1"),
        ("emulator-5564", ""),
    ] {
        let mut invalid = value.clone();
        invalid.serial = serial.into();
        invalid.grpc_token = token.into();
        assert_eq!(
            peer::PhonePeer::bind(invalid).await.err().unwrap().code(),
            "phone_peer_configuration"
        );
        assert!(!root.path().join(".phone-peer.pb").exists());
    }
    let path = root.path().join("avd/running");
    std::fs::create_dir_all(&path).unwrap();
    let file = path.join("pid_0.ini");
    std::fs::write(&file, b"foreign advertisement\n").unwrap();
    assert_eq!(
        peer::PhonePeer::bind(value.clone())
            .await
            .err()
            .unwrap()
            .code(),
        "phone_peer_foreign_metadata"
    );
    assert_eq!(std::fs::read(file).unwrap(), b"foreign advertisement\n");
    assert!(!root.path().join(".phone-peer.pb").exists());
    let file = path.join("pid_0.ini");
    let marker = format!(
        "aiphone.bridge={}\n",
        value.transport.as_ref().unwrap().bridge_id
    );
    std::fs::write(&file, marker.as_bytes()).unwrap();
    assert_eq!(
        peer::PhonePeer::bind(value).await.err().unwrap().code(),
        "phone_peer_foreign_metadata"
    );
    assert_eq!(std::fs::read(file).unwrap(), marker.as_bytes());
}

#[tokio::test]
async fn phone_peer_cleanup_preserves_changed_files_and_closes_its_listeners() {
    let (host, _targets) = host().await;
    let (end, stop) = watch::channel(false);
    let address = host.receipt.endpoint.parse().unwrap();
    let task = tokio::spawn(host.serve(stop));
    let root = tempfile::tempdir().unwrap();
    let value = phone_peer_request(root.path(), address);
    let peer = peer::PhonePeer::bind(value.clone()).await.unwrap();
    let owner = std::fs::read(root.path().join(".phone-peer.pb")).unwrap();
    let path = root.path().join("avd/running/pid_0.ini");
    std::fs::write(&path, b"manual replacement\n").unwrap();
    let ports = [peer.receipt().adb_port, peer.receipt().emulator_port];
    let (_stop, stop) = watch::channel(true);
    assert_eq!(
        peer.serve(stop).await.unwrap_err().code(),
        "phone_peer_cleanup_unknown"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"manual replacement\n");
    for port in ports {
        assert!(
            TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port as u16))
                .await
                .is_err()
        );
    }
    assert_eq!(
        peer::PhonePeer::bind(value.clone())
            .await
            .err()
            .unwrap()
            .code(),
        "phone_peer_foreign_metadata"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"manual replacement\n");
    let mut foreign = value;
    foreign.transport.as_mut().unwrap().secret = vec![1; 32];
    assert_eq!(
        peer::PhonePeer::bind(foreign).await.err().unwrap().code(),
        "phone_peer_owner_conflict"
    );
    assert_eq!(
        std::fs::read(root.path().join(".phone-peer.pb")).unwrap(),
        owner
    );
    stop_task(end, task).await;
}

#[cfg(unix)]
#[tokio::test]
async fn phone_peer_rejects_an_advertisement_directory_alias() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("avd")).unwrap();
    let value = phone_peer_request(root.path(), "127.0.0.1:23000".parse().unwrap());
    assert_eq!(
        peer::PhonePeer::bind(value).await.err().unwrap().code(),
        "source_resource_directory"
    );
    assert!(!root.path().join(".phone-peer.pb").exists());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
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
