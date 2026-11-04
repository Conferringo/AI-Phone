use crate::{Error, Result};
use ai_phone_api::wire::{DeviceBridgeConfig, DeviceBridgeRole};
use std::net::SocketAddr;

pub(super) struct Config {
    pub role: DeviceBridgeRole,
    pub endpoint: SocketAddr,
    pub id: String,
    pub secret: Vec<u8>,
    pub ports: [u16; 2],
}
fn invalid() -> Error {
    Error::Invalid("device_bridge_configuration")
}
fn endpoint(value: &str, role: DeviceBridgeRole) -> Result<SocketAddr> {
    let address: SocketAddr = value.parse().map_err(|_| invalid())?;
    let private = match address.ip() {
        std::net::IpAddr::V4(ip) => ip.is_private() || ip.is_loopback(),
        std::net::IpAddr::V6(ip) => ip.is_unique_local() || ip.is_loopback(),
    };
    if !private || (role == DeviceBridgeRole::Guest && address.port() == 0) {
        return Err(invalid());
    }
    Ok(address)
}
fn credentials(id: &str, secret: &[u8]) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && secret.len() == 32
}
fn port(value: u32, default: u16) -> Result<u16> {
    let value = if value == 0 {
        default
    } else {
        u16::try_from(value).map_err(|_| invalid())?
    };
    if value < 1024 {
        return Err(invalid());
    }
    Ok(value)
}
fn ports(value: &DeviceBridgeConfig, role: DeviceBridgeRole) -> Result<[u16; 2]> {
    if role == DeviceBridgeRole::Host && (value.adb_port != 0 || value.emulator_port != 0) {
        return Err(invalid());
    }
    let result = [
        port(value.adb_port, 15037)?,
        port(value.emulator_port, 8558)?,
    ];
    if result[0] == result[1] {
        return Err(invalid());
    }
    Ok(result)
}
impl TryFrom<DeviceBridgeConfig> for Config {
    type Error = Error;
    fn try_from(value: DeviceBridgeConfig) -> Result<Self> {
        let role = match DeviceBridgeRole::try_from(value.role) {
            Ok(DeviceBridgeRole::Host) => DeviceBridgeRole::Host,
            Ok(DeviceBridgeRole::Guest) => DeviceBridgeRole::Guest,
            _ => return Err(invalid()),
        };
        if value.version != 1 || !credentials(&value.bridge_id, &value.secret) {
            return Err(invalid());
        }
        Ok(Self {
            endpoint: endpoint(&value.endpoint, role)?,
            ports: ports(&value, role)?,
            role,
            id: value.bridge_id,
            secret: value.secret,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request(role: DeviceBridgeRole) -> DeviceBridgeConfig {
        DeviceBridgeConfig {
            version: 1,
            role: role as i32,
            endpoint: "127.0.0.1:23000".into(),
            bridge_id: "a".repeat(32),
            secret: vec![7; 32],
            ..Default::default()
        }
    }
    #[test]
    fn accepts_private_installer_roles_and_preserves_existing_guest_ports() {
        for role in [DeviceBridgeRole::Host, DeviceBridgeRole::Guest] {
            let value = Config::try_from(request(role)).unwrap();
            assert_eq!(value.role, role);
            assert_eq!(value.endpoint, "127.0.0.1:23000".parse().unwrap());
            assert_eq!(value.id, "a".repeat(32));
            assert_eq!(value.secret, vec![7; 32]);
            assert_eq!(value.ports, [15037, 8558]);
        }
    }
    #[test]
    fn rejects_public_or_ambiguous_endpoints_and_incomplete_credentials() {
        for endpoint in [
            "0.0.0.0:23000",
            "8.8.8.8:23000",
            "[::]:23000",
            "example.com:23000",
            "127.0.0.1:0",
        ] {
            let mut value = request(DeviceBridgeRole::Guest);
            value.endpoint = endpoint.into();
            assert!(Config::try_from(value).is_err(), "{endpoint}");
        }
        for field in ["version", "role", "id", "secret", "port", "host_ports"] {
            let mut value = request(DeviceBridgeRole::Guest);
            match field {
                "version" => value.version = 2,
                "role" => value.role = 0,
                "id" => value.bridge_id = "wrong".into(),
                "secret" => value.secret.pop().map(|_| ()).unwrap(),
                "port" => value.adb_port = 65536,
                "host_ports" => {
                    value.role = DeviceBridgeRole::Host as i32;
                    value.adb_port = 12345;
                }
                _ => unreachable!(),
            }
            assert!(Config::try_from(value).is_err(), "{field}");
        }
    }
    #[test]
    fn supports_selected_wsl_addresses_and_relocated_guest_ports() {
        for endpoint in ["172.22.64.1:23000", "192.168.65.2:23000", "[fd00::1]:23000"] {
            let mut value = request(DeviceBridgeRole::Guest);
            value.endpoint = endpoint.into();
            value.adb_port = 23001;
            value.emulator_port = 23002;
            assert_eq!(Config::try_from(value).unwrap().ports, [23001, 23002]);
        }
        let mut host = request(DeviceBridgeRole::Host);
        host.endpoint = "127.0.0.1:0".into();
        assert_eq!(Config::try_from(host).unwrap().endpoint.port(), 0);
        let mut guest = request(DeviceBridgeRole::Guest);
        guest.adb_port = 8558;
        assert!(
            Config::try_from(guest).is_err(),
            "services cannot share a listener"
        );
    }
}
