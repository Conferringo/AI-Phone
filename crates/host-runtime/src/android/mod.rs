//! Offline Android resources and device creation. Preparation is not boot readiness.
use crate::{Error, Installation, Result, control::Control};
use std::path::{Path, PathBuf};
pub mod cli;
mod device;
pub mod input;
pub mod maintenance;
mod manifest;
mod resources;
pub mod session;
#[cfg(target_os = "macos")]
pub use session::avo_runtime;

pub const AVD_NAME: &str = "ai_phone_p2";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    #[default]
    Goldfish,
    Avo,
    Whpx,
}

impl Engine {
    pub fn installed(sdk: &Path) -> Self {
        if sdk.join("avo/avo-vmm").is_file() {
            Self::Avo
        } else if sdk.join("cuttlefish/bzImage").is_file() {
            Self::Whpx
        } else {
            Self::Goldfish
        }
    }

    fn sdk_images(self, abi: &str) -> PathBuf {
        match self {
            Self::Goldfish => format!("system-images/android-35/default/{abi}").into(),
            Self::Avo => "avo/images".into(),
            Self::Whpx => "cuttlefish".into(),
        }
    }

    fn api(self) -> u32 {
        match self {
            Self::Avo => 37,
            Self::Goldfish | Self::Whpx => 35,
        }
    }

    pub fn serial(self, console_port: u16) -> String {
        match self {
            Self::Goldfish => format!("emulator-{console_port}"),
            #[cfg(target_os = "macos")]
            Self::Avo => format!(
                "localfilesystem:{}",
                avo_runtime(console_port).join("adb.sock").display()
            ),
            #[cfg(not(target_os = "macos"))]
            Self::Avo => unreachable!("Avo is available only on macOS"),
            Self::Whpx => format!("127.0.0.1:{}", console_port + 1),
        }
    }
}

pub struct Resources {
    pub cpus: u32,
    pub memory_mb: u32,
    pub storage_gb: u32,
}
impl Resources {
    fn validate(&self) -> Result<()> {
        if ![1, 2, 4].contains(&self.cpus)
            || ![1536, 2048, 3072, 4096].contains(&self.memory_mb)
            || ![16, 24, 32].contains(&self.storage_gb)
        {
            return Err(Error::Invalid("invalid_device_resources"));
        }
        Ok(())
    }
}

pub struct Prepared {
    pub sdk_root: PathBuf,
    pub avd_home: PathBuf,
    pub reused_sdk: bool,
    pub reused_device: bool,
}
impl Prepared {
    pub fn wire(&self) -> Result<ai_phone_api::wire::NativeAndroidPrepared> {
        Ok(ai_phone_api::wire::NativeAndroidPrepared {
            version: 1,
            sdk_root: self
                .sdk_root
                .to_str()
                .ok_or(Error::Invalid("android_device_path"))?
                .into(),
            avd_home: self
                .avd_home
                .to_str()
                .ok_or(Error::Invalid("android_device_path"))?
                .into(),
            avd_name: AVD_NAME.into(),
            reused_sdk: self.reused_sdk,
            reused_device: self.reused_device,
        })
    }
}

impl Installation {
    pub fn prepare_android(
        &mut self,
        payload: &Path,
        identity: &str,
        resources: Resources,
        available: &dyn Fn(&Path) -> u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Prepared> {
        let platform = manifest::Platform::current()?;
        self.prepare_android_for(
            payload,
            identity,
            resources,
            platform,
            &Control {
                available,
                cancelled,
            },
        )
    }

    fn prepare_android_for(
        &mut self,
        payload: &Path,
        identity: &str,
        resources: Resources,
        platform: manifest::Platform,
        control: &Control<'_>,
    ) -> Result<Prepared> {
        resources.validate()?;
        control.check()?;
        let (sdk_root, reused_sdk) =
            resources::stage(&self.home, payload, identity, platform, control)?;
        let (avd_home, reused_device) = device::create(
            &self.home,
            &sdk_root,
            platform.spec().abi,
            resources,
            control,
        )?;
        Ok(Prepared {
            sdk_root,
            avd_home,
            reused_sdk,
            reused_device,
        })
    }
}

#[cfg(test)]
mod tests;
