#![cfg(unix)]

use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn call(root: &Path, user: &Path, operation: &str, arguments: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .arg("--managed-engine")
        .arg(root)
        .arg(user)
        .arg(operation)
        .args(arguments)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap()
}
fn value(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn layout(base: &Path) -> (PathBuf, PathBuf) {
    let home = base.join("installation");
    fs::create_dir_all(home.join("user/data")).unwrap();
    let home = home.canonicalize().unwrap();
    let root = home.join("releases/a");
    fs::create_dir_all(&root).unwrap();
    symlink(home.join("user/data"), root.join("data")).unwrap();
    (home, root)
}
fn temporary() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("ae-")
        .tempdir_in("/tmp")
        .unwrap()
}
fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn save(base: &Path, body: &str) -> (PathBuf, PathBuf, PathBuf) {
    let (home, root) = layout(base);
    let state = PathBuf::from(
        value(call(&root, base, "state", &[&home]))
            .as_str()
            .unwrap(),
    );
    value(call(&root, base, "claim", &[&state]));
    let bin = home.join("bin");
    fs::create_dir(&bin).unwrap();
    executable(&bin.join("docker"), "#!/bin/sh\nexit 0\n");
    let command = home.join("limactl");
    executable(&command, body);
    value(call(&root, base, "save", &[&home, &state, &command, &bin]));
    (home, root, state)
}

#[test]
fn absent_selection_is_read_only_and_contract_rejects_extra_arguments() {
    let base = temporary();
    let root = base.path().join("absent");
    assert_eq!(
        value(call(&root, base.path(), "environment", &[])),
        json!({})
    );
    assert_eq!(
        value(call(&root, base.path(), "selection", &[])),
        Value::Null
    );
    assert_eq!(value(call(&root, base.path(), "start", &[])), json!(false));
    assert!(!root.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .arg("--managed-engine-contract")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"ai-phone-managed-engine-v1\n");
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_host-resources"))
            .args(["--managed-engine-contract", "extra"])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn restart_restores_owned_environment_and_runs_only_selected_program() {
    let base = temporary();
    let (home, root, state) = save(
        base.path(),
        "#!/bin/sh\nprintf '%s\\n' \"$LIMA_HOME\" \"$@\" > \"$LIMA_HOME/started\"\nprintf 'engine output\\n'\n",
    );
    assert_eq!(
        value(call(&root, base.path(), "environment", &[])),
        json!({"AI_PHONE_MANAGED_BIN":home.join("bin"), "LIMA_HOME":state})
    );
    assert_eq!(value(call(&root, base.path(), "start", &[])), json!(true));
    assert_eq!(
        fs::read_to_string(state.join("started")).unwrap(),
        format!("{}\nstart\n--tty=false\nai-phone\n", state.display())
    );
    let other = home.join("releases/b");
    fs::create_dir(&other).unwrap();
    symlink(home.join("user/data"), other.join("data")).unwrap();
    assert_eq!(
        value(call(&other, base.path(), "state", &[&home])),
        json!(state)
    );
    assert_eq!(
        value(call(&other, base.path(), "environment", &[])),
        value(call(&root, base.path(), "environment", &[]))
    );
    assert_eq!(
        fs::metadata(&state).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(root.join("data/deployment/container-host.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    value(call(&other, base.path(), "claim", &[&state]));
    assert!(state.join("started").exists());
}

#[test]
fn unknown_or_replaced_state_is_preserved_and_not_adopted() {
    let base = temporary();
    let (home, root) = layout(base.path());
    let unknown = base.path().join("unowned");
    fs::create_dir(&unknown).unwrap();
    fs::write(unknown.join("keep"), "device disk").unwrap();
    assert!(
        !call(&root, base.path(), "claim", &[&unknown])
            .status
            .success()
    );
    assert_eq!(fs::read(unknown.join("keep")).unwrap(), b"device disk");
    let alias = base.path().join("alias");
    symlink(&unknown, &alias).unwrap();
    assert!(
        !call(&root, base.path(), "claim", &[&alias])
            .status
            .success()
    );
    let state = PathBuf::from(
        value(call(&root, base.path(), "state", &[&home]))
            .as_str()
            .unwrap(),
    );
    value(call(&root, base.path(), "claim", &[&state]));
    fs::write(
        state.join(".aiphone-owner.json"),
        br#"{"format":1,"data_root":"another"}"#,
    )
    .unwrap();
    assert!(
        !call(&root, base.path(), "claim", &[&state])
            .status
            .success()
    );
}

#[test]
fn changed_selection_or_executable_is_rejected_without_starting_or_repairing() {
    let base = temporary();
    let (home, root, state) = save(base.path(), "#!/bin/sh\nexit 0\n");
    let record = root.join("data/deployment/container-host.json");
    let bytes = fs::read(&record).unwrap();
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    for (field, changed) in [
        ("format", json!(true)),
        ("format", json!(2)),
        ("home", json!("different")),
        ("data_root", json!("different")),
        ("state", json!("different")),
        ("command", Value::Null),
        ("command", json!(123)),
        ("command_sha256", Value::Null),
        ("command_sha256", json!("invalid")),
        ("wrapper_sha256", Value::Null),
        ("wrapper_sha256", json!("0".repeat(64))),
        ("extra", json!("unexpected")),
    ] {
        let mut selected = original.clone();
        selected[field] = changed;
        fs::write(&record, serde_json::to_vec(&selected).unwrap()).unwrap();
        assert!(
            !call(&root, base.path(), "environment", &[])
                .status
                .success(),
            "{field}"
        );
    }
    fs::write(&record, &bytes).unwrap();
    fs::set_permissions(home.join("limactl"), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        !call(&root, base.path(), "environment", &[])
            .status
            .success()
    );
    fs::set_permissions(home.join("limactl"), fs::Permissions::from_mode(0o755)).unwrap();
    executable(&home.join("bin/docker"), "#!/bin/sh\nexit 9\n");
    assert!(!call(&root, base.path(), "start", &[]).status.success());
    assert_eq!(fs::read(&record).unwrap(), bytes);
    assert!(!state.join("started").exists());
}

#[test]
fn legacy_state_stays_in_place_and_invalid_layout_or_ownership_is_rejected() {
    let base = temporary();
    let (home, root) = layout(base.path());
    let legacy = home.join("lima");
    fs::create_dir(&legacy).unwrap();
    fs::set_permissions(&legacy, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        value(call(&root, base.path(), "state", &[&home])),
        json!(legacy)
    );
    value(call(&root, base.path(), "claim", &[&legacy]));
    fs::write(legacy.join("keep"), "original disk").unwrap();
    value(call(&root, base.path(), "claim", &[&legacy]));
    assert_eq!(fs::read(legacy.join("keep")).unwrap(), b"original disk");
    fs::set_permissions(&legacy, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        !call(&root, base.path(), "claim", &[&legacy])
            .status
            .success()
    );
    assert!(
        !call(&root, base.path(), "state", &[base.path()])
            .status
            .success()
    );
    let invalid = base.path().join("invalid");
    fs::create_dir_all(invalid.join("data")).unwrap();
    assert!(
        !call(&invalid, base.path(), "state", &[base.path()])
            .status
            .success()
    );
    let long_user = base.path().join("long".repeat(30));
    fs::create_dir(&long_user).unwrap();
    fs::remove_file(legacy.join("keep")).unwrap();
    fs::remove_file(legacy.join(".aiphone-owner.json")).unwrap();
    fs::remove_dir(&legacy).unwrap();
    assert_eq!(
        call(&root, &long_user, "state", &[&home]).stderr,
        b"managed_engine_socket_path_too_long\n"
    );
}

#[test]
fn selection_boundaries_missing_state_and_failed_start_are_reported() {
    let base = temporary();
    let (home, root, state) = save(base.path(), "#!/bin/sh\nexit 9\n");
    assert!(!call(&root, base.path(), "start", &[]).status.success());
    let record = root.join("data/deployment/container-host.json");
    let bytes = fs::read(&record).unwrap();
    let mut padded = bytes.clone();
    padded.resize(16384, b' ');
    fs::write(&record, &padded).unwrap();
    value(call(&root, base.path(), "environment", &[]));
    padded.push(b' ');
    fs::write(&record, padded).unwrap();
    assert!(
        !call(&root, base.path(), "environment", &[])
            .status
            .success()
    );
    fs::write(&record, &bytes).unwrap();
    let moved = home.join("record-copy");
    fs::rename(&record, &moved).unwrap();
    symlink(&moved, &record).unwrap();
    assert!(
        !call(&root, base.path(), "environment", &[])
            .status
            .success()
    );
    fs::remove_file(&record).unwrap();
    fs::rename(&moved, &record).unwrap();
    fs::remove_dir_all(&state).unwrap();
    assert!(
        !call(&root, base.path(), "environment", &[])
            .status
            .success()
    );
    assert!(!state.exists());
}
