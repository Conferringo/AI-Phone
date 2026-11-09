use crate::*;
use axum::extract::{
    State,
    ws::{Message as SocketMessage, WebSocket, WebSocketUpgrade},
};
use tokio::sync::{Notify, broadcast};

pub(crate) struct RunSignals {
    pub queue: Notify,
    pub input: Notify,
    pub speech: crate::speech::activity::SpeechActivity,
    pub handoff: Notify,
    pub handoff_export: Notify,
    pub channel_update: Notify,
    chat: broadcast::Sender<Uuid>,
}

impl Default for RunSignals {
    fn default() -> Self {
        Self {
            queue: Notify::new(),
            input: Notify::new(),
            speech: Default::default(),
            handoff: Notify::new(),
            handoff_export: Notify::new(),
            channel_update: Notify::new(),
            chat: broadcast::channel(32).0,
        }
    }
}

impl RunSignals {
    pub fn chat_changed(&self, conversation: Uuid) {
        let _ = self.chat.send(conversation);
    }
}

pub async fn events(
    State(app): State<Shared>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if let Err(response) = crate::speech::websocket_authorization(&app, &headers).await {
        return response;
    }
    let changes = app.run_signals.chat.subscribe();
    ws.max_message_size(128)
        .on_upgrade(move |socket| serve(socket, changes))
}

async fn serve(mut socket: WebSocket, mut changes: broadcast::Receiver<Uuid>) {
    loop {
        let conversation_id = tokio::select! {
            event = changes.recv() => match event {
                Ok(conversation) => conversation.to_string(),
                Err(broadcast::error::RecvError::Lagged(_)) => String::new(),
                Err(broadcast::error::RecvError::Closed) => return,
            },
            incoming = socket.recv() => {
                if matches!(incoming, None | Some(Err(_) | Ok(SocketMessage::Close(_)))) {
                    return;
                }
                continue;
            }
        };
        let event = wire::ChatChanged { conversation_id };
        if socket
            .send(SocketMessage::Binary(event.encode_to_vec().into()))
            .await
            .is_err()
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL"]
    async fn accepted_message_wakes_worker_and_notifies_authenticated_chat() {
        crate::worker::steering_regressions::database_case(|app| async move {
            registry::initialize(&app.db, &app.settings).await.unwrap();
            let session = "a".repeat(64);
            sqlx::query("INSERT INTO admin_sessions(token_hash) VALUES($1)")
                .bind(format!("{:x}", Sha256::digest(session.as_bytes())))
                .execute(&app.db)
                .await
                .unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let router = Router::new()
                .route("/events", axum::routing::get(events))
                .with_state(app.clone());
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let mut request = format!("ws://{address}/events")
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("origin", "http://127.0.0.1:17860".parse().unwrap());
            request.headers_mut().insert(
                "cookie",
                format!("ai_phone_session={session}").parse().unwrap(),
            );
            let stream = tokio::net::TcpStream::connect(address).await.unwrap();
            let (mut socket, _) = tokio_tungstenite::client_async(request, stream)
                .await
                .unwrap();
            let mut headers = HeaderMap::new();
            headers.insert(
                "authorization",
                format!("Bearer {}", app.token).parse().unwrap(),
            );
            headers.insert("content-type", MEDIA.parse().unwrap());
            let input = wire::ChatSendRequest {
                text: "response timing fixture".into(),
                request_id: Uuid::now_v7().to_string(),
                receipt_only: true,
                ..Default::default()
            };
            let started = std::time::Instant::now();
            let result = http::send(
                State(app.clone()),
                headers,
                Ok(input.encode_to_vec().into()),
            )
            .await;
            assert_eq!(result.status(), StatusCode::ACCEPTED);
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                app.run_signals.queue.notified(),
            )
            .await
            .unwrap();
            let frame = tokio::time::timeout(std::time::Duration::from_secs(1), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .into_data();
            let event = wire::ChatChanged::decode(frame).unwrap();
            eprintln!(
                "local send to committed conversation WebSocket: {} ms",
                started.elapsed().as_millis()
            );
            let id = Uuid::parse_str(&event.conversation_id).unwrap();
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM messages WHERE conversation_id=$1 AND role='user'",
            )
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert_eq!(count, 1);
            drop(socket);
            server.abort();
            let _ = server.await;
        })
        .await;
    }
}
