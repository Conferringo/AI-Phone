#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn invoke(root: &Path, path: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .args([
            "--runtime-environment",
            root.to_str().unwrap(),
            root.to_str().unwrap(),
        ])
        .env_clear()
        .env("PATH", path)
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
fn profile(root: &Path, value: Value) {
    fs::create_dir_all(root.join("data/deployment")).unwrap();
    fs::write(
        root.join("data/deployment/profile.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
}

#[test]
fn native_environment_keeps_profile_path_owner_resources_and_user_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let literal = "$HOME/literal:$(not-a-command):~/工具/bin:/usr/bin:/usr/bin";
    profile(
        &root,
        json!({"launch_path":literal,"locale":"ru","sdk_root":"/chosen SDK","java_home":"/chosen Java","host_proxy":"http://proxy:7890","phone":{"template":"android35-arm64","cpus":2,"memory_mb":2048,"storage_gb":24},"desktop":{"cpus":4,"memory_mb":4096}}),
    );
    fs::write(root.join("data/user.img"), "device disk").unwrap();
    let before = fs::read(root.join("data/deployment/profile.json")).unwrap();
    let a = value(invoke(&root, "/caller/a"));
    assert_eq!(
        a["AI_PHONE_NATIVE_HOST_PROGRAM"],
        env!("CARGO_BIN_EXE_host-resources")
    );
    assert_eq!(a, value(invoke(&root, "/caller/b")));
    assert_eq!(
        a["PATH"],
        format!(
            "{}/.tools/bin:/opt/homebrew/bin:/usr/local/bin:$HOME/literal:$(not-a-command):~/工具/bin:/usr/bin",
            root.display()
        )
    );
    for (key, expected) in [
        ("AI_PHONE_DEFAULT_LOCALE", "ru"),
        ("AI_PHONE_PHONE_CPUS", "2"),
        ("AI_PHONE_PHONE_MEMORY", "2048"),
        ("AI_PHONE_PHONE_STORAGE", "24"),
        ("AI_PHONE_DESKTOP_MEMORY", "4096m"),
        ("AI_PHONE_ANDROID_ABI", "arm64-v8a"),
        ("AI_PHONE_EMULATOR_PROXY", ""),
        ("JAVA_HOME", "/chosen Java"),
    ] {
        assert_eq!(a[key], expected, "{key}");
    }
    assert_eq!(
        fs::read(root.join("data/deployment/profile.json")).unwrap(),
        before
    );
    assert_eq!(
        fs::read_to_string(root.join("data/user.img")).unwrap(),
        "device disk"
    );
}

#[test]
fn legacy_profile_path_tracks_caller_without_rewriting_saved_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    profile(root, json!({"phone":{"template":"android35-x86_64"}}));
    let a = value(invoke(root, "/caller/a"));
    let b = value(invoke(root, "/caller/b"));
    assert_ne!(a["PATH"], b["PATH"]);
    assert_eq!(a["AI_PHONE_ANDROID_ABI"], "x86_64");
    assert_eq!(a["AI_PHONE_PHONE_STORAGE"], "16");
    assert_eq!(a["AI_PHONE_PHONE_MEMORY"], "3072");
    assert!(
        !serde_json::from_slice::<Value>(
            &fs::read(root.join("data/deployment/profile.json")).unwrap()
        )
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("launch_path")
    );
}

#[test]
fn source_environment_uses_legacy_proxy_only_without_a_saved_profile() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join(".env"),"HOST_HTTP_PROXY=http://old:7890\nCONTAINER_HTTP_PROXY=http://guest:7891\nAI_PHONE_LOCAL_ENTRY=1\nMODEL_KEY=PRIVATE_DO_NOT_EXPORT\n").unwrap();
    let a = value(invoke(root, "/caller"));
    assert_eq!(a["AI_PHONE_EMULATOR_PROXY"], "http://old:7890");
    assert_eq!(a["AI_PHONE_LOCAL_ENTRY"], "1");
    assert!(a.get("MODEL_KEY").is_none());
    profile(root, json!({"host_proxy":""}));
    let b = value(invoke(root, "/caller"));
    assert_eq!(b["AI_PHONE_HOST_PROXY"], "");
    assert_eq!(b["AI_PHONE_EMULATOR_PROXY"], "");
}

#[test]
fn invalid_saved_path_is_rejected_without_any_configuration_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for path in [
        Value::Null,
        json!(1),
        json!(true),
        json!([]),
        json!({}),
        json!(""),
        json!(" "),
        json!("/bin\nBAD"),
        json!("/bin\u{0085}BAD"),
    ] {
        profile(root, json!({"launch_path":path}));
        let before = fs::read(root.join("data/deployment/profile.json")).unwrap();
        let result = invoke(root, "/caller");
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains("_profile"));
        assert_eq!(
            fs::read(root.join("data/deployment/profile.json")).unwrap(),
            before
        );
    }
}

#[test]
fn data_alias_is_resolved_without_creating_guest_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    let user = temp.path().join("user-data");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&user).unwrap();
    std::os::unix::fs::symlink(&user, root.join("data")).unwrap();
    let env = value(invoke(&root, "/caller"));
    let canonical = user.canonicalize().unwrap();
    assert_eq!(env["AI_PHONE_DATA_ROOT"], canonical.to_str().unwrap());
    assert_eq!(
        env["ANDROID_AVD_HOME"],
        canonical.join("android/avd").to_str().unwrap()
    );
    assert!(!user.join("android").exists());
}

#[test]
fn managed_engine_path_precedes_the_saved_launch_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    profile(&root, json!({"launch_path":"/saved/tools"}));
    let env = value(
        Command::new(env!("CARGO_BIN_EXE_host-resources"))
            .args([
                "--runtime-environment",
                root.to_str().unwrap(),
                root.to_str().unwrap(),
            ])
            .env_clear()
            .env("PATH", "/caller")
            .env("AI_PHONE_MANAGED_BIN", "/owned/engine")
            .output()
            .unwrap(),
    );
    assert_eq!(
        env["PATH"],
        format!(
            "/owned/engine:{}/.tools/bin:/opt/homebrew/bin:/usr/local/bin:/saved/tools",
            root.display()
        )
    );
    assert!(!root.join("data/deployment/tool-runtime.json").exists());
}

#[test]
fn installed_image_environment_reads_native_metadata_without_starting_an_engine() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    profile(root, json!({}));
    let sha = "1".repeat(64);
    let mut images = serde_json::Map::new();
    for role in ["workspace", "desktop", "postgres", "redis"] {
        images.insert(role.into(),json!({"source_digest":format!("sha256:{sha}"),"export_digest":format!("sha256:{sha}"),"config_digest":format!("sha256:{sha}"),"ref":format!("ai-phone-bundle/{role}:sha-{sha}"),"size":1}));
    }
    let manifest = json!({"format":1,"os":"linux","architecture":"arm64","redistribution":"review-pending","images":images,"archive":{"path":"images.tar.gz","size":1,"sha256":sha}});
    let path = root.join("data/deployment/image-runtime.json");
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let env = value(invoke(root, "/no-host-tools"));
    for role in ["workspace", "desktop", "postgres", "redis"] {
        assert_eq!(
            env[format!("AI_PHONE_{}_IMAGE", role.to_uppercase())],
            manifest["images"][role]["ref"]
        );
    }
    assert_eq!(env["AI_PHONE_IMAGE_PULL_POLICY"], "never");
    fs::write(path, b"{}").unwrap();
    assert!(!invoke(root, "/no-host-tools").status.success());
}
