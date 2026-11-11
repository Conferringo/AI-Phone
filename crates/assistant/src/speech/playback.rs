//! Streaming synthesis for the existing Agent voice profile. Disconnect cancels upstream work.
use super::*;
use axum::extract::ws::{Message as SocketMessage, WebSocket, WebSocketUpgrade};
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use std::time::Duration;
type Output = SplitSink<WebSocket, SocketMessage>;

pub(crate) async fn route(
    axum::extract::State(app): axum::extract::State<Shared>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if let Err(response) = websocket_authorization(&app, &headers).await {
        return response;
    }
    ws.max_message_size(8192)
        .max_frame_size(8192)
        .on_upgrade(move |socket| serve(app, socket))
        .into_response()
}
async fn send(output: &mut Output, event: wire::voice_speech_stream_output::Event) -> Result<()> {
    output
        .send(SocketMessage::Binary(
            wire::VoiceSpeechStreamOutput { event: Some(event) }
                .encode_to_vec()
                .into(),
        ))
        .await
        .map_err(|_| "语音播放连接已断开".into())
}
async fn request(socket: &mut WebSocket) -> Result<wire::VoiceSpeechRequest> {
    let message = tokio::time::timeout(Duration::from_secs(10), socket.recv())
        .await
        .map_err(|_| "语音播放请求超时")?
        .ok_or("语音播放连接已断开")?
        .map_err(|_| "语音播放连接已断开")?;
    let SocketMessage::Binary(bytes) = message else {
        return Err("语音播放请求格式错误".into());
    };
    wire::VoiceSpeechRequest::decode(bytes).map_err(|_| "语音播放请求格式错误".into())
}
async fn relay(output: &mut Output, response: reqwest::Response) -> Result<()> {
    let mut stream = response.bytes_stream();
    let mut total = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "语音服务传输中断")?;
        total += chunk.len();
        if total > provider::AUDIO_LIMIT {
            return Err("语音服务响应过大".into());
        }
        for part in chunk.chunks(16384) {
            send(
                output,
                wire::voice_speech_stream_output::Event::WavChunk(part.to_vec()),
            )
            .await?;
        }
    }
    if total == 0 {
        return Err("语音服务没有返回音频".into());
    }
    send(output, wire::voice_speech_stream_output::Event::Done(true)).await
}
async fn serve(app: Shared, mut socket: WebSocket) {
    let request = request(&mut socket).await;
    let (mut output, mut input) = socket.split();
    let result = tokio::select! {
        result = async {
            let request = request?;
            let draft = configured(&app, &request.agent_id, &request.device_id, request.configuration_revision).await?;
            if draft.speech_model.is_empty() {
                return Err("未配置远程播报".into());
            }
            let client = app.settings.http_client(&draft.proxy_url)?;
            let response = provider::speech_response(&client, &draft, &request.text).await?;
            relay(&mut output, response).await
        } => result,
        _ = async {while let Some(Ok(message))=input.next().await {
            if !matches!(message,SocketMessage::Ping(_)|SocketMessage::Pong(_)){break;}
        }} => return,
    };
    if let Err(message) = result {
        let _ = send(
            &mut output,
            wire::voice_speech_stream_output::Event::Error(wire::VoiceStreamError { message }),
        )
        .await;
    }
    let _ = output.close().await;
}

#[cfg(test)]
mod tests;
