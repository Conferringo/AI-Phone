//! Rooted Android host primitives. APK resources and dual-device setup are separate.
use crate::Result;
use std::ffi::OsString;

pub const CONTRACT: &str = "ai-phone-android-root-v1";

#[cfg(target_os = "linux")]
mod linux;

pub async fn run(arguments: &[OsString]) -> Result<()> {
    #[cfg(target_os = "linux")]
    return linux::run(arguments).await;
    #[cfg(not(target_os = "linux"))]
    {
        let _ = arguments;
        Err(crate::Error::Invalid("android_root_requires_android"))
    }
}
