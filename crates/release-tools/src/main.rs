mod android_package;
mod capsule;
#[cfg(unix)]
mod wsl_qualification;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn execute(arguments: Vec<std::ffi::OsString>) -> Result<Option<serde_json::Value>> {
    if arguments
        .first()
        .is_some_and(|value| value == "decode-capsule")
    {
        use std::io::{self, Read};
        let [_, codec] = arguments.as_slice() else {
            return Err("decode-capsule requires gzip or xz".into());
        };
        let input = io::stdin().lock();
        let mut decoded: Box<dyn Read> = match codec.to_str() {
            Some("gzip") => Box::new(flate2::read::GzDecoder::new(input)),
            Some("xz") => Box::new(xz2::read::XzDecoder::new(input)),
            _ => return Err("Unsupported capsule codec".into()),
        };
        io::copy(&mut decoded, &mut io::stdout().lock())?;
        return Ok(None);
    }
    if arguments
        .first()
        .is_some_and(|value| value == "windows-sdk-files")
    {
        if arguments.len() != 1 {
            return Err("windows-sdk-files takes the file list through stdin".into());
        }
        let files = serde_json::from_reader(std::io::stdin().lock())?;
        return Ok(Some(serde_json::to_value(android_package::select(files))?));
    }
    if arguments.first().is_some_and(|value| value == "encode-xz") {
        if arguments.len() != 4 {
            return Err("encode-xz requires architecture, preset and output directory".into());
        }
        capsule::encode(
            &mut std::io::stdin().lock(),
            std::io::stdout().lock(),
            arguments[1].to_str().ok_or("Invalid image architecture")?,
            arguments[2].to_str().ok_or("Invalid XZ preset")?.parse()?,
            std::path::Path::new(&arguments[3]),
        )?;
        return Ok(None);
    }
    #[cfg(unix)]
    if arguments
        .first()
        .is_some_and(|value| value == "wsl-qualify")
    {
        if arguments.len() != 1 {
            return Err("qualification_arguments".into());
        }
        return tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(wsl_qualification::run())
            .map(Some);
    }
    Err("Unknown release tool command".into())
}

fn main() {
    match execute(std::env::args_os().skip(1).collect()) {
        Ok(Some(value)) => println!("{value}"),
        Ok(None) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
