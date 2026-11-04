//! Offline Android resources and device creation. Preparation is not boot readiness.
use crate::{Error, Installation, Result, control::Control};
use std::path::{Path, PathBuf};
pub mod cli;
mod device;
mod manifest;
mod resources;

pub const AVD_NAME: &str = "ai_phone_p2";

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
