//! Actual component process ownership; no real SDK, device or user-state inputs.
#![cfg(unix)]
use ai_phone_api::wire::{
    DeviceBridgeConfig, DeviceBridgeOpen, DeviceBridgeOpened, DeviceBridgePhone, DeviceBridgeRole,
    DeviceBridgeStarted,
};
use ai_phone_host_runtime::native_ipc;
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    process::Command,
};

async fn check(signal: bool) {
    let host = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = host.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = host.accept().await.unwrap();
            tokio::spawn(async move {
                let request: DeviceBridgeOpen =
                    native_ipc::read_bounded(&mut stream, 512).await.unwrap();
                assert_eq!(request.bridge_id, "a".repeat(32));
                native_ipc::write_bounded(&mut stream, &DeviceBridgeOpened { accepted: true }, 512)
                    .await
                    .unwrap();
                let (mut input, mut output) = stream.split();
                let _ = tokio::io::copy(&mut input, &mut output).await;
            });
        }
    });
    let ports = [
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
        std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
    ];
    let adb = ports[0].local_addr().unwrap().port();
    let grpc = ports[1].local_addr().unwrap().port();
    drop(ports);
    let root = tempfile::tempdir().unwrap();
    let request = DeviceBridgePhone {
        transport: Some(DeviceBridgeConfig {
            version: 1,
            role: DeviceBridgeRole::Guest as i32,
            bridge_id: "a".repeat(32),
            secret: vec![7; 32],
            endpoint,
            adb_port: adb.into(),
            emulator_port: grpc.into(),
        }),
        runtime_directory: root.path().to_str().unwrap().into(),
        serial: "emulator-5564".into(),
        grpc_token: "fixture-token".into(),
        owner_pipe: true,
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .arg("--phone-peer")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    native_ipc::write_bounded(&mut input, &request, 16384)
        .await
        .unwrap();
    let receipt = tokio::time::timeout(
        Duration::from_secs(10),
        native_ipc::read_bounded::<DeviceBridgeStarted>(&mut output, 512),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(receipt.bridge_id, "a".repeat(32));
    assert_eq!(receipt.adb_port, u32::from(adb));
    let mut socket = TcpStream::connect(("127.0.0.1", adb)).await.unwrap();
    socket.write_all(b"active-transfer").await.unwrap();
    let mut bytes = [0; 15];
    socket.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"active-transfer");
    let owner = std::fs::read(root.path().join(".phone-peer.pb")).unwrap();
    assert!(root.path().join("avd/running/pid_0.ini").is_file());
    let input = if signal {
        assert!(
            std::process::Command::new("/bin/sh")
                .args([
                    "-c",
                    "kill -TERM \"$1\"",
                    "aiphone-test",
                    &child.id().unwrap().to_string()
                ])
                .status()
                .unwrap()
                .success()
        );
        Some(input)
    } else {
        drop(input);
        None
    };
    let exited = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    if exited.is_err() {
        child.kill().await.unwrap();
        child.wait().await.unwrap();
    }
    assert!(
        exited
            .expect("owner EOF must end the component without a signal")
            .unwrap()
            .success()
    );
    drop(input);
    assert_eq!(
        std::fs::read(root.path().join(".phone-peer.pb")).unwrap(),
        owner
    );
    assert!(!root.path().join("avd/running/pid_0.ini").exists());
    assert_eq!(socket.read(&mut bytes).await.unwrap(), 0);
    for port in [adb, grpc] {
        assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_ok());
    }
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn closing_the_owner_pipe_drains_the_actual_peer_and_preserves_pair_identity() {
    check(false).await;
}

#[cfg(unix)]
#[tokio::test]
async fn sigterm_drains_the_peer_while_its_owner_pipe_stays_open() {
    check(true).await;
}
