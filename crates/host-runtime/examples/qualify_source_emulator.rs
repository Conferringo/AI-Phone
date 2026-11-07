//! Maintainer-only upstream SDK qualification; never replaces an installed SDK.
#[cfg(target_os = "linux")]
#[path = "source_emulator/mod.rs"]
mod linux;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    return tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(linux::run(
            std::env::args_os().skip(1).collect(),
            std::env::var("AI_PHONE_DISPOSABLE_QUALIFICATION").as_deref() == Ok("1"),
        ));
    #[cfg(not(target_os = "linux"))]
    Err("source SDK qualification requires a disposable Linux x64 host".into())
}
