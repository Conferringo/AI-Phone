use std::process::Command;

#[test]
fn source_supervisor_qualifies_without_installation_or_model_state() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .arg("--service-supervision-contract")
        .current_dir(directory.path())
        .env_clear()
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        format!("{}\n", ai_phone_host_runtime::supervision::CONTRACT).as_bytes()
    );
    assert!(output.stderr.is_empty());
    assert_eq!(directory.path().read_dir().unwrap().count(), 0);
}
