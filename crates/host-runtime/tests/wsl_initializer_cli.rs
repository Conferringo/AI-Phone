use std::{fs, process::Command};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::{io::Write, process::Stdio};

#[test]
fn initializer_contract_runs_with_empty_path_without_reading_an_installation() {
    let folder = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .arg("--wsl-backend-initializer-contract")
        .env_clear()
        .env("PATH", folder.path())
        .current_dir(folder.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"ai-phone-wsl-initializer-v1\n");
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn invalid_initializer_arguments_never_touch_a_home_or_start_a_service() {
    let folder = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .args(["--initialize-wsl-backend", "unexpected"])
        .env_clear()
        .env("PATH", folder.path())
        .current_dir(folder.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.stderr, b"wsl_initializer_arguments\n");
    assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn malformed_stdin_is_rejected_before_fixed_wsl_paths_are_used() {
    let folder = tempfile::tempdir().unwrap();
    for request in [b"[]".to_vec(), vec![b' '; 8193]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_host-resources"))
            .arg("--initialize-wsl-backend")
            .env_clear()
            .env("PATH", folder.path())
            .current_dir(folder.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&request).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert_eq!(output.stderr, b"wsl_initialization_request\n");
        assert!(output.stdout.is_empty());
    }
    assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[cfg(all(unix, not(all(target_os = "linux", target_arch = "x86_64"))))]
#[test]
fn unsupported_hosts_reject_before_touching_fixed_wsl_paths() {
    let folder = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .arg("--initialize-wsl-backend")
        .env_clear()
        .env("PATH", folder.path())
        .current_dir(folder.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.stderr, b"wsl_initializer_host\n");
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 0);
}
