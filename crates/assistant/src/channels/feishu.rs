use super::{
    State, socket,
    store::{Config, Result},
    transport::{Context, Payload},
};
use futures_util::{FutureExt, StreamExt, future::BoxFuture};
use prost::Message;
use reqwest::{
    Client,
    multipart::{Form, Part},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message as Packet;

#[derive(Clone, PartialEq, Message)]
struct Header {
    #[prost(string, required, tag = "1")]
    key: String,
    #[prost(string, required, tag = "2")]
    value: String,
}
#[derive(Clone, PartialEq, Message)]
struct Frame {
    #[prost(uint64, required, tag = "1")]
    seq_id: u64,
    #[prost(uint64, required, tag = "2")]
    log_id: u64,
    #[prost(int32, required, tag = "3")]
    service: i32,
    #[prost(int32, required, tag = "4")]
    method: i32,
    #[prost(message, repeated, tag = "5")]
    headers: Vec<Header>,
    #[prost(string, optional, tag = "6")]
    payload_encoding: Option<String>,
    #[prost(string, optional, tag = "7")]
    payload_type: Option<String>,
    #[prost(bytes = "vec", optional, tag = "8")]
    payload: Option<Vec<u8>>,
    #[prost(string, optional, tag = "9")]
    log_id_new: Option<String>,
}
impl Frame {
    fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.key == key)
            .map(|header| header.value.as_str())
    }
}
struct Token {
    value: String,
    expires: Instant,
}
pub(super) struct Feishu {
    client: Client,
    config: Config,
    context: Context,
    token: Mutex<Option<Token>>,
    reconnect: AtomicU64,
}
impl Feishu {
    async fn token(&self) -> Result<String> {
        let mut cached = self.token.lock().await;
        if let Some(token) = cached
            .as_ref()
            .filter(|token| Instant::now() < token.expires)
        {
            return Ok(token.value.clone());
        }
        let response = self
            .context
            .request(
                self.client
                    .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
                    .json(&json!({"app_id":self.config.app_id,"app_secret":self.config.secret})),
            )
            .await?
            .error_for_status()?;
        let value: Value = response.json().await?;
        if value["code"] != 0 {
            return Err("feishu_authentication".into());
        }
        let token = value["tenant_access_token"]
            .as_str()
            .ok_or("feishu_token")?
            .to_owned();
        let expires = value["expire"].as_u64().ok_or("feishu_token_expiry")?;
        *cached = Some(Token {
            value: token.clone(),
            expires: Instant::now() + Duration::from_secs(expires.saturating_sub(60)),
        });
        Ok(token)
    }
    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let request = self
            .client
            .post(format!("https://open.feishu.cn/open-apis{path}"))
            .bearer_auth(self.token().await?)
            .json(&body);
        let response=tokio::select!{_ = self.context.cancel.cancelled()=>return Err("channel_stopped".into()),result=request.send()=>result?}.error_for_status()?;
        let value: Value = response.json().await?;
        if value["code"] != 0 {
            return Err(format!("feishu_error_{}", value["code"]).into());
        }
        Ok(value["data"].clone())
    }
    async fn image(&self, bytes: Vec<u8>) -> Result<String> {
        let form = Form::new().text("image_type", "message").part(
            "image",
            Part::bytes(bytes)
                .file_name("screen.png")
                .mime_str("image/png")?,
        );
        let request = self
            .client
            .post("https://open.feishu.cn/open-apis/im/v1/images")
            .bearer_auth(self.token().await?)
            .multipart(form);
        let response=tokio::select!{_ = self.context.cancel.cancelled()=>return Err("channel_stopped".into()),result=request.send()=>result?}.error_for_status()?;
        let value: Value = response.json().await?;
        if value["code"] != 0 {
            return Err("feishu_image_upload".into());
        }
        Ok(value["data"]["image_key"]
            .as_str()
            .ok_or("feishu_image_key")?
            .into())
    }
    async fn event(&self, payload: &[u8]) -> Result<()> {
        let value: Value = serde_json::from_slice(payload)?;
        if value["header"]["event_type"] != "im.message.receive_v1" {
            return Ok(());
        }
        let data = &value["event"];
        let message = &data["message"];
        if message["message_type"] != "text"
            || (message["chat_type"] == "group"
                && message["mentions"]
                    .as_array()
                    .is_none_or(|mentions| mentions.is_empty()))
        {
            return Ok(());
        }
        let content: Value = serde_json::from_str(
            message["content"]
                .as_str()
                .ok_or("feishu_message_content")?,
        )?;
        let sender = data["sender"]["sender_id"]["open_id"]
            .as_str()
            .ok_or("feishu_sender")?;
        let id = message["message_id"].as_str().ok_or("feishu_message_id")?;
        self.context
            .message(
                sender.into(),
                id.into(),
                content["text"].as_str().ok_or("feishu_text")?.into(),
                json!({"message":id}),
                message["chat_type"] == "p2p",
                data["sender"]["sender_type"] != "user",
            )
            .await
    }
    async fn session(&self) -> Result<()> {
        let response = self
            .context
            .request(
                self.client
                    .post("https://open.feishu.cn/callback/ws/endpoint")
                    .json(&json!({"AppID":self.config.app_id,"AppSecret":self.config.secret})),
            )
            .await?
            .error_for_status()?;
        let configuration: Value = response.json().await?;
        if configuration["code"] != 0 {
            return Err("feishu_authentication".into());
        }
        let data = &configuration["data"];
        let url = reqwest::Url::parse(data["URL"].as_str().ok_or("feishu_gateway")?)?;
        let service: i32 = url
            .query_pairs()
            .find(|(key, _)| key == "service_id")
            .ok_or("feishu_service")?
            .1
            .parse()?;
        let interval = Arc::new(AtomicU64::new(
            data["ClientConfig"]["PingInterval"]
                .as_u64()
                .ok_or("feishu_ping_interval")?,
        ));
        self.reconnect.store(
            data["ClientConfig"]["ReconnectInterval"]
                .as_u64()
                .ok_or("feishu_reconnect_interval")?,
            Ordering::Relaxed,
        );
        let cancel = self.context.cancel.child_token();
        let (socket, mut incoming) =
            socket::connect(&self.client, url.as_str(), cancel.clone()).await?;
        self.context.status("connected", "");
        let receive = async {
            let mut fragments: HashMap<String, Vec<Option<Vec<u8>>>> = HashMap::new();
            while let Some(packet) = incoming.next().await {
                let packet = packet?;
                if !packet.is_binary() {
                    continue;
                }
                let mut frame = Frame::decode(packet.into_data())?;
                if frame.method == 0 && frame.header("type") == Some("pong") {
                    if let Some(payload) = &frame.payload {
                        let pong: Value = serde_json::from_slice(payload)?;
                        interval.store(
                            pong["PingInterval"]
                                .as_u64()
                                .ok_or("feishu_ping_interval")?,
                            Ordering::Relaxed,
                        );
                    }
                    continue;
                }
                if frame.method != 1 || frame.header("type") != Some("event") {
                    continue;
                }
                let id = frame
                    .header("message_id")
                    .ok_or("feishu_event_id")?
                    .to_owned();
                let total: usize = frame
                    .header("sum")
                    .ok_or("feishu_fragment_count")?
                    .parse()?;
                let index: usize = frame
                    .header("seq")
                    .ok_or("feishu_fragment_index")?
                    .parse()?;
                let parts = fragments
                    .entry(id.clone())
                    .or_insert_with(|| vec![None; total]);
                *parts.get_mut(index).ok_or("feishu_fragment_index")? = frame.payload.take();
                if parts.iter().any(Option::is_none) {
                    continue;
                }
                let payload: Vec<_> = fragments
                    .remove(&id)
                    .unwrap()
                    .into_iter()
                    .flatten()
                    .flatten()
                    .collect();
                let code = if self.event(&payload).await.is_ok() {
                    200
                } else {
                    500
                };
                frame.payload = Some(serde_json::to_vec(&json!({"code":code}))?);
                socket.send(Packet::Binary(frame.encode_to_vec().into()))?;
            }
            Err::<(), Box<dyn std::error::Error + Send + Sync>>("feishu_connection_closed".into())
        };
        let heartbeat = async {
            while !self.context.cancel.is_cancelled() {
                let frame = Frame {
                    service,
                    method: 0,
                    headers: vec![Header {
                        key: "type".into(),
                        value: "ping".into(),
                    }],
                    ..Default::default()
                };
                socket.send(Packet::Binary(frame.encode_to_vec().into()))?;
                tokio::time::sleep(Duration::from_secs(interval.load(Ordering::Relaxed))).await;
            }
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        };
        let result = tokio::select! {_ = self.context.cancel.cancelled()=>Ok(()),result=receive=>result,result=heartbeat=>result};
        cancel.cancel();
        result
    }
}
impl Feishu {
    pub fn send<'a>(
        &'a self,
        target: &'a Value,
        payload: Payload,
        id: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        async move {
            let (kind,content)=match payload{Payload::Text(text)=>("text",json!({"text":text})),Payload::Image(bytes)=>("image",json!({"image_key":self.image(bytes).await?}))};
            let mut body=json!({"msg_type":kind,"content":content.to_string(),"uuid":if kind=="image"{super::store::event_id("feishu",&self.config.app_id,&format!("{id}:image"))}else{id.into()}});
            let path=if let Some(owner)=target["openId"].as_str(){body["receive_id"]=owner.into();"/im/v1/messages?receive_id_type=open_id".into()}else{format!("/im/v1/messages/{}/reply",target["message"].as_str().ok_or("feishu_target")?)};
            self.post(&path,body).await?;Ok(())
        }.boxed()
    }
}
pub(super) fn start(context: Context, config: Config) -> Result<Arc<Feishu>> {
    let transport = Arc::new(Feishu {
        client: State::external_client(&config.proxy_url, Duration::from_secs(15))?,
        config,
        context,
        token: Mutex::new(None),
        reconnect: AtomicU64::new(3),
    });
    let session = transport.clone();
    transport.context.tasks.spawn(async move {
        while !session.context.cancel.is_cancelled() {
            match session.session().await {Ok(())=>break,Err(error)=>{if error.to_string()=="feishu_authentication"{session.context.status("failed","Feishu authentication failed");break;}session.context.status("reconnecting","Feishu connection interrupted");}}
            tokio::select!{_ = session.context.cancel.cancelled()=>break,_ = tokio::time::sleep(Duration::from_secs(session.reconnect.load(Ordering::Relaxed)))=>{}}
        }
    });
    Ok(transport)
}
