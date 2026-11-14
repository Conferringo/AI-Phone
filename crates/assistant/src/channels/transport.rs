use super::{
    State,
    store::{Config, Result},
};
use futures_util::future::BoxFuture;
use serde_json::Value;
use std::sync::{Arc, RwLock};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub(super) enum Payload {
    Text(String),
    Image(Vec<u8>),
}
pub(super) enum Transport {
    Telegram(Arc<super::telegram::Telegram>),
    Wecom(Arc<super::wecom::Wecom>),
    Feishu(Arc<super::feishu::Feishu>),
    Qq(Arc<super::qq::Qq>),
    WhatsApp(Arc<super::whatsapp::WhatsApp>),
}
impl Transport {
    fn send<'a>(
        &'a self,
        target: &'a Value,
        payload: Payload,
        id: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        match self {
            Self::Telegram(client) => client.send(target, payload, id),
            Self::Wecom(client) => client.send(target, payload, id),
            Self::Feishu(client) => client.send(target, payload, id),
            Self::Qq(client) => client.send(target, payload, id),
            Self::WhatsApp(client) => client.send(target, payload, id),
        }
    }
    pub fn unlink(&self) -> Option<BoxFuture<'_, Result<bool>>> {
        match self {
            Self::WhatsApp(client) => Some(client.logout()),
            _ => None,
        }
    }
}
#[derive(Default)]
pub(super) struct Live {
    pub state: String,
    pub error: String,
    pub qr_png: Vec<u8>,
}
#[derive(Clone)]
pub(super) struct Context {
    pub state: Arc<State>,
    pub key: String,
    pub version: String,
    pub live: Arc<RwLock<Live>>,
    pub cancel: CancellationToken,
    pub tasks: TaskTracker,
}
impl Context {
    pub async fn request(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        if self.cancel.is_cancelled() {
            return Err("channel_stopped".into());
        }
        Ok(
            tokio::select! {_ = self.cancel.cancelled()=>return Err("channel_stopped".into()),response=request.send()=>response.map_err(reqwest::Error::without_url)?},
        )
    }
    pub fn status(&self, state: &str, error: &str) {
        let mut live = self.live.write().unwrap();
        live.state = state.into();
        live.error = error.into();
        if state == "connected" {
            live.qr_png.clear();
        }
        self.state.changed.notify_one();
    }
    pub async fn message(
        &self,
        sender: String,
        id: String,
        text: String,
        target: Value,
        direct: bool,
        bot: bool,
    ) -> Result<()> {
        if bot || sender.is_empty() {
            return Ok(());
        }
        self.state
            .receive(self, sender, id, text, target, direct)
            .await
    }
    pub async fn offset(&self, offset: i64) -> Result<()> {
        let mut store = self.state.store.lock().await;
        if let Some(config) = store
            .vault
            .configs
            .get_mut(&self.key)
            .filter(|config| config.version == self.version)
        {
            config.offset = offset;
            store.save()?;
        }
        Ok(())
    }
}
#[derive(Clone)]
pub(super) struct Connection {
    pub context: Context,
    pub transport: Arc<Transport>,
}
impl Connection {
    pub async fn stop(&self) {
        self.context.cancel.cancel();
        self.context.tasks.close();
        self.context.tasks.wait().await;
    }
    pub async fn send(&self, target: &Value, payload: Payload, id: &str) -> Result<()> {
        let active = self
            .context
            .state
            .store
            .lock()
            .await
            .vault
            .configs
            .get(&self.context.key)
            .is_some_and(|config| config.version == self.context.version);
        if !active || self.context.cancel.is_cancelled() {
            return Err("channel_binding_changed".into());
        }
        self.transport.send(target, payload, id).await
    }
}
pub(super) fn start(state: Arc<State>, key: String, config: Config) -> Result<Connection> {
    let context = Context {
        state,
        key,
        version: config.version.clone(),
        live: Arc::new(RwLock::new(Live {
            state: "connecting".into(),
            ..Default::default()
        })),
        cancel: CancellationToken::new(),
        tasks: TaskTracker::new(),
    };
    let provider = context
        .key
        .split_once('/')
        .ok_or("invalid_channel_scope")?
        .1;
    let transport = match provider {
        "telegram" => Transport::Telegram(super::telegram::start(context.clone(), config)?),
        "wecom" => Transport::Wecom(super::wecom::start(context.clone(), config)?),
        "feishu" => Transport::Feishu(super::feishu::start(context.clone(), config)?),
        "qq" => Transport::Qq(super::qq::start(context.clone(), config)?),
        "whatsapp" => Transport::WhatsApp(super::whatsapp::start(context.clone(), config)?),
        _ => return Err("invalid_channel_provider".into()),
    };
    Ok(Connection {
        context,
        transport: Arc::new(transport),
    })
}
