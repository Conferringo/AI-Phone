use std::{
    io::Write,
    process::{Command, Stdio},
};

fn invoke(arguments: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn reports_the_bridge_contract_without_installation_state() {
    let output = invoke(&["--device-bridge-contract"], b"");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"ai-phone-device-bridge-v1\n");
}
#[test]
fn rejects_unrecognized_arguments_and_malformed_input_before_readiness() {
    for (arguments, input) in [
        (vec!["--device-bridge-contract", "extra"], vec![]),
        (vec!["--device-bridge", "extra"], vec![]),
        (vec!["--device-bridge"], vec![0, 0, 0, 1, 255]),
        (vec!["--device-bridge"], vec![0, 0, 2, 1]),
    ] {
        let output = invoke(&arguments, &input);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn native_android_component_has_a_separate_protobuf_contract_and_no_false_receipt() {
    let output = invoke(&["--prepare-android-contract"], b"");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"ai-phone-native-android-v1\n");
    let folder = tempfile::tempdir().unwrap();
    let home = folder.path().join("installation");
    for input in [vec![], vec![255], vec![0; 8193]] {
        let output = invoke(&["--prepare-android", home.to_str().unwrap()], &input);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!home.exists());
    }
    assert!(!invoke(&["--prepare-android"], b"").status.success());
}
