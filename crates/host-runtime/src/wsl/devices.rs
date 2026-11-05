//! Device completion is an observed protobuf receipt, separate from registration.
use super::Native;
use crate::{Error, Result};
use ai_phone_api::wire::NativeWslDevicesReady;

#[cfg(windows)]
use ai_phone_api::wire::NativeWslDeviceSetupRequest;
#[cfg(any(windows, test))]
use prost::Message;

impl Native<'_> {
    pub fn complete_devices(
        &mut self,
        distribution: &str,
        bridge_id: &str,
    ) -> Result<NativeWslDevicesReady> {
        #[cfg(windows)]
        {
            super::devices::request(bridge_id)?;
            if !distribution.starts_with("AIPhone-wsl-")
                || distribution.len() > 80
                || !distribution
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return Err(Error::Invalid("wsl_device_setup_request"));
            }
            let capability = super::command::run(
                super::phone_command(distribution, super::PhoneOperation::DeviceSetupContract)?,
                std::time::Duration::from_secs(15),
                self.cancelled,
            )?;
            if capability != format!("{}\n", super::DEVICE_SETUP_CONTRACT).as_bytes() {
                return Err(Error::Invalid("wsl_device_setup_upgrade_required"));
            }
            let payload = NativeWslDeviceSetupRequest {
                version: 1,
                bridge_id: bridge_id.into(),
            }
            .encode_to_vec();
            let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
            frame.extend(payload);
            let bytes = super::command::run_input(
                super::phone_command(distribution, super::PhoneOperation::SetupDevices)?,
                &frame,
                std::time::Duration::from_secs(600),
                self.cancelled,
            );
            match bytes {
                Ok(bytes) => acknowledge(&bytes, bridge_id, (self.cancelled)()),
                Err(error)
                    if matches!(error, Error::Cancelled)
                        || error.code() == "wsl_command_unknown" =>
                {
                    Err(Error::Invalid("wsl_device_setup_unknown"))
                }
                Err(error) => Err(error),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (distribution, bridge_id);
            Err(Error::Invalid("wsl_host_unsupported"))
        }
    }
}
#[cfg(any(windows, test))]
fn request(bridge_id: &str) -> Result<()> {
    if bridge_id.len() != 32 || !bridge_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Invalid("wsl_device_setup_request"));
    }
    Ok(())
}
#[cfg(any(windows, test))]
fn acknowledge(bytes: &[u8], bridge_id: &str, cancelled: bool) -> Result<NativeWslDevicesReady> {
    if cancelled {
        return Err(Error::Invalid("wsl_device_setup_unknown"));
    }
    if bytes.len() < 4
        || bytes.len() > 516
        || u32::from_be_bytes(
            bytes[..4]
                .try_into()
                .map_err(|_| Error::Invalid("wsl_device_setup_receipt"))?,
        ) as usize
            != bytes.len() - 4
    {
        return Err(Error::Invalid("wsl_device_setup_receipt"));
    }
    let value = NativeWslDevicesReady::decode(&bytes[4..])
        .map_err(|_| Error::Invalid("wsl_device_setup_receipt"))?;
    if value.version != 1
        || value.bridge_id != bridge_id
        || value.desktop_id.is_empty()
        || value.desktop_id.len() > 128
        || value.desktop_id.chars().any(char::is_whitespace)
    {
        return Err(Error::Invalid("wsl_device_setup_receipt"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_requires_an_exact_pair_and_a_bounded_ready_receipt() {
        let id = "a".repeat(32);
        request(&id).unwrap();
        assert!(request("../foreign").is_err());
        let value = NativeWslDevicesReady {
            version: 1,
            bridge_id: id.clone(),
            desktop_id: "fixture-desktop".into(),
        };
        let payload = value.encode_to_vec();
        let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
        frame.extend(payload);
        assert_eq!(acknowledge(&frame, &id, false).unwrap(), value);
        assert!(
            acknowledge(&frame, &id, true).is_err(),
            "Cancellation must not publish a valid late receipt"
        );
        assert!(acknowledge(&frame, &"b".repeat(32), false).is_err());
        for invalid in [&[][..], &frame[..frame.len() - 1], b"ready", &vec![0; 517]] {
            assert!(acknowledge(invalid, &id, false).is_err());
        }
        let mut trailing = frame;
        trailing.push(0);
        assert!(acknowledge(&trailing, &id, false).is_err());
    }
}
