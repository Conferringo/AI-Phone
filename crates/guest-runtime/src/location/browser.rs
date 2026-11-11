//! Persistent CDP sessions own overrides until the feature is disabled.
use super::Fix;
use crate::{Error, Result};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    time::Duration,
};
use tokio::{net::TcpStream, sync::mpsc, task::JoinHandle};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
const DEADLINE: Duration = Duration::from_millis(2500);

struct Session {
    writer: SplitSink<Socket, Message>,
    replies: mpsc::UnboundedReceiver<Value>,
    reader: JoinHandle<Result<()>>,
    sequence: u64,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

async fn read_replies(
    mut socket: SplitStream<Socket>,
    replies: mpsc::UnboundedSender<Value>,
) -> Result<()> {
    while let Some(frame) = socket.next().await {
        match frame? {
            Message::Text(text) => {
                let value: Value = serde_json::from_str(&text)?;
                if value.get("id").is_some() && replies.send(value).is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => (),
        }
    }
    Ok(())
}

impl Session {
    async fn connect(url: &str) -> Result<Self> {
        let (socket, _) = tokio::time::timeout(DEADLINE, connect_async(url)).await??;
        let (writer, reader) = socket.split();
        let (send, replies) = mpsc::unbounded_channel();
        Ok(Self {
            writer,
            replies,
            reader: tokio::spawn(read_replies(reader, send)),
            sequence: 0,
        })
    }

    async fn command(&mut self, method: &str, params: Value) -> Result<()> {
        self.sequence += 1;
        let id = self.sequence;
        tokio::time::timeout(DEADLINE, async {
            self.writer
                .send(Message::Text(
                    json!({"id":id,"method":method,"params":params})
                        .to_string()
                        .into(),
                ))
                .await?;
            while let Some(reply) = self.replies.recv().await {
                if reply.get("id").and_then(Value::as_u64) != Some(id) {
                    continue;
                }
                if reply.get("error").is_some() {
                    return Err("browser_rejected".into());
                }
                return Ok(());
            }
            Err::<(), Error>("browser_closed".into())
        })
        .await?
    }

    async fn close(mut self) {
        let _ = self
            .command("Emulation.clearGeolocationOverride", json!({}))
            .await;
        let _ = self.writer.close().await;
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    web_socket_debugger_url: Option<String>,
}

pub(super) struct Browser {
    client: reqwest::Client,
    targets: String,
    sessions: HashMap<String, Session>,
}

impl Browser {
    pub(super) fn new(targets: String) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_millis(1500))
                .build()?,
            targets,
            sessions: HashMap::new(),
        })
    }

    pub(super) async fn apply(&mut self, fix: Fix) -> Result<()> {
        let Fix::Enabled { point, speed } = fix else {
            for session in std::mem::take(&mut self.sessions).into_values() {
                session.close().await;
            }
            return Ok(());
        };
        let pages = self
            .client
            .get(&self.targets)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Page>>()
            .await?;
        let pages: Vec<_> = pages
            .into_iter()
            .filter_map(|page| {
                if page.kind != "page" {
                    return None;
                }
                page.web_socket_debugger_url
                    .filter(|url| !url.is_empty())
                    .map(|url| (page.id, url))
            })
            .collect();
        if pages.is_empty() {
            return Err("browser_unavailable".into());
        }
        let alive: HashSet<_> = pages.iter().map(|(id, _)| id.as_str()).collect();
        self.sessions
            .retain(|id, session| alive.contains(id.as_str()) && !session.replies.is_closed());
        for (id, url) in pages {
            let session = match self.sessions.entry(id) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(Session::connect(&url).await?),
            };
            session.command("Emulation.setGeolocationOverride",json!({"latitude":point.latitude,"longitude":point.longitude,"accuracy":point.accuracy_m.max(1.0),"speed":speed})).await?;
        }
        Ok(())
    }
}
