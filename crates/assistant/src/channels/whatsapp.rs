use super::{
    State, socket,
    store::{Config, Result},
    transport::{Context, Payload},
};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt, future::BoxFuture, stream::SplitSink};
use image::{ImageFormat, Luma};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Cursor, os::unix::fs::PermissionsExt, sync::Arc, time::Duration};
use tokio::sync::{Mutex, RwLock, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use whatsapp_rust::{
    Client, Jid, SendOptions, UploadOptions,
    prelude::{Bot, EventKind, MessageExt, MessageField, wa},
    wacore::{
        download::MediaType,
        net::{self, DisconnectReason, TransportEvent},
    },
};
use whatsapp_rust_sqlite_storage::SqliteStore;

struct Network {
    client: reqwest::Client,
    cancel: CancellationToken,
}
struct Wire {
    writer: Mutex<SplitSink<socket::UpgradedSocket, Message>>,
    cancel: CancellationToken,
}
#[async_trait]
impl net::Transport for Wire {
    async fn send(&self, data: whatsapp_rust::bytes::Bytes) -> anyhow::Result<()> {
        let mut writer = self.writer.lock().await;
        tokio::select! {_ = self.cancel.cancelled()=>anyhow::bail!("channel_stopped"),result=writer.send(Message::Binary(data))=>result?};
        Ok(())
    }
    async fn disconnect(&self) {
        self.cancel.cancel();
    }
}
#[async_trait]
impl net::TransportFactory for Network {
    async fn create_transport(
        &self,
    ) -> anyhow::Result<(
        Arc<dyn net::Transport>,
        async_channel::Receiver<TransportEvent>,
    )> {
        let cancel = self.cancel.child_token();
        let stream = socket::upgrade(
            &self.client,
            net::WHATSAPP_WEB_WS_URL,
            Some(net::WHATSAPP_WEB_ORIGIN),
            &cancel,
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let (writer, mut reader) = stream.split();
        let wire = Arc::new(Wire {
            writer: Mutex::new(writer),
            cancel: cancel.clone(),
        });
        let (sender, receiver) = async_channel::bounded(64);
        sender.send(TransportEvent::Connected).await?;
        let owner = wire.clone();
        tokio::spawn(async move {
            let reason = loop {
                let item = tokio::select! {_ = cancel.cancelled()=>break DisconnectReason::Unknown,item=reader.next()=>item};
                let message = match item {
                    Some(Ok(Message::Binary(data))) => TransportEvent::DataReceived(data),
                    Some(Ok(Message::Ping(data))) => {
                        let mut writer = owner.writer.lock().await;
                        if writer.send(Message::Pong(data)).await.is_err() {
                            break DisconnectReason::StreamEnded;
                        }
                        continue;
                    }
                    Some(Ok(Message::Close(frame))) => {
                        break DisconnectReason::ServerClose {
                            code: frame.as_ref().map(|f| u16::from(f.code)),
                            reason: frame.map(|f| f.reason.to_string()).unwrap_or_default(),
                        };
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => break DisconnectReason::ReadError(error.to_string()),
                    None => break DisconnectReason::StreamEnded,
                };
                tokio::select! {_ = cancel.cancelled()=>break DisconnectReason::Unknown,result=sender.send(message)=>if result.is_err(){break DisconnectReason::Unknown;}}
            };
            let _ = sender.try_send(TransportEvent::Disconnected(reason));
            cancel.cancel();
        });
        Ok((wire, receiver))
    }
}
#[async_trait]
impl net::HttpClient for Network {
    async fn execute(&self, input: net::HttpRequest) -> anyhow::Result<net::HttpResponse> {
        let mut request = self.client.request(input.method.parse()?, &input.url);
        for (name, value) in input.headers {
            request = request.header(name, value);
        }
        if let Some(body) = input.body {
            request = request.body(body);
        }
        let operation = async {
            let response = request.send().await.map_err(reqwest::Error::without_url)?;
            let status_code = response.status().as_u16();
            Ok(net::HttpResponse {
                status_code,
                body: response.bytes().await?.to_vec(),
            })
        };
        tokio::select! {_ = self.cancel.cancelled()=>anyhow::bail!("channel_stopped"),result=operation=>result}
    }
}

pub(super) struct WhatsApp {
    context: Context,
    client: RwLock<Option<Arc<Client>>>,
    ready: watch::Sender<bool>,
}
fn peer(jid: &Jid) -> String {
    let text = jid.to_string();
    let (user, server) = text.split_once('@').unwrap();
    let user = user.split(':').next().unwrap();
    format!(
        "{user}@{}",
        if server == "s.whatsapp.net" {
            "c.us"
        } else {
            server
        }
    )
}
fn qr(code: &str) -> Result<Vec<u8>> {
    let image = qrcode::QrCode::new(code)?
        .render::<Luma<u8>>()
        .min_dimensions(280, 280)
        .build();
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(image).write_to(&mut output, ImageFormat::Png)?;
    Ok(output.into_inner())
}
impl WhatsApp {
    async fn connected(&self, client: Arc<Client>) -> Result<()> {
        let identity = peer(&client.pn().ok_or("whatsapp_identity_missing")?);
        let mut store = self.context.state.store.lock().await;
        if store.vault.configs.iter().any(|(key, config)| {
            key != &self.context.key
                && key.ends_with("/whatsapp")
                && config.identity.as_deref() == Some(identity.as_str())
        }) {
            return Err("channel_bot_in_use".into());
        }
        let config = store
            .vault
            .configs
            .get_mut(&self.context.key)
            .filter(|c| c.version == self.context.version)
            .ok_or("channel_binding_changed")?;
        config.identity = Some(identity);
        store.save()?;
        drop(store);
        *self.client.write().await = Some(client);
        self.ready.send_replace(true);
        self.context.status("connected", "");
        Ok(())
    }
    async fn run(self: Arc<Self>, config: Config) -> Result<()> {
        let session = self
            .context
            .state
            .root
            .join("data/channels/native-whatsapp")
            .join(&config.app_id);
        fs::create_dir_all(&session)?;
        fs::set_permissions(&session, fs::Permissions::from_mode(0o700))?;
        let store = SqliteStore::new(
            session
                .join("session.sqlite")
                .to_str()
                .ok_or("invalid_session_path")?,
        )
        .await?;
        let client = State::external_client(&config.proxy_url, Duration::from_secs(60))?;
        let qr_owner = self.clone();
        let connected = self.clone();
        let messages = self.clone();
        let disconnected = self.clone();
        let logged_out = self.clone();
        let bot=Bot::builder().with_backend(store)
            .with_transport_factory(Network{client:client.clone(),cancel:self.context.cancel.clone()})
            .with_http_client(Network{client,cancel:self.context.cancel.clone()})
            .skip_history_sync()
            .on_qr_code(move |code,_|{let owner=qr_owner.clone();async move {
                match qr(&code){Ok(png)=>{owner.context.live.write().unwrap().qr_png=png;owner.context.status("waiting_qr","");},Err(_)=>{owner.context.status("failed","whatsapp_qr_failed");owner.context.cancel.cancel();}}
            }})
            .on_connected(move |client|{let owner=connected.clone();async move {
                if let Err(error)=owner.connected(client).await{owner.context.status("failed",&error.to_string());owner.context.cancel.cancel();}
            }})
            .on_event_for(&[EventKind::Disconnected],move |_,_|{let owner=disconnected.clone();async move {owner.ready.send_replace(false);if !owner.context.cancel.is_cancelled(){owner.context.status("connecting","");}}})
            .on_logged_out(move |_|{let owner=logged_out.clone();async move {owner.ready.send_replace(false);owner.context.status("failed","whatsapp_logged_out");owner.context.cancel.cancel();}})
            .on_message(move |message|{let owner=messages.clone();async move {
                if message.info.source.is_from_me||message.info.source.is_group{return;}
                let chat=peer(&message.info.source.chat);
                if !(chat.ends_with("@c.us")||chat.ends_with("@lid")){return;}
                let Some(text)=message.message.text_content().filter(|text|!text.trim().is_empty()).map(str::to_owned) else{return};
                let mut ready=owner.ready.subscribe();
                let established=tokio::select!{_ = owner.context.cancel.cancelled()=>false,result=ready.wait_for(|ready|*ready)=>result.is_ok()};
                if !established{return;}
                let id=format!("false_{chat}_{}",message.info.id);
                if let Err(error)=owner.context.message(chat.clone(),id,text,json!({"chat":chat}),true,false).await{owner.context.status("failed",&error.to_string());owner.context.cancel.cancel();}
            }})
            .build().await?;
        let client = bot.client();
        *self.client.write().await = Some(client.clone());
        let running = bot.run();
        tokio::pin!(running);
        tokio::select! {_ = self.context.cancel.cancelled()=>{client.disconnect().await;running.await;},_ = &mut running=>{}}
        self.ready.send_replace(false);
        Ok(())
    }
    pub fn send<'a>(
        &'a self,
        target: &'a Value,
        payload: Payload,
        id: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if !*self.ready.borrow() {
                return Err("whatsapp_not_connected".into());
            }
            let client = self
                .client
                .read()
                .await
                .clone()
                .ok_or("whatsapp_not_connected")?;
            let to = target["chat"]
                .as_str()
                .ok_or("whatsapp_target")?
                .replace("@c.us", "@s.whatsapp.net")
                .parse::<Jid>()?;
            let (kind, message) = match payload {
                Payload::Text(text) => {
                    if text.is_empty() || text.len() > 65000 {
                        return Err("whatsapp_text_length".into());
                    }
                    (
                        "text",
                        wa::Message {
                            conversation: Some(text),
                            ..Default::default()
                        },
                    )
                }
                Payload::Image(bytes) => {
                    let image = image::load_from_memory(&bytes)?;
                    let mut thumbnail = Cursor::new(Vec::new());
                    image
                        .thumbnail(100, 100)
                        .to_rgb8()
                        .write_to(&mut thumbnail, ImageFormat::Jpeg)?;
                    let upload = client
                        .upload(bytes, MediaType::Image, UploadOptions::default())
                        .await?;
                    (
                        "image",
                        wa::Message {
                            image_message: MessageField::some(wa::message::ImageMessage {
                                url: Some(upload.url),
                                direct_path: Some(upload.direct_path),
                                mimetype: Some("image/png".into()),
                                media_key: Some(upload.media_key.to_vec()),
                                file_sha256: Some(upload.file_sha256.to_vec()),
                                file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
                                file_length: Some(upload.file_length),
                                media_key_timestamp: Some(upload.media_key_timestamp),
                                width: Some(image.width()),
                                height: Some(image.height()),
                                jpeg_thumbnail: Some(thumbnail.into_inner()),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    )
                }
            };
            let identity = format!("{:X}", Sha256::digest(format!("{id}/{kind}")))[..32].to_owned();
            tokio::select! {_ = self.context.cancel.cancelled()=>return Err("channel_stopped".into()),result=client.send_message_with_options(to,message,SendOptions::default().with_message_id(identity))=>{result?;}}
            Ok(())
        })
    }
    pub fn logout(&self) -> BoxFuture<'_, Result<bool>> {
        Box::pin(async move {
            let client = self
                .client
                .read()
                .await
                .clone()
                .ok_or("whatsapp_not_connected")?;
            client.logout().await;
            self.context.cancel.cancel();
            Ok(false)
        })
    }
}
pub(super) fn start(context: Context, config: Config) -> Result<Arc<WhatsApp>> {
    uuid::Uuid::parse_str(&config.app_id)?;
    let (ready, _) = watch::channel(false);
    let owner = Arc::new(WhatsApp {
        context,
        client: RwLock::new(None),
        ready,
    });
    let task = owner.clone();
    owner.context.tasks.spawn(async move {
        if let Err(error) = task.clone().run(config).await {
            task.context.status("failed", &error.to_string());
            task.context.cancel.cancel();
        }
    });
    Ok(owner)
}
