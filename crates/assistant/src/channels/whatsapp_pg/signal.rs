use super::{PgStore, database_error};
use async_trait::async_trait;
use sqlx::{Decode, Postgres, Type};
use std::sync::Arc;
use whatsapp_rust::{
    bytes::Bytes,
    store::{MsgSecretEntry, MsgSecretStore, SignalStore, error::Result},
};

const IDENTITY: i16 = 0;
const SESSION: i16 = 1;
const SENDER_KEY: i16 = 2;

impl PgStore {
    async fn signal_value<T>(&self, kind: i16, address: &str) -> Result<Option<T>>
    where
        T: for<'r> Decode<'r, Postgres> + Type<Postgres> + Send + Unpin,
    {
        sqlx::query_scalar(
            "SELECT value FROM whatsapp_signal WHERE binding_id=$1 AND kind=$2 AND address=$3",
        )
        .bind(self.binding)
        .bind(kind)
        .bind(address)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn put_signal(&self, kind: i16, address: &str, value: &[u8]) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_signal (binding_id,kind,address,value) VALUES ($1,$2,$3,$4)
             ON CONFLICT (binding_id,kind,address) DO UPDATE SET value=EXCLUDED.value",
        )
        .bind(self.binding)
        .bind(kind)
        .bind(address)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn put_signal_batch<T: AsRef<[u8]> + Sync>(
        &self,
        kind: i16,
        entries: &[(Arc<str>, T)],
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let (addresses, values): (Vec<&str>, Vec<&[u8]>) = entries
            .iter()
            .map(|(address, value)| (address.as_ref(), value.as_ref()))
            .unzip();
        sqlx::query(
            "INSERT INTO whatsapp_signal (binding_id,kind,address,value)
             SELECT DISTINCT ON (address) $1,$2,address,value
             FROM unnest($3::text[],$4::bytea[]) WITH ORDINALITY AS batch(address,value,position)
             ORDER BY address,position DESC
             ON CONFLICT (binding_id,kind,address) DO UPDATE SET value=EXCLUDED.value",
        )
        .bind(self.binding)
        .bind(kind)
        .bind(addresses)
        .bind(values)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn delete_signal(&self, kind: i16, address: &str) -> Result<()> {
        sqlx::query("DELETE FROM whatsapp_signal WHERE binding_id=$1 AND kind=$2 AND address=$3")
            .bind(self.binding)
            .bind(kind)
            .bind(address)
            .execute(&self.pool)
            .await
            .map_err(database_error)?;
        Ok(())
    }

    async fn put_prekey(&self, signed: bool, id: u32, value: &[u8], uploaded: bool) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_prekeys (binding_id,signed,key_id,value,uploaded)
             VALUES ($1,$2,$3,$4,$5) ON CONFLICT (binding_id,signed,key_id)
             DO UPDATE SET value=EXCLUDED.value,uploaded=EXCLUDED.uploaded",
        )
        .bind(self.binding)
        .bind(signed)
        .bind(i64::from(id))
        .bind(value)
        .bind(uploaded)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn prekey(&self, signed: bool, id: u32) -> Result<Option<Vec<u8>>> {
        sqlx::query_scalar(
            "SELECT value FROM whatsapp_prekeys WHERE binding_id=$1 AND signed=$2 AND key_id=$3",
        )
        .bind(self.binding)
        .bind(signed)
        .bind(i64::from(id))
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn delete_prekey(&self, signed: bool, id: u32) -> Result<()> {
        sqlx::query("DELETE FROM whatsapp_prekeys WHERE binding_id=$1 AND signed=$2 AND key_id=$3")
            .bind(self.binding)
            .bind(signed)
            .bind(i64::from(id))
            .execute(&self.pool)
            .await
            .map_err(database_error)?;
        Ok(())
    }
}

#[async_trait]
impl SignalStore for PgStore {
    async fn put_identity(&self, address: &str, key: [u8; 32]) -> Result<()> {
        self.put_signal(IDENTITY, address, &key).await
    }

    async fn put_identities_batch(&self, identities: &[(Arc<str>, [u8; 32])]) -> Result<()> {
        self.put_signal_batch(IDENTITY, identities).await
    }

    async fn load_identity(&self, address: &str) -> Result<Option<[u8; 32]>> {
        self.signal_value(IDENTITY, address).await
    }

    async fn delete_identity(&self, address: &str) -> Result<()> {
        self.delete_signal(IDENTITY, address).await
    }

    async fn get_session(&self, address: &str) -> Result<Option<Bytes>> {
        Ok(self
            .signal_value::<Vec<u8>>(SESSION, address)
            .await?
            .map(Bytes::from))
    }

    async fn has_session(&self, address: &str) -> Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM whatsapp_signal WHERE binding_id=$1 AND kind=$2 AND address=$3)",
        )
        .bind(self.binding)
        .bind(SESSION)
        .bind(address)
        .fetch_one(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn has_signal_state_for_user(&self, user: &str) -> Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM whatsapp_signal WHERE binding_id=$1 AND kind IN (0,1)
             AND split_part(split_part(address, '@', 1), ':', 1)=$2)",
        )
        .bind(self.binding)
        .bind(user)
        .fetch_one(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn put_session(&self, address: &str, session: &[u8]) -> Result<()> {
        self.put_signal(SESSION, address, session).await
    }

    async fn put_sessions_batch(&self, sessions: &[(Arc<str>, Bytes)]) -> Result<()> {
        self.put_signal_batch(SESSION, sessions).await
    }

    async fn delete_session(&self, address: &str) -> Result<()> {
        self.delete_signal(SESSION, address).await
    }

    async fn store_prekey(&self, id: u32, record: &[u8], uploaded: bool) -> Result<()> {
        self.put_prekey(false, id, record, uploaded).await
    }

    async fn store_prekeys_batch(&self, keys: &[(u32, Bytes)], uploaded: bool) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let (ids, records): (Vec<i64>, Vec<&[u8]>) = keys
            .iter()
            .map(|(id, record)| (i64::from(*id), record.as_ref()))
            .unzip();
        sqlx::query(
            "INSERT INTO whatsapp_prekeys (binding_id,signed,key_id,value,uploaded)
             SELECT DISTINCT ON (key_id) $1,false,key_id,value,$2
             FROM unnest($3::bigint[],$4::bytea[]) WITH ORDINALITY AS batch(key_id,value,position)
             ORDER BY key_id,position DESC ON CONFLICT (binding_id,signed,key_id)
             DO UPDATE SET value=EXCLUDED.value,uploaded=EXCLUDED.uploaded",
        )
        .bind(self.binding)
        .bind(uploaded)
        .bind(ids)
        .bind(records)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn load_prekey(&self, id: u32) -> Result<Option<Bytes>> {
        Ok(self.prekey(false, id).await?.map(Bytes::from))
    }

    async fn load_prekeys_batch(&self, ids: &[u32]) -> Result<Vec<(u32, Bytes)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as::<_, (i64, Vec<u8>)>(
            "SELECT key_id,value FROM whatsapp_prekeys
             WHERE binding_id=$1 AND NOT signed AND key_id=ANY($2)",
        )
        .bind(self.binding)
        .bind(ids.iter().copied().map(i64::from).collect::<Vec<_>>())
        .fetch_all(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(rows
            .into_iter()
            .map(|(id, record)| (id as u32, Bytes::from(record)))
            .collect())
    }

    async fn mark_prekeys_uploaded(&self, ids: &[u32]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "UPDATE whatsapp_prekeys SET uploaded=true
             WHERE binding_id=$1 AND NOT signed AND key_id=ANY($2)",
        )
        .bind(self.binding)
        .bind(ids.iter().copied().map(i64::from).collect::<Vec<_>>())
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn remove_prekey(&self, id: u32) -> Result<()> {
        self.delete_prekey(false, id).await
    }

    async fn get_max_prekey_id(&self) -> Result<u32> {
        let id = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(key_id),0) FROM whatsapp_prekeys WHERE binding_id=$1 AND NOT signed",
        )
        .bind(self.binding)
        .fetch_one(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(id as u32)
    }

    async fn store_signed_prekey(&self, id: u32, record: &[u8]) -> Result<()> {
        self.put_prekey(true, id, record, false).await
    }

    async fn load_signed_prekey(&self, id: u32) -> Result<Option<Vec<u8>>> {
        self.prekey(true, id).await
    }

    async fn load_all_signed_prekeys(&self) -> Result<Vec<(u32, Vec<u8>)>> {
        let rows = sqlx::query_as::<_, (i64, Vec<u8>)>(
            "SELECT key_id,value FROM whatsapp_prekeys WHERE binding_id=$1 AND signed",
        )
        .bind(self.binding)
        .fetch_all(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(rows
            .into_iter()
            .map(|(id, record)| (id as u32, record))
            .collect())
    }

    async fn remove_signed_prekey(&self, id: u32) -> Result<()> {
        self.delete_prekey(true, id).await
    }

    async fn put_sender_key(&self, address: &str, record: &[u8]) -> Result<()> {
        self.put_signal(SENDER_KEY, address, record).await
    }

    async fn put_sender_keys_batch(&self, sender_keys: &[(Arc<str>, Bytes)]) -> Result<()> {
        self.put_signal_batch(SENDER_KEY, sender_keys).await
    }

    async fn get_sender_key(&self, address: &str) -> Result<Option<Vec<u8>>> {
        self.signal_value(SENDER_KEY, address).await
    }

    async fn delete_sender_key(&self, address: &str) -> Result<()> {
        self.delete_signal(SENDER_KEY, address).await
    }
}

#[async_trait]
impl MsgSecretStore for PgStore {
    async fn put_msg_secrets(&self, entries: Vec<MsgSecretEntry>) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }
        sqlx::query(
            "INSERT INTO whatsapp_msg_secrets (binding_id,chat,sender,msg_id,secret,expires_at,message_ts)
             SELECT DISTINCT ON (chat,sender,msg_id) $1,chat,sender,msg_id,secret,
               CASE WHEN bool_or(expires_at=0) OVER message THEN 0 ELSE max(expires_at) OVER message END,
               max(message_ts) OVER message
             FROM unnest($2::text[],$3::text[],$4::text[],$5::bytea[],$6::bigint[],$7::bigint[])
               WITH ORDINALITY AS batch(chat,sender,msg_id,secret,expires_at,message_ts,position)
             WINDOW message AS (PARTITION BY chat,sender,msg_id)
             ORDER BY chat,sender,msg_id,position DESC
             ON CONFLICT (binding_id,chat,sender,msg_id) DO UPDATE SET secret=EXCLUDED.secret,
               expires_at=CASE WHEN whatsapp_msg_secrets.expires_at=0 OR EXCLUDED.expires_at=0 THEN 0
                 ELSE GREATEST(whatsapp_msg_secrets.expires_at,EXCLUDED.expires_at) END,
               message_ts=GREATEST(whatsapp_msg_secrets.message_ts,EXCLUDED.message_ts)",
        )
        .bind(self.binding)
        .bind(entries.iter().map(|entry| entry.chat.as_ref()).collect::<Vec<_>>())
        .bind(entries.iter().map(|entry| entry.sender.as_ref()).collect::<Vec<_>>())
        .bind(entries.iter().map(|entry| entry.msg_id.as_ref()).collect::<Vec<_>>())
        .bind(entries.iter().map(|entry| entry.secret.as_slice()).collect::<Vec<_>>())
        .bind(entries.iter().map(|entry| entry.expires_at).collect::<Vec<_>>())
        .bind(entries.iter().map(|entry| entry.message_ts).collect::<Vec<_>>())
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(entries.len())
    }

    async fn get_msg_secret(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
    ) -> Result<Option<Vec<u8>>> {
        Ok(self
            .get_msg_secret_with_ts(chat, sender, msg_id)
            .await?
            .map(|(secret, _)| secret))
    }

    async fn get_msg_secret_with_ts(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
    ) -> Result<Option<(Vec<u8>, i64)>> {
        sqlx::query_as(
            "SELECT secret,message_ts FROM whatsapp_msg_secrets
             WHERE binding_id=$1 AND chat=$2 AND sender=$3 AND msg_id=$4",
        )
        .bind(self.binding)
        .bind(chat)
        .bind(sender)
        .bind(msg_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn delete_expired_msg_secrets(&self, cutoff_timestamp: i64) -> Result<u32> {
        let deleted = sqlx::query(
            "DELETE FROM whatsapp_msg_secrets WHERE binding_id=$1 AND expires_at<>0 AND expires_at<=$2",
        )
        .bind(self.binding)
        .bind(cutoff_timestamp)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(deleted.rows_affected() as u32)
    }
}
