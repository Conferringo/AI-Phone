//! Binary-level error framing: raw media stdout must never carry JSON diagnostics.
use std::{
    io::Write,
    process::{Command, Stdio},
};

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
