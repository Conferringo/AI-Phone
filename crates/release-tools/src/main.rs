#[cfg(target_os = "linux")]
mod apt;
#[cfg(unix)]
mod wsl_qualification;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn execute() -> Result<serde_json::Value> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
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
            .block_on(wsl_qualification::run());
    }
    #[cfg(target_os = "linux")]
    return apt::run(arguments.into_iter());
    #[cfg(not(target_os = "linux"))]
    {
        let _ = arguments;
        Err("APT release tooling requires Linux".into())
    }
}

fn main() {
    match execute() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
