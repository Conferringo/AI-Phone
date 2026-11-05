use super::*;

pub(super) fn audio() -> Vec<u8> {
    // A real 16 kHz mono PCM16 container; fixture samples are silent, not recognized speech.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&68u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 1, 0]);
    bytes.extend_from_slice(&16000u32.to_le_bytes());
    bytes.extend_from_slice(&32000u32.to_le_bytes());
    bytes.extend_from_slice(&[2, 0, 16, 0]);
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&32u32.to_le_bytes());
    bytes.resize(76, 0);
    bytes
}

pub(super) fn draft() -> wire::VoiceDraft {
    wire::VoiceDraft {
        base_url: "http://127.0.0.1:1/v1".into(),
        api_key: "fixture-voice-key".into(),
        recognition_model: "audio-model".into(),
        speech_model: "speech-model".into(),
        voice: "fixture".into(),
        input_protocol: wire::VoiceInputProtocol::AudioChat as i32,
        ..Default::default()
    }
}
#[test]
fn voice_configuration_is_independent_and_public_profiles_have_no_credential() {
    let d = normalize(draft()).unwrap();
    let profile = public("agent", 3, &d);
    assert!(profile.configured && profile.has_api_key);
    assert_eq!(profile.revision, 3);
    assert!(
        !profile
            .encode_to_vec()
            .windows(d.api_key.len())
            .any(|v| v == d.api_key.as_bytes())
    );
    assert!(!public("agent", 4, &normalize(wire::VoiceDraft::default()).unwrap()).configured);
    let mut invalid = d.clone();
    invalid.input_protocol = 99;
    assert!(normalize(invalid).is_err());
    let mut invalid = d.clone();
    invalid.base_url = "file:///tmp/audio".into();
    assert!(normalize(invalid).is_err());
    let mut invalid = d;
    invalid.api_key = "bad\nheader".into();
    assert!(normalize(invalid).is_err());
}
#[test]
fn live_transcription_is_a_saved_voice_protocol() {
    let mut value = draft();
    value.input_protocol = wire::VoiceInputProtocol::RealtimeTranscription as i32;
    assert!(normalize(value).is_ok(), "live input must be configurable");
}
#[tokio::test]
async fn transport_rejections_preserve_authentication_status() {
    // decode owns authentication; the voice route must not turn it into a 400.
    let root = std::env::temp_dir().join(format!("voice-auth-{}", Uuid::new_v4()));
    let app = Arc::new(App {
        db: sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap(),
        root: root.clone(),
        token: "fixture-token".into(),
        phone_token: String::new(),
        settings: settings::SettingsStore::test_configured(&root),
        local: reqwest::Client::new(),
        remote: reqwest::Client::new(),
        active: Mutex::new(Default::default()),
        login_attempts: Mutex::new(vec![]),
        workspace_init: Mutex::new(()),
        location_mutation: Mutex::new(()),
        network_mutation: Mutex::new(()),
    });
    let response = route(
        axum::extract::State(app),
        axum::extract::Path("settings".into()),
        HeaderMap::new(),
        Ok(Bytes::new()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    tokio::fs::remove_dir_all(root).await.unwrap();
}
#[tokio::test]
#[ignore = "real disposable PostgreSQL; no production model or device calls"]
async fn agent_voice_save_conflicts_clear_and_device_scope_survive_reopen() {
    crate::worker::steering_regressions::database_case(|app| async move {
        registry::initialize(&app.db, &app.settings).await.unwrap();
        let id = Uuid::from_u128(2).to_string();
        let chat_revision = app.settings.current();
        let first = save(
            &app,
            wire::VoiceSettingsSaveRequest {
                agent_id: id.clone(),
                expected_revision: 0,
                draft: Some(draft()),
            },
        )
        .await
        .unwrap();
        assert_eq!(first.revision, 1);
        assert!(first.configured);
        assert_eq!(app.settings.current(), chat_revision);
        assert!(
            save(
                &app,
                wire::VoiceSettingsSaveRequest {
                    agent_id: id.clone(),
                    expected_revision: 0,
                    draft: Some(draft())
                }
            )
            .await
            .is_err()
        );
        let (revision, saved) = load(&app, &id).await.unwrap();
        assert_eq!(revision, 1);
        assert_eq!(saved, draft());
        configured(&app, &id, registry::PHONE, revision)
            .await
            .unwrap();
        assert!(
            configured(&app, &id, "foreign-device", revision)
                .await
                .is_err()
        );
        assert!(
            configured(&app, &id, registry::PHONE, revision + 1)
                .await
                .is_err()
        );
        let cleared = save(
            &app,
            wire::VoiceSettingsSaveRequest {
                agent_id: id.clone(),
                expected_revision: 1,
                draft: Some(wire::VoiceDraft::default()),
            },
        )
        .await
        .unwrap();
        assert_eq!(cleared.revision, 2);
        assert!(!cleared.configured);
        assert!(configured(&app, &id, registry::PHONE, 2).await.is_err());
        assert!(
            save(
                &app,
                wire::VoiceSettingsSaveRequest {
                    agent_id: id,
                    expected_revision: 1,
                    draft: Some(draft())
                }
            )
            .await
            .is_err()
        );
    })
    .await;
}

#[tokio::test]
async fn real_http_audio_adapter_transcribes_interprets_speaks_and_reports_quota_without_retry() {
    use axum::routing::post;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let quota = Arc::new(AtomicUsize::new(0));
    let calls = quota.clone();
    let audio = audio();
    let expected = audio.clone();
    let output = audio.clone();
    let router=Router::new()
        .route("/v1/audio/transcriptions",post(|headers:HeaderMap,body:Bytes|async move {
            assert_eq!(headers["authorization"],"Bearer fixture-voice-key");
            let text=String::from_utf8_lossy(&body);assert!(text.contains("speech.wav")&&text.contains("audio-model"));
            axum::Json(json!({"text":"打开微信"}))
        }))
        .route("/v1/chat/completions",post(|axum::Json(value):axum::Json<serde_json::Value>|async move {
            assert_eq!(value["messages"][1]["content"][1]["input_audio"]["format"],"wav");
            axum::Json(json!({"choices":[{"message":{"content":"{\"literal\":\"不是，是北京\",\"text\":\"打开北京天气\",\"corrects_request_id\":\"previous\"}"}}]}))
        }))
        .route("/v1/audio/speech",post(move |axum::Json(value):axum::Json<serde_json::Value>|{let output=output.clone();async move {
            assert_eq!(value["response_format"],"wav");assert_eq!(value["input"],"正在处理");output
        }}))
        .route("/quota/audio/transcriptions",post(move ||{let calls=calls.clone();async move {calls.fetch_add(1,Ordering::SeqCst);StatusCode::PAYMENT_REQUIRED}}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut config = draft();
    config.base_url = format!("http://{address}/v1");
    config.input_protocol = wire::VoiceInputProtocol::Transcription as i32;
    let request = wire::VoiceTranscribeRequest {
        session_id: Uuid::new_v4().to_string(),
        utterance_id: "segment".into(),
        wav: audio,
        context: vec![wire::VoiceContext {
            request_id: "previous".into(),
            literal: "上海".into(),
            text: "打开上海天气".into(),
        }],
        ..Default::default()
    };
    let client = reqwest::Client::new();
    let transcript = provider::transcribe(&client, &config, request.clone())
        .await
        .unwrap();
    assert_eq!(transcript.literal, "打开微信");
    config.input_protocol = wire::VoiceInputProtocol::AudioChat as i32;
    let transcript = provider::transcribe(&client, &config, request.clone())
        .await
        .unwrap();
    assert_eq!(transcript.text, "打开北京天气");
    assert_eq!(transcript.corrects_request_id, "previous");
    assert_eq!(
        provider::speech_response(&client, &config, "正在处理")
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .to_vec(),
        expected
    );
    config.base_url = format!("http://{address}/quota");
    config.input_protocol = wire::VoiceInputProtocol::Transcription as i32;
    assert_eq!(
        provider::transcribe(&client, &config, request)
            .await
            .unwrap_err(),
        "语音服务 HTTP 402"
    );
    assert_eq!(quota.load(Ordering::SeqCst), 1);
    server.abort();
    let _ = server.await;
}
