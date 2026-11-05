use super::{Backend, InitializeRequest, Initializer, Registration};
use crate::{Error, Result};
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

#[cfg(windows)]
fn registry<T>(value: windows_registry::Result<T>) -> Result<T> {
    value.map_err(|_| Error::Invalid("wsl_registry_invalid"))
}

#[cfg(windows)]
fn read_registration(key: windows_registry::Key) -> Result<Registration> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    let path = registry(key.get_hstring("BasePath"))?;
    Ok(Registration {
        name: registry(key.get_string("DistributionName"))?,
        base_path: PathBuf::from(OsString::from_wide(&path)),
        version: registry(key.get_u32("Version"))?,
    })
}

#[cfg(windows)]
fn read_registrations(
    key: &windows_registry::Key,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<Registration>> {
    let mut rows = Vec::new();
    for name in registry(key.keys())? {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        // Lxss also has settings keys. Distro registration IDs are braced.
        if !name.starts_with('{') || !name.ends_with('}') {
            continue;
        }
        rows.push(read_registration(registry(key.open(name))?)?);
    }
    Ok(rows)
}

pub struct Native<'a> {
    pub(super) cancelled: &'a dyn Fn() -> bool,
}

#[cfg(windows)]
impl Initializer for Native<'_> {
    fn initialize(&mut self, distribution: &str, request: &InitializeRequest) -> Result<()> {
        let mut command = system_program("wsl.exe")?;
        command.args([
            "--distribution",
            distribution,
            "--user",
            "root",
            "--cd",
            "/",
            "--exec",
            "/opt/aiphone/tools/python/bin/python3",
            "-I",
            "-B",
            "/opt/aiphone/bootstrap/wsl_backend_entry.py",
            "--initialize",
            "/dev/stdin",
        ]);
        super::initialize::run(command, request, self.cancelled)
    }
}
impl<'a> Native<'a> {
    pub fn new(cancelled: &'a dyn Fn() -> bool) -> Self {
        Self { cancelled }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum PhoneOperation {
    Contract,
    Address,
    Peer,
    Status,
    DeviceSetupContract,
    #[cfg(windows)]
    SetupDevices,
}

pub(crate) fn phone_command(
    distribution: &str,
    operation: PhoneOperation,
) -> Result<std::process::Command> {
    #[cfg(windows)]
    {
        let mut command = system_program("wsl.exe")?;
        let flag = match operation {
            PhoneOperation::Contract => "--phone-peer-contract",
            PhoneOperation::Address => "--phone-guest-address",
            PhoneOperation::Peer => "--phone-peer",
            PhoneOperation::Status => "--phone-peer-status",
            PhoneOperation::DeviceSetupContract => "--device-setup-contract",
            PhoneOperation::SetupDevices => "--setup-wsl-devices",
        };
        command.args([
            "--distribution",
            distribution,
            "--user",
            "root",
            "--cd",
            "/",
            "--exec",
            "/usr/local/libexec/aiphone-bootstrap",
            flag,
        ]);
        Ok(command)
    }
    #[cfg(not(windows))]
    {
        let _ = (distribution, operation);
        Err(Error::Invalid("wsl_host_unsupported"))
    }
}

#[cfg(windows)]
fn system_program(relative: &str) -> Result<std::process::Command> {
    use std::os::windows::process::CommandExt;
    let root = std::env::var_os("SystemRoot").ok_or(Error::Invalid("wsl_system_path"))?;
    let mut command =
        std::process::Command::new(PathBuf::from(root).join("System32").join(relative));
    command.creation_flags(0x08000000);
    Ok(command)
}

#[cfg(windows)]
impl Backend for Native<'_> {
    fn registrations(&mut self) -> Result<Vec<Registration>> {
        if (self.cancelled)() {
            return Err(Error::Cancelled);
        }
        let key = match windows_registry::CURRENT_USER
            .open(r"Software\Microsoft\Windows\CurrentVersion\Lxss")
        {
            Ok(key) => key,
            // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND / ERROR_PATH_NOT_FOUND).
            Err(error) if matches!(error.code().0 as u32, 0x8007_0002 | 0x8007_0003) => {
                return Ok(Vec::new());
            }
            Err(_) => return Err(Error::Invalid("wsl_registry_invalid")),
        };
        read_registrations(&key, self.cancelled)
    }
    fn import(&mut self, name: &str, directory: &Path, rootfs: &Path) -> Result<()> {
        let mut command = system_program("wsl.exe")?;
        command
            .arg("--import")
            .arg(name)
            .arg(directory)
            .arg(rootfs)
            .args(["--version", "2"]);
        super::command::run(
            command,
            std::time::Duration::from_secs(1800),
            self.cancelled,
        )?;
        Ok(())
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;

    struct FixtureKey(String);
    impl Drop for FixtureKey {
        fn drop(&mut self) {
            let result = windows_registry::CURRENT_USER.remove_tree(&self.0);
            assert!(result.is_ok(), "owned registry fixture cleanup failed");
        }
    }

    #[test]
    fn native_registry_fixture_preserves_unicode_and_ignores_settings_keys() {
        let nonce = tempfile::tempdir().unwrap();
        let name = format!(
            r"Software\AIPhone-HostTest-{}",
            nonce.path().file_name().unwrap().to_str().unwrap()
        );
        assert!(windows_registry::CURRENT_USER.open(&name).is_err());
        let owner = FixtureKey(name);
        let root = windows_registry::CURRENT_USER.create(&owner.0).unwrap();
        let key = root
            .create("{00000000-0000-4000-8000-000000000001}")
            .unwrap();
        key.set_string("DistributionName", "AIPhone-wsl-fixture")
            .unwrap();
        key.set_string("BasePath", r"C:\AI Phone\设备\桌面")
            .unwrap();
        key.set_u32("Version", 2).unwrap();
        root.create("TryStoreWSL").unwrap();
        let rows = read_registrations(&root, &|| false).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "AIPhone-wsl-fixture");
        assert_eq!(rows[0].base_path, PathBuf::from(r"C:\AI Phone\设备\桌面"));
        assert_eq!(rows[0].version, 2);
        root.create("{00000000-0000-4000-8000-000000000002}")
            .unwrap();
        assert_eq!(
            read_registrations(&root, &|| false).err().unwrap().code(),
            "wsl_registry_invalid"
        );
    }

    #[test]
    fn native_registry_query_runs_without_launching_or_importing_a_distro() {
        // This is a read-only adapter check, not a Windows guest boot test.
        Native::new(&|| false).registrations().unwrap();
    }
}

#[cfg(not(windows))]
impl Backend for Native<'_> {
    fn registrations(&mut self) -> Result<Vec<Registration>> {
        let _ = self.cancelled;
        Err(Error::Invalid("wsl_host_unsupported"))
    }
    fn import(&mut self, _: &str, _: &Path, _: &Path) -> Result<()> {
        Err(Error::Invalid("wsl_host_unsupported"))
    }
}

#[cfg(not(windows))]
impl Initializer for Native<'_> {
    fn initialize(&mut self, _: &str, _: &InitializeRequest) -> Result<()> {
        Err(Error::Invalid("wsl_host_unsupported"))
    }
}
