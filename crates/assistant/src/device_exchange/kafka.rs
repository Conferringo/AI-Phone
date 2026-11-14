use super::{App, Shared, Uuid, wire};
use anyhow::{Context, Result};
use prost::Message;
use rskafka::{
    client::{
        ClientBuilder,
        partition::{Compression, PartitionClient, UnknownTopicHandling},
    },
    record::Record,
};
use sqlx::PgPool;
use std::{env, net::SocketAddr, time::Duration};

const TOPIC: &str = "ai-phone.device-handoff.accepted.v1";
const BROKER_TIMEOUT: Duration = Duration::from_secs(5);
const BATCH_SIZE: i64 = 64;

fn broker_address(value: &str) -> Result<SocketAddr> {
    let address: SocketAddr = value
        .parse()
        .context("Kafka requires a loopback IP and port")?;
    anyhow::ensure!(
        address.ip().is_loopback(),
        "Kafka requires a loopback IP and port"
    );
    Ok(address)
}

fn broker() -> Result<Option<SocketAddr>> {
    match env::var("AI_PHONE_KAFKA_BROKER") {
        Ok(value) if !value.is_empty() => broker_address(&value).map(Some),
        Ok(_) | Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn configured() -> bool {
    matches!(broker(), Ok(Some(_)))
}

async fn connect(address: SocketAddr, topic: &str) -> Result<PartitionClient> {
    let client = ClientBuilder::new(vec![address.to_string()])
        .client_id("ai-phone-handoff")
        .build()
        .await?;
    Ok(client
        .partition_client(topic, 0, UnknownTopicHandling::Error)
        .await?)
}

async fn publish_batch(db: &PgPool, partition: &PartitionClient) -> Result<usize> {
    let rows: Vec<(Uuid, String, String, i64)> = sqlx::query_as(
        "SELECT id,source_device_id,target_device_id,floor(extract(epoch FROM created_at)*1000)::bigint
         FROM device_messages WHERE kafka_pending ORDER BY created_at,id LIMIT $1",
    )
    .bind(BATCH_SIZE)
    .fetch_all(db)
    .await?;
    if rows.is_empty() {
        return Ok(0);
    }

    let mut ids = Vec::with_capacity(rows.len());
    let mut records = Vec::with_capacity(rows.len());
    for (id, source_device_id, target_device_id, created_at_ms) in rows {
        ids.push(id);
        let event = wire::DeviceMessage {
            id: id.to_string(),
            source_device_id,
            target_device_id,
            created_at_ms: created_at_ms as u64,
            ..Default::default()
        };
        records.push(Record {
            key: Some(event.id.as_bytes().to_vec()),
            value: Some(event.encode_to_vec()),
            headers: Default::default(),
            timestamp: (std::time::UNIX_EPOCH + Duration::from_millis(event.created_at_ms)).into(),
        });
    }
    tokio::time::timeout(
        BROKER_TIMEOUT,
        partition.produce(records, Compression::NoCompression),
    )
    .await??;
    // A crash after the Kafka acknowledgement can replay these IDs. No device action is repeated.
    sqlx::query("UPDATE device_messages SET kafka_pending=false WHERE id=ANY($1)")
        .bind(&ids)
        .execute(db)
        .await?;
    Ok(ids.len())
}

async fn relay(app: &App, address: SocketAddr) -> Result<()> {
    let partition = tokio::time::timeout(BROKER_TIMEOUT, connect(address, TOPIC)).await??;
    loop {
        if publish_batch(&app.db, &partition).await? < BATCH_SIZE as usize {
            app.run_signals.handoff_export.notified().await;
        }
    }
}

pub(super) async fn work(app: Shared) {
    match broker() {
        Ok(Some(address)) => loop {
            if let Err(error) = relay(&app, address).await {
                eprintln!("Kafka handoff export paused: {error:#}");
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        },
        Ok(None) => (),
        Err(error) => eprintln!("Kafka handoff export unavailable: {error:#}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskafka::client::partition::OffsetAt;
    use tokio::net::TcpListener;

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL and a disposable Kafka broker"]
    async fn kafka_handoff_replay() {
        let address = broker_address(&env::var("AI_PHONE_TEST_KAFKA_BROKER").unwrap()).unwrap();
        let topic = format!("ai-phone-handoff-fixture-{}", Uuid::new_v4().simple());
        let client = ClientBuilder::new(vec![address.to_string()])
            .build()
            .await
            .unwrap();
        let controller = client.controller_client().unwrap();
        controller.create_topic(&topic, 1, 1, 5000).await.unwrap();
        let partition = client
            .partition_client(&topic, 0, UnknownTopicHandling::Retry)
            .await
            .unwrap();

        crate::workbench::persistence_tests::database_case(|app| {
            Box::pin(async move {
                let link = Uuid::new_v4();
                let first = Uuid::new_v4();
                let second = Uuid::new_v4();
                let previous = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO devices(id,name,kind,system,runtime_state) VALUES
                     ('fixture-a','A','phone','test','registered'),
                     ('fixture-b','B','desktop','test','registered')",
                )
                .execute(&app.db)
                .await
                .unwrap();
                sqlx::query(
                    "INSERT INTO device_links(id,source_device_id,target_device_id)
                     VALUES($1,'fixture-a','fixture-b')",
                )
                .bind(link)
                .execute(&app.db)
                .await
                .unwrap();
                sqlx::query(
                    "INSERT INTO device_messages(
                         id,link_id,link_revision,source_device_id,target_device_id,text,kafka_pending,created_at)
                     VALUES($1,$4,1,'fixture-a','fixture-b','old fixture',false,now()-interval '3 seconds'),
                           ($2,$4,1,'fixture-a','fixture-b','private fixture A',true,now()-interval '2 seconds'),
                           ($3,$4,1,'fixture-a','fixture-b','private fixture B',true,now()-interval '1 second')",
                )
                .bind(previous)
                .bind(first)
                .bind(second)
                .bind(link)
                .execute(&app.db)
                .await
                .unwrap();
                let unavailable = TcpListener::bind("127.0.0.1:0").await.unwrap();
                assert!(
                    !matches!(
                        tokio::time::timeout(
                            Duration::from_millis(250),
                            connect(unavailable.local_addr().unwrap(), TOPIC)
                        )
                        .await,
                        Ok(Ok(_))
                    ),
                    "a socket without a Kafka server cannot acknowledge a handoff"
                );
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(
                        "SELECT count(*) FROM device_messages WHERE kafka_pending"
                    )
                    .fetch_one(&app.db)
                    .await
                    .unwrap(),
                    2
                );

                assert_eq!(publish_batch(&app.db, &partition).await.unwrap(), 2);
                assert_eq!(publish_batch(&app.db, &partition).await.unwrap(), 0);

                let subscriber = connect(address, &topic).await.unwrap();
                let offset = subscriber.get_offset(OffsetAt::Earliest).await.unwrap();
                let (events, _) = subscriber.fetch_records(offset, 1..8192, 2000).await.unwrap();
                assert_eq!(events.len(), 2);
                for (record, id) in events.iter().zip([first, second]) {
                    let payload = wire::DeviceMessage::decode(
                        record.record.value.as_ref().unwrap().as_slice(),
                    )
                    .unwrap();
                    assert_eq!(payload.id, id.to_string());
                    assert_eq!(record.record.key.as_deref(), Some(payload.id.as_bytes()));
                    assert_eq!(payload.source_device_id, "fixture-a");
                    assert_eq!(payload.target_device_id, "fixture-b");
                    assert!(payload.created_at_ms > 0);
                    assert!(payload.text.is_empty() && payload.run_id.is_empty());
                }

                let replay = connect(address, &topic).await.unwrap();
                let (replayed, _) = replay.fetch_records(offset, 1..8192, 2000).await.unwrap();
                assert_eq!(events, replayed);
                controller.delete_topic(&topic, 5000).await.unwrap();
            })
        })
        .await;
    }
}
