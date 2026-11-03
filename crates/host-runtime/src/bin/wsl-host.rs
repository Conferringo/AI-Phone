//! Native registration stage for the complete Windows installer.
use ai_phone_host_runtime::{Error, Installation, wsl};
use std::{env, path::Path};

fn execute(arguments: &[std::ffi::OsString]) -> ai_phone_host_runtime::Result<wsl::Registered> {
    let [operation, rootfs, home, identity, expanded] = arguments else {
        return Err(Error::Invalid("wsl_arguments"));
    };
    if operation != "import" {
        return Err(Error::Invalid("wsl_arguments"));
    }
    let sha256 = identity
        .to_str()
        .ok_or(Error::Invalid("wsl_rootfs_identity"))?;
    let expanded_bytes = expanded
        .to_str()
        .and_then(|s| s.parse().ok())
        .ok_or(Error::Invalid("wsl_rootfs_identity"))?;
    let mut backend = wsl::Native::new(&|| false);
    Installation::lock(Path::new(home))?.ensure_wsl(
        wsl::Rootfs {
            path: Path::new(rootfs),
            sha256,
            expanded_bytes,
        },
        &mut backend,
        &|p| fs2::available_space(p).unwrap_or(0),
        &|| false,
    )
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
