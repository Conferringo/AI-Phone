//! The fixed native component commands shared by the standalone and bundled binary.
use crate::{Error, Result};
use std::ffi::OsString;

enum Service {
    Bridge,
    Supervisor,
}
fn start(service: Service, arguments: &[OsString]) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    match service {
        Service::Bridge => runtime.block_on(crate::device_bridge::run(arguments)),
        Service::Supervisor => runtime.block_on(crate::supervision::run(arguments)),
    }
}
fn contract(arguments: &[OsString], value: &str) -> Result<()> {
    if arguments.len() != 1 {
        return Err(Error::Invalid("native_component_arguments"));
    }
    println!("{value}");
    Ok(())
}
/// Recognized commands cannot fall through into model, storage or source setup.
pub fn run(arguments: &[OsString]) -> Option<Result<()>> {
    match arguments.first()?.to_str()? {
        "--device-bridge" => Some(start(Service::Bridge, &arguments[1..])),
        "--prepare-android" => Some(crate::android::cli::run(&arguments[1..])),
        "--prepare-android-contract" => Some(contract(arguments, crate::android::cli::CONTRACT)),
        "--supervise" => Some(start(Service::Supervisor, &arguments[1..])),
        "--device-bridge-contract" => Some(contract(arguments, crate::device_bridge::CONTRACT)),
        "--service-supervision-contract" => Some(contract(arguments, crate::supervision::CONTRACT)),
        _ => None,
    }
}
