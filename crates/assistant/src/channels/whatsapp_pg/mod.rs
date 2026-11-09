mod app_sync;
mod device;
mod protocol;
mod signal;

use sqlx::PgPool;
use uuid::Uuid;
use whatsapp_rust::wacore::store::error::StoreError;

pub(super) struct PgStore {
    pool: PgPool,
    binding: Uuid,
}

impl PgStore {
    pub(super) fn new(pool: PgPool, binding: Uuid) -> Self {
        Self { pool, binding }
    }
}

fn database_error(error: sqlx::Error) -> StoreError {
    StoreError::Database(Box::new(error))
}

fn serialization_error(error: impl std::error::Error + Send + Sync + 'static) -> StoreError {
    StoreError::Serialization(Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use whatsapp_rust::wacore::store::traits::{
        AppSyncStore, DeviceStore, MsgSecretEntry, MsgSecretStore, ProtocolStore, SignalStore,
    };

    #[tokio::test]
    #[ignore = "requires a labelled disposable PostgreSQL; no production writes"]
    async fn whatsapp_session_survives_reopen_and_stays_scoped() {
        crate::workbench::persistence_tests::database_case(|app| {
            Box::pin(async move {
                let binding = Uuid::new_v4();
                let first = PgStore::new(app.db.clone(), binding);
                first.create().await.unwrap();
                let mut device = first.load().await.unwrap().unwrap();
                device.push_name = "fixture account".into();
                first.save(&device).await.unwrap();
                first
                    .put_identities_batch(&[("test@lid".into(), [7; 32])])
                    .await
                    .unwrap();
                first
                    .put_sessions_batch(&[("test@lid".into(), b"ratchet".as_slice().into())])
                    .await
                    .unwrap();
                first
                    .set_version("regular", Default::default())
                    .await
                    .unwrap();
                first
                    .store_pending_inbound("chat", "sender", "in", b"incoming")
                    .await
                    .unwrap();
                first
                    .store_sent_message("chat", "out", b"outgoing")
                    .await
                    .unwrap();
                first
                    .store_prekeys_batch(&[(42, b"prekey".as_slice().into())], false)
                    .await
                    .unwrap();
                first.mark_prekeys_uploaded(&[42]).await.unwrap();
                let secret = MsgSecretEntry {
                    chat: "chat".into(),
                    sender: "sender".into(),
                    msg_id: "out".into(),
                    secret: [7; 32],
                    expires_at: 0,
                    message_ts: 100,
                };
                let replacement = MsgSecretEntry {
                    secret: [8; 32],
                    expires_at: 200,
                    message_ts: 50,
                    ..secret.clone()
                };
                first
                    .put_msg_secrets(vec![secret, replacement])
                    .await
                    .unwrap();
                drop(first);

                let reopened = PgStore::new(app.db.clone(), binding);
                let saved = reopened.load().await.unwrap().unwrap();
                assert_eq!(saved.push_name, device.push_name);
                assert_eq!(saved.adv_secret_key, device.adv_secret_key);
                assert_eq!(
                    reopened.load_identity("test@lid").await.unwrap(),
                    Some([7; 32])
                );
                assert_eq!(
                    reopened.get_session("test@lid").await.unwrap().as_deref(),
                    Some(b"ratchet".as_slice())
                );
                assert_eq!(
                    reopened
                        .get_pending_inbound("chat", "sender", "in")
                        .await
                        .unwrap()
                        .as_deref(),
                    Some(b"incoming".as_slice())
                );
                assert_eq!(
                    reopened.load_prekey(42).await.unwrap().as_deref(),
                    Some(b"prekey".as_slice())
                );
                assert_eq!(reopened.delete_expired_msg_secrets(300).await.unwrap(), 0);
                assert_eq!(
                    reopened
                        .get_msg_secret_with_ts("chat", "sender", "out")
                        .await
                        .unwrap(),
                    Some((vec![8; 32], 100))
                );
                let (one, two) = tokio::join!(
                    reopened.take_sent_message("chat", "out"),
                    reopened.take_sent_message("chat", "out")
                );
                let results = [one.unwrap(), two.unwrap()];
                assert_eq!(
                    results
                        .iter()
                        .filter_map(Option::as_deref)
                        .collect::<Vec<_>>(),
                    vec![b"outgoing".as_slice()]
                );

                let other = PgStore::new(app.db.clone(), Uuid::new_v4());
                assert!(!other.exists().await.unwrap());
                assert!(other.get_session("test@lid").await.unwrap().is_none());
                assert!(
                    other
                        .get_pending_inbound("chat", "sender", "in")
                        .await
                        .unwrap()
                        .is_none()
                );
            })
        })
        .await;
    }
}
