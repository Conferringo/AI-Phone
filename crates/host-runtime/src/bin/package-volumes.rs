use ai_phone_host_runtime::{Error, Result, package};
use std::{env, ffi::OsString, path::Path};

fn text(value: &OsString) -> Result<&str> {
    value.to_str().ok_or(Error::Invalid("package_argument"))
}

fn split(input: &OsString, folder: &OsString, sha256: &OsString) -> Result<serde_json::Value> {
    let sealed = package::split(
        Path::new(input),
        Path::new(folder),
        package::MAX_PART_SIZE,
        text(sha256)?,
        &|path| fs2::available_space(path).unwrap_or(0),
        &|| false,
    )?;
    Ok(
        serde_json::json!({"format":1,"manifest":sealed.path.file_name().and_then(|name|name.to_str()),"manifest_sha256":sealed.sha256,"size":sealed.manifest.size,
        "sha256":sealed.manifest.sha256,"volumes":sealed.manifest.parts.len()}),
    )
}

fn restore(manifest: &OsString, folder: &OsString, sha256: &OsString) -> Result<serde_json::Value> {
    let output = package::restore(
        Path::new(manifest),
        text(sha256)?,
        Path::new(folder),
        &|path| fs2::available_space(path).unwrap_or(0),
        &|| false,
    )?;
    Ok(serde_json::json!({"restored":output.file_name().and_then(|name|name.to_str())}))
}

fn execute(arguments: &[OsString]) -> Result<serde_json::Value> {
    match arguments {
        [operation, input, folder, sha256] if operation == "split" => split(input, folder, sha256),
        [operation, manifest, folder, sha256] if operation == "restore" => {
            restore(manifest, folder, sha256)
        }
        [operation, manifest, sha256] if operation == "verify" => {
            let value = package::verify(Path::new(manifest), text(sha256)?, &|| false)?;
            Ok(
                serde_json::json!({"verified":true,"sha256":value.sha256,"size":value.size,"volumes":value.parts.len()}),
            )
        }
        _ => Err(Error::Invalid("package_arguments")),
    }
}

fn main() {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    match execute(&arguments) {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{}", error.code());
            std::process::exit(1);
        }
    }
}
