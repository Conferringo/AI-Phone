use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn request() -> Value {
    json!({"locale":"en","phone":{"cpus":2,"memory_mb":2048,"storage_gb":16},"desktop":{"cpus":2,"memory_mb":2560}})
}

fn invoke(root: &Path, value: &Value, path: &str, template: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_host-resources"));
    command
        .arg("--prepare-host-profile")
        .arg(root)
        .arg(root.join("SDK with spaces"))
        .arg(template)
        .env_clear()
        .env("PATH", path)
        .env("JAVA_HOME", root.join("java 工具"));
    send(&mut command, value)
}

fn send(command: &mut Command, value: &Value) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(value).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"{\"prepared\":true}\n");
}

#[test]
fn native_profile_creates_private_configuration_and_resumes_without_rewriting_user_state() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let path = "$HOME/literal:$(not-a-command):~/工具/bin";
    success(invoke(root, &request(), path, "android35-x86_64"));
    let profile_path = root.join("data/deployment/profile.json");
    let profile = fs::read(&profile_path).unwrap();
    let value: Value = serde_json::from_slice(&profile).unwrap();
    assert_eq!(value["launch_path"], path);
    assert_eq!(value["phone"]["template"], "android35-x86_64");
    assert_eq!(value["desktop"]["template"], "debian13-openbox");
    assert_eq!(
        value["sdk_root"],
        root.join("SDK with spaces").to_str().unwrap()
    );
    let environment = fs::read(root.join(".env")).unwrap();
    let text = String::from_utf8(environment.clone()).unwrap();
    let password = text
        .lines()
        .find_map(|line| line.strip_prefix("POSTGRES_PASSWORD="))
        .unwrap();
    assert_eq!(password.len(), 48);
    assert!(password.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let disk = root.join("data/userdata.img");
    fs::write(&disk, "original device disk").unwrap();
    let mut changed = request();
    changed["locale"] = json!("de");
    changed["phone"]["memory_mb"] = json!(4096);
    success(invoke(
        root,
        &changed,
        "/different/caller/path",
        "android35-x86_64",
    ));
    assert_eq!(fs::read(&profile_path).unwrap(), profile);
    assert_eq!(fs::read(root.join(".env")).unwrap(), environment);
    assert_eq!(fs::read(disk).unwrap(), b"original device disk");
    let conflict = invoke(root, &request(), path, "android35-arm64");
    assert!(!conflict.status.success());
    assert_eq!(fs::read(profile_path).unwrap(), profile);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join(".env"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(root.join("data/deployment/profile.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn malformed_deployment_requests_cannot_create_profile_or_database_credentials() {
    let root = tempfile::tempdir().unwrap();
    let mut bad = Vec::new();
    for (field, value) in [
        ("locale", json!("unknown")),
        ("phone", json!({})),
        ("desktop", json!([])),
    ] {
        let mut changed = request();
        changed[field] = value;
        bad.push(changed);
    }
    for value in [json!(true), json!(2.5), json!(3), json!(0)] {
        let mut changed = request();
        changed["phone"]["cpus"] = value;
        bad.push(changed);
    }
    let mut changed = request();
    changed["extra"] = json!(true);
    bad.push(changed);
    for value in bad {
        assert!(
            !invoke(root.path(), &value, "/literal", "android35-x86_64")
                .status
                .success()
        );
        assert!(!root.path().join("data").exists());
        assert!(!root.path().join(".env").exists());
    }
}

#[test]
fn profile_capability_is_read_only_and_rejects_mixed_arguments() {
    let root = tempfile::tempdir().unwrap();
    for arguments in [
        vec!["--host-profile-contract"],
        vec!["--host-profile-contract", "--prepare-host-profile"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
            .args(&arguments)
            .env_clear()
            .env("PATH", root.path())
            .current_dir(root.path())
            .output()
            .unwrap();
        if arguments.len() == 1 {
            assert!(output.status.success());
            assert_eq!(output.stdout, b"ai-phone-host-profile-v1\n");
        } else {
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
        }
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[cfg(unix)]
#[test]
fn persistent_links_and_existing_credentials_keep_the_same_user_data_owner() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("releases/source");
    let user = root.path().join("user");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(user.join("data")).unwrap();
    symlink("../../user/.env", source.join(".env")).unwrap();
    symlink("../../user/data", source.join("data")).unwrap();
    success(invoke(&source, &request(), "/literal", "android35-arm64"));
    assert!(source.join(".env").is_symlink());
    assert!(source.join("data").is_symlink());
    assert!(user.join("data/deployment/profile.json").is_file());
    fs::write(user.join(".env"), b"KEEP_PRIVATE_DATABASE_VALUE\n").unwrap();
    let second = root.path().join("releases/second");
    fs::create_dir(&second).unwrap();
    symlink("../../user/.env", second.join(".env")).unwrap();
    symlink("../../user/data", second.join("data")).unwrap();
    let profile = fs::read(user.join("data/deployment/profile.json")).unwrap();
    success(invoke(
        &second,
        &request(),
        "/new/caller",
        "android35-arm64",
    ));
    assert_eq!(
        fs::read(user.join("data/deployment/profile.json")).unwrap(),
        profile
    );
    assert_eq!(
        fs::read(user.join(".env")).unwrap(),
        b"KEEP_PRIVATE_DATABASE_VALUE\n"
    );
}

#[cfg(unix)]
#[test]
fn failed_credential_publication_leaves_no_half_configured_profile() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    symlink("missing-user/.env", root.path().join(".env")).unwrap();
    let output = invoke(root.path(), &request(), "/literal", "android35-arm64");
    assert!(!output.status.success());
    assert!(!root.path().join("data").exists());
    fs::create_dir(root.path().join("missing-user")).unwrap();
    success(invoke(
        root.path(),
        &request(),
        "/literal",
        "android35-arm64",
    ));
    assert!(root.path().join("missing-user/.env").is_file());
    assert!(root.path().join("data/deployment/profile.json").is_file());
}

#[test]
fn legacy_profiles_are_validated_without_rewriting_fields_or_private_values() {
    let root = tempfile::tempdir().unwrap();
    success(invoke(
        root.path(),
        &request(),
        "/literal",
        "android35-arm64",
    ));
    let path = root.path().join("data/deployment/profile.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("launch_path");
    value["emulator_proxy"] = json!("legacy-value");
    let legacy = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(&path, &legacy).unwrap();
    let credentials = fs::read(root.path().join(".env")).unwrap();
    success(invoke(
        root.path(),
        &request(),
        "/changed/caller",
        "android35-arm64",
    ));
    assert_eq!(fs::read(&path).unwrap(), legacy);
    for (field, invalid) in [
        (
            "host_proxy",
            json!("http://PRIVATE_USER:PRIVATE_PASSWORD@example.test:1"),
        ),
        ("container_proxy", json!("file:///tmp/PRIVATE_FILE")),
        ("host_proxy", json!("https://example.test/path")),
        ("local_entry", Value::Null),
        ("launch_path", Value::Null),
        ("launch_path", json!("")),
        ("launch_path", json!("/path\u{85}/bin")),
    ] {
        let mut changed = value.clone();
        changed[field] = invalid;
        let bytes = serde_json::to_vec_pretty(&changed).unwrap();
        fs::write(&path, &bytes).unwrap();
        let output = invoke(root.path(), &request(), "/literal", "android35-arm64");
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read(root.path().join(".env")).unwrap(), credentials);
    }
}

#[test]
fn explicit_profile_edits_use_the_same_validator_and_preserve_database_and_device_bytes() {
    let root = tempfile::tempdir().unwrap();
    success(invoke(
        root.path(),
        &request(),
        "/literal",
        "android35-arm64",
    ));
    let path = root.path().join("data/deployment/profile.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let credentials = fs::read(root.path().join(".env")).unwrap();
    let disk = root.path().join("data/userdata.img");
    fs::write(&disk, "keep device").unwrap();
    value["phone"]["memory_mb"] = json!(4096);
    value["locale"] = json!("ru");
    value["private_extension"] = json!({"keep":"value"});
    let edit = |value: &Value| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_host-resources"));
        command
            .arg("--write-host-profile")
            .arg(root.path())
            .env_clear()
            .env("PATH", root.path());
        send(&mut command, value)
    };
    success(edit(&value));
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
        value
    );
    let before = fs::read(&path).unwrap();
    value["host_proxy"] = json!("http://PRIVATE_USER:PRIVATE_PASSWORD@example.test");
    let failed = edit(&value);
    assert!(!failed.status.success());
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("PRIVATE_"));
    assert!(failed.stdout.is_empty());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read(root.path().join(".env")).unwrap(), credentials);
    assert_eq!(fs::read(disk).unwrap(), b"keep device");
}

#[test]
fn profile_validation_needs_no_installation_and_accepts_both_supported_phone_templates() {
    let root = tempfile::tempdir().unwrap();
    success(invoke(
        root.path(),
        &request(),
        "/literal",
        "android35-arm64",
    ));
    let profile: Value = serde_json::from_slice(
        &fs::read(root.path().join("data/deployment/profile.json")).unwrap(),
    )
    .unwrap();
    let empty = tempfile::tempdir().unwrap();
    for template in ["android35-arm64", "android35-x86_64"] {
        let mut value = profile.clone();
        value["phone"]["template"] = json!(template);
        let mut command = Command::new(env!("CARGO_BIN_EXE_host-resources"));
        command
            .arg("--validate-host-profile")
            .env_clear()
            .env("PATH", empty.path())
            .current_dir(empty.path());
        let output = send(&mut command, &value);
        assert!(output.status.success());
        assert_eq!(output.stdout, b"{\"valid\":true}\n");
    }
    assert_eq!(fs::read_dir(empty.path()).unwrap().count(), 0);
}
