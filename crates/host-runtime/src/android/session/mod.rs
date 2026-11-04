//! Fixed native SDK lifecycle, independent of device focus and model execution.
mod adb;
mod cli;
mod plan;
mod process;
mod readiness;
pub use cli::run;
pub const CONTRACT: &str = "ai-phone-native-android-session-v1";

#[cfg(test)]
mod tests;
