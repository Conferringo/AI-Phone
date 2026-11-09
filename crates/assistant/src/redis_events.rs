use futures_util::StreamExt;
use redis::{AsyncConnectionConfig, Client};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::{sync::Notify, time};

const RUN_SETTLED: &str = "ai-phone:run-settled:v1";
const IO_TIMEOUT: Duration = Duration::from_millis(500);

pub(crate) fn client() -> Option<Client> {
    let port: u16 = std::env::var("AI_PHONE_REDIS_PORT").ok()?.parse().ok()?;
    Client::open(format!("redis://127.0.0.1:{port}/")).ok()
}

pub(crate) async fn publish(client: Client, settled: &Notify) {
    let config = AsyncConnectionConfig::new()
        .set_connection_timeout(Some(IO_TIMEOUT))
        .set_response_timeout(Some(IO_TIMEOUT));
    let mut connection = None;
    loop {
        settled.notified().await;
        if connection.is_none() {
            connection = client
                .get_multiplexed_async_connection_with_config(&config)
                .await
                .ok();
        }
        if let Some(writer) = connection.as_mut()
            && redis::cmd("PUBLISH")
                .arg(RUN_SETTLED)
                .arg(1)
                .query_async::<usize>(writer)
                .await
                .is_err()
        {
            connection = None;
        }
    }
}

pub(crate) async fn subscribe(client: Client, connected: &AtomicBool, changed: &Notify) {
    loop {
        let subscription = time::timeout(IO_TIMEOUT, async {
            let mut subscription = client.get_async_pubsub().await?;
            subscription.subscribe(RUN_SETTLED).await?;
            Ok::<_, redis::RedisError>(subscription)
        })
        .await;
        if let Ok(Ok(mut subscription)) = subscription {
            connected.store(true, Ordering::Release);
            changed.notify_one();
            let mut messages = subscription.on_message();
            while messages.next().await.is_some() {
                changed.notify_one();
            }
            connected.store(false, Ordering::Release);
            changed.notify_one();
        }
        time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires disposable Redis"]
    async fn committed_wake_reaches_channel() {
        let port: u16 = std::env::var("AI_PHONE_TEST_REDIS_PORT")
            .expect("disposable Redis port")
            .parse()
            .unwrap();
        assert_ne!(port, 56382, "use a disposable Redis instance");
        let client = Client::open(format!("redis://127.0.0.1:{port}/")).unwrap();
        let connected = AtomicBool::new(false);
        let settled = Notify::new();
        let changed = Notify::new();
        let check = async {
            changed.notified().await;
            assert!(connected.load(Ordering::Acquire));
            settled.notify_one();
            changed.notified().await;
            assert!(connected.load(Ordering::Acquire));
        };
        time::timeout(Duration::from_secs(3), async {
            tokio::select! {
                _ = check => {}
                _ = publish(client.clone(), &settled) => unreachable!(),
                _ = subscribe(client, &connected, &changed) => unreachable!(),
            }
        })
        .await
        .unwrap();
    }
}
