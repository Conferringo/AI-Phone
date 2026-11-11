//! The maintainer worker must never enter a normal host's service namespace.
#![cfg(unix)]
#[test]
fn rejects_a_normal_host_before_starting_the_fixture() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_release-tools"))
        .arg("wsl-qualify")
        .env_clear()
        .env("container", "docker")
        .env("AI_PHONE_WSL_QUALIFICATION_NONCE", "a".repeat(32))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "qualification_environment"
    );
}
