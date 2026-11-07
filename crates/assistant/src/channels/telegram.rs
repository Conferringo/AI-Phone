use super::{
    State,
    store::{Config, Result},
    transport::{Context, Payload},
};
use futures_util::{FutureExt, future::BoxFuture};
use reqwest::{
    Client,
    multipart::{Form, Part},
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

pub(super) fn identity(token: &str) -> Option<&str> {
    let (id, key) = token.split_once(':')?;
    (id.len() >= 5
        && id.len() <= 20
        && id.bytes().all(|b| b.is_ascii_digit())
        && key.len() >= 20
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
    .then_some(id)
}
pub(super) struct Telegram {
    client: Client,
    token: String,
    context: Context,
}
impl Telegram {
    async fn api(&self, method: &str, body: Value) -> Result<Value> {
        let request = self
            .client
            .post(format!(
                "https://api.telegram.org/bot{}/{method}",
                self.token
            ))
            .json(&body);
        let response = self.context.request(request).await?;
        if !response.status().is_success() {
            return Err(format!("telegram_http_{}", response.status().as_u16()).into());
        }
        let value: Value = response.json().await.map_err(|_| "telegram_response")?;
        if value["ok"] != true {
            return Err(format!("telegram_api_{}", value["error_code"]).into());
        }
        Ok(value["result"].clone())
    }
    async fn authenticate(&self) -> Result<()> {
        let me = self.api("getMe", json!({})).await?;
        if me["is_bot"] != true
            || me["id"].as_i64().map(|id| id.to_string()).as_deref() != identity(&self.token)
        {
            return Err("telegram_identity".into());
        }
        if self.api("getWebhookInfo", json!({})).await?["url"]
            .as_str()
            .is_some_and(|url| !url.is_empty())
        {
            return Err("telegram_webhook_in_use".into());
        }
        Ok(())
    }
    async fn run(&self, mut offset: i64) -> Result<()> {
        while !self.context.cancel.is_cancelled() {
            let updates = self
                .api(
                    "getUpdates",
                    json!({"offset":offset,"timeout":25,"limit":100,"allowed_updates":["message"]}),
                )
                .await?;
            self.context.status("connected", "");
            for update in updates.as_array().ok_or("telegram_updates")? {
                let next = update["update_id"].as_i64().ok_or("telegram_update_id")?;
                if next < offset {
                    continue;
                }
                let message = &update["message"];
                if let (Some(text), Some(sender), Some(chat)) = (
                    message["text"].as_str(),
                    message["from"]["id"].as_i64(),
                    message["chat"]["id"].as_i64(),
                ) && message["chat"]["type"] == "private"
                    && message["from"]["is_bot"] != true
                    && sender == chat
                    && text.len() <= 24_000
                {
                    self.context
                        .message(
                            sender.to_string(),
                            next.to_string(),
                            text.into(),
                            json!({"chat":chat.to_string()}),
                            true,
                            false,
                        )
                        .await?;
                }
                offset = next + 1;
                self.context.offset(offset).await?;
            }
        }
        Ok(())
    }
}
impl Telegram {
    pub fn send<'a>(
        &'a self,
        target: &'a Value,
        payload: Payload,
        _id: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        async move {
            let chat = target["chat"].as_str().ok_or("telegram_target")?;
            match payload {
                Payload::Text(text) => {
                    self.api(
                        "sendMessage",
                        json!({"chat_id":chat,"text":text.chars().take(4000).collect::<String>()}),
                    )
                    .await?;
                }
                Payload::Image(bytes) => {
                    let form = Form::new().text("chat_id", chat.to_owned()).part(
                        "photo",
                        Part::bytes(bytes)
                            .file_name("screen.png")
                            .mime_str("image/png")?,
                    );
                    let request = self
                        .client
                        .post(format!(
                            "https://api.telegram.org/bot{}/sendPhoto",
                            self.token
                        ))
                        .multipart(form);
                    let response = self.context.request(request).await?;
                    if !response.status().is_success() {
                        return Err(format!("telegram_http_{}", response.status().as_u16()).into());
                    }
                    let value: Value = response.json().await.map_err(|_| "telegram_response")?;
                    if value["ok"] != true {
                        return Err("telegram_image_delivery".into());
                    }
                }
            }
            Ok(())
        }
        .boxed()
    }
}
pub(super) fn start(context: Context, config: Config) -> Result<Arc<Telegram>> {
    if identity(&config.secret) != Some(config.app_id.as_str()) {
        return Err("telegram_token".into());
    }
    let transport = Arc::new(Telegram {
        client: State::external_client(&config.proxy_url, Duration::from_secs(40))?,
        token: config.secret,
        context,
    });
    let session = transport.clone();
    transport.context.tasks.spawn(async move {
        if let Err(error)=session.authenticate().await{if !session.context.cancel.is_cancelled(){session.context.status("failed",&error.to_string());}return;}
        let mut offset=config.offset;
        loop {
            match session.run(offset).await {
                Ok(())=>break,
                Err(error)=>{
                    if session.context.cancel.is_cancelled(){break;}
                    let message=error.to_string();
                    if ["telegram_http_401","telegram_http_403","telegram_http_409","telegram_identity","telegram_webhook_in_use"].iter().any(|code|message==*code) {session.context.status("failed",&message);break;}
                    session.context.status("reconnecting","Telegram connection interrupted");
                }
            }
            let store=session.context.state.store.lock().await;
            let Some(config)=store.vault.configs.get(&session.context.key) else{break};
            offset=config.offset;
            drop(store);
            tokio::select!{_ = session.context.cancel.cancelled()=>break,_ = tokio::time::sleep(Duration::from_secs(3))=>{}}
        }
    });
    Ok(transport)
}
