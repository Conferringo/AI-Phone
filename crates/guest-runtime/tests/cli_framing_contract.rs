//! Binary-level error framing: raw media stdout must never carry JSON diagnostics.
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn version_probe_needs_no_desktop_input_or_state() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_desktop-bridge"))
        .arg("--version")
        .current_dir(root.path())
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "desktop-bridge 0.1.0\n"
    );
    assert!(output.stderr.is_empty());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn invalid_capture_mode_has_empty_raw_stdout_and_nonzero_exit() {
    let output = Command::new(env!("CARGO_BIN_EXE_desktop-bridge"))
        .args(["capture", "unknown-fixture-mode"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "JSON diagnostics corrupt PCM/MJPEG framing"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown capture mode"));
}

#[test]
fn ordinary_bridge_errors_keep_existing_json_stdout_contract() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_desktop-bridge"))
        .arg("unknown-fixture-operation")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["success"], false);
    assert!(error["error"].as_str().unwrap().contains("未知操作"));
    assert!(output.stderr.is_empty());
}

#[test]
fn audio_supervision_missing_runtime_is_stderr_only_and_creates_nothing() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("missing-runtime");
    let output = Command::new(env!("CARGO_BIN_EXE_desktop-bridge"))
        .arg("audio-supervise")
        .arg(root.path().join("fixture.pa"))
        .env("XDG_RUNTIME_DIR", &runtime)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert!(!runtime.exists());
}
