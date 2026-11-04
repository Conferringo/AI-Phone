use super::super::{Prepared, Resources, device, manifest::Platform};
use crate::{Error, Result, filesystem};
use ai_phone_api::wire::{NativeAndroidSessionOwner, NativeAndroidStart};
use std::{
    fs::{self, File, OpenOptions},
    net::{Ipv4Addr, TcpListener},
    path::PathBuf,
    process::Stdio,
};
use tokio::process::Command;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Ports {
    pub adb: u16,
    pub console: u16,
    pub grpc: u16,
}
fn port(value: u32, default: u16) -> Result<u16> {
    let value = if value == 0 {
        default
    } else {
        u16::try_from(value).map_err(|_| Error::Invalid("android_session_ports"))?
    };
    if value < 1024 {
        return Err(Error::Invalid("android_session_ports"));
    }
    Ok(value)
}
impl Ports {
    pub fn parse(request: &NativeAndroidStart) -> Result<Self> {
        let value = Self {
            adb: port(request.adb_port, 15037)?,
            console: port(request.console_port, 5564)?,
            grpc: port(request.grpc_port, 8558)?,
        };
        if !(5554..=5682).contains(&value.console) || !value.console.is_multiple_of(2) {
            return Err(Error::Invalid("android_session_ports"));
        }
        let unique: std::collections::BTreeSet<_> =
            [value.adb, value.console, value.console + 1, value.grpc]
                .into_iter()
                .collect();
        if unique.len() != 4 {
            return Err(Error::Invalid("android_session_ports"));
        }
        Ok(value)
    }
    pub fn reserve(self) -> Result<Vec<TcpListener>> {
        [self.adb, self.console, self.console + 1, self.grpc]
            .into_iter()
            .map(|port| {
                TcpListener::bind((Ipv4Addr::LOCALHOST, port))
                    .map_err(|_| Error::Invalid("android_session_port_busy"))
            })
            .collect()
    }
}
pub(super) struct Plan {
    pub sdk: PathBuf,
    pub avd: PathBuf,
    pub home: PathBuf,
    pub ports: Ports,
    pub resources: Resources,
    pub suffix: &'static str,
    pub abi: &'static str,
}
impl Plan {
    pub fn parse(request: NativeAndroidStart) -> Result<Self> {
        let ports = Ports::parse(&request)?;
        let phone = request
            .phone
            .ok_or(Error::Invalid("android_session_phone"))?;
        if phone.version != 1 || phone.avd_name != super::super::AVD_NAME {
            return Err(Error::Invalid("android_session_phone"));
        }
        Self::from_prepared(
            Prepared {
                sdk_root: phone.sdk_root.into(),
                avd_home: phone.avd_home.into(),
                reused_sdk: phone.reused_sdk,
                reused_device: phone.reused_device,
            },
            ports,
        )
    }
    pub fn from_prepared(phone: Prepared, ports: Ports) -> Result<Self> {
        Self::from_prepared_for(phone, ports, Platform::current()?)
    }
    pub(super) fn from_prepared_for(
        phone: Prepared,
        ports: Ports,
        platform: Platform,
    ) -> Result<Self> {
        let spec = platform.spec();
        filesystem::existing_directory(&phone.sdk_root)?;
        filesystem::existing_directory(&phone.avd_home)?;
        let sdk = phone.sdk_root.canonicalize()?;
        let avd = phone.avd_home.canonicalize()?;
        device::verify_existing(&avd, &sdk, spec.abi)?;
        let resources = device::applied(&avd)?;
        let home = private_home(&avd)?;
        Ok(Self {
            sdk,
            avd,
            home,
            ports,
            resources,
            suffix: spec.suffix,
            abi: spec.abi,
        })
    }
    pub fn owner(&self) -> NativeAndroidSessionOwner {
        NativeAndroidSessionOwner {
            version: 1,
            sdk_root: self.sdk.to_string_lossy().into(),
            avd_home: self.avd.to_string_lossy().into(),
            adb_port: self.ports.adb.into(),
            console_port: self.ports.console.into(),
            grpc_port: self.ports.grpc.into(),
        }
    }
    pub fn serial(&self) -> String {
        format!("emulator-{}", self.ports.console)
    }
    fn command(&self, role: &str, program: &str) -> Result<Command> {
        let path = self
            .sdk
            .join(role)
            .join(format!("{program}{}", self.suffix));
        filesystem::regular(&path)?;
        let sdk = device::text(&self.sdk)?;
        let home = device::text(&self.home)?;
        let mut command = Command::new(path);
        command
            .current_dir(&self.home)
            .env("ANDROID_HOME", &sdk)
            .env("ANDROID_SDK_ROOT", &sdk)
            .env("ANDROID_AVD_HOME", device::text(&self.avd)?)
            .env("ANDROID_USER_HOME", format!("{home}/android"))
            .env("ANDROID_EMULATOR_HOME", format!("{home}/android"))
            .env("HOME", format!("{home}/profile"))
            .env("USERPROFILE", format!("{home}/profile"))
            .env("LOCALAPPDATA", format!("{home}/profile/local"))
            .env("APPDATA", format!("{home}/profile/roaming"))
            .env("ADB_MDNS_AUTO_CONNECT", "0")
            .env_remove("ADB_VENDOR_KEYS")
            .env_remove("ANDROID_SERIAL")
            .env_remove("ADB_TRACE")
            .env("ANDROID_ADB_SERVER_PORT", self.ports.adb.to_string())
            .env(
                "ADB_SERVER_SOCKET",
                format!("tcp:127.0.0.1:{}", self.ports.adb),
            )
            .stdin(Stdio::null());
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            command.env_remove(key);
        }
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        Ok(command)
    }
    pub fn adb_server(&self) -> Result<Command> {
        let mut command = self.command("platform-tools", "adb")?;
        command.args([
            "--one-device",
            &self.serial(),
            "-L",
            &format!("tcp:{}", self.ports.adb),
            "server",
            "nodaemon",
        ]);
        Ok(command)
    }
    pub fn emulator(&self) -> Result<Command> {
        let mut command = self.command("emulator", "emulator")?;
        command.args([
            "-avd",
            super::super::AVD_NAME,
            "-port",
            &self.ports.console.to_string(),
            "-grpc",
            &self.ports.grpc.to_string(),
            "-grpc-use-token",
            "-no-window",
            "-audio",
            "none",
            "-no-boot-anim",
            "-no-snapshot",
            "-writable-system",
            "-no-metrics",
            "-feature",
            "-DeviceSkinOverlay",
            "-gpu",
            if cfg!(target_os = "linux") {
                "swiftshader_indirect"
            } else {
                "host"
            },
            "-memory",
            &self.resources.memory_mb.to_string(),
            "-cores",
            &self.resources.cpus.to_string(),
            "-netspeed",
            "full",
            "-netdelay",
            "none",
        ]);
        Ok(command)
    }
    pub fn stop(&self) -> Result<Command> {
        let mut command = self.command("platform-tools", "adb")?;
        command.args(["-s", &self.serial(), "emu", "kill"]);
        Ok(command)
    }
    pub fn log(&self) -> Result<File> {
        let path = self.home.join("sdk.log");
        if fs::symlink_metadata(&path).is_ok() {
            filesystem::regular(&path)?;
        }
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        Ok(options.open(path)?)
    }
}

fn private_home(avd: &std::path::Path) -> Result<PathBuf> {
    let home = avd
        .parent()
        .ok_or(Error::Invalid("android_session_phone"))?
        .join("native");
    filesystem::directory(&home, false)?;
    for name in ["android", "profile", "profile/local", "profile/roaming"] {
        filesystem::directory(&home.join(name), false)?;
    }
    Ok(home)
}
