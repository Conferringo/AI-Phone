#[cfg(target_os = "linux")]
mod apt;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn execute() -> Result<serde_json::Value> {
    #[cfg(target_os = "linux")]
    return apt::run(std::env::args_os().skip(1));
    #[cfg(not(target_os = "linux"))]
    Err("APT release tooling requires Linux".into())
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
