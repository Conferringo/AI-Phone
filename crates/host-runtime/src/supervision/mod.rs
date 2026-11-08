//! Application-owned Linux service lifetime, separate from model and database work.
pub const CONTRACT: &str = "ai-phone-linux-supervision-v2";

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;

pub async fn run(arguments: &[std::ffi::OsString]) -> crate::Result<()> {
    #[cfg(target_os = "linux")]
    return linux::run(arguments).await;
    #[cfg(not(target_os = "linux"))]
    {
        let _ = arguments;
        Err(crate::Error::Invalid("service_supervision_requires_linux"))
    }
}

pub async fn control(arguments: &[std::ffi::OsString]) -> crate::Result<()> {
    #[cfg(target_os = "linux")]
    return linux::control::run(arguments).await;
    #[cfg(target_os = "macos")]
    return macos::run(arguments).await;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = arguments;
        Err(crate::Error::Invalid("service_control_requires_linux"))
    }
}
