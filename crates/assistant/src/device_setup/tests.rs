use super::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::{Request, StatusCode},
    response::Response,
};
use prost::Message;
use std::{
    collections::VecDeque,
    fs,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

struct Files(std::path::PathBuf);
impl Files {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("aiphone-native-setup-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("data/deployment")).unwrap();
        fs::write(
            root.join("data/deployment/profile.json"),
            br#"{"desktop":{"cpus":2,"memory_mb":2048}}"#,
        )
        .unwrap();
        Self(root)
    }
    fn store(&self) -> Store {
        Store::new(&self.0).unwrap()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
struct Reply {
    path: &'static str,
    status: StatusCode,
    body: Vec<u8>,
    media: bool,
}
impl Reply {
    fn pb(path: &'static str, value: impl Message) -> Self {
        Self {
            path,
            status: StatusCode::OK,
            body: value.encode_to_vec(),
            media: true,
        }
    }
}
#[derive(Default)]
struct Trace {
    replies: VecDeque<Reply>,
    calls: Vec<(String, Vec<u8>)>,
    errors: Vec<String>,
}
struct Server {
    trace: Arc<Mutex<Trace>>,
    task: tokio::task::JoinHandle<()>,
    port: u16,
}
impl Server {
    async fn new(replies: Vec<Reply>) -> Self {
        let trace = Arc::new(Mutex::new(Trace {
            replies: replies.into(),
            ..Default::default()
        }));
        let router = Router::new().fallback(respond).with_state(trace.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { trace, task, port }
    }
    fn rpc(&self) -> Rpc {
        Rpc::new(
            "fixture-assistant".into(),
            "fixture-runner".into(),
            CancellationToken::new(),
        )
        .unwrap()
        .fixture_ports(self.port, self.port)
    }
    fn checked(&self) -> Vec<(String, Vec<u8>)> {
        let trace = self.trace.lock().unwrap();
        assert!(trace.errors.is_empty(), "{:?}", trace.errors);
        assert!(trace.replies.is_empty(), "unconsumed fixture response");
        trace.calls.clone()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn respond(State(trace): State<Arc<Mutex<Trace>>>, request: Request<Body>) -> Response {
    let path = request.uri().path().to_owned();
    let expected = if matches!(path.as_str(), "/v1/observe" | "/v1/notifications") {
        "Bearer fixture-runner"
    } else {
        "Bearer fixture-assistant"
    };
    let valid = request.method() == "POST"
        && request
            .headers()
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            == Some(expected)
        && request
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            == Some("application/x-protobuf");
    let body = to_bytes(request.into_body(), 1024 * 1024).await.unwrap();
    let mut trace = trace.lock().unwrap();
    trace.calls.push((path.clone(), body.to_vec()));
    if !valid {
        trace.errors.push("request envelope".into());
    }
    let Some(reply) = trace.replies.pop_front() else {
        trace.errors.push(format!("unexpected {path}"));
        return Response::builder().status(500).body(Body::empty()).unwrap();
    };
    if reply.path != path {
        trace
            .errors
            .push(format!("expected {} got {path}", reply.path));
    }
    let mut response = Response::builder().status(reply.status);
    if reply.media {
        response = response.header("content-type", "application/x-protobuf");
    }
    response.body(Body::from(reply.body)).unwrap()
}

#[tokio::test]
async fn notification_readiness_requires_both_live_channels_and_the_runner_scope() {
    let server = Server::new(vec![
        Reply::pb(
            "/v1/notifications",
            wire::PhoneNotificationList {
                connected: true,
                ..Default::default()
            },
        ),
        Reply::pb(
            "/v1/notifications",
            wire::PhoneNotificationList {
                attention_connected: true,
                ..Default::default()
            },
        ),
        Reply::pb(
            "/v1/notifications",
            wire::PhoneNotificationList {
                connected: true,
                attention_connected: true,
                ..Default::default()
            },
        ),
    ])
    .await;
    verify_notifications(&server.rpc()).await.unwrap();
    let calls = server.checked();
    assert_eq!(calls.len(), 3);
    for (_, body) in calls {
        let value = wire::DeviceRequest::decode(body.as_slice()).unwrap();
        assert_eq!(value.phone_id, "phone-local-01");
        assert_eq!(value.owner_id, "local-user");
    }
}

#[tokio::test]
async fn notification_unknown_or_cancelled_observation_cannot_publish_readiness() {
    let server = Server::new(vec![Reply {
        path: "/v1/notifications",
        status: StatusCode::OK,
        body: vec![0xff],
        media: true,
    }])
    .await;
    assert_eq!(
        verify_notifications(&server.rpc()).await.unwrap_err().0,
        "device_setup_protobuf"
    );
    assert_eq!(server.checked().len(), 1);
    let server = Server::new(vec![]).await;
    let rpc = server.rpc();
    rpc.cancelled.cancel();
    assert_eq!(
        verify_notifications(&rpc).await.unwrap_err().0,
        "device_setup_cancelled_remote_may_continue"
    );
    assert!(server.checked().is_empty());
}
fn registry(managed: bool) -> wire::RegistryState {
    wire::RegistryState {
        devices: vec![wire::RegisteredDevice {
            id: "fixture-desktop".into(),
            kind: "desktop".into(),
            runtime_state: if managed { "managed" } else { "registered" }.into(),
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn phone() -> wire::DeviceObservation {
    wire::DeviceObservation {
        phone_id: "phone-local-01".into(),
        root_identity: "0".into(),
        width: 1,
        height: 1,
        png: b"\x89PNG\r\n\x1a\nfixture".to_vec(),
        ..Default::default()
    }
}
fn frame() -> wire::DesktopObservation {
    wire::DesktopObservation {
        device_id: "fixture-desktop".into(),
        width: 1,
        height: 1,
        png: b"\x89PNG\r\n\x1a\nfixture".to_vec(),
        ..Default::default()
    }
}
fn first(managed: bool) -> Vec<Reply> {
    vec![
        Reply::pb("/v1/registry/state", registry(managed)),
        Reply::pb("/v1/observe", phone()),
    ]
}
fn job(intent: &Intent, status: &str) -> wire::DesktopProvisionJob {
    wire::DesktopProvisionJob {
        id: intent.request_id.clone(),
        device_id: intent.device_id.clone(),
        status: status.into(),
        cpus: 2,
        memory_mb: 2048,
        template_id: "debian13-openbox".into(),
        ..Default::default()
    }
}
fn jobs(job: wire::DesktopProvisionJob) -> wire::DeviceLinkState {
    wire::DeviceLinkState {
        jobs: vec![job],
        ..Default::default()
    }
}

#[tokio::test]
async fn managed_retry_only_observes_both_devices_without_writing_intent() {
    let files = Files::new();
    let store = files.store();
    let mut replies = first(true);
    replies.push(Reply::pb("/v1/desktop/observe", frame()));
    let server = Server::new(replies).await;
    activate(&store, &server.rpc()).await.unwrap();
    assert!(store.load().unwrap().is_none());
    let calls = server.checked();
    assert_eq!(calls.len(), 3);
    let request = wire::DeviceRequest::decode(calls[1].1.as_slice()).unwrap();
    assert_eq!(request.phone_id, "phone-local-01");
    assert_eq!(request.owner_id, "local-user");
    assert!(!request.metadata_only);
    assert!(!request.include_ui_tree);
    assert!(!request.include_elements);
}
#[tokio::test]
async fn phone_rejections_never_register_or_provision() {
    for variant in 0..12 {
        let files = Files::new();
        let mut observation = phone();
        match variant {
            5 => observation.png.clear(),
            6 => observation.width = 0,
            7 => observation.height = 0,
            8 => observation.root_identity.clear(),
            9 => observation.root_identity = "2000".into(),
            10 => observation.root_identity = "uid=0(root)".into(),
            11 => observation.phone_id = "other".into(),
            _ => {}
        }
        let mut response = Reply::pb("/v1/observe", observation);
        match variant {
            0 => response.status = StatusCode::UNAUTHORIZED,
            1 => {
                response.status = StatusCode::SERVICE_UNAVAILABLE;
                response.body = b"fixture-private-response".to_vec();
                response.media = false;
            }
            2 => response.media = false,
            3 => response.body = vec![255],
            4 => response.body.clear(),
            _ => {}
        }
        let server = Server::new(vec![
            Reply::pb("/v1/registry/state", registry(false)),
            response,
        ])
        .await;
        let error = activate(&files.store(), &server.rpc())
            .await
            .unwrap_err()
            .to_string();
        assert!(!error.contains("fixture-private"));
        assert_eq!(server.checked().len(), 2);
        assert!(files.store().load().unwrap().is_none());
    }
}
#[tokio::test]
async fn provisioning_unknown_is_observed_and_never_dispatched_again() {
    let files = Files::new();
    let store = files.store();
    let intent = store
        .select(Some("fixture-desktop"), store.resources().unwrap())
        .unwrap();
    assert!(store.begin(&intent, "provision").unwrap());
    let mut replies = first(false);
    replies.push(Reply::pb(
        "/v1/desktop/state",
        wire::DeviceLinkState::default(),
    ));
    let server = Server::new(replies).await;
    assert_eq!(
        activate(&store, &server.rpc()).await.unwrap_err().0,
        "device_setup_provision_unknown"
    );
    assert_eq!(server.checked().len(), 3);
    assert!(!store.begin(&intent, "provision").unwrap());
}
#[tokio::test]
async fn saved_job_is_recovered_without_new_provision_and_requires_live_frame() {
    let files = Files::new();
    let store = files.store();
    let intent = store
        .select(Some("fixture-desktop"), store.resources().unwrap())
        .unwrap();
    store.begin(&intent, "provision").unwrap();
    let mut replies = first(false);
    replies.extend([
        Reply::pb("/v1/desktop/state", jobs(job(&intent, "running"))),
        Reply::pb("/v1/desktop/state", jobs(job(&intent, "completed"))),
        Reply::pb("/v1/desktop/observe", frame()),
    ]);
    let server = Server::new(replies).await;
    activate(&store, &server.rpc()).await.unwrap();
    assert_eq!(server.checked().len(), 5);
}
#[tokio::test]
async fn new_provision_uses_saved_request_and_matching_resources() {
    let files = Files::new();
    let store = files.store();
    let intent = store
        .select(Some("fixture-desktop"), store.resources().unwrap())
        .unwrap();
    let mut replies = first(false);
    replies.extend([
        Reply::pb("/v1/desktop/state", wire::DeviceLinkState::default()),
        Reply::pb("/v1/desktop/provision", job(&intent, "queued")),
        Reply::pb("/v1/desktop/state", jobs(job(&intent, "completed"))),
        Reply::pb("/v1/desktop/observe", frame()),
    ]);
    let server = Server::new(replies).await;
    activate(&store, &server.rpc()).await.unwrap();
    let calls = server.checked();
    let request = wire::DesktopProvisionRequest::decode(calls[3].1.as_slice()).unwrap();
    assert_eq!(request.request_id, intent.request_id);
    assert_eq!(request.device_id, intent.device_id);
    assert_eq!(request.cpus, 2);
    assert_eq!(request.memory_mb, 2048);
    assert_eq!(request.template_id, "debian13-openbox");
    assert!(!store.begin(&intent, "provision").unwrap());
}
#[tokio::test]
async fn failed_job_never_becomes_ready_and_private_error_is_not_returned() {
    let files = Files::new();
    let store = files.store();
    let intent = store
        .select(Some("fixture-desktop"), store.resources().unwrap())
        .unwrap();
    let mut failed = job(&intent, "failed");
    failed.stage = "启动桌面".into();
    failed.error = "OCI runtime fixture-private-secret".into();
    let mut replies = first(false);
    replies.push(Reply::pb("/v1/desktop/state", jobs(failed)));
    let server = Server::new(replies).await;
    assert_eq!(
        activate(&store, &server.rpc()).await.unwrap_err().0,
        "device_setup_provision_failed"
    );
    assert_eq!(server.checked().len(), 3);
}
#[tokio::test]
async fn cancellation_before_request_has_no_network_or_mutation() {
    let files = Files::new();
    let server = Server::new(vec![]).await;
    let rpc = server.rpc();
    rpc.cancelled.cancel();
    assert_eq!(
        activate(&files.store(), &rpc).await.unwrap_err().0,
        "device_setup_cancelled_remote_may_continue"
    );
    assert!(server.checked().is_empty());
    assert!(files.store().load().unwrap().is_none());
}

#[tokio::test]
async fn cancellation_after_dispatch_keeps_the_attempt_unknown_without_replay() {
    let files = Files::new();
    let store = files.store();
    let intent = store
        .select(Some("fixture-desktop"), store.resources().unwrap())
        .unwrap();
    assert!(store.begin(&intent, "provision").unwrap());
    let received = Arc::new(tokio::sync::Notify::new());
    let notification = received.clone();
    let request_id = intent.request_id.clone();
    let router = Router::new().fallback(move |request: Request<Body>| {
        let notification = notification.clone();
        let request_id = request_id.clone();
        async move {
            assert_eq!(request.uri().path(), "/v1/desktop/provision");
            let body = to_bytes(request.into_body(), 1024 * 1024).await.unwrap();
            let request = wire::DesktopProvisionRequest::decode(body).unwrap();
            assert_eq!(request.request_id, request_id);
            notification.notify_one();
            // The remote side has the full request, but no result is available.
            std::future::pending::<Response>().await
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Server {
        trace: Arc::new(Mutex::new(Trace::default())),
        task: tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }),
        port,
    };
    let rpc = server.rpc();
    let operation = rpc.call::<_, wire::DesktopProvisionJob>(
        "desktop/provision",
        wire::DesktopProvisionRequest {
            device_id: intent.device_id.clone(),
            request_id: intent.request_id.clone(),
            cpus: 2,
            memory_mb: 2048,
            template_id: "debian13-openbox".into(),
        },
    );
    tokio::pin!(operation);
    tokio::select! {
        _ = received.notified() => rpc.cancelled.cancel(),
        result = &mut operation => panic!("remote response was withheld: {result:?}"),
        _ = tokio::time::sleep(Duration::from_secs(3)) => panic!("request never reached the remote fixture"),
    }
    assert_eq!(
        operation.await.unwrap_err().0,
        "device_setup_cancelled_remote_may_continue"
    );
    assert!(!store.begin(&intent, "provision").unwrap());
    assert_eq!(store.load().unwrap().unwrap().request_id, intent.request_id);
}
#[test]
fn durable_identity_and_attempts_are_create_once_and_preserve_user_bytes() {
    let files = Files::new();
    fs::write(files.0.join("data/chat-fixture"), b"keep").unwrap();
    let store = files.store();
    let resources = store.resources().unwrap();
    let first = store.select(Some("fixture-desktop"), resources).unwrap();
    let again = store.select(Some("fixture-desktop"), resources).unwrap();
    assert_eq!(first.request_id, again.request_id);
    assert!(store.select(Some("another-device"), resources).is_err());
    assert!(store.begin(&first, "register").unwrap());
    assert!(!store.begin(&again, "register").unwrap());
    assert_eq!(
        fs::read(files.0.join("data/chat-fixture")).unwrap(),
        b"keep"
    );
    fs::write(store.folder_for_test().join("device-setup.json"), b"{").unwrap();
    assert!(store.load().is_err());
}

#[tokio::test]
async fn registration_uses_saved_identity_before_provision() {
    let files = Files::new();
    let store = files.store();
    let intent = store
        .select(Some("fixture-desktop"), store.resources().unwrap())
        .unwrap();
    let replies = vec![
        Reply::pb("/v1/registry/state", wire::RegistryState::default()),
        Reply::pb("/v1/observe", phone()),
        Reply::pb("/v1/registry/device-save", registry(false)),
        Reply::pb("/v1/desktop/state", wire::DeviceLinkState::default()),
        Reply::pb("/v1/desktop/provision", job(&intent, "queued")),
        Reply::pb("/v1/desktop/state", jobs(job(&intent, "completed"))),
        Reply::pb("/v1/desktop/observe", frame()),
    ];
    let server = Server::new(replies).await;
    activate(&store, &server.rpc()).await.unwrap();
    let calls = server.checked();
    let request = wire::DeviceSaveRequest::decode(calls[2].1.as_slice()).unwrap();
    assert_eq!(request.id, intent.device_id);
    assert_eq!(request.expected_revision, 0);
    assert_eq!(request.kind, "desktop");
    assert!(!store.begin(&intent, "register").unwrap());
}
#[tokio::test]
async fn attempted_registration_is_not_repeated_when_observation_cannot_confirm_it() {
    let files = Files::new();
    let store = files.store();
    let intent = store
        .select(Some("fixture-desktop"), store.resources().unwrap())
        .unwrap();
    store.begin(&intent, "register").unwrap();
    let server = Server::new(vec![
        Reply::pb("/v1/registry/state", wire::RegistryState::default()),
        Reply::pb("/v1/observe", phone()),
    ])
    .await;
    assert_eq!(
        activate(&store, &server.rpc()).await.unwrap_err().0,
        "device_setup_registration_unknown"
    );
    assert_eq!(server.checked().len(), 2);
}
#[tokio::test]
async fn an_unrelated_completed_job_does_not_resolve_an_unknown_request() {
    let files = Files::new();
    let store = files.store();
    let intent = store
        .select(Some("fixture-desktop"), store.resources().unwrap())
        .unwrap();
    store.begin(&intent, "provision").unwrap();
    let mut unrelated = job(&intent, "completed");
    unrelated.id = Uuid::new_v4().to_string();
    unrelated.device_id = "other-device".into();
    let mut replies = first(false);
    replies.push(Reply::pb("/v1/desktop/state", jobs(unrelated)));
    let server = Server::new(replies).await;
    assert_eq!(
        activate(&store, &server.rpc()).await.unwrap_err().0,
        "device_setup_provision_unknown"
    );
    assert_eq!(server.checked().len(), 3);
}
