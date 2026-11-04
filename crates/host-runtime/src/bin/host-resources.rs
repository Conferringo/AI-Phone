use ai_phone_host_runtime::Installation;
use std::{env, path::Path};

fn main() {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    if let Some(result) = ai_phone_host_runtime::component::run(&arguments) {
        if let Err(error) = result {
            eprintln!("{}", error.code());
            std::process::exit(1);
        }
        return;
    }
    if arguments.len() != 4 || arguments[0] != "stage" {
        eprintln!("Usage: host-resources stage <source.payload> <installation> <sha256>");
        std::process::exit(2);
    }
    let Some(identity) = arguments[3].to_str() else {
        eprintln!("source_resource_identity");
        std::process::exit(2);
    };
    let result = Installation::lock(Path::new(&arguments[2])).and_then(|mut installation| {
        installation.stage_source(
            Path::new(&arguments[1]),
            identity,
            &|path| fs2::available_space(path).unwrap_or(0),
            &|| false,
        )
    });
    match result {
        Ok(source) => println!("{} {}", source.identity, source.directory.display()),
        Err(error) => {
            eprintln!("{}", error.code());
            std::process::exit(1);
        }
    }
}
