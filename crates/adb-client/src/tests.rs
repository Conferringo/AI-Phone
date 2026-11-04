use super::*;
use tokio::net::TcpListener;

async fn listener() -> (TcpListener, Client) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let client = Client::new(
        listener.local_addr().unwrap().port(),
        "emulator-5564".into(),
    )
    .unwrap();
    (listener, client)
}
async fn request(socket: &mut TcpStream, expected: &str) {
    let bytes = string(socket, 65535).await.unwrap();
    assert_eq!(bytes, expected.as_bytes());
    socket.write_all(b"OKAY").await.unwrap();
}
async fn shell(socket: &mut TcpStream, text: &str) {
    request(socket, "host:transport:emulator-5564").await;
    request(socket, &format!("shell,v2,raw:{text}")).await;
    let mut close = [0; 5];
    socket.read_exact(&mut close).await.unwrap();
    assert_eq!(close, [4, 0, 0, 0, 0]);
}
async fn packet(socket: &mut TcpStream, id: u8, bytes: &[u8]) {
    socket.write_u8(id).await.unwrap();
    socket.write_u32_le(bytes.len() as u32).await.unwrap();
    socket.write_all(bytes).await.unwrap();
}
fn limits() -> Limits {
    Limits {
        stdout: 128,
        stderr: 32,
    }
}

#[tokio::test]
async fn shell_preserves_unicode_binary_streams_and_remote_failure_exit_code() {
    let (listener, client) = listener().await;
    let worker = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        shell(&mut socket, "printf '中文'").await;
        packet(&mut socket, 1, b"first\0").await;
        packet(&mut socket, 2, b"error\n").await;
        packet(&mut socket, 1, "中文".as_bytes()).await;
        packet(&mut socket, 3, &[7]).await;
    });
    let result = client
        .shell("printf '中文'", limits(), Duration::from_secs(1))
        .await
        .unwrap();
    worker.await.unwrap();
    assert_eq!(
        result,
        Output {
            code: 7,
            stdout: "first\0中文".as_bytes().to_vec(),
            stderr: b"error\n".to_vec()
        }
    );
}
#[tokio::test]
async fn incomplete_shell_exit_is_unknown_instead_of_success() {
    for invalid in [vec![], vec![3, 2, 0, 0, 0, 0, 0], vec![9, 0, 0, 0, 0]] {
        let (listener, client) = listener().await;
        let worker = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            shell(&mut socket, "true").await;
            packet(&mut socket, 1, b"unconfirmed").await;
            socket.write_all(&invalid).await.unwrap();
        });
        assert!(matches!(
            client.shell("true", limits(), Duration::from_secs(1)).await,
            Err(Error::Io(_) | Error::Protocol)
        ));
        worker.await.unwrap();
    }
}
#[tokio::test]
async fn output_budget_rejects_announced_size_before_waiting_for_body() {
    let (listener, client) = listener().await;
    let worker = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        shell(&mut socket, "oversized").await;
        socket.write_all(&[1, 255, 255, 255, 255]).await.unwrap();
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).await.unwrap(), 0);
    });
    assert!(matches!(
        client
            .shell("oversized", limits(), Duration::from_secs(1))
            .await,
        Err(Error::Limit)
    ));
    worker.await.unwrap();
}
#[tokio::test]
async fn timeout_drops_connection_without_retrying_the_command() {
    let (listener, client) = listener().await;
    let worker = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        shell(&mut socket, "sleep 30").await;
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).await.unwrap(), 0);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), listener.accept())
                .await
                .is_err()
        );
    });
    assert!(matches!(
        client
            .shell("sleep 30", limits(), Duration::from_millis(50))
            .await,
        Err(Error::Timeout)
    ));
    worker.await.unwrap();
}
#[tokio::test]
async fn host_state_is_framed_and_exactly_serial_scoped() {
    let (listener, client) = listener().await;
    let worker = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket, "host-serial:emulator-5564:get-state").await;
        socket.write_all(b"0006device").await.unwrap();
    });
    assert_eq!(
        client.state(128, Duration::from_secs(1)).await.unwrap(),
        b"device"
    );
    worker.await.unwrap();
}
#[tokio::test]
async fn explicit_transport_rejection_never_opens_a_device_service() {
    let (listener, client) = listener().await;
    let worker = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        assert_eq!(
            string(&mut socket, 128).await.unwrap(),
            b"host:transport:emulator-5564"
        );
        socket.write_all(b"FAIL0007offline").await.unwrap();
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).await.unwrap(), 0);
    });
    assert!(
        matches!(client.shell("input tap 1 1",limits(),Duration::from_secs(1)).await,Err(Error::Rejected(value)) if value=="offline")
    );
    worker.await.unwrap();
}
#[tokio::test]
async fn raw_capture_preserves_every_byte_and_its_limit() {
    let (listener, client) = listener().await;
    let worker = tokio::spawn(async move {
        for bytes in [b"\0\xff\r\n".as_slice(), b"too much"] {
            let (mut socket, _) = listener.accept().await.unwrap();
            request(&mut socket, "host:transport:emulator-5564").await;
            request(&mut socket, "exec:screencap -p").await;
            socket.write_all(bytes).await.unwrap();
        }
    });
    assert_eq!(
        client
            .raw("exec:screencap -p", 4, Duration::from_secs(1))
            .await
            .unwrap(),
        b"\0\xff\r\n"
    );
    assert!(matches!(
        client
            .raw("exec:screencap -p", 4, Duration::from_secs(1))
            .await,
        Err(Error::Limit)
    ));
    worker.await.unwrap();
}
#[test]
fn invalid_scope_is_rejected_before_any_connection() {
    for serial in ["", "a\nb", "a\0b", "a b"] {
        assert!(Client::new(15037, serial.into()).is_err());
    }
    assert!(Client::new(0, "emulator-5564".into()).is_err());
}

#[tokio::test]
async fn malformed_length_is_not_accepted_as_a_hexadecimal_frame() {
    let (listener, client) = listener().await;
    let worker = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket, "host:version").await;
        socket.write_all(b"+001x").await.unwrap();
    });
    assert!(matches!(
        client.version(Duration::from_secs(1)).await,
        Err(Error::Protocol)
    ));
    worker.await.unwrap();
}

#[tokio::test]
async fn stream_limits_apply_to_cumulative_bytes_separately() {
    for id in [1, 2] {
        let (listener, client) = listener().await;
        let worker = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            shell(&mut socket, "many packets").await;
            packet(&mut socket, id, b"abcd").await;
            packet(&mut socket, id, b"efgh").await;
        });
        assert!(matches!(
            client
                .shell(
                    "many packets",
                    Limits {
                        stdout: 7,
                        stderr: 7
                    },
                    Duration::from_secs(1)
                )
                .await,
            Err(Error::Limit)
        ));
        worker.await.unwrap();
    }
}
