use super::{PgStore, database_error, serialization_error};
use async_trait::async_trait;
use std::sync::Arc;
use whatsapp_rust::wacore::store::{
    Device, device::DEVICE_PROPS, error::Result, traits::DeviceStore,
};

#[async_trait]
impl DeviceStore for PgStore {
    async fn save(&self, device: &Device) -> Result<()> {
        let state = serde_json::to_vec(device).map_err(serialization_error)?;
        sqlx::query(
            "INSERT INTO whatsapp_devices(binding_id,state) VALUES($1,$2)
             ON CONFLICT(binding_id) DO UPDATE SET state=EXCLUDED.state",
        )
        .bind(self.binding)
        .bind(state)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(())
    }

    async fn load(&self) -> Result<Option<Device>> {
        let state: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT state FROM whatsapp_devices WHERE binding_id=$1")
                .bind(self.binding)
                .fetch_optional(&self.pool)
                .await
                .map_err(database_error)?;
        state
            .map(|state| {
                let mut device: Device =
                    serde_json::from_slice(&state).map_err(serialization_error)?;
                device.device_props = Arc::new(DEVICE_PROPS.clone());
                Ok(device)
            })
            .transpose()
    }

    async fn exists(&self) -> Result<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM whatsapp_devices WHERE binding_id=$1)")
            .bind(self.binding)
            .fetch_one(&self.pool)
            .await
            .map_err(database_error)
    }

    async fn create(&self) -> Result<i32> {
        let state = serde_json::to_vec(&Device::new()).map_err(serialization_error)?;
        sqlx::query(
            "INSERT INTO whatsapp_devices(binding_id,state) VALUES($1,$2)
             ON CONFLICT(binding_id) DO NOTHING",
        )
        .bind(self.binding)
        .bind(state)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        // The protocol backend has one device per binding, as with its legacy store.
        Ok(1)
    }
}
