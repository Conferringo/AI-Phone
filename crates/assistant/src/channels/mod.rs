mod feishu;
mod qq;
mod socket;
mod store;
mod telegram;
mod transport;
mod wecom;
mod whatsapp;
mod whatsapp_pg;
mod worker;

use ai_phone_api::wire;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State as Extract},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use prost::Message;
use reqwest::Client;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::Path as FilePath,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use store::{Config, Job, PROVIDERS, Pairing, Result, Stage, Store, event_id, now, scope};
use tokio::sync::{Mutex, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use transport::{Connection, Context, Payload};
use uuid::Uuid;

struct State {
    db: sqlx::PgPool,
    token: String,
    runner_token: String,
    local: Client,
    store: Mutex<Store>,
    connections: RwLock<BTreeMap<String, Connection>>,
    mutation: Mutex<()>,
    changed: Notify,
    broker_connected: AtomicBool,
    stop: CancellationToken,
}
impl State {
    fn external_client(proxy: &str, timeout: Duration) -> Result<Client> {
        let mut builder = Client::builder()
            .no_proxy()
            .http1_only()
            .tcp_nodelay(true)
            .timeout(timeout);
        let host_proxy = std::env::var("AI_PHONE_HOST_PROXY").unwrap_or_default();
        let selected = if proxy.is_empty() { &host_proxy } else { proxy };
        if !selected.is_empty() {
            builder = builder.proxy(reqwest::Proxy::all(selected)?);
        }
        Ok(builder.build()?)
    }
    async fn rpc<I: Message, O: Message + Default>(&self, path: &str, input: I) -> Result<O> {
        let response = self
            .local
            .post(format!("http://127.0.0.1:17863/v1/{path}"))
            .bearer_auth(&self.token)
            .header("content-type", crate::MEDIA)
            .body(input.encode_to_vec())
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Box::new(RpcError(response.status().as_u16())));
        }
        Ok(O::decode(response.bytes().await?)?)
    }
    async fn find_agent(&self, id: &str) -> Result<Option<String>> {
        let registry: wire::RegistryState =
            self.rpc("registry/state", wire::RegistryRequest {}).await?;
        let selected = if id.is_empty() {
            registry.active_agent_id
        } else {
            id.to_owned()
        };
        if selected.is_empty() {
            return Ok(None);
        }
        if !registry.agents.iter().any(|agent| agent.id == selected) {
            return Err("agent_not_found".into());
        }
        Ok(Some(selected))
    }
    async fn agent(&self, id: &str) -> Result<String> {
        self.find_agent(id)
            .await?
            .ok_or_else(|| "agent_not_found".into())
    }
    async fn device(&self, agent: &str) -> Result<String> {
        let registry: wire::RegistryState =
            self.rpc("registry/state", wire::RegistryRequest {}).await?;
        let owner = registry
            .agents
            .iter()
            .find(|a| a.id == agent)
            .ok_or("agent_not_found")?;
        let devices: Vec<_> = registry
            .devices
            .iter()
            .filter(|device| {
                device.runtime_state == "managed" && owner.device_ids.contains(&device.id)
            })
            .collect();
        devices
            .iter()
            .find(|device| device.default_agent_id == agent)
            .or(devices.first())
            .map(|device| device.id.clone())
            .ok_or_else(|| "device_unavailable".into())
    }
    async fn restart(self: &Arc<Self>, key: String, config: Config) -> Result<()> {
        let previous = self.connections.write().await.remove(&key);
        if let Some(previous) = previous {
            previous.stop().await;
        }
        let connection = transport::start(self.clone(), key.clone(), config)?;
        self.connections.write().await.insert(key, connection);
        self.changed.notify_one();
        Ok(())
    }
    async fn list(&self, agent: &str) -> wire::ChannelListResponse {
        let store = self.store.lock().await;
        let connections = self.connections.read().await;
        wire::ChannelListResponse {
            bindings: PROVIDERS
                .iter()
                .map(|provider| {
                    let key = scope(agent, provider);
                    let config = store.vault.configs.get(&key);
                    let connection = connections.get(&key);
                    let live = connection.map(|connection| connection.context.live.read().unwrap());
                    let pairing = config.and_then(|config| config.pairing.as_ref());
                    let unknown = store.vault.jobs.iter().any(|job| {
                        job.agent_id == agent
                            && job.provider == *provider
                            && job.stage == Stage::Unknown
                    });
                    wire::ChannelBinding {
                        provider: (*provider).into(),
                        app_id: config
                            .map(|config| config.app_id.clone())
                            .unwrap_or_default(),
                        status: if config.is_none() {
                            "unbound".into()
                        } else {
                            live.as_ref()
                                .map(|live| live.state.clone())
                                .unwrap_or_else(|| "connecting".into())
                        },
                        error: if unknown {
                            "Delivery unconfirmed; check the original app".into()
                        } else {
                            live.as_ref()
                                .map(|live| live.error.clone())
                                .filter(|s| !s.is_empty())
                                .or_else(|| store.vault.disconnect_warnings.get(&key).cloned())
                                .unwrap_or_default()
                        },
                        configured: config.is_some(),
                        paired: config.is_some_and(|config| !config.allowed.is_empty()),
                        pairing_command: pairing
                            .filter(|pair| pair.expires > now())
                            .map(|pair| format!("/pair {}", pair.code))
                            .unwrap_or_default(),
                        pairing_expires_at_ms: pairing.map(|pair| pair.expires).unwrap_or_default(),
                        qr_png: live
                            .as_ref()
                            .map(|live| live.qr_png.clone())
                            .unwrap_or_default(),
                        proxy_url: config
                            .map(|config| config.proxy_url.clone())
                            .unwrap_or_default(),
                    }
                })
                .collect(),
        }
    }
    async fn cancel_jobs(&self, agent: &str, provider: &str) -> Result<()> {
        let mut store = self.store.lock().await;
        let mut runs = vec![];
        for job in
            store.vault.jobs.iter_mut().filter(|job| {
                job.agent_id == agent && job.provider == provider && job.stage.pending()
            })
        {
            job.stage = Stage::Canceled;
            job.png = None;
            if let Some(run) = &job.run_id {
                runs.push(run.clone());
            }
        }
        store.save()?;
        drop(store);
        for run_id in runs {
            let _: Result<wire::ChatState> = self
                .rpc(
                    "cancel",
                    wire::ChatRunRequest {
                        run_id,
                        ..Default::default()
                    },
                )
                .await;
        }
        Ok(())
    }
    async fn receive(
        &self,
        context: &Context,
        sender: String,
        id: String,
        text: String,
        target: Value,
        direct: bool,
    ) -> Result<()> {
        let mut store = self.store.lock().await;
        let (agent, provider) = context.key.split_once('/').ok_or("invalid_channel_scope")?;
        let Some(config) = store
            .vault
            .configs
            .get_mut(&context.key)
            .filter(|config| config.version == context.version)
        else {
            return Ok(());
        };
        if text.trim().starts_with("/pair ") || text.trim().starts_with("配对 ") {
            let paired = config
                .pairing
                .as_mut()
                .is_some_and(|pair| pair.matches(&text, direct));
            if paired {
                config.allowed = vec![sender];
                config.pairing = None;
            }
            store.save()?;
            drop(store);
            if paired {
                let connection = self.connections.read().await.get(&context.key).cloned();
                if let Some(connection) = connection {
                    tokio::spawn(async move {
                        let _ = connection
                            .send(
                                &target,
                                Payload::Text("Paired. Send device instructions here.".into()),
                                &id,
                            )
                            .await;
                    });
                }
            }
            return Ok(());
        }
        if !config.allowed.contains(&sender)
            || id.is_empty()
            || id.len() > 512
            || text.trim().is_empty()
            || text.len() > 24_000
        {
            return Ok(());
        }
        let event = event_id(provider, &config.app_id, &id);
        if store.vault.jobs.len() >= 10_000
            || store
                .vault
                .jobs
                .iter()
                .any(|job| job.agent_id == agent && job.id == event)
            || store
                .vault
                .jobs
                .iter()
                .filter(|job| job.stage.pending())
                .count()
                >= 200
        {
            return Ok(());
        }
        let screenshot = text.contains("截图")
            || text.contains("截屏")
            || text.to_lowercase().contains("screenshot");
        let screenshot_only = matches!(
            text.trim().to_lowercase().as_str(),
            "截图" | "截屏" | "手机截图" | "screenshot" | "phone screenshot" | "screen screenshot"
        );
        store.vault.jobs.push(Job {
            agent_id: agent.into(),
            device_id: None,
            provider: provider.into(),
            version: context.version.clone(),
            id: event,
            text: format!("[{provider} · {sender}]\n{text}"),
            target,
            run_id: None,
            stage: Stage::Queued,
            at: now(),
            outbound: false,
            push_id: None,
            png: None,
            fingerprint: None,
            text_sent: false,
            image_sent: false,
            screenshot,
            screenshot_only,
            error: String::new(),
        });
        store.save()?;
        drop(store);
        self.changed.notify_one();
        Ok(())
    }
    async fn push(&self, input: wire::ChannelPushRequest) -> Result<wire::ChannelPushResult> {
        let agent = self.agent(&input.agent_id).await?;
        if Uuid::parse_str(&input.request_id).is_err()
            || input.text.len() > 12_000
            || (input.text.trim().is_empty() && input.png.is_empty())
            || (!input.png.is_empty() && !valid_png(&input.png))
        {
            return Err("invalid_push".into());
        }
        let mut providers: Vec<String> = if input.providers.is_empty() {
            PROVIDERS
                .iter()
                .map(|provider| (*provider).into())
                .collect()
        } else {
            input.providers
        };
        let mut selected = std::collections::BTreeSet::new();
        providers.retain(|provider| selected.insert(provider.clone()));
        let mut hash = Sha256::new();
        hash.update(input.text.as_bytes());
        hash.update([0]);
        hash.update(&input.png);
        let fingerprint = format!("{:x}", hash.finalize());
        let mut store = self.store.lock().await;
        let mut deliveries = vec![];
        for provider in providers {
            if !PROVIDERS.contains(&provider.as_str()) {
                return Err("invalid_provider".into());
            }
            let Some(config) = store.vault.configs.get(&scope(&agent, &provider)).cloned() else {
                deliveries.push(delivery(&provider, "unbound", "Not configured"));
                continue;
            };
            let Some(owner) = config.allowed.first() else {
                deliveries.push(delivery(&provider, "unpaired", "Not paired"));
                continue;
            };
            let id = event_id(&provider, &config.app_id, &input.request_id);
            if let Some(job) = store
                .vault
                .jobs
                .iter()
                .find(|job| job.agent_id == agent && job.id == id)
            {
                if job.fingerprint.as_ref() != Some(&fingerprint) {
                    return Err("request_conflict".into());
                }
                deliveries.push(delivery(&provider, job.stage.name(), &job.error));
                continue;
            }
            if store.vault.jobs.len() >= 10_000
                || store
                    .vault
                    .jobs
                    .iter()
                    .filter(|job| job.stage.pending())
                    .count()
                    >= 200
            {
                deliveries.push(delivery(&provider, "failed", "Queue full"));
                continue;
            }
            let target = match provider.as_str() {
                "feishu" => json!({"openId":owner}),
                "qq" => json!({"scope":"c2c","targetId":owner}),
                _ => json!({"chat":owner}),
            };
            store.vault.jobs.push(Job {
                agent_id: agent.clone(),
                device_id: None,
                provider: provider.clone(),
                version: config.version,
                id,
                text: input.text.clone(),
                target,
                run_id: None,
                stage: Stage::Queued,
                at: now(),
                outbound: true,
                push_id: Some(input.request_id.clone()),
                png: (!input.png.is_empty()).then(|| STANDARD.encode(&input.png)),
                fingerprint: Some(fingerprint.clone()),
                text_sent: false,
                image_sent: false,
                screenshot: false,
                screenshot_only: false,
                error: String::new(),
            });
            deliveries.push(delivery(&provider, "queued", ""));
        }
        store.save()?;
        drop(store);
        self.changed.notify_one();
        Ok(wire::ChannelPushResult { deliveries })
    }
}
fn valid_png(bytes: &[u8]) -> bool {
    bytes.len() >= 8
        && bytes.len() <= 4 * 1024 * 1024
        && bytes[..8] == [137, 80, 78, 71, 13, 10, 26, 10]
}
#[derive(Debug)]
struct RpcError(u16);
impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "assistant_http_{}", self.0)
    }
}
impl std::error::Error for RpcError {}
fn delivery(provider: &str, status: &str, error: &str) -> wire::ChannelDelivery {
    wire::ChannelDelivery {
        provider: provider.into(),
        status: status.into(),
        error: error.into(),
    }
}
fn pb<T: Message>(value: T) -> Response {
    (
        [
            (axum::http::header::CONTENT_TYPE, crate::MEDIA),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        value.encode_to_vec(),
    )
        .into_response()
}
fn error(status: StatusCode, code: &str) -> Response {
    (
        status,
        pb(wire::ApiError {
            code: code.into(),
            message: code.into(),
        }),
    )
        .into_response()
}

async fn handle(
    Extract(state): Extract<Arc<State>>,
    Path(operation): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        != Some(format!("Bearer {}", state.token).as_str())
    {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    if headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        != Some(crate::MEDIA)
    {
        return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type");
    }
    match dispatch(&state, &operation, &body).await {
        Ok(response) => response,
        Err(_) => error(StatusCode::BAD_REQUEST, "invalid_channel_request"),
    }
}
async fn dispatch(state: &Arc<State>, operation: &str, body: &[u8]) -> Result<Response> {
    if operation == "push" {
        return Ok(pb(state
            .push(wire::ChannelPushRequest::decode(body)?)
            .await?));
    }
    if operation == "push-status" {
        let input = wire::ChannelPushStatusRequest::decode(body)?;
        let agent = state.agent(&input.agent_id).await?;
        Uuid::parse_str(&input.request_id)?;
        let store = state.store.lock().await;
        return Ok(pb(wire::ChannelPushResult {
            deliveries: PROVIDERS
                .iter()
                .map(|provider| {
                    let job = store.vault.jobs.iter().find(|job| {
                        job.agent_id == agent
                            && job.provider == *provider
                            && job.push_id.as_deref() == Some(input.request_id.as_str())
                    });
                    delivery(
                        provider,
                        job.map(|job| job.stage.name()).unwrap_or_else(|| {
                            if store.vault.configs.contains_key(&scope(&agent, provider)) {
                                "not_found"
                            } else {
                                "unbound"
                            }
                        }),
                        job.map(|job| job.error.as_str()).unwrap_or_default(),
                    )
                })
                .collect(),
        }));
    }
    if body.len() > 65_536 {
        return Ok(error(StatusCode::PAYLOAD_TOO_LARGE, "too_large"));
    }
    if operation == "list" {
        let input = wire::ChannelListRequest::decode(body)?;
        let channels = match state.find_agent(&input.agent_id).await? {
            Some(agent) => state.list(&agent).await,
            None => wire::ChannelListResponse::default(),
        };
        return Ok(pb(channels));
    }
    let _mutation = state.mutation.lock().await;
    let (agent, provider, binding) = match operation {
        "bind" => {
            let input = wire::ChannelBindRequest::decode(body)?;
            (
                state.agent(&input.agent_id).await?,
                input.provider.clone(),
                Some(input),
            )
        }
        "pair" => {
            let input = wire::ChannelPairRequest::decode(body)?;
            (state.agent(&input.agent_id).await?, input.provider, None)
        }
        "unbind" => {
            let input = wire::ChannelUnbindRequest::decode(body)?;
            (state.agent(&input.agent_id).await?, input.provider, None)
        }
        _ => return Ok(error(StatusCode::NOT_FOUND, "not_found")),
    };
    if !PROVIDERS.contains(&provider.as_str()) {
        return Err("invalid_provider".into());
    }
    let key = scope(&agent, &provider);
    if operation == "unbind" {
        let connection = state.connections.write().await.remove(&key);
        {
            let mut store = state.store.lock().await;
            store.vault.configs.remove(&key);
            store.save()?;
        }
        if let Some(connection) = connection {
            if let Some(unlink) = connection.transport.unlink()
                && !matches!(unlink.await, Ok(true))
            {
                state
                    .store
                    .lock()
                    .await
                    .vault
                    .disconnect_warnings
                    .insert(key.clone(), "Remote logout unconfirmed".into());
            }
            connection.stop().await;
        }
        state.store.lock().await.save()?;
        state.cancel_jobs(&agent, &provider).await?;
        return Ok(pb(state.list(&agent).await));
    }
    let mut store = state.store.lock().await;
    let old = store.vault.configs.get(&key).cloned();
    let config = if operation == "pair" {
        let mut config = old.ok_or("channel_not_configured")?;
        config.allowed.clear();
        config.pairing = Some(Pairing::new());
        config.version = Uuid::new_v4().to_string();
        config
    } else {
        let input = binding.ok_or("invalid_binding")?;
        let mut config = Config {
            app_id: input.app_id.trim().into(),
            secret: if input.secret.is_empty() {
                old.as_ref()
                    .map(|old| old.secret.clone())
                    .unwrap_or_default()
            } else {
                input.secret
            },
            allowed: vec![],
            version: Uuid::new_v4().to_string(),
            pairing: None,
            offset: 0,
            proxy_url: input.proxy_url,
            identity: None,
        };
        if provider == "telegram" {
            config.app_id = telegram::identity(&config.secret)
                .ok_or("telegram_token")?
                .into();
        }
        if provider == "whatsapp" {
            config.app_id = old
                .as_ref()
                .filter(|_| !input.reset_session)
                .map(|old| old.app_id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            config.secret = old
                .as_ref()
                .map(|old| old.secret.clone())
                .unwrap_or_else(|| {
                    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
                });
        }
        if let Some(old) = old.filter(|old| old.app_id == config.app_id && !input.reset_session) {
            config.allowed = old.allowed;
            config.offset = old.offset;
            config.identity = old.identity;
        }
        if config.app_id.is_empty()
            || config.app_id.len() > 128
            || !config
                .app_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            || !(8..=1024).contains(&config.secret.len())
        {
            return Err("invalid_binding".into());
        }
        if store.vault.configs.iter().any(|(other, c)| {
            other != &key && other.ends_with(&format!("/{provider}")) && c.app_id == config.app_id
        }) {
            return Err("channel_bot_in_use".into());
        }
        if config.allowed.is_empty() {
            config.pairing = Some(Pairing::new());
        }
        config
    };
    store.vault.configs.insert(key.clone(), config.clone());
    store.save()?;
    drop(store);
    state.cancel_jobs(&agent, &provider).await?;
    state.restart(key, config).await?;
    Ok(pb(state.list(&agent).await))
}

pub async fn run(root: &FilePath) -> Result<()> {
    let token = fs::read_to_string(root.join("data/assistant/token"))?
        .trim()
        .to_owned();
    let runner_token = fs::read_to_string(root.join("data/phone-runner/token"))?
        .trim()
        .to_owned();
    let state = Arc::new(State {
        db: crate::database::connect(root, false)
            .await
            .map_err(|error| error.to_string())?,
        token,
        runner_token,
        local: Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()?,
        store: Mutex::new(Store::open(root)?),
        connections: RwLock::new(BTreeMap::new()),
        mutation: Mutex::new(()),
        changed: Notify::new(),
        broker_connected: AtomicBool::new(false),
        stop: CancellationToken::new(),
    });
    let configs = state.store.lock().await.vault.configs.clone();
    for (key, config) in configs {
        state.restart(key, config).await?;
    }
    let worker = tokio::spawn(worker::run(state.clone()));
    let subscriber = crate::redis_events::client().map(|redis| {
        let state = state.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = state.stop.cancelled() => {}
                _ = crate::redis_events::subscribe(redis, &state.broker_connected, &state.changed) => {}
            }
        })
    });
    let router = Router::new()
        .route("/v1/{operation}", post(handle))
        .layer(DefaultBodyLimit::max(5 * 1024 * 1024))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 17864)).await?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::select! {_ = tokio::signal::ctrl_c()=>{},_ = terminate.recv()=>{}}
        })
        .await?;
    state.stop.cancel();
    let connections = std::mem::take(&mut *state.connections.write().await);
    for connection in connections.into_values() {
        connection.stop().await;
    }
    worker.await?;
    if let Some(subscriber) = subscriber {
        subscriber.await?;
    }
    state.store.lock().await.save()?;
    Ok(())
}
