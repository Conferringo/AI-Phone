//! Route managed phone operations to the retained Android runtime.
use crate::*;

pub(crate) const NO_PHONE_BINDING: &str = "此Agent未授权手机";
pub(crate) const PHONE_BINDING_REVOKED: &str = "该任务没有此设备的授权";

pub struct Target {
    pub runner: u16,
    pub token: String,
    pub serial: String,
}
pub async fn target(app: &App, id: &str) -> Result<Target, String> {
    if id == registry::PHONE {
        return Ok(Target {
            runner: 17862,
            token: app.phone_token.clone(),
            serial: "emulator-5564".into(),
        });
    }
    Err("手机未接入或已归档".into())
}

pub async fn for_run(app: &App, run: Uuid) -> Result<String, String> {
    let (id, authorized): (String, bool) = sqlx::query_as(
        "SELECT d.id,EXISTS(SELECT 1 FROM agent_device_bindings a
            WHERE a.agent_id=r.agent_id AND a.device_id=d.id)
         FROM runs r JOIN run_device_bindings b ON b.run_id=r.id
         JOIN devices d ON d.id=b.device_id
         WHERE r.id=$1 AND d.kind='phone' AND d.runtime_state='managed'
         ORDER BY (d.id=r.device_id) DESC,d.created_at,d.id LIMIT 1",
    )
    .bind(run)
    .fetch_optional(&app.db)
    .await
    .map_err(|e| e.to_string())?
    .ok_or(NO_PHONE_BINDING)?;
    if !authorized {
        return Err(PHONE_BINDING_REVOKED.into());
    }
    Ok(id)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routing_ignores_request_specific_wire_types() {
        let id = "second-phone";
        let messages = vec![
            wire::DeviceRequest {
                phone_id: id.into(),
                include_elements: true,
                ..Default::default()
            }
            .encode_to_vec(),
            wire::TakeControlRequest {
                phone_id: id.into(),
                controller: "assistant".into(),
                ..Default::default()
            }
            .encode_to_vec(),
            wire::DeviceActionRequest {
                phone_id: id.into(),
                run_id: "run-1".into(),
                control_epoch: 5,
                ..Default::default()
            }
            .encode_to_vec(),
        ];
        for bytes in messages {
            assert_eq!(
                wire::PhoneScope::decode(bytes.as_slice()).unwrap().phone_id,
                id
            );
        }
    }
}
