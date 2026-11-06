//! Explicit maintainer check against an existing Docker engine, using uniquely labelled fixture images.
#[path = "../tests/fixtures/image_capsule.rs"]
mod capsule;
use serde_json::{Value, json};
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn output(command: &mut Command) -> Result<Output> {
    let value = command.output()?;
    if !value.status.success() {
        return Err(format!("command failed: {}", String::from_utf8_lossy(&value.stderr)).into());
    }
    Ok(value)
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [native, root] = args.as_slice() else {
        return Err(
            "Usage: qualify_image_import <qualified native program> <new evidence directory>"
                .into(),
        );
    };
    let native = PathBuf::from(native);
    let root = PathBuf::from(root);
    fs::create_dir(&root)?;
    let contract = output(Command::new(&native).arg("--image-activation-contract"))?;
    if contract.stdout != b"ai-phone-image-activation-v1\n" {
        return Err("unexpected native image contract".into());
    }
    let engine = output(Command::new("docker").args(["info", "--format", "{{.Architecture}}"]))?;
    let architecture = match std::str::from_utf8(&engine.stdout)?.trim() {
        "arm64" | "aarch64" => "arm64",
        "amd64" | "x86_64" => "amd64",
        _ => return Err("unsupported engine architecture".into()),
    };
    let nonce = format!(
        "aiphone-image-qa-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let mut owned = Vec::new();
    let mut receipts = Vec::new();
    let validation = (|| -> Result<()> {
        for codec in ["gzip", "xz", "oci"] {
            let folder = root.join(codec);
            fs::create_dir(&folder)?;
            let owner = format!("{nonce}-{codec}");
            let make = if codec == "oci" {
                capsule::create
            } else {
                capsule::create_layered
            };
            let (payload, manifest, _) = make(
                &folder,
                codec != "gzip",
                architecture,
                &owner,
                codec == "oci",
            );
            let expected = if codec == "oci" {
                manifest.clone()
            } else {
                let expected = root.join(format!("{codec}-expected-import"));
                fs::create_dir(&expected)?;
                capsule::create_layered(&expected, true, architecture, &owner, true).1
            };
            for image in expected["images"]
                .as_object()
                .ok_or("image fixture")?
                .values()
            {
                owned.push((image.clone(), owner.clone()));
            }
            fs::create_dir(folder.join("data"))?;
            fs::write(folder.join("data/retained"), b"private user state fixture")?;
            let first = output(
                Command::new(&native)
                    .arg("--activate-images")
                    .arg(&folder)
                    .arg(&payload),
            )?;
            let environment: Value = serde_json::from_slice(&first.stdout)?;
            if environment["AI_PHONE_IMAGE_PULL_POLICY"] != "never" {
                return Err("unexpected pull policy".into());
            }
            let record = folder.join("data/deployment/image-runtime.json");
            let selected: Value = serde_json::from_slice(&fs::read(&record)?)?;
            if selected["images"] != expected["images"]
                || selected["archive"] != manifest["archive"]
            {
                return Err("published manifest changed".into());
            }
            output(
                Command::new(&native)
                    .arg("--activate-images")
                    .arg(&folder)
                    .arg(&payload),
            )?;
            if fs::read(folder.join("data/retained"))? != b"private user state fixture" {
                return Err("private state changed".into());
            }
            receipts.push(json!({"codec":codec,"architecture":architecture,"import":true,"resume":true,"private_state_preserved":true}));
        }
        Ok(())
    })();
    let mut cleanup_errors = Vec::new();
    for (image, owner) in &owned {
        let reference = image["ref"].as_str().ok_or("fixture reference")?;
        let result = (|| -> Result<()> {
            let inspect = Command::new("docker")
                .args(["image", "inspect", reference])
                .output()?;
            if !inspect.status.success() {
                return Err("could not verify the owned image for cleanup".into());
            }
            let rows: Value = serde_json::from_slice(&inspect.stdout)?;
            let row = rows
                .as_array()
                .filter(|rows| rows.len() == 1)
                .map(|rows| &rows[0])
                .ok_or("image response")?;
            if row["Config"]["Labels"]["dev.aiphone.qa"] != *owner
                || (row["Id"] != image["config_digest"] && row["Id"] != image["export_digest"])
            {
                return Err("refusing cleanup of a different image owner".into());
            }
            output(Command::new("docker").args(["image", "rm", "--no-prune", reference]))?;
            Ok(())
        })();
        if let Err(error) = result {
            cleanup_errors.push(format!("{reference}: {error}"));
        }
    }
    let receipt = json!({"checks":receipts,"owned_references":owned.len(),"cleanup_success":cleanup_errors.is_empty(),"cleanup_errors":cleanup_errors,"validation_error":validation.as_ref().err().map(ToString::to_string)});
    fs::write(
        root.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("{receipt}");
    validation?;
    if !cleanup_errors.is_empty() {
        return Err("owned image cleanup incomplete; see receipt".into());
    }
    Ok(())
}
