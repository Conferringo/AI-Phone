//! The fixed native component commands shared by the standalone and bundled binary.
use crate::{Error, Result};
use std::ffi::OsString;

enum Service {
    Bridge,
    Supervisor,
    Android,
    AndroidMaintenance,
    PhonePeer,
    Source,
    GuestAddress,
    PeerStatus,
    Input,
    Control,
    GuestAppResources,
    #[cfg(windows)]
    VmOwner,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    VmGuest,
    #[cfg(unix)]
    Images,
    #[cfg(unix)]
    WslBackend,
    #[cfg(unix)]
    UnixHost,
    #[cfg(unix)]
    NetworkObservation,
    #[cfg(unix)]
    NetworkControl,
    #[cfg(unix)]
    SourceServices,
}
fn start(service: Service, arguments: &[OsString]) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    match service {
        Service::Bridge => runtime.block_on(crate::device_bridge::run(arguments)),
        Service::Supervisor => runtime.block_on(crate::supervision::run(arguments)),
        Service::Android => runtime.block_on(crate::android::session::run(arguments)),
        Service::AndroidMaintenance => {
            runtime.block_on(crate::android::maintenance::run(arguments))
        }
        Service::PhonePeer => runtime.block_on(crate::device_bridge::run_peer(arguments)),
        Service::Source => runtime.block_on(crate::source_cli::run(arguments)),
        Service::GuestAddress => runtime.block_on(crate::device_bridge::guest::address(arguments)),
        Service::PeerStatus => runtime.block_on(crate::device_bridge::guest::status(arguments)),
        Service::Input => runtime.block_on(crate::android::input::run(arguments)),
        Service::Control => runtime.block_on(crate::supervision::control(arguments)),
        Service::GuestAppResources => runtime.block_on(crate::guest_app_resources::run(arguments)),
        #[cfg(windows)]
        Service::VmOwner => runtime.block_on(crate::qemu_host::worker(arguments)),
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        Service::VmGuest => runtime.block_on(crate::linux_vm::guest(arguments)),
        #[cfg(unix)]
        Service::Images => runtime.block_on(crate::images::run(arguments)),
        #[cfg(unix)]
        Service::WslBackend => runtime.block_on(crate::wsl::backend::run(arguments)),
        #[cfg(unix)]
        Service::UnixHost => runtime.block_on(crate::unix_initializer::run(arguments)),
        #[cfg(unix)]
        Service::NetworkObservation => runtime.block_on(crate::network_observation::run(arguments)),
        #[cfg(unix)]
        Service::NetworkControl => runtime.block_on(crate::network_observation::control(arguments)),
        #[cfg(unix)]
        Service::SourceServices => runtime.block_on(crate::source_services::run(arguments)),
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
        "--stage-source" => Some(start(Service::Source, &arguments[1..])),
        "--source-staging-contract" => Some(contract(arguments, crate::source_cli::CONTRACT)),
        "--device-bridge" => Some(start(Service::Bridge, &arguments[1..])),
        "--phone-peer" => Some(start(Service::PhonePeer, &arguments[1..])),
        "--phone-guest-address" => Some(start(Service::GuestAddress, &arguments[1..])),
        "--phone-peer-status" => Some(start(Service::PeerStatus, &arguments[1..])),
        "--setup-phone-input" => Some(start(Service::Input, &arguments[1..])),
        "--phone-input-setup-contract" => {
            Some(contract(arguments, crate::android::input::CONTRACT))
        }
        "--phone-peer-contract" => Some(contract(arguments, crate::device_bridge::PEER_CONTRACT)),
        #[cfg(windows)]
        "--qemu-backend-owner" => Some(start(Service::VmOwner, &arguments[1..])),
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        "--qemu-guest-control" => Some(start(Service::VmGuest, &arguments[1..])),
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        "--qemu-guest-control-contract" => Some(contract(arguments, "ai-phone-qemu-bootstrap-v2")),
        "--prepare-android" => Some(crate::android::cli::run(&arguments[1..])),
        "--prepare-android-contract" => Some(contract(arguments, crate::android::cli::CONTRACT)),
        "--android-session" => Some(start(Service::Android, &arguments[1..])),
        "--android-maintenance" => Some(start(Service::AndroidMaintenance, &arguments[1..])),
        "--android-session-contract" => {
            Some(contract(arguments, crate::android::session::CONTRACT))
        }
        "--supervise" => Some(start(Service::Supervisor, &arguments[1..])),
        "--service-control" => Some(start(Service::Control, &arguments[1..])),
        #[cfg(unix)]
        "--source-services" => Some(start(Service::SourceServices, &arguments[1..])),
        "--device-bridge-contract" => Some(contract(arguments, crate::device_bridge::CONTRACT)),
        "--service-supervision-contract" => Some(contract(arguments, crate::supervision::CONTRACT)),
        "--prepare-host-profile" => Some(crate::profile::run(&arguments[1..])),
        "--guest-app-resources" => Some(start(Service::GuestAppResources, &arguments[1..])),
        "--guest-app-resources-contract" => {
            Some(contract(arguments, crate::guest_app_resources::CONTRACT))
        }
        "--prepare-device-network" => Some(crate::network_paths::run(&arguments[1..])),
        #[cfg(unix)]
        "--observe-device-network" => Some(start(Service::NetworkObservation, &arguments[1..])),
        #[cfg(unix)]
        "--control-device-network" => Some(start(Service::NetworkControl, &arguments[1..])),
        "--write-host-profile" => Some(crate::profile::write(&arguments[1..])),
        "--validate-host-profile" => Some(crate::profile::check(&arguments[1..])),
        "--host-profile-contract" => Some(contract(arguments, crate::profile::CONTRACT)),
        #[cfg(unix)]
        "--runtime-environment" => Some(crate::runtime_environment::run(&arguments[1..])),
        #[cfg(unix)]
        "--runtime-environment-contract" => {
            Some(contract(arguments, crate::runtime_environment::CONTRACT))
        }
        #[cfg(unix)]
        "--unix-host-initializer-contract" => {
            Some(contract(arguments, crate::unix_initializer::CONTRACT))
        }
        #[cfg(unix)]
        "--initialize-unix-host" => Some(start(Service::UnixHost, &arguments[1..])),
        "--wsl-backend-initializer-contract" => Some(contract(
            arguments,
            crate::wsl::BACKEND_INITIALIZER_CONTRACT,
        )),
        #[cfg(unix)]
        "--initialize-wsl-backend" => Some(start(Service::WslBackend, &arguments[1..])),
        #[cfg(unix)]
        "--activate-images" => Some(start(Service::Images, &arguments[1..])),
        #[cfg(unix)]
        "--image-activation-contract" => Some(contract(arguments, crate::images::CONTRACT)),
        #[cfg(unix)]
        "--managed-engine" => Some(crate::managed_engine::run(&arguments[1..])),
        #[cfg(unix)]
        "--managed-engine-contract" => Some(contract(arguments, crate::managed_engine::CONTRACT)),
        _ => None,
    }
}

pub(crate) async fn interrupt() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {result=tokio::signal::ctrl_c()=>result,_=terminate.recv()=>Ok(())}
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}
