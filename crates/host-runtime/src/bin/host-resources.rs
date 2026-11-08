use std::env;

fn main() {
    let mut arguments: Vec<_> = env::args_os().skip(1).collect();
    if arguments.len() == 4 && arguments[0] == "stage" {
        arguments[0] = "--stage-source".into();
    }
    if let Some(result) = ai_phone_host_runtime::component::run(&arguments) {
        if let Err(error) = result {
            eprintln!("{}", error.code());
            std::process::exit(1);
        }
        return;
    }
    eprintln!("Usage: host-resources stage <source.payload> <installation> <sha256>");
    std::process::exit(2);
}
