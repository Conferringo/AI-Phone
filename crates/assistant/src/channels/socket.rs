use super::store::Result;
use futures_util::{SinkExt, StreamExt, stream::SplitStream};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message,
        handshake::{client::generate_key, derive_accept_key},
        protocol::Role,
    },
};
use tokio_util::sync::CancellationToken;

pub(super) type UpgradedSocket = WebSocketStream<reqwest::Upgraded>;
pub(super) type Incoming = SplitStream<UpgradedSocket>;
#[derive(Clone)]
pub(super) struct Socket {
    outgoing: mpsc::UnboundedSender<Message>,
}
impl Socket {
    pub fn json(&self, value: Value) -> Result<()> {
        self.send(Message::Text(value.to_string().into()))
    }
    pub fn send(&self, message: Message) -> Result<()> {
        self.outgoing
            .send(message)
            .map_err(|_| "channel_socket_closed".into())
    }
}
pub(super) async fn upgrade(
    client: &reqwest::Client,
    url: &str,
    origin: Option<&str>,
    cancel: &CancellationToken,
) -> Result<UpgradedSocket> {
    if cancel.is_cancelled() {
        return Err("channel_stopped".into());
    }
    let mut url = reqwest::Url::parse(url)?;
    let scheme = match url.scheme() {
        "wss" => "https",
        "ws" => "http",
        _ => return Err("channel_socket_scheme".into()),
    };
    url.set_scheme(scheme)
        .map_err(|_| "channel_socket_scheme")?;
    let key = generate_key();
    let mut request = client
        .get(url)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", &key);
    if let Some(origin) = origin {
        request = request.header("Origin", origin);
    }
    let response = tokio::select! {_ = cancel.cancelled()=>return Err("channel_stopped".into()),response=request.send()=>response.map_err(reqwest::Error::without_url)?};
    if response.status() != reqwest::StatusCode::SWITCHING_PROTOCOLS
        || response
            .headers()
            .get("Sec-WebSocket-Accept")
            .and_then(|value| value.to_str().ok())
            != Some(derive_accept_key(key.as_bytes()).as_str())
    {
        return Err("channel_socket_upgrade_refused".into());
    }
    let stream = response.upgrade().await?;
    Ok(WebSocketStream::from_raw_socket(stream, Role::Client, None).await)
}
pub(super) async fn connect(
    client: &reqwest::Client,
    url: &str,
    cancel: CancellationToken,
) -> Result<(Socket, Incoming)> {
    let socket = upgrade(client, url, None, &cancel).await?;
    let (mut writer, reader) = socket.split();
    let (outgoing, mut messages) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let forwarding = async move {
            while let Some(message) = messages.recv().await {
                writer.send(message).await?;
            }
            Ok::<_, tokio_tungstenite::tungstenite::Error>(())
        };
        tokio::select! {_ = cancel.cancelled()=>{},_ = forwarding=>{}}
    });
    Ok((Socket { outgoing }, reader))
}
