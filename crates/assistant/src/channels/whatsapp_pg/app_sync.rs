use std::collections::HashMap;

use sqlx::types::Json;
use whatsapp_rust::wacore::{
    appstate::{hash::HashState, processor::AppStateMutationMAC},
    store::{
        error::Result,
        traits::{AppStateSyncKey, AppSyncStore},
    },
};

use super::{PgStore, database_error, serialization_error};

#[async_trait::async_trait]
impl AppSyncStore for PgStore {
    async fn get_sync_key(&self, key_id: &[u8]) -> Result<Option<AppStateSyncKey>> {
        let key = sqlx::query_as::<_, (Vec<u8>, Vec<u8>, i64)>(
            "SELECT key_data, fingerprint, key_timestamp FROM whatsapp_sync_keys
             WHERE binding_id = $1 AND key_id = $2",
        )
        .bind(self.binding)
        .bind(key_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(
            key.map(|(key_data, fingerprint, timestamp)| AppStateSyncKey {
                key_data,
                fingerprint,
                timestamp,
            }),
        )
    }

    async fn set_sync_key(&self, key_id: &[u8], key: AppStateSyncKey) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_sync_keys (binding_id, key_id, key_data, fingerprint, key_timestamp)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (binding_id, key_id) DO UPDATE SET key_data = EXCLUDED.key_data,
                fingerprint = EXCLUDED.fingerprint, key_timestamp = EXCLUDED.key_timestamp",
        )
        .bind(self.binding)
        .bind(key_id)
        .bind(key.key_data)
        .bind(key.fingerprint)
        .bind(key.timestamp)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn get_version(&self, name: &str) -> Result<HashState> {
        let state = sqlx::query_scalar::<_, Json<HashState>>(
            "SELECT state FROM whatsapp_sync_versions WHERE binding_id = $1 AND collection = $2",
        )
        .bind(self.binding)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(state.map(|Json(state)| state).unwrap_or_default())
    }

    async fn set_version(&self, name: &str, state: HashState) -> Result<()> {
        sqlx::query(
            "INSERT INTO whatsapp_sync_versions (binding_id, collection, state)
             VALUES ($1, $2, $3) ON CONFLICT (binding_id, collection) DO UPDATE SET state = EXCLUDED.state",
        )
        .bind(self.binding)
        .bind(name)
        .bind(Json(state))
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn put_mutation_macs(
        &self,
        name: &str,
        version: u64,
        mutations: &[AppStateMutationMAC],
    ) -> Result<()> {
        let (indices, values): (Vec<_>, Vec<_>) = mutations
            .iter()
            .map(|mutation| (mutation.index_mac.as_slice(), mutation.value_mac.as_slice()))
            .unzip();
        sqlx::query(
            "INSERT INTO whatsapp_mutation_macs (binding_id, collection, version, index_mac, value_mac)
             SELECT DISTINCT ON (index_mac) $1, $2, $3, index_mac, value_mac
             FROM unnest($4::bytea[], $5::bytea[]) WITH ORDINALITY AS batch(index_mac, value_mac, position)
             ORDER BY index_mac, position DESC
             ON CONFLICT (binding_id, collection, index_mac)
             DO UPDATE SET version = EXCLUDED.version, value_mac = EXCLUDED.value_mac",
        )
        .bind(self.binding)
        .bind(name)
        .bind(version as i64)
        .bind(indices)
        .bind(values)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn get_mutation_mac(&self, name: &str, index_mac: &[u8]) -> Result<Option<Vec<u8>>> {
        sqlx::query_scalar(
            "SELECT value_mac FROM whatsapp_mutation_macs
             WHERE binding_id = $1 AND collection = $2 AND index_mac = $3",
        )
        .bind(self.binding)
        .bind(name)
        .bind(index_mac)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)
    }

    async fn get_mutation_macs(
        &self,
        name: &str,
        index_macs: &[[u8; 32]],
    ) -> Result<HashMap<[u8; 32], Vec<u8>>> {
        let indices: Vec<_> = index_macs.iter().map(|index| index.as_slice()).collect();
        let rows = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
            "SELECT index_mac, value_mac FROM whatsapp_mutation_macs
             WHERE binding_id = $1 AND collection = $2 AND index_mac = ANY($3)",
        )
        .bind(self.binding)
        .bind(name)
        .bind(indices)
        .fetch_all(&self.pool)
        .await
        .map_err(database_error)?;
        rows.into_iter()
            .map(|(index, value)| {
                let index = index.as_slice().try_into().map_err(serialization_error)?;
                Ok((index, value))
            })
            .collect()
    }

    async fn delete_mutation_macs(&self, name: &str, index_macs: &[Vec<u8>]) -> Result<()> {
        sqlx::query(
            "DELETE FROM whatsapp_mutation_macs
             WHERE binding_id = $1 AND collection = $2 AND index_mac = ANY($3)",
        )
        .bind(self.binding)
        .bind(name)
        .bind(index_macs)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn clear_mutation_macs(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM whatsapp_mutation_macs WHERE binding_id = $1 AND collection = $2")
            .bind(self.binding)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(database_error)?;
        Ok(())
    }

    async fn get_latest_sync_key_id(&self) -> Result<Option<Vec<u8>>> {
        sqlx::query_scalar(
            "SELECT key_id FROM whatsapp_sync_keys WHERE binding_id = $1 ORDER BY key_id DESC LIMIT 1",
        )
        .bind(self.binding)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)
    }
}
