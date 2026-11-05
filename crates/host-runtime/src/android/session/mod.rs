//! Fixed native SDK lifecycle, independent of device focus and model execution.
mod adb;
mod cli;
mod link;
mod plan;
mod process;
mod readiness;
pub use cli::run;
pub const CONTRACT: &str = "ai-phone-native-android-session-v1";

pub(crate) async fn backend_observation() -> crate::Result<ai_phone_api::wire::NativeAndroidBooted>
{
    readiness::observe(
        &adb::Adb {
            port: 15037,
            serial: "emulator-5564".into(),
        },
        true,
    )
    .await
}

#[cfg(test)]
mod tests;
