//! Standalone process fixture; production component producers never package this binary.
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{Read, Write},
    path::PathBuf,
};
mod image_capsule;

fn main() {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if let [mode, home, architecture, nonce, codec] = arguments.as_slice()
        && mode == "--fixture-capsule"
    {
        let make = if codec.starts_with("layered-") {
            image_capsule::create_layered
        } else {
            image_capsule::create
        };
        let codec = codec.strip_prefix("layered-").unwrap_or(codec);
        let (_, manifest, _) = make(
            &PathBuf::from(home),
            codec != "gzip",
            architecture,
            nonce,
            codec == "oci",
        );
        println!("{manifest}");
        return;
    }
    let home = PathBuf::from(env::var_os("AI_PHONE_IMAGE_TEST_STATE").expect("fixture state"));
    if let Some(socket) = env::var_os("AI_PHONE_IMAGE_TEST_SOCKET") {
        assert_eq!(env::var_os("DOCKER_HOST"), Some(socket));
        assert!(env::var_os("DOCKER_CONTEXT").is_none());
        fs::write(home.join("socket-verified"), "yes").unwrap();
    }
    let manifest: Value =
        serde_json::from_slice(&fs::read(home.join("expected.json")).unwrap()).unwrap();
    let record = home.join("ready.json");
    let mode = fs::read_to_string(home.join("mode")).unwrap_or_default();
    let mut events = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(home.join("events"))
        .unwrap();
    let mut ready: Value = fs::read(&record)
        .map(|bytes| serde_json::from_slice(&bytes).unwrap())
        .unwrap_or(json!({}));
    match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["info", "--format", "{{.Architecture}}"] => {
            writeln!(events, "info").unwrap();
            fs::write(home.join("queried"), "yes").unwrap();
            if mode == "rename" {
                let replacement = home.join("replacement");
                fs::write(&replacement, "replacement").unwrap();
                fs::rename(replacement, home.join("images.payload")).unwrap();
            }
            if mode == "inplace" {
                fs::write(home.join("images.payload"), "changed").unwrap();
            }
            println!(
                "{}",
                match mode.as_str() {
                    "bad-info" => "unknown",
                    "wrong-arch" => "x86_64",
                    _ => manifest["architecture"].as_str().unwrap(),
                }
            );
        }
        ["image", "inspect", name, ..] => {
            writeln!(events, "inspect {name}").unwrap();
            if mode == "malformed-inspect" {
                println!("[]");
                return;
            }
            if ready.get(*name).is_none() {
                std::process::exit(1);
            }
            let mut value = ready[*name].clone();
            if mode.starts_with("index") {
                if arguments.iter().any(|argument| argument == "--platform") {
                    fs::write(home.join("platform-query"), "yes").unwrap();
                    if mode == "index-missing" {
                        std::process::exit(1);
                    }
                } else {
                    value["Id"] = json!(format!("sha256:{}", "f".repeat(64)));
                    value["Descriptor"] =
                        json!({"mediaType":"application/vnd.oci.image.index.v1+json"});
                }
            }
            println!("{}", json!([value]));
        }
        ["image", "load", "--quiet"] => {
            writeln!(events, "load").unwrap();
            if mode.starts_with("block") {
                #[cfg(unix)]
                if mode == "block-ignore-term" {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap()
                        .block_on(async {
                            let mut signal = tokio::signal::unix::signal(
                                tokio::signal::unix::SignalKind::terminate(),
                            )
                            .unwrap();
                            publish_pid(&home);
                            loop {
                                signal.recv().await;
                            }
                        });
                }
                publish_pid(&home);
                std::thread::sleep(std::time::Duration::from_secs(30));
            }
            if mode == "reject" {
                std::process::exit(3);
            }
            let mut body = Vec::new();
            std::io::stdin().read_to_end(&mut body).unwrap();
            fs::write(home.join("imported"), &body).unwrap();
            let raw = if body.starts_with(&[0x1f, 0x8b]) {
                let mut raw = Vec::new();
                flate2::read::GzDecoder::new(body.as_slice())
                    .read_to_end(&mut raw)
                    .unwrap();
                raw
            } else {
                body
            };
            let mut archive = tar::Archive::new(raw.as_slice());
            let mut objects = std::collections::BTreeMap::new();
            for entry in archive.entries().unwrap() {
                let mut entry = entry.unwrap();
                let name = entry.path().unwrap().to_str().unwrap().to_owned();
                let mut value = Vec::new();
                entry.read_to_end(&mut value).unwrap();
                objects.insert(name, value);
            }
            let index: Value = serde_json::from_slice(&objects["index.json"]).unwrap();
            for row in index["manifests"]
                .as_array()
                .unwrap()
                .iter()
                .take(if mode == "unknown-partial" { 2 } else { 4 })
            {
                let reference = row["annotations"]["io.containerd.image.name"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("docker.io/")
                    .unwrap();
                ready[reference] = json!({"Id":row["digest"],"Architecture":manifest["architecture"],"Os":"linux"});
            }
            fs::write(record, serde_json::to_vec(&ready).unwrap()).unwrap();
            if mode.starts_with("unknown") {
                std::process::exit(3);
            }
        }
        _ => panic!("unexpected fixture command: {arguments:?}"),
    }
}

fn publish_pid(home: &std::path::Path) {
    fs::write(
        home.join("load.pid.pending"),
        std::process::id().to_string(),
    )
    .unwrap();
    fs::rename(home.join("load.pid.pending"), home.join("load.pid")).unwrap();
}
