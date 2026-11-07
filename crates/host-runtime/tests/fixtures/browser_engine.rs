//! An isolated Docker CLI peer; never contacts an engine or network.
use serde_json::{Value, json};
use std::{env, fs, io, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let state = PathBuf::from(env::var_os("BROWSER_TEST_STATE").ok_or("state missing")?);
    let mut events: Vec<Value> = if state.with_extension("events").exists() {
        serde_json::from_slice(&fs::read(state.with_extension("events"))?)?
    } else {
        vec![]
    };
    events.push(json!(args));
    fs::write(state.with_extension("events"), serde_json::to_vec(&events)?)?;
    if env::var("BROWSER_TEST_FAIL").ok().as_ref() == args.first() {
        return Err("engine unavailable".into());
    }
    if env::var("BROWSER_TEST_OUTPUT_FOR").ok().as_ref() == args.first() {
        if let Ok(size) = env::var("BROWSER_TEST_OUTPUT_BYTES") {
            println!("{}", "x".repeat(size.parse()?));
        } else {
            println!("{}", env::var("BROWSER_TEST_OUTPUT")?);
        }
        return Ok(());
    }
    let field = |name: &str| -> Result<String, Box<dyn std::error::Error>> {
        Ok(args
            .get(
                args.iter()
                    .position(|a| a == name)
                    .ok_or("argument missing")?
                    + 1,
            )
            .ok_or("value missing")?
            .clone())
    };
    match args.first().map(String::as_str) {
        Some("run") if field("--entrypoint")? == "/usr/bin/chromium" => {
            println!("Chromium 156.0.8078.47 fixture")
        }
        Some("run") => println!("1000:1000"),
        Some("container") => {
            if state.exists() {
                let row: Value = serde_json::from_slice(&fs::read(&state)?)?;
                println!("{}", row["Id"].as_str().ok_or("id missing")?);
            }
        }
        Some("inspect") => println!("[{}]", fs::read_to_string(&state)?),
        Some("create") => {
            let mut labels = serde_json::Map::new();
            for pair in args.windows(2).filter(|pair| pair[0] == "--label") {
                let (key, value) = pair[1].split_once('=').ok_or("invalid label")?;
                labels.insert(key.into(), json!(value));
            }
            let image = args
                .iter()
                .find(|a| a.starts_with("sha256:"))
                .ok_or("image missing")?;
            let row = json!({"Id":"b".repeat(64),"Name":format!("/{}",field("--name")?),"Image":image,"Config":{"Labels":labels},"State":{"Running":true}});
            fs::write(&state, serde_json::to_vec(&row)?)?;
            if env::var_os("BROWSER_TEST_UNKNOWN_CREATE").is_some() {
                return Err("create result unknown".into());
            }
            println!("{}", "b".repeat(64));
        }
        Some("start") => {
            io::copy(&mut io::stdin().lock(), &mut io::stdout().lock())?;
        }
        Some("stop") => {
            let mut row: Value = serde_json::from_slice(&fs::read(&state)?)?;
            row["State"]["Running"] = json!(false);
            fs::write(&state, serde_json::to_vec(&row)?)?;
        }
        Some("rm") => {
            fs::remove_file(&state)?;
        }
        _ => return Err("unexpected browser command".into()),
    }
    Ok(())
}
