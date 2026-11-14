use super::{
    State, socket,
    store::{Config, Result},
    transport::{Context, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{FutureExt, StreamExt, future::BoxFuture};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message as Packet;

struct Token {
    value: String,
    expires: Instant,
}
#[derive(Default)]
struct Gateway {
    session: Option<String>,
    sequence: Option<u64>,
}
pub(super) struct Qq {
    client: Client,
    config: Config,
    context: Context,
    token: Mutex<Option<Token>>,
    gateway: StdMutex<Gateway>,
    message_sequence: AtomicU32,
}
impl Qq {
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
                    .post("https://bots.qq.com/app/getAppAccessToken")
                    .json(&json!({"appId":self.config.app_id,"clientSecret":self.config.secret})),
            )
            .await?
            .error_for_status()?;
        let value: Value = response.json().await?;
        let token = value["access_token"]
            .as_str()
            .ok_or("qq_authentication")?
            .to_owned();
        let expires: u64 = value["expires_in"].to_string().trim_matches('"').parse()?;
        *cached = Some(Token {
            value: token.clone(),
            expires: Instant::now() + Duration::from_secs(expires.saturating_sub(60)),
        });
        Ok(token)
    }
    async fn api(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut request = self
            .client
            .request(method, format!("https://api.sgroup.qq.com{path}"))
            .header("Authorization", format!("QQBot {}", self.token().await?))
            .header("X-Union-Appid", &self.config.app_id);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = self.context.request(request).await?;
        if !response.status().is_success() {
            return Err(format!("qq_http_{}", response.status().as_u16()).into());
        }
        Ok(response.json().await?)
    }
    async fn event(&self, event: &str, data: &Value) -> Result<()> {
        let (kind, sender, target) = match event {
            "C2C_MESSAGE_CREATE" => (
                "c2c",
                data["author"]["user_openid"].as_str(),
                data["author"]["user_openid"].as_str(),
            ),
            "GROUP_AT_MESSAGE_CREATE" => (
                "group",
                data["author"]["member_openid"].as_str(),
                data["group_openid"].as_str(),
            ),
            _ => return Ok(()),
        };
        let (Some(sender), Some(target), Some(id), Some(text)) = (
            sender,
            target,
            data["id"].as_str(),
            data["content"].as_str(),
        ) else {
            return Err("qq_message_fields".into());
        };
        self.context
            .message(
                sender.into(),
                id.into(),
                text.trim().into(),
                json!({"scope":kind,"targetId":target,"msgId":id}),
                kind == "c2c",
                data["author"]["bot"] == true,
            )
            .await
    }
    async fn session(&self) -> Result<()> {
        let gateway = self.api(Method::GET, "/gateway", None).await?;
        let url = gateway["url"].as_str().ok_or("qq_gateway")?;
        let cancel = self.context.cancel.child_token();
        let (socket, mut incoming) = socket::connect(&self.client, url, cancel.clone()).await?;
        let mut heartbeat: Option<tokio::time::Interval> = None;
        let result=async {
            loop {
                tokio::select! {
                    _ = self.context.cancel.cancelled()=>return Ok(()),
                    _ = async {match heartbeat.as_mut(){Some(timer)=>{timer.tick().await;},None=>futures_util::future::pending::<()>().await}}=>{
                        socket.json(json!({"op":1,"d":self.gateway.lock().unwrap().sequence}))?;
                    }
                    packet=incoming.next()=>{
                        let packet=packet.ok_or("qq_connection_closed")??;
                        if let Packet::Close(frame)=packet {
                            if let Some(frame)=frame {
                                let code:u16=frame.code.into();
                                if matches!(code,4004|4914|4915){return Err("qq_authentication".into());}
                                if matches!(code,4006|4007|4009){*self.gateway.lock().unwrap()=Gateway::default();}
                            }
                            return Err("qq_connection_closed".into());
                        }
                        if !packet.is_text()&&!packet.is_binary(){continue;}
                        let frame:Value=serde_json::from_slice(&packet.into_data())?;
                        if let Some(sequence)=frame["s"].as_u64(){self.gateway.lock().unwrap().sequence=Some(sequence);}
                        match frame["op"].as_u64().ok_or("qq_gateway_opcode")? {
                            10=>{
                                let duration=frame["d"]["heartbeat_interval"].as_u64().ok_or("qq_heartbeat_interval")?;
                                let mut timer=tokio::time::interval(Duration::from_millis(duration));timer.tick().await;heartbeat=Some(timer);
                                let token=self.token().await?;
                                let gateway=self.gateway.lock().unwrap();
                                if let Some(session)=&gateway.session {
                                    socket.json(json!({"op":6,"d":{"token":format!("QQBot {token}"),"session_id":session,"seq":gateway.sequence}}))?;
                                } else {
                                    socket.json(json!({"op":2,"d":{"token":format!("QQBot {token}"),"intents":1u64<<25,"shard":[0,1]}}))?;
                                }
                            }
                            0=>{
                                let event=frame["t"].as_str().ok_or("qq_event_type")?;
                                if event=="READY" {self.gateway.lock().unwrap().session=Some(frame["d"]["session_id"].as_str().ok_or("qq_session_id")?.into());self.context.status("connected","");}
                                else if event=="RESUMED"{self.context.status("connected","");}
                                else {self.event(event,&frame["d"]).await?;}
                            }
                            7=>return Err("qq_server_reconnect".into()),
                            9=>{*self.gateway.lock().unwrap()=Gateway::default();return Err("qq_session_invalid".into());}
                            11=>{},
                            _=>{},
                        }
                    }
                }
            }
        }.await;
        cancel.cancel();
        result
    }
}
impl Qq {
    pub fn send<'a>(
        &'a self,
        target: &'a Value,
        payload: Payload,
        _id: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        async move {
            let segment=match target["scope"].as_str(){Some("c2c")=>"users",Some("group")=>"groups",_=>return Err("qq_target_scope".into())};
            let target_id=target["targetId"].as_str().ok_or("qq_target")?;
            let mut body=match payload {
                Payload::Text(text)=>json!({"content":text,"msg_type":0}),
                Payload::Image(bytes)=>{
                    let value=self.api(Method::POST,&format!("/v2/{segment}/{target_id}/files"),Some(json!({"file_type":1,"srv_send_msg":false,"file_data":STANDARD.encode(bytes)}))).await?;
                    json!({"msg_type":7,"media":{"file_info":value["file_info"].as_str().ok_or("qq_image_upload")?}})
                }
            };
            body["msg_seq"]=(self.message_sequence.fetch_add(1,Ordering::Relaxed)%65_536).into();
            if let Some(id)=target["msgId"].as_str(){body["msg_id"]=id.into();}
            let result=self.api(Method::POST,&format!("/v2/{segment}/{target_id}/messages"),Some(body)).await?;
            if result["id"].as_str().is_none(){return Err("qq_delivery_unconfirmed".into());}
            Ok(())
        }.boxed()
    }
}
pub(super) fn start(context: Context, config: Config) -> Result<Arc<Qq>> {
    let transport = Arc::new(Qq {
        client: State::external_client(&config.proxy_url, Duration::from_secs(15))?,
        config,
        context,
        token: Mutex::new(None),
        gateway: StdMutex::new(Gateway::default()),
        message_sequence: AtomicU32::new(1),
    });
    let session = transport.clone();
    transport.context.tasks.spawn(async move {
        while !session.context.cancel.is_cancelled() {
            match session.session().await {Ok(())=>break,Err(error)=>{if matches!(error.to_string().as_str(),"qq_authentication"|"qq_http_401"|"qq_http_403"){session.context.status("failed","QQ authentication failed");break;}session.context.status("reconnecting","QQ connection interrupted");}}
            tokio::select!{_ = session.context.cancel.cancelled()=>break,_ = tokio::time::sleep(Duration::from_secs(3))=>{}}
        }
    });
    Ok(transport)
}
