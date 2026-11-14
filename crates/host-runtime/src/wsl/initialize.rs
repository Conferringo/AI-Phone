//! Configuration crosses stdin; a backend acknowledgement is not device readiness.
use crate::{Error, Result};

pub struct InitializeRequest(Vec<u8>);

impl InitializeRequest {
    pub fn read(path: &std::path::Path) -> Result<Self> {
        Self::from_json(crate::filesystem::bounded_file(path)?)
    }

    pub fn from_json(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() > 8192
            || !serde_json::from_slice::<serde_json::Value>(&bytes)
                .is_ok_and(|value| value.is_object())
        {
            return Err(Error::Invalid("wsl_initialization_request"));
        }
        Ok(Self(bytes))
    }
}

impl AsRef<[u8]> for InitializeRequest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

pub trait Initializer {
    fn initialize(&mut self, distribution: &str, request: &InitializeRequest) -> Result<()>;
}

#[cfg(any(windows, test))]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Acknowledgement {
    backend_started: bool,
    phone_ready: bool,
    complete_device_bundle: bool,
    windows_boot_verified: bool,
}

#[cfg(any(windows, test))]
fn acknowledge(bytes: &[u8]) -> Result<()> {
    let last = bytes
        .split(|byte| *byte == b'\n')
        .rfind(|line| !line.iter().all(u8::is_ascii_whitespace))
        .ok_or(Error::Invalid("wsl_initialization_receipt"))?;
    let value: Acknowledgement =
        serde_json::from_slice(last).map_err(|_| Error::Invalid("wsl_initialization_receipt"))?;
    if !value.backend_started
        || value.phone_ready
        || value.complete_device_bundle
        || value.windows_boot_verified
    {
        return Err(Error::Invalid("wsl_initialization_receipt"));
    }
    Ok(())
}

#[cfg(any(windows, test))]
pub(super) fn run(
    command: std::process::Command,
    request: &InitializeRequest,
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let result = super::command::run_input(
        command,
        request.as_ref(),
        std::time::Duration::from_secs(1800),
        cancelled,
    );
    match result {
        // wsl.exe exiting does not prove that the guest initializer stopped.
        Err(Error::Cancelled) => Err(Error::Invalid("wsl_initialization_unknown")),
        Err(error) if error.code() == "wsl_command_unknown" => {
            Err(Error::Invalid("wsl_initialization_unknown"))
        }
        Err(error) => Err(error),
        Ok(bytes) => acknowledge(&bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, process::Command};

    fn request() -> InitializeRequest {
        InitializeRequest::from_json(br#"{"locale":"en","phone":{"cpus":2}}"#.to_vec()).unwrap()
    }

    #[test]
    fn requests_are_bounded_configuration_objects() {
        for bytes in [b"[]".to_vec(), b"partial".to_vec(), vec![b' '; 8193]] {
            assert_eq!(
                InitializeRequest::from_json(bytes).err().unwrap().code(),
                "wsl_initialization_request"
            );
        }
        let oversized =
            serde_json::to_vec(&serde_json::json!({"padding":"x".repeat(8192)})).unwrap();
        assert_eq!(
            InitializeRequest::from_json(oversized)
                .err()
                .unwrap()
                .code(),
            "wsl_initialization_request"
        );
        let maximum =
            serde_json::to_vec(&serde_json::json!({"padding":"x".repeat(8192-14)})).unwrap();
        assert_eq!(maximum.len(), 8192);
        InitializeRequest::from_json(maximum).unwrap();
        request();
    }

    #[test]
    fn fixture() {
        let Ok(mode) = std::env::var("AI_PHONE_WSL_INITIALIZE_FIXTURE") else {
            return;
        };
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes).unwrap();
        assert_eq!(
            bytes,
            request().0,
            "configuration must reach stdin unchanged"
        );
        if let Ok(path) = std::env::var("AI_PHONE_WSL_INITIALIZE_READY") {
            std::fs::write(path, b"input-consumed").unwrap();
        }
        if mode == "wait" {
            loop {
                std::thread::park();
            }
        }
        println!("owned engine and service preparation logs");
        println!(
            "{}",
            serde_json::json!({"backend_started":true,"phone_ready":false,
                "complete_device_bundle":false,"windows_boot_verified":false})
        );
        std::process::exit(if mode == "fail" { 7 } else { 0 });
    }

    fn command(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "wsl::initialize::tests::fixture", "--nocapture"])
            .env("AI_PHONE_WSL_INITIALIZE_FIXTURE", mode);
        command
    }

    #[test]
    fn only_a_successful_exit_and_final_backend_receipt_acknowledge_initialization() {
        run(command("ok"), &request(), &|| false).unwrap();
        assert_eq!(
            run(command("fail"), &request(), &|| false)
                .unwrap_err()
                .code(),
            "wsl_command_failed"
        );
        for body in [b"no receipt".as_slice(), b"\xff"] {
            assert_eq!(
                acknowledge(body).unwrap_err().code(),
                "wsl_initialization_receipt"
            );
        }
        let receipt = serde_json::json!({"backend_started":true,"phone_ready":false,
                "complete_device_bundle":false,"windows_boot_verified":false});
        for (field, value) in [
            ("backend_started", false),
            ("phone_ready", true),
            ("complete_device_bundle", true),
            ("windows_boot_verified", true),
        ] {
            let mut changed = receipt.clone();
            changed[field] = value.into();
            assert_eq!(
                acknowledge(&serde_json::to_vec(&changed).unwrap())
                    .unwrap_err()
                    .code(),
                "wsl_initialization_receipt"
            );
        }
    }

    #[test]
    fn cancellation_after_launch_preserves_an_unknown_guest_outcome() {
        let folder = tempfile::tempdir().unwrap();
        let ready = folder.path().join("ready");
        let mut child = command("wait");
        child.env("AI_PHONE_WSL_INITIALIZE_READY", &ready);
        assert_eq!(
            run(child, &request(), &|| ready.is_file())
                .unwrap_err()
                .code(),
            "wsl_initialization_unknown"
        );
        assert!(matches!(
            run(command("ok"), &request(), &|| true),
            Err(Error::Cancelled)
        ));
    }
}
