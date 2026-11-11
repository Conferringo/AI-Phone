use super::*;
use futures_util::stream;
use sha2::{Digest, Sha256};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
};
use tokio_tungstenite::{
    WebSocketStream, client_async,
    tungstenite::{Message as ClientMessage, client::IntoClientRequest},
};
type Client = WebSocketStream<TcpStream>;

async fn listen(
    router: Router,
) -> (
    std::net::SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await
            .unwrap();
    });
    (address, tx, task)
}
async fn configure(app: &App, base_url: String) {
    registry::initialize(&app.db, &app.settings).await.unwrap();
    sqlx::query("INSERT INTO admin_sessions(token_hash) VALUES($1)")
        .bind(format!("{:x}", Sha256::digest("b".repeat(64).as_bytes())))
        .execute(&app.db)
        .await
        .unwrap();
    save(
        app,
        wire::VoiceSettingsSaveRequest {
            agent_id: Uuid::from_u128(2).to_string(),
            expected_revision: 0,
            draft: Some(wire::VoiceDraft {
                base_url,
                ..super::super::tests::draft()
            }),
        },
    )
    .await
    .unwrap();
}
fn message() -> wire::VoiceSpeechRequest {
    wire::VoiceSpeechRequest {
        device_id: "phone-local-01".into(),
        agent_id: Uuid::from_u128(2).to_string(),
        configuration_revision: 1,
        text: "正在处理".into(),
    }
}
async fn connect(address: std::net::SocketAddr) -> Client {
    let mut request = format!("ws://{address}/").into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", "http://localhost:17860".parse().unwrap());
    request.headers_mut().insert(
        "cookie",
        format!("ai_phone_session={}", "b".repeat(64))
            .parse()
            .unwrap(),
    );
    client_async(request, TcpStream::connect(address).await.unwrap())
        .await
        .unwrap()
        .0
}
async fn submit(socket: &mut Client, request: wire::VoiceSpeechRequest) {
    socket
        .send(ClientMessage::Binary(request.encode_to_vec().into()))
        .await
        .unwrap();
}
async fn event(socket: &mut Client) -> wire::voice_speech_stream_output::Event {
    let bytes = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data();
    wire::VoiceSpeechStreamOutput::decode(bytes)
        .unwrap()
        .event
        .unwrap()
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; authenticated streaming, EOF and upstream cancellation"]
async fn playback_starts_before_provider_eof_and_disconnect_cancels_synthesis() {
    crate::worker::steering_regressions::database_case(|app| async move {
        let (requests, mut received) = mpsc::unbounded_channel();
        let fixture = Router::new().route(
            "/v1/audio/speech",
            post(
                move |headers: HeaderMap, axum::Json(value): axum::Json<serde_json::Value>| {
                    assert_eq!(headers["authorization"], "Bearer fixture-voice-key");
                    assert_eq!(value["response_format"], "wav");
                    assert_eq!(value["input"], "正在处理");
                    let (tx, rx) = mpsc::channel::<std::result::Result<Bytes, std::io::Error>>(2);
                    requests.send(tx).unwrap();
                    async move {
                        axum::body::Body::from_stream(stream::unfold(rx, |mut rx| async {
                            rx.recv().await.map(|value| (value, rx))
                        }))
                    }
                },
            ),
        );
        let (provider, stop_provider, provider_task) = listen(fixture).await;
        configure(&app, format!("http://{provider}/v1")).await;
        let (address, stop, task) = listen(
            Router::new()
                .route("/", axum::routing::get(route))
                .with_state(app.clone()),
        )
        .await;
        let audio = super::super::tests::audio();
        let mut socket = connect(address).await;
        submit(&mut socket, message()).await;
        let source = tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap()
            .unwrap();
        source
            .send(Ok(Bytes::copy_from_slice(&audio[..48])))
            .await
            .unwrap();
        let wire::voice_speech_stream_output::Event::WavChunk(first) = event(&mut socket).await
        else {
            panic!("No audio before EOF")
        };
        assert_eq!(first, audio[..48]);
        assert!(!source.is_closed());
        source
            .send(Ok(Bytes::copy_from_slice(&audio[48..])))
            .await
            .unwrap();
        drop(source);
        let wire::voice_speech_stream_output::Event::WavChunk(last) = event(&mut socket).await
        else {
            panic!("Missing suffix")
        };
        assert_eq!([first, last].concat(), audio);
        assert!(matches!(
            event(&mut socket).await,
            wire::voice_speech_stream_output::Event::Done(true)
        ));
        let _ = socket.close(None).await;
        let mut socket = connect(address).await;
        submit(&mut socket, message()).await;
        let source = tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap()
            .unwrap();
        source
            .send(Ok(Bytes::copy_from_slice(&audio[..48])))
            .await
            .unwrap();
        assert!(matches!(
            event(&mut socket).await,
            wire::voice_speech_stream_output::Event::WavChunk(_)
        ));
        socket.close(None).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), source.closed())
            .await
            .expect("provider generation not canceled");
        drop(source);
        assert!(received.try_recv().is_err(), "must not retry synthesis");
        stop.send(()).unwrap();
        task.await.unwrap();
        stop_provider.send(()).unwrap();
        provider_task.await.unwrap();
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; voice error frames and one provider request"]
async fn playback_quota_failure_is_explicit_without_retry_or_key_leak() {
    crate::worker::steering_regressions::database_case(|app| async move {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = calls.clone();
        let fixture = Router::new().route(
            "/v1/audio/speech",
            post(move || {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async { StatusCode::PAYMENT_REQUIRED }
            }),
        );
        let (provider, stop_provider, provider_task) = listen(fixture).await;
        configure(&app, format!("http://{provider}/v1")).await;
        let (address, stop, task) = listen(
            Router::new()
                .route("/", axum::routing::get(route))
                .with_state(app.clone()),
        )
        .await;
        let mut socket = connect(address).await;
        submit(&mut socket, message()).await;
        let wire::voice_speech_stream_output::Event::Error(error) = event(&mut socket).await else {
            panic!("Missing quota failure")
        };
        assert_eq!(error.message, "语音服务 HTTP 402");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let _ = socket.close(None).await;
        stop.send(()).unwrap();
        task.await.unwrap();
        stop_provider.send(()).unwrap();
        provider_task.await.unwrap();
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; origin, cookie, device and voice revision"]
async fn playback_checks_origin_session_binding_and_revision_before_provider_use() {
    crate::worker::steering_regressions::database_case(|app| async move {
        configure(&app, "http://127.0.0.1:1/v1".into()).await;
        let (address, stop, task) = listen(
            Router::new()
                .route("/", axum::routing::get(route))
                .with_state(app.clone()),
        )
        .await;
        for (origin, cookie, expected) in [
            ("http://foreign.invalid", "b".repeat(64), 403),
            ("http://localhost:17860", "invalid".into(), 401),
        ] {
            let mut request = format!("ws://{address}/").into_client_request().unwrap();
            request
                .headers_mut()
                .insert("origin", origin.parse().unwrap());
            request.headers_mut().insert(
                "cookie",
                format!("ai_phone_session={cookie}").parse().unwrap(),
            );
            let Err(tokio_tungstenite::tungstenite::Error::Http(response)) =
                client_async(request, TcpStream::connect(address).await.unwrap()).await
            else {
                panic!("Unauthorized playback accepted")
            };
            assert_eq!(response.status().as_u16(), expected);
        }
        for (device, revision, expected) in [
            ("unknown", 1, "Agent 未绑定此设备"),
            ("phone-local-01", 2, "语音配置已更新，请重新开启 Copilot"),
        ] {
            let mut socket = connect(address).await;
            submit(
                &mut socket,
                wire::VoiceSpeechRequest {
                    device_id: device.into(),
                    configuration_revision: revision,
                    ..message()
                },
            )
            .await;
            let wire::voice_speech_stream_output::Event::Error(error) = event(&mut socket).await
            else {
                panic!("Invalid configuration accepted")
            };
            assert_eq!(error.message, expected);
            let _ = socket.close(None).await;
        }
        stop.send(()).unwrap();
        task.await.unwrap();
    })
    .await;
}
