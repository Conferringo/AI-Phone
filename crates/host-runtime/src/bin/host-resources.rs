use ai_phone_host_runtime::Installation;
use std::{env, path::Path};

fn main() {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    if arguments.len() == 1 && arguments[0] == "--service-supervision-contract" {
        println!("{}", ai_phone_host_runtime::supervision::CONTRACT);
        return;
    }
    #[cfg(target_os = "linux")]
    if arguments
        .first()
        .is_some_and(|value| value == "--supervise")
    {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(ai_phone_host_runtime::Error::from)
            .and_then(|runtime| {
                runtime.block_on(ai_phone_host_runtime::supervision::run(&arguments[1..]))
            });
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
