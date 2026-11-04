//! Persistent transcription transport. The existing chat Outbox still owns accepted work.
mod ledger;
use super::*;
use axum::extract::ws::{Message as ClientMessage, WebSocket, WebSocketUpgrade};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message as ProviderMessage,
        handshake::{client::generate_key, derive_accept_key},
        protocol::{Role, WebSocketConfig},
    },
};
type Provider = WebSocketStream<reqwest::Upgraded>;

pub(crate) async fn route(
    axum::extract::State(app): axum::extract::State<Shared>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if let Err(response) = websocket_authorization(&app, &headers).await {
        return response;
    }
    ws.max_message_size(4096)
        .max_frame_size(4096)
        .on_upgrade(move |socket| serve(app, socket))
        .into_response()
}
async fn send(socket: &mut WebSocket, event: wire::voice_stream_output::Event) -> Result<()> {
    socket
        .send(ClientMessage::Binary(
            wire::VoiceStreamOutput { event: Some(event) }
                .encode_to_vec()
                .into(),
        ))
        .await
        .map_err(|_| "语音连接已断开".into())
}
async fn connect(client: &reqwest::Client, draft: &wire::VoiceDraft) -> Result<Provider> {
    let key = generate_key();
    let mut request = client
        .get(format!(
            "{}/realtime?intent=transcription",
            draft.base_url.trim_end_matches('/')
        ))
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", &key);
    if !draft.api_key.is_empty() {
        request = request.bearer_auth(&draft.api_key);
    }
    let response = request.send().await.map_err(|_| "实时语音连接失败")?;
    if response.status() != StatusCode::SWITCHING_PROTOCOLS {
        return Err(format!("语音服务 HTTP {}", response.status().as_u16()));
    }
    if response
        .headers()
        .get("sec-websocket-accept")
        .and_then(|v| v.to_str().ok())
        != Some(derive_accept_key(key.as_bytes()).as_str())
    {
        return Err("实时语音握手无效".into());
    }
    let stream = response.upgrade().await.map_err(|_| "实时语音升级失败")?;
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(128 * 1024);
    config.max_frame_size = Some(128 * 1024);
    let mut provider = WebSocketStream::from_raw_socket(stream, Role::Client, Some(config)).await;
    provider.send(ProviderMessage::Text(json!({"type":"session.update","session":{"type":"transcription","audio":{"input":{"format":{"type":"audio/pcm","rate":24000},"transcription":{"model":draft.recognition_model},"turn_detection":null}}}}).to_string().into())).await.map_err(|_|"实时语音配置发送失败")?;
    configure(&mut provider).await?;
    Ok(provider)
}
async fn configure(provider: &mut Provider) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(event) = provider.next().await {
            let event = event.map_err(|_| "实时语音连接中断")?;
            if let ProviderMessage::Text(text) = event {
                let value: Value = serde_json::from_str(&text).map_err(|_| "实时语音响应无效")?;
                match value["type"].as_str() {
                    Some("session.updated") => return Ok::<(), String>(()),
                    Some("error") => return Err("语音服务拒绝实时配置".into()),
                    _ => {}
                }
            }
        }
        Err("实时语音连接已关闭".into())
    })
    .await
    .map_err(|_| "实时语音配置超时")??;
    Ok(())
}
async fn prepare(app: &App, socket: &mut WebSocket) -> Result<Provider> {
    let frame = tokio::time::timeout(Duration::from_secs(10), socket.recv())
        .await
        .map_err(|_| "缺少语音会话配置")?
        .ok_or("语音连接已关闭")?
        .map_err(|_| "语音连接失败")?;
    let ClientMessage::Binary(bytes) = frame else {
        return Err("实时语音需要protobuf帧".into());
    };
    let start = wire::VoiceStreamInput::decode(bytes)
        .map_err(|_| "实时语音帧无效")?
        .event;
    let Some(wire::voice_stream_input::Event::Start(start)) = start else {
        return Err("缺少语音会话配置".into());
    };
    Uuid::parse_str(&start.session_id).map_err(|_| "语音会话无效")?;
    let draft = super::configured(
        app,
        &start.agent_id,
        &start.device_id,
        start.configuration_revision,
    )
    .await?;
    if draft.input_protocol != wire::VoiceInputProtocol::RealtimeTranscription as i32 {
        return Err("请选择实时转写协议".into());
    }
    let client = app.settings.http_client(&draft.proxy_url)?;
    tokio::select! {result=connect(&client,&draft)=>result,_=socket.recv()=>Err("语音连接已关闭".into())}
}
async fn serve(app: Shared, mut socket: WebSocket) {
    match prepare(&app, &mut socket).await {
        Ok(provider) => {
            if send(&mut socket, wire::voice_stream_output::Event::Ready(true))
                .await
                .is_ok()
            {
                pipe(socket, provider).await;
            }
        }
        Err(message) => {
            let _ = send(
                &mut socket,
                wire::voice_stream_output::Event::Error(wire::VoiceStreamError { message }),
            )
            .await;
            let _ = socket.send(ClientMessage::Close(None)).await;
        }
    }
}
async fn pipe(socket: WebSocket, mut provider: Provider) {
    let (mut outgoing, mut incoming) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let reader = async {
        while let Some(frame) = incoming.next().await {
            match frame {
                Ok(ClientMessage::Binary(bytes)) => {
                    if tx.send(bytes).await.is_err() {
                        break;
                    }
                }
                Ok(ClientMessage::Ping(_) | ClientMessage::Pong(_)) => {}
                Ok(ClientMessage::Close(_)) | Err(_) => return Ok(()),
                _ => return Err("实时语音需要protobuf帧".to_string()),
            }
        }
        Ok(())
    };
    let writer = async {
        let mut ledger = ledger::Ledger::default();
        loop {
            tokio::select! {
                Some(bytes)=rx.recv()=>{
                    let event=wire::VoiceStreamInput::decode(bytes).map_err(|_|"实时语音帧无效")?.event.ok_or("缺少实时语音事件")?;
                    for event in ledger.input(event)? {
                        provider.send(ProviderMessage::Text(event.to_string().into())).await.map_err(|_|"实时语音发送失败")?;
                    }
                }
                event=provider.next()=>{
                    let event=event.ok_or("实时语音服务已断开")?.map_err(|_|"实时语音连接中断")?;
                    if let ProviderMessage::Text(text)=event {
                        let value=serde_json::from_str(&text).map_err(|_|"实时语音响应无效")?;
                        if let Some(transcript)=ledger.output(value)? {
                            let event=wire::VoiceStreamOutput{event:Some(wire::voice_stream_output::Event::Transcript(transcript))};
                            outgoing.send(ClientMessage::Binary(event.encode_to_vec().into())).await.map_err(|_|"语音连接已断开")?;
                        }
                    }else if matches!(event,ProviderMessage::Close(_)){return Err("实时语音服务已关闭".into());}
                }
            }
        }
    };
    let result: Result<()> = tokio::select! {result=reader=>result,result=writer=>result};
    if let Err(message) = result {
        let error = wire::VoiceStreamOutput {
            event: Some(wire::voice_stream_output::Event::Error(
                wire::VoiceStreamError { message },
            )),
        };
        let _ = outgoing
            .send(ClientMessage::Binary(error.encode_to_vec().into()))
            .await;
    }
}

#[cfg(test)]
mod tests;
