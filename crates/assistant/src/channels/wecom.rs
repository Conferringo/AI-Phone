use super::{
    State,
    socket::{self, Socket},
    store::{Config, Result},
    transport::{Context, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{FutureExt, StreamExt, future::BoxFuture};
use md5::{Digest, Md5};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct Peer {
    socket: Socket,
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Value>>>>,
    cancel: CancellationToken,
}
impl Peer {
    async fn request(&self, command: &str, body: Value) -> Result<Value> {
        let id = format!("{command}_{}", Uuid::new_v4().simple());
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().unwrap().insert(id.clone(), sender);
        let response=async {
            self.socket.json(json!({"cmd":command,"headers":{"req_id":id},"body":body}))?;
            tokio::select!{_ = self.cancel.cancelled()=>Err("channel_stopped".into()),response=tokio::time::timeout(Duration::from_secs(10),receiver)=>response??}
        }.await;
        self.pending.lock().unwrap().remove(&id);
        response
    }
}
pub(super) struct Wecom {
    active: RwLock<Option<Arc<Peer>>>,
    context: Context,
    client: reqwest::Client,
}
impl Wecom {
    fn peer(&self) -> Result<Arc<Peer>> {
        self.active
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| "wecom_not_connected".into())
    }
    async fn session(&self, config: &Config) -> Result<()> {
        let cancel = self.context.cancel.child_token();
        let (socket, mut incoming) = socket::connect(
            &self.client,
            "wss://openws.work.weixin.qq.com",
            cancel.clone(),
        )
        .await?;
        let peer = Arc::new(Peer {
            socket,
            pending: Mutex::new(HashMap::new()),
            cancel: cancel.clone(),
        });
        *self.active.write().unwrap() = Some(peer.clone());
        let receive = async {
            while let Some(message) = incoming.next().await {
                let message = message?;
                if !message.is_text() && !message.is_binary() {
                    continue;
                }
                let frame: Value = serde_json::from_slice(&message.into_data())?;
                if frame["cmd"] == "aibot_msg_callback" && frame["body"]["msgtype"] == "text" {
                    let body = &frame["body"];
                    if let (Some(sender), Some(id), Some(text)) = (
                        body["from"]["userid"].as_str(),
                        body["msgid"].as_str(),
                        body["text"]["content"].as_str(),
                    ) {
                        let chat = body["chatid"].as_str().unwrap_or(sender);
                        self.context
                            .message(
                                sender.into(),
                                id.into(),
                                text.into(),
                                json!({"chat":chat}),
                                body["chattype"] == "single",
                                false,
                            )
                            .await?;
                    }
                } else if frame["body"]["event"]["eventtype"] == "disconnected_event" {
                    return Err("wecom_session_replaced".into());
                } else if let Some(id) = frame["headers"]["req_id"].as_str() {
                    let sender = peer.pending.lock().unwrap().remove(id);
                    if let Some(sender) = sender {
                        let result = if frame["errcode"] == 0 {
                            Ok(frame)
                        } else {
                            Err(format!("wecom_error_{}", frame["errcode"]).into())
                        };
                        let _ = sender.send(result);
                    }
                }
            }
            Err::<(), _>("wecom_connection_closed".into())
        };
        let authenticated = async {
            peer.request(
                "aibot_subscribe",
                json!({"bot_id":config.app_id,"secret":config.secret}),
            )
            .await
            .map_err(|_| "wecom_authentication")?;
            self.context.status("connected", "");
            while !self.context.cancel.is_cancelled() {
                tokio::time::sleep(Duration::from_secs(30)).await;
                peer.request("ping", Value::Null).await?;
            }
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        };
        let result = tokio::select! {_ = self.context.cancel.cancelled()=>Ok(()),result=receive=>result,result=authenticated=>result};
        cancel.cancel();
        peer.pending.lock().unwrap().clear();
        *self.active.write().unwrap() = None;
        result
    }
}
impl Wecom {
    pub fn send<'a>(
        &'a self,
        target: &'a Value,
        payload: Payload,
        _id: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        async move {
            let chat=target["chat"].as_str().ok_or("wecom_target")?;
            let peer=self.peer()?;
            let body=match payload {
                Payload::Text(text)=>json!({"chatid":chat,"msgtype":"markdown","markdown":{"content":text}}),
                Payload::Image(bytes)=>{
                    let chunks=bytes.len().div_ceil(512*1024);
                    let init=peer.request("aibot_upload_media_init",json!({"type":"image","filename":"screen.png","total_size":bytes.len(),"total_chunks":chunks,"md5":format!("{:x}",Md5::digest(&bytes))})).await?;
                    let upload=init["body"]["upload_id"].as_str().ok_or("wecom_upload_id")?;
                    for (index,chunk) in bytes.chunks(512*1024).enumerate(){peer.request("aibot_upload_media_chunk",json!({"upload_id":upload,"chunk_index":index,"base64_data":STANDARD.encode(chunk)})).await?;}
                    let finish=peer.request("aibot_upload_media_finish",json!({"upload_id":upload})).await?;
                    let media=finish["body"]["media_id"].as_str().ok_or("wecom_media_id")?;
                    json!({"chatid":chat,"msgtype":"image","image":{"media_id":media}})
                }
            };
            peer.request("aibot_send_msg",body).await?;Ok(())
        }.boxed()
    }
}
pub(super) fn start(context: Context, config: Config) -> Result<Arc<Wecom>> {
    let transport = Arc::new(Wecom {
        active: RwLock::new(None),
        client: State::external_client(&config.proxy_url, Duration::from_secs(15))?,
        context,
    });
    let session = transport.clone();
    transport.context.tasks.spawn(async move {
        while !session.context.cancel.is_cancelled() {
            match session.session(&config).await {
                Ok(())=>break,
                Err(error)=>{let message=error.to_string();if matches!(message.as_str(),"wecom_authentication"|"wecom_session_replaced"){session.context.status("failed",&message);break;}session.context.status("reconnecting","WeCom connection interrupted");}
            }
            tokio::select!{_ = session.context.cancel.cancelled()=>break,_ = tokio::time::sleep(Duration::from_secs(3))=>{}}
        }
    });
    Ok(transport)
}
