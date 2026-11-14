use sqlx::types::Json;
use whatsapp_rust::wacore::store::{
    error::Result,
    traits::{
        DeviceListRecord, LidPnMappingEntry, PendingInboundKey, PendingInboundRow, ProtocolStore,
        TcTokenEntry,
    },
};

use super::{PgStore, database_error};

type LidRow = (String, String, i64, i64, String);

fn lid_mapping(
    (lid, phone_number, created_at, updated_at, learning_source): LidRow,
) -> LidPnMappingEntry {
    LidPnMappingEntry {
        lid,
        phone_number,
        created_at,
        updated_at,
        learning_source,
    }
}

#[async_trait::async_trait]
impl ProtocolStore for PgStore {
    async fn get_sender_key_devices(&self, group_jid: &str) -> Result<Vec<(String, bool)>> {
        sqlx::query_as(
            "SELECT device_jid, has_key FROM whatsapp_sender_key_devices
             WHERE binding_id = $1 AND group_jid = $2",
        )
        .bind(self.binding)
        .bind(group_jid)
        .fetch_all(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn set_sender_key_status(&self, group_jid: &str, entries: &[(&str, bool)]) -> Result<()> {
        let (devices, states): (Vec<_>, Vec<_>) = entries.iter().copied().unzip();
        sqlx::query(
            "INSERT INTO whatsapp_sender_key_devices (binding_id, group_jid, device_jid, has_key)
             SELECT DISTINCT ON (device_jid) $1, $2, device_jid, has_key
             FROM unnest($3::text[], $4::boolean[]) WITH ORDINALITY AS batch(device_jid, has_key, position)
             ORDER BY device_jid, position DESC
             ON CONFLICT (binding_id, group_jid, device_jid) DO UPDATE SET has_key = EXCLUDED.has_key",
        )
        .bind(self.binding)
        .bind(group_jid)
        .bind(devices)
        .bind(states)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn clear_sender_key_devices(&self, group_jid: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM whatsapp_sender_key_devices WHERE binding_id = $1 AND group_jid = $2",
        )
        .bind(self.binding)
        .bind(group_jid)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn delete_sender_key_device_rows(&self, device_jids: &[&str]) -> Result<()> {
        sqlx::query(
            "DELETE FROM whatsapp_sender_key_devices WHERE binding_id = $1 AND device_jid = ANY($2)",
        )
        .bind(self.binding)
        .bind(device_jids)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn clear_all_sender_key_devices(&self) -> Result<()> {
        sqlx::query("DELETE FROM whatsapp_sender_key_devices WHERE binding_id = $1")
            .bind(self.binding)
            .execute(&self.pool)
            .await
            .map_err(database_error)?;
        Ok(())
    }

    async fn get_lid_mapping(&self, lid: &str) -> Result<Option<LidPnMappingEntry>> {
        let row = sqlx::query_as::<_, LidRow>(
            "SELECT lid, phone_number, created_at, updated_at, learning_source
             FROM whatsapp_lid_mappings WHERE binding_id = $1 AND lid = $2",
        )
        .bind(self.binding)
        .bind(lid)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(row.map(lid_mapping))
    }

    async fn get_pn_mapping(&self, phone: &str) -> Result<Option<LidPnMappingEntry>> {
        let row = sqlx::query_as::<_, LidRow>(
            "SELECT lid, phone_number, created_at, updated_at, learning_source
             FROM whatsapp_lid_mappings WHERE binding_id = $1 AND phone_number = $2
             ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(self.binding)
        .bind(phone)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(row.map(lid_mapping))
    }

    async fn put_lid_mapping(&self, entry: &LidPnMappingEntry) -> Result<()> {
        self.put_lid_mappings(std::slice::from_ref(entry)).await
    }

    async fn put_lid_mappings(&self, entries: &[LidPnMappingEntry]) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_lid_mappings
                (binding_id, lid, phone_number, created_at, updated_at, learning_source)
             SELECT DISTINCT ON (lid) $1, lid, phone_number,
                first_value(created_at) OVER (PARTITION BY lid ORDER BY position),
                updated_at, learning_source
             FROM ROWS FROM (
                jsonb_to_recordset($2::jsonb) AS
                (lid text, phone_number text, created_at bigint, updated_at bigint, learning_source text)
             ) WITH ORDINALITY AS batch(lid, phone_number, created_at, updated_at, learning_source, position)
             ORDER BY lid, position DESC
             ON CONFLICT (binding_id, lid) DO UPDATE SET phone_number = EXCLUDED.phone_number,
                updated_at = EXCLUDED.updated_at, learning_source = EXCLUDED.learning_source",
        )
        .bind(self.binding)
        .bind(Json(entries))
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn get_all_lid_mappings(&self) -> Result<Vec<LidPnMappingEntry>> {
        let rows = sqlx::query_as::<_, LidRow>(
            "SELECT lid, phone_number, created_at, updated_at, learning_source
             FROM whatsapp_lid_mappings WHERE binding_id = $1",
        )
        .bind(self.binding)
        .fetch_all(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(rows.into_iter().map(lid_mapping).collect())
    }

    async fn save_base_key(&self, address: &str, message_id: &str, base_key: &[u8]) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_base_keys (binding_id, address, message_id, base_key)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (binding_id, address, message_id) DO UPDATE SET base_key = EXCLUDED.base_key",
        )
        .bind(self.binding)
        .bind(address)
        .bind(message_id)
        .bind(base_key)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn has_same_base_key(
        &self,
        address: &str,
        message_id: &str,
        current_base_key: &[u8],
    ) -> Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM whatsapp_base_keys
             WHERE binding_id = $1 AND address = $2 AND message_id = $3 AND base_key = $4)",
        )
        .bind(self.binding)
        .bind(address)
        .bind(message_id)
        .bind(current_base_key)
        .fetch_one(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn delete_base_key(&self, address: &str, message_id: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM whatsapp_base_keys WHERE binding_id = $1 AND address = $2 AND message_id = $3",
        )
        .bind(self.binding)
        .bind(address)
        .bind(message_id)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn update_device_list(&self, record: DeviceListRecord) -> Result<()> {
        self.update_device_lists(vec![record]).await
    }

    async fn update_device_lists(&self, records: Vec<DeviceListRecord>) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_device_registry (binding_id, user_id, record)
             SELECT DISTINCT ON (record->>'user') $1, record->>'user', record
             FROM jsonb_array_elements($2::jsonb) WITH ORDINALITY AS batch(record, position)
             ORDER BY record->>'user', position DESC
             ON CONFLICT (binding_id, user_id) DO UPDATE SET record = EXCLUDED.record",
        )
        .bind(self.binding)
        .bind(Json(records))
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn get_devices(&self, user: &str) -> Result<Option<DeviceListRecord>> {
        let record = sqlx::query_scalar::<_, Json<DeviceListRecord>>(
            "SELECT record FROM whatsapp_device_registry WHERE binding_id = $1 AND user_id = $2",
        )
        .bind(self.binding)
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(record.map(|Json(record)| record))
    }

    async fn delete_devices(&self, user: &str) -> Result<()> {
        sqlx::query("DELETE FROM whatsapp_device_registry WHERE binding_id = $1 AND user_id = $2")
            .bind(self.binding)
            .bind(user)
            .execute(&self.pool)
            .await
            .map_err(database_error)?;
        Ok(())
    }

    async fn get_group_metadata(&self, group_jid: &str) -> Result<Option<Vec<u8>>> {
        sqlx::query_scalar(
            "SELECT metadata FROM whatsapp_group_metadata WHERE binding_id = $1 AND group_jid = $2",
        )
        .bind(self.binding)
        .bind(group_jid)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn put_group_metadata(&self, group_jid: &str, blob: &[u8]) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_group_metadata (binding_id, group_jid, metadata)
             VALUES ($1, $2, $3)
             ON CONFLICT (binding_id, group_jid) DO UPDATE SET metadata = EXCLUDED.metadata",
        )
        .bind(self.binding)
        .bind(group_jid)
        .bind(blob)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn delete_group_metadata(&self, group_jid: &str) -> Result<()> {
        sqlx::query("DELETE FROM whatsapp_group_metadata WHERE binding_id = $1 AND group_jid = $2")
            .bind(self.binding)
            .bind(group_jid)
            .execute(&self.pool)
            .await
            .map_err(database_error)?;
        Ok(())
    }

    async fn get_tc_token(&self, jid: &str) -> Result<Option<TcTokenEntry>> {
        let token = sqlx::query_as::<_, (Vec<u8>, i64, Option<i64>)>(
            "SELECT token, token_timestamp, sender_timestamp FROM whatsapp_tc_tokens
             WHERE binding_id = $1 AND jid = $2",
        )
        .bind(self.binding)
        .bind(jid)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(
            token.map(|(token, token_timestamp, sender_timestamp)| TcTokenEntry {
                token,
                token_timestamp,
                sender_timestamp,
            }),
        )
    }

    async fn put_tc_token(&self, jid: &str, entry: &TcTokenEntry) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_tc_tokens (binding_id, jid, token, token_timestamp, sender_timestamp)
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT (binding_id, jid)
             DO UPDATE SET token = EXCLUDED.token, token_timestamp = EXCLUDED.token_timestamp,
                sender_timestamp = EXCLUDED.sender_timestamp",
        )
        .bind(self.binding)
        .bind(jid)
        .bind(&entry.token)
        .bind(entry.token_timestamp)
        .bind(entry.sender_timestamp)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn delete_tc_token(&self, jid: &str) -> Result<()> {
        sqlx::query("DELETE FROM whatsapp_tc_tokens WHERE binding_id = $1 AND jid = $2")
            .bind(self.binding)
            .bind(jid)
            .execute(&self.pool)
            .await
            .map_err(database_error)?;
        Ok(())
    }

    async fn get_all_tc_token_jids(&self) -> Result<Vec<String>> {
        sqlx::query_scalar("SELECT jid FROM whatsapp_tc_tokens WHERE binding_id = $1")
            .bind(self.binding)
            .fetch_all(&self.pool)
            .await
            .map_err(database_error)
    }

    async fn delete_expired_tc_tokens(&self, token_cutoff: i64, sender_cutoff: i64) -> Result<u32> {
        let deleted = sqlx::query(
            "DELETE FROM whatsapp_tc_tokens WHERE binding_id = $1
             AND (octet_length(token) = 0 OR token_timestamp < $2)
             AND (sender_timestamp IS NULL OR sender_timestamp < $3)",
        )
        .bind(self.binding)
        .bind(token_cutoff)
        .bind(sender_cutoff)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(deleted.rows_affected() as u32)
    }

    async fn touch_tc_token_sender_timestamp(
        &self,
        jid: &str,
        sender_timestamp: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_tc_tokens (binding_id, jid, token, token_timestamp, sender_timestamp)
             VALUES ($1, $2, ''::bytea, $3, $3) ON CONFLICT (binding_id, jid)
             DO UPDATE SET sender_timestamp = greatest(whatsapp_tc_tokens.sender_timestamp, EXCLUDED.sender_timestamp)",
        )
        .bind(self.binding)
        .bind(jid)
        .bind(sender_timestamp)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn store_received_tc_token(
        &self,
        jid: &str,
        token: &[u8],
        token_timestamp: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_tc_tokens (binding_id, jid, token, token_timestamp)
             VALUES ($1, $2, $3, $4) ON CONFLICT (binding_id, jid)
             DO UPDATE SET token = EXCLUDED.token, token_timestamp = EXCLUDED.token_timestamp
             WHERE octet_length(whatsapp_tc_tokens.token) = 0
                OR EXCLUDED.token_timestamp >= whatsapp_tc_tokens.token_timestamp",
        )
        .bind(self.binding)
        .bind(jid)
        .bind(token)
        .bind(token_timestamp)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn store_sent_message(
        &self,
        chat_jid: &str,
        message_id: &str,
        payload: &[u8],
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_sent_messages (binding_id, chat_jid, message_id, payload)
             VALUES ($1, $2, $3, $4) ON CONFLICT (binding_id, chat_jid, message_id)
             DO UPDATE SET payload = EXCLUDED.payload, stored_at = EXCLUDED.stored_at",
        )
        .bind(self.binding)
        .bind(chat_jid)
        .bind(message_id)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn take_sent_message(&self, chat_jid: &str, message_id: &str) -> Result<Option<Vec<u8>>> {
        sqlx::query_scalar(
            "DELETE FROM whatsapp_sent_messages WHERE binding_id = $1 AND chat_jid = $2 AND message_id = $3
             RETURNING payload",
        )
        .bind(self.binding)
        .bind(chat_jid)
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn delete_expired_sent_messages(&self, cutoff_timestamp: i64) -> Result<u32> {
        let deleted = sqlx::query(
            "DELETE FROM whatsapp_sent_messages WHERE binding_id = $1 AND stored_at < $2",
        )
        .bind(self.binding)
        .bind(cutoff_timestamp)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(deleted.rows_affected() as u32)
    }

    async fn store_pending_inbound(
        &self,
        chat: &str,
        sender: &str,
        id: &str,
        message: &[u8],
    ) -> Result<()> {
        self.store_pending_inbound_batch(&[PendingInboundRow {
            chat,
            sender,
            id,
            message,
        }])
        .await
    }

    async fn get_pending_inbound(
        &self,
        chat: &str,
        sender: &str,
        id: &str,
    ) -> Result<Option<Vec<u8>>> {
        sqlx::query_scalar(
            "SELECT message FROM whatsapp_pending_inbound
             WHERE binding_id = $1 AND chat_jid = $2 AND sender_jid = $3 AND message_id = $4",
        )
        .bind(self.binding)
        .bind(chat)
        .bind(sender)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn delete_pending_inbound(&self, chat: &str, sender: &str, id: &str) -> Result<()> {
        self.delete_pending_inbound_batch(&[PendingInboundKey { chat, sender, id }])
            .await
    }

    async fn delete_expired_pending_inbound(&self, cutoff_timestamp: i64) -> Result<u32> {
        let deleted = sqlx::query(
            "DELETE FROM whatsapp_pending_inbound WHERE binding_id = $1 AND stored_at < $2",
        )
        .bind(self.binding)
        .bind(cutoff_timestamp)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(deleted.rows_affected() as u32)
    }

    async fn store_pending_inbound_batch(&self, rows: &[PendingInboundRow<'_>]) -> Result<()> {
        let chats: Vec<_> = rows.iter().map(|row| row.chat).collect();
        let senders: Vec<_> = rows.iter().map(|row| row.sender).collect();
        let ids: Vec<_> = rows.iter().map(|row| row.id).collect();
        let messages: Vec<_> = rows.iter().map(|row| row.message).collect();
        sqlx::query(
            "INSERT INTO whatsapp_pending_inbound (binding_id, chat_jid, sender_jid, message_id, message)
             SELECT DISTINCT ON (chat_jid, sender_jid, message_id) $1, chat_jid, sender_jid, message_id, message
             FROM unnest($2::text[], $3::text[], $4::text[], $5::bytea[]) WITH ORDINALITY
                AS batch(chat_jid, sender_jid, message_id, message, position)
             ORDER BY chat_jid, sender_jid, message_id, position DESC
             ON CONFLICT (binding_id, chat_jid, sender_jid, message_id)
             DO UPDATE SET message = EXCLUDED.message, stored_at = EXCLUDED.stored_at",
        )
        .bind(self.binding)
        .bind(chats)
        .bind(senders)
        .bind(ids)
        .bind(messages)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn delete_pending_inbound_batch(&self, keys: &[PendingInboundKey<'_>]) -> Result<()> {
        let chats: Vec<_> = keys.iter().map(|key| key.chat).collect();
        let senders: Vec<_> = keys.iter().map(|key| key.sender).collect();
        let ids: Vec<_> = keys.iter().map(|key| key.id).collect();
        sqlx::query(
            "DELETE FROM whatsapp_pending_inbound AS pending
             USING unnest($2::text[], $3::text[], $4::text[]) AS batch(chat_jid, sender_jid, message_id)
             WHERE pending.binding_id = $1 AND pending.chat_jid = batch.chat_jid
                AND pending.sender_jid = batch.sender_jid AND pending.message_id = batch.message_id",
        )
        .bind(self.binding)
        .bind(chats)
        .bind(senders)
        .bind(ids)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }
}
