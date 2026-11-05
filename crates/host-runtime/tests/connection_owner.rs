use ai_phone_api::wire::{DeviceBridgeConfig, DeviceBridgePhone, DeviceBridgeRole};
use ai_phone_host_runtime::{Error, device_bridge::Connection};
use std::{
    net::{Ipv4Addr, TcpListener},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{process::Command, sync::watch};

fn program(root: &Path) -> PathBuf {
    let path = root.join(if cfg!(windows) {
        "fixture.exe"
    } else {
        "fixture"
    });
    let built =
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .arg("--edition=2024")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/link_peer.rs"))
            .arg("-o")
            .arg(&path)
            .output()
            .unwrap();
    assert!(built.status.success(), "fixture compiler: {built:?}");
    path
}
fn request() -> DeviceBridgePhone {
    DeviceBridgePhone {
        transport: Some(DeviceBridgeConfig {
            version: 1,
            role: DeviceBridgeRole::Guest as i32,
            bridge_id: "a".repeat(32),
            secret: vec![7; 32],
            ..Default::default()
        }),
        runtime_directory: "unused by fixture".into(),
        serial: "emulator-5564".into(),
        grpc_token: "private-fixture".into(),
        owner_pipe: false,
    }
}
fn command(program: &Path, folder: &Path, mode: &str) -> Command {
    let mut command = Command::new(program);
    command.arg(folder).arg(mode);
    command
}
fn released(folder: &Path) {
    assert_eq!(std::fs::read(folder.join("owner-eof")).unwrap(), b"drained");
    let address = std::fs::read_to_string(folder.join("endpoint")).unwrap();
    assert!(
        TcpListener::bind(address).is_ok(),
        "the host listener must be gone before return"
    );
}

#[tokio::test]
async fn actual_child_receipt_cleanup_failure_and_startup_cancellation_are_owned() {
    let root = tempfile::tempdir().unwrap();
    let program = program(root.path());
    for (mode, error) in [
        ("ok", None),
        ("wrong", Some("phone_peer_start_receipt")),
        ("exit7", Some("phone_peer_exit_failed")),
    ] {
        let folder = root.path().join(mode);
        std::fs::create_dir(&folder).unwrap();
        let (_end, mut stop) = watch::channel(false);
        let result = Connection::start(
            request(),
            Ipv4Addr::LOCALHOST,
            command(&program, &folder, mode),
            &mut stop,
        )
        .await;
        match result {
            Ok(mut link) => {
                let result = link.shutdown().await;
                if let Some(code) = error {
                    assert_eq!(result.unwrap_err().code(), code);
                } else {
                    result.unwrap();
                }
            }
            Err(error) => assert_eq!(Some(error.code()), error_code(mode)),
        }
        released(&folder);
    }
    let folder = root.path().join("cancel");
    std::fs::create_dir(&folder).unwrap();
    let (end, mut stop) = watch::channel(false);
    let marker = folder.join("input-consumed");
    let cancel = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !marker.is_file() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        end.send(true).unwrap();
    });
    let result = Connection::start(
        request(),
        Ipv4Addr::LOCALHOST,
        command(&program, &folder, "wait"),
        &mut stop,
    )
    .await;
    assert!(matches!(result, Err(Error::Cancelled)));
    cancel.await.unwrap();
    released(&folder);
}
fn error_code(mode: &str) -> Option<&'static str> {
    if mode == "wrong" {
        Some("phone_peer_start_receipt")
    } else {
        panic!("unexpected startup failure")
    }
}
