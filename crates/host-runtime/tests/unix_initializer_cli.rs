#![cfg(unix)]
use serde_json::json;
use std::{fs, path::Path, process::Command};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_host-resources"))
}
fn request(root: &Path) {
    fs::write(root.join("request.json"),serde_json::to_vec(&json!({"locale":"en","phone":{"cpus":2,"memory_mb":2048,"storage_gb":16},"desktop":{"cpus":2,"memory_mb":2048}})).unwrap()).unwrap();
}

#[test]
fn unix_initializer_contract_is_native_and_does_not_open_a_host_home() {
    let output = command()
        .arg("--unix-host-initializer-contract")
        .env_clear()
        .env("PATH", "/no-tools")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"ai-phone-unix-host-initializer-v1\n");
}

#[test]
fn missing_or_invalid_arguments_cannot_create_installation_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    request(root);
    let home = root.join("home");
    for args in [
        vec![],
        vec![root.to_str().unwrap()],
        vec![
            root.to_str().unwrap(),
            home.to_str().unwrap(),
            root.join("request.json").to_str().unwrap(),
            "not-a-digest",
            "also-invalid",
        ],
    ] {
        let output = command()
            .arg("--initialize-unix-host")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unix_host_arguments"));
        assert!(!home.exists());
    }
}

#[test]
fn missing_bundled_resources_fail_before_touching_saved_devices_or_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    request(root);
    let home = root.join("home");
    fs::create_dir(&home).unwrap();
    fs::write(home.join("keep"), b"existing device and account fixture").unwrap();
    let resource = root.join("resources");
    fs::create_dir(&resource).unwrap();
    let id = "0".repeat(64);
    let output = command()
        .arg("--initialize-unix-host")
        .arg(resource)
        .arg(&home)
        .arg(root.join("request.json"))
        .args([&id, &id])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("ready"));
    assert_eq!(
        fs::read(home.join("keep")).unwrap(),
        b"existing device and account fixture"
    );
    assert_eq!(fs::read_dir(home).unwrap().count(), 1);
}
