//! Native registration and backend initialization for the Windows installer.
use ai_phone_host_runtime::{Error, Installation, wsl};
use std::{env, path::Path};

fn execute(arguments: &[std::ffi::OsString]) -> ai_phone_host_runtime::Result<wsl::Registered> {
    let [operation, source, home, identity, configuration] = arguments else {
        return Err(Error::Invalid("wsl_arguments"));
    };
    if operation != "import" && operation != "start-bundle" {
        return Err(Error::Invalid("wsl_arguments"));
    }
    let sha256 = identity
        .to_str()
        .ok_or(Error::Invalid("wsl_rootfs_identity"))?;
    let mut backend = wsl::Native::new(&|| false);
    if operation == "start-bundle" {
        let request = wsl::InitializeRequest::read(Path::new(configuration))?;
        Installation::lock(Path::new(home))?.start_wsl_bundle(
            Path::new(source),
            sha256,
            &request,
            &mut backend,
            &|p| fs2::available_space(p).unwrap_or(0),
            &|| false,
        )
    } else {
        let expanded_bytes = configuration
            .to_str()
            .and_then(|s| s.parse().ok())
            .ok_or(Error::Invalid("wsl_rootfs_identity"))?;
        Installation::lock(Path::new(home))?.ensure_wsl(
            wsl::Rootfs {
                path: Path::new(source),
                sha256,
                expanded_bytes,
            },
            &mut backend,
            &|p| fs2::available_space(p).unwrap_or(0),
            &|| false,
        )
    }
}

fn main() {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    match execute(&arguments) {
        Ok(value) => println!(
            "{}",
            serde_json::to_string(&value).expect("registration result")
        ),
        Err(error) => {
            eprintln!("{}", error.code());
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_initialization_input_cannot_create_an_installation() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("request.json");
        std::fs::write(&input, b"partial configuration").unwrap();
        let home = root.path().join("preserve missing home");
        let arguments = [
            "start-bundle".into(),
            root.path().join("missing bundle").into_os_string(),
            home.clone().into_os_string(),
            "0".repeat(64).into(),
            input.into_os_string(),
        ];
        assert_eq!(
            execute(&arguments).err().unwrap().code(),
            "wsl_initialization_request"
        );
        assert!(!home.exists());
    }

    #[test]
    fn unknown_operation_is_rejected_before_creating_state() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("missing home");
        let arguments = [
            "unsupported".into(),
            "rootfs".into(),
            home.clone().into_os_string(),
            "identity".into(),
            "0".into(),
        ];
        assert_eq!(execute(&arguments).err().unwrap().code(), "wsl_arguments");
        assert!(!home.exists());
    }
}
