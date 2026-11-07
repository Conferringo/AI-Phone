//! The actual resource command must work without Bun, Python or user devices.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};
fn pin(bytes: &[u8], url: &str) -> Value {
    json!({"file":"fixture.apk","size_bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(bytes)),"url":url,"version":"fixture"})
}
fn config(root: &Path, value: Value) {
    fs::create_dir_all(root.join("config")).unwrap();
    fs::write(root.join("config/clash-apps.json"),serde_json::to_vec(&json!({"android":value,"linux":value,"alternatives":{"android-x86_64":value,"linux-amd64":value}})).unwrap()).unwrap();
}
fn call(root: &Path, operation: &str, abi: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .args(["--guest-app-resources", root.to_str().unwrap(), operation])
        .env_clear()
        .env("PATH", "")
        .env("AI_PHONE_ANDROID_ABI", abi)
        .env(
            "AI_PHONE_CLASH_ANDROID_APK",
            root.join(format!("runtime/guest-apps/clash-{abi}.apk")),
        )
        .env("AI_PHONE_IMAGE_PULL_POLICY", "never")
        .env(
            "AI_PHONE_DESKTOP_IMAGE",
            format!("ai-phone-bundle/desktop:sha-{}", "a".repeat(64)),
        )
        .output()
        .unwrap()
}
#[test]
fn offline_both_abis_verify_and_download_without_an_interpreter_or_source_cache() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"pinned guest application fixture";
    config(
        root.path(),
        pin(bytes, "http://127.0.0.1:1/must-not-download"),
    );
    fs::create_dir_all(root.path().join("runtime/guest-apps")).unwrap();
    for abi in ["arm64-v8a", "x86_64"] {
        fs::write(
            root.path()
                .join(format!("runtime/guest-apps/clash-{abi}.apk")),
            bytes,
        )
        .unwrap();
        for operation in ["verify", "download"] {
            let output = call(root.path(), operation, abi);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["verified"], true);
        }
    }
    assert!(!root.path().join(".tools").exists());
}

#[cfg(unix)]
#[test]
fn parent_eof_cancels_an_inflight_stream_and_removes_its_unpublished_file() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        process::Stdio,
        sync::mpsc,
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    config(
        root.path(),
        pin(
            b"complete expected body",
            &format!("http://{}/app", listener.local_addr().unwrap()),
        ),
    );
    let (sent, received) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = [0; 4096];
        assert!(stream.read(&mut bytes).unwrap() > 0);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 22\r\n\r\npartial")
            .unwrap();
        sent.send(()).unwrap();
        assert_eq!(stream.read(&mut bytes).unwrap(), 0);
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .args([
            "--guest-app-resources",
            root.path().to_str().unwrap(),
            "download",
            "android",
            "--stdin-lease",
        ])
        .env_clear()
        .env("PATH", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A valid ownership command must fetch before the owner closes its pipe.
    if received.recv_timeout(Duration::from_secs(2)).is_err() {
        let status = child.try_wait().unwrap();
        if status.is_none() {
            child.kill().unwrap();
        }
        let output = child.wait_with_output().unwrap();
        panic!(
            "stream not started: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    drop(child.stdin.take());
    let until = Instant::now() + Duration::from_secs(2);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= until {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("native acquisition outlived its owner");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("installation_cancelled"));
    let folder = root.path().join(".tools/clash-apps/android");
    if folder.exists() {
        assert_eq!(fs::read_dir(folder).unwrap().count(), 0);
    }
    server.join().unwrap();
}

#[test]
fn desktop_is_omitted_only_for_the_exact_pinned_offline_image() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"application";
    config(
        root.path(),
        pin(bytes, "http://127.0.0.1:1/must-not-download"),
    );
    fs::create_dir_all(root.path().join("runtime/guest-apps")).unwrap();
    let apk = root.path().join("runtime/guest-apps/clash-arm64-v8a.apk");
    fs::write(&apk, bytes).unwrap();
    for (policy, image, success) in [
        (
            "never",
            format!("ai-phone-bundle/desktop:sha-{}", "a".repeat(64)),
            true,
        ),
        (
            "missing",
            format!("ai-phone-bundle/desktop:sha-{}", "a".repeat(64)),
            false,
        ),
        ("never", "custom:latest".into(), false),
        (
            "never",
            format!("ai-phone-bundle/desktop:sha-{}", "A".repeat(64)),
            false,
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
            .args([
                "--guest-app-resources",
                root.path().to_str().unwrap(),
                "verify",
            ])
            .env_clear()
            .env("PATH", "")
            .env("AI_PHONE_CLASH_ANDROID_APK", &apk)
            .env("AI_PHONE_IMAGE_PULL_POLICY", policy)
            .env("AI_PHONE_DESKTOP_IMAGE", image)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(!root.path().join(".tools").exists());
}
#[test]
fn bundled_selection_rejects_missing_override_foreign_path_and_unsupported_abi() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"application";
    config(
        root.path(),
        pin(bytes, "http://127.0.0.1:1/must-not-download"),
    );
    fs::create_dir_all(root.path().join("runtime/guest-apps")).unwrap();
    let apk = root.path().join("runtime/guest-apps/clash-arm64-v8a.apk");
    fs::write(&apk, bytes).unwrap();
    for (abi, path, expected) in [
        ("arm64-v8a", None, "guest_app_selection"),
        (
            "arm64-v8a",
            Some(root.path().join("outside.apk")),
            "guest_app_selection",
        ),
        ("mips", Some(apk.clone()), "guest_app_abi"),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_host-resources"));
        command
            .args([
                "--guest-app-resources",
                root.path().to_str().unwrap(),
                "download",
                "android",
            ])
            .env_clear()
            .env("PATH", "")
            .env("AI_PHONE_ANDROID_ABI", abi);
        if let Some(path) = path {
            command.env("AI_PHONE_CLASH_ANDROID_APK", path);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
    }
    fs::write(&apk, b"corrupted").unwrap();
    let output = call(root.path(), "download", "arm64-v8a");
    assert!(String::from_utf8_lossy(&output.stderr).contains("guest_app_integrity"));
    assert!(!root.path().join(".tools").exists());
}
#[cfg(unix)]
#[test]
fn bundled_aliases_directories_and_non_directory_components_never_fall_back_to_download() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    config(
        root.path(),
        pin(b"fixture", "http://127.0.0.1:1/must-not-download"),
    );
    let folder = root.path().join("runtime/guest-apps");
    fs::create_dir_all(folder.parent().unwrap()).unwrap();
    let outside = root.path().join("outside");
    fs::create_dir(&outside).unwrap();
    for target in [root.path().join("missing"), outside] {
        symlink(target, &folder).unwrap();
        let output = call(root.path(), "download", "arm64-v8a");
        assert!(String::from_utf8_lossy(&output.stderr).contains("guest_app_component"));
        fs::remove_file(&folder).unwrap();
    }
    fs::create_dir(&folder).unwrap();
    let apk = folder.join("clash-arm64-v8a.apk");
    let outside = root.path().join("outside.apk");
    fs::write(&outside, b"fixture").unwrap();
    symlink(outside, &apk).unwrap();
    assert!(
        String::from_utf8_lossy(&call(root.path(), "download", "arm64-v8a").stderr)
            .contains("guest_app_component")
    );
    fs::remove_file(&apk).unwrap();
    fs::create_dir(&apk).unwrap();
    assert!(
        String::from_utf8_lossy(&call(root.path(), "download", "arm64-v8a").stderr)
            .contains("guest_app_component")
    );
    fs::remove_dir_all(&folder).unwrap();
    fs::write(&folder, b"not a component").unwrap();
    assert!(
        String::from_utf8_lossy(&call(root.path(), "download", "arm64-v8a").stderr)
            .contains("guest_app_component")
    );
    assert!(!root.path().join(".tools").exists());
}
#[test]
fn source_desktop_directory_override_is_used_by_the_native_owner() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"cached source app";
    config(
        root.path(),
        pin(bytes, "http://127.0.0.1:1/must-not-download"),
    );
    let folder = root.path().join(".tools/clash-apps/linux-amd64");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("fixture.apk"), bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .args([
            "--guest-app-resources",
            root.path().to_str().unwrap(),
            "download",
            "linux",
        ])
        .env_clear()
        .env("PATH", "")
        .env("AI_PHONE_CLASH_DESKTOP_DIR", "linux-amd64")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["paths"]["linux"],
        folder.join("fixture.apk").to_str().unwrap()
    );
}

#[test]
fn capability_probe_requires_no_root_runtime_or_interpreter() {
    let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .arg("--guest-app-resources-contract")
        .env_clear()
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"ai-phone-guest-app-resources-v1\n");
    let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .args(["--guest-app-resources-contract", "extra"])
        .env_clear()
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
