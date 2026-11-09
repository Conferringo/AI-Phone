use super::*;
use futures_util::FutureExt;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
};
use tokio_tungstenite::{client_async, tungstenite::client::IntoClientRequest};
use tokio_util::sync::CancellationToken;
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
    let handle = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await
            .unwrap();
    });
    (address, tx, handle)
}
async fn client(address: std::net::SocketAddr) -> Client {
    let request = format!("ws://{address}/").into_client_request().unwrap();
    client_async(request, TcpStream::connect(address).await.unwrap())
        .await
        .unwrap()
        .0
}
async fn output(socket: &mut Client) -> wire::voice_stream_output::Event {
    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    wire::VoiceStreamOutput::decode(message.into_data())
        .unwrap()
        .event
        .unwrap()
}
async fn input(socket: &mut Client, event: wire::voice_stream_input::Event) {
    socket
        .send(ProviderMessage::Binary(
            wire::VoiceStreamInput { event: Some(event) }
                .encode_to_vec()
                .into(),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn actual_websocket_delivers_before_commit_reuses_provider_and_drains_on_disconnect() {
    let (events, mut received) = mpsc::unbounded_channel();
    let (closed, mut closure) = mpsc::unbounded_channel();
    let fixture=Router::new().route("/v1/realtime",axum::routing::get(move |headers:HeaderMap,ws:WebSocketUpgrade| {
        assert_eq!(headers["authorization"],"Bearer fixture-only");let events=events.clone();let closed=closed.clone();
        async move {ws.on_upgrade(move |mut socket|async move{
            let mut index=0;let mut partial=false;let mut audio_bytes=0;
            while let Some(Ok(message))=socket.recv().await {
                let ClientMessage::Text(text)=message else {break;};let value:Value=serde_json::from_str(&text).unwrap();
                events.send(value["type"].as_str().unwrap().to_owned()).unwrap();
                let reply=match value["type"].as_str().unwrap(){
                    "session.update"=>{assert_eq!(value["session"]["audio"]["input"]["format"]["rate"],24000);assert_eq!(value["session"]["audio"]["input"]["turn_detection"],Value::Null);json!({"type":"session.updated"})},
                    "input_audio_buffer.append"=>{
                        let pcm=STANDARD.decode(value["audio"].as_str().unwrap()).unwrap();audio_bytes+=pcm.len();
                        if partial {continue;}partial=true;json!({"type":"conversation.item.input_audio_transcription.delta","item_id":format!("item{index}"),"delta":"打开"})
                    },
                    "input_audio_buffer.commit"=>{assert_eq!(audio_bytes,2400);audio_bytes=0;let reply=json!({"type":"conversation.item.input_audio_transcription.completed","item_id":format!("item{index}"),"transcript":"打开天气"});index+=1;partial=false;reply},
                    _=>panic!("Unexpected provider event"),
                };
                if socket.send(ClientMessage::Text(reply.to_string().into())).await.is_err(){break;}
            }
            let _=closed.send(());
        })}
    }));
    let (address, stop_provider, provider_task) = listen(fixture).await;
    let draft = wire::VoiceDraft {
        base_url: format!("http://{address}/v1"),
        api_key: "fixture-only".into(),
        recognition_model: "fixture-transcribe".into(),
        ..Default::default()
    };
    let activity = Arc::new(super::super::activity::SpeechActivity::default());
    let gateway_activity = activity.clone();
    let conversation = Uuid::new_v4();
    let session = Uuid::new_v4();
    let router = Router::new().route(
        "/",
        axum::routing::get(move |ws: WebSocketUpgrade| {
            let draft = draft.clone();
            let activity = gateway_activity.clone();
            async move {
                ws.on_upgrade(move |mut socket| async move {
                    let provider = connect(&reqwest::Client::new(), &draft).await.unwrap();
                    send(&mut socket, wire::voice_stream_output::Event::Ready(true))
                        .await
                        .unwrap();
                    pipe(socket, provider, &activity, Some(conversation), session).await;
                })
            }
        }),
    );
    let (address, stop_gateway, gateway_task) = listen(router).await;
    let mut socket = client(address).await;
    assert!(matches!(
        output(&mut socket).await,
        wire::voice_stream_output::Event::Ready(true)
    ));
    for _ in 0..2 {
        let id = Uuid::new_v4().to_string();
        input(
            &mut socket,
            wire::voice_stream_input::Event::Audio(wire::VoiceStreamAudio {
                utterance_id: id.clone(),
                pcm16: [0, 10].repeat(800),
            }),
        )
        .await;
        let wire::voice_stream_output::Event::Transcript(partial) = output(&mut socket).await
        else {
            panic!("Missing partial");
        };
        assert_eq!(partial.utterance_id, id);
        assert!(!partial.r#final);
        let cancel = CancellationToken::new();
        let hold = activity.wait(conversation, &cancel);
        tokio::pin!(hold);
        assert!(hold.as_mut().now_or_never().is_none());
        input(
            &mut socket,
            wire::voice_stream_input::Event::Commit(wire::VoiceStreamCommit {
                utterance_id: id.clone(),
            }),
        )
        .await;
        let wire::voice_stream_output::Event::Transcript(final_) = output(&mut socket).await else {
            panic!("Missing final");
        };
        assert_eq!(final_.utterance_id, id);
        assert!(final_.r#final);
        assert_eq!(final_.text, "打开天气");
        assert!(hold.as_mut().now_or_never().is_none());
        activity.accepted(conversation, session, &id);
        assert!(hold.await.unwrap());
    }
    socket.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), closure.recv())
        .await
        .unwrap()
        .unwrap();
    let mut all = vec![];
    while let Ok(event) = received.try_recv() {
        all.push(event);
    }
    assert_eq!(all.iter().filter(|e| *e == "session.update").count(), 1);
    assert_eq!(
        all.iter()
            .filter(|e| *e == "input_audio_buffer.commit")
            .count(),
        2
    );
    stop_gateway.send(()).unwrap();
    stop_provider.send(()).unwrap();
    gateway_task.await.unwrap();
    provider_task.await.unwrap();
}

#[tokio::test]
async fn quota_failure_does_not_retry_or_leak_credentials() {
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls = count.clone();
    let router = Router::new().route(
        "/v1/realtime",
        axum::routing::get(move || {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { StatusCode::PAYMENT_REQUIRED }
        }),
    );
    let (address, stop, task) = listen(router).await;
    let draft = wire::VoiceDraft {
        base_url: format!("http://{address}/v1"),
        api_key: "must-not-leak".into(),
        ..Default::default()
    };
    assert_eq!(
        connect(&reqwest::Client::new(), &draft).await.unwrap_err(),
        "语音服务 HTTP 402"
    );
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    stop.send(()).unwrap();
    task.await.unwrap();
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; cookie, configuration and device binding"]
async fn browser_stream_checks_session_origin_binding_and_voice_revision() {
    crate::worker::steering_regressions::database_case(|app| async move {
        use sha2::{Digest, Sha256};
        registry::initialize(&app.db, &app.settings).await.unwrap();
        let token = "a".repeat(64);
        sqlx::query("INSERT INTO admin_sessions(token_hash) VALUES($1)")
            .bind(format!("{:x}", Sha256::digest(token.as_bytes())))
            .execute(&app.db)
            .await
            .unwrap();
        let voice = super::super::tests::draft();
        super::super::save(
            &app,
            wire::VoiceSettingsSaveRequest {
                agent_id: Uuid::from_u128(2).to_string(),
                expected_revision: 0,
                draft: Some(wire::VoiceDraft {
                    input_protocol: 3,
                    ..voice
                }),
            },
        )
        .await
        .unwrap();
        let router = Router::new()
            .route("/", axum::routing::get(route))
            .with_state(app.clone());
        let (address, stop, server) = listen(router).await;
        for (origin, cookie, status) in [
            (
                "http://foreign.invalid",
                format!("ai_phone_session={token}"),
                403,
            ),
            (
                "http://localhost:17860",
                "ai_phone_session=invalid".into(),
                401,
            ),
        ] {
            let mut request = format!("ws://{address}/").into_client_request().unwrap();
            request
                .headers_mut()
                .insert("origin", origin.parse().unwrap());
            request
                .headers_mut()
                .insert("cookie", cookie.parse().unwrap());
            let Err(tokio_tungstenite::tungstenite::Error::Http(response)) =
                client_async(request, TcpStream::connect(address).await.unwrap()).await
            else {
                panic!("Unauthorized socket accepted");
            };
            assert_eq!(response.status().as_u16(), status);
        }
        for (device, revision, expected) in [
            ("phone-unknown", 1, "Agent 未绑定此设备"),
            ("phone-local-01", 2, "语音配置已更新，请重新开启 Copilot"),
        ] {
            let mut request = format!("ws://{address}/").into_client_request().unwrap();
            request
                .headers_mut()
                .insert("origin", "http://localhost:17860".parse().unwrap());
            request.headers_mut().insert(
                "cookie",
                format!("ai_phone_session={token}").parse().unwrap(),
            );
            let mut socket = client_async(request, TcpStream::connect(address).await.unwrap())
                .await
                .unwrap()
                .0;
            input(
                &mut socket,
                wire::voice_stream_input::Event::Start(wire::VoiceStreamStart {
                    device_id: device.into(),
                    agent_id: Uuid::from_u128(2).to_string(),
                    configuration_revision: revision,
                    session_id: Uuid::new_v4().to_string(),
                }),
            )
            .await;
            let wire::voice_stream_output::Event::Error(error) = output(&mut socket).await else {
                panic!("Invalid binding accepted");
            };
            assert_eq!(error.message, expected);
            assert!(matches!(
                tokio::time::timeout(Duration::from_secs(2), socket.next())
                    .await
                    .unwrap(),
                Some(Ok(ProviderMessage::Close(_)))
            ));
            let _ = socket.close(None).await;
        }
        stop.send(()).unwrap();
        server.await.unwrap();
    })
    .await;
}
