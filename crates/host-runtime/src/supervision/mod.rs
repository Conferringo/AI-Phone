//! Application-owned Linux service lifetime, separate from model and database work.
pub const CONTRACT: &str = "ai-phone-linux-supervision-v1";

#[cfg(target_os = "linux")]
mod linux;

pub async fn run(arguments: &[std::ffi::OsString]) -> crate::Result<()> {
    #[cfg(target_os = "linux")]
    return linux::run(arguments).await;
    #[cfg(not(target_os = "linux"))]
    {
        let _ = arguments;
        Err(crate::Error::Invalid("service_supervision_requires_linux"))
    }
}
