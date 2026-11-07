#![cfg(unix)]
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::{
        fd::OwnedFd,
        unix::{
            fs::{PermissionsExt, symlink},
            net::UnixStream,
        },
    },
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
const OWNER: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
fn command(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_host-resources"));
    cmd.arg("--channel-browser")
        .env_clear()
        .env("PATH", "")
        .env("AI_PHONE_PROJECT_ROOT", root)
        .env(
            "AI_PHONE_DESKTOP_IMAGE",
            format!("ai-phone-bundle/desktop:sha-{}", "a".repeat(64)),
        )
        .env("AI_PHONE_BROWSER_SESSION", "fixture")
        .env("AI_PHONE_BROWSER_OWNER", OWNER)
        .env("AI_PHONE_DOCKER", env!("CARGO_BIN_EXE_browser-test-engine"))
        .env("BROWSER_TEST_STATE", root.join("engine.json"));
    cmd
}
fn ok(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn profile(root: &Path) -> PathBuf {
    root.join("data/channels/whatsapp/session-fixture")
}
fn record(root: &Path) -> PathBuf {
    let key = format!("{:x}", Sha256::digest(profile(root).to_str().unwrap()));
    root.join("data/channels/whatsapp/.browser-runtime")
        .join(key + ".json")
}
fn owned(root: &Path, token: &str) {
    let profile = profile(root);
    fs::create_dir_all(&profile).unwrap();
    let marker = profile.join(".aiphone-runtime.json");
    fs::write(
        &marker,
        serde_json::to_vec(
            &json!({"format":1,"mode":"container","session_id":"fixture","owner_token":token}),
        )
        .unwrap(),
    )
    .unwrap();
    fs::set_permissions(marker, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(profile.join("account"), "retain existing account").unwrap();
}
fn launch(root: &Path) -> Command {
    let source = command(root);
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", "exec 3<&0 4>&1; exec \"$@\"", "--"])
        .arg(env!("CARGO_BIN_EXE_host-resources"))
        .arg("--channel-browser")
        .arg(format!("--user-data-dir={}", profile(root).display()))
        .arg("--remote-debugging-pipe");
    cmd.env_clear()
        .envs(source.get_envs().map(|(k, v)| (k, v.unwrap())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}
fn wait_for(root: &Path, event: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if fs::read_to_string(root.join("engine.events")).is_ok_and(|v| v.contains(event)) {
            return;
        }
        assert!(Instant::now() < deadline, "browser did not reach {event}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn check_runs_a_native_engine_without_python_and_creates_no_session() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let result = ok(command(&root).arg("--check").output().unwrap());
    assert_eq!(result["available"], true);
    assert!(result["version"].as_str().unwrap().starts_with("Chromium "));
    assert!(!root.join("data").exists());
}
#[test]
fn invalid_inputs_fail_before_any_engine_or_account_write() {
    for (key, value) in [
        ("AI_PHONE_BROWSER_SESSION", "../other"),
        ("AI_PHONE_BROWSER_OWNER", "bad"),
        ("AI_PHONE_DESKTOP_IMAGE", "latest"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let output = command(temp.path())
            .env(key, value)
            .arg("--stop")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!temp.path().join("data").exists());
        assert!(!temp.path().join("engine.events").exists());
    }
}
#[test]
fn superseded_stop_is_harmless_but_erase_cannot_remove_a_new_account() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    owned(&root, &"f".repeat(64));
    assert_eq!(
        ok(command(&root).arg("--stop").output().unwrap())["superseded"],
        true
    );
    assert!(
        !command(&root)
            .arg("--erase")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(
        fs::read_to_string(profile(&root).join("account")).unwrap(),
        "retain existing account"
    );
}
#[test]
fn explicit_owned_erase_does_not_touch_the_neighbouring_session() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    owned(&root, OWNER);
    let other = root.join("data/channels/whatsapp/session-other");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("account"), "other").unwrap();
    assert_eq!(
        ok(command(&root).arg("--erase").output().unwrap())["erased"],
        true
    );
    assert!(!profile(&root).exists());
    assert_eq!(fs::read_to_string(other.join("account")).unwrap(), "other");
    assert!(!record(&root).exists());
}
#[test]
fn host_accounts_and_profile_aliases_are_preserved() {
    for alias in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::create_dir_all(profile(&root).parent().unwrap()).unwrap();
        let target = if alias {
            root.join("original")
        } else {
            profile(&root)
        };
        fs::create_dir(&target).unwrap();
        fs::write(target.join("account"), "original").unwrap();
        if alias {
            symlink(&target, profile(&root)).unwrap();
        }
        assert!(
            !command(&root)
                .arg("--erase")
                .output()
                .unwrap()
                .status
                .success()
        );
        assert_eq!(
            fs::read_to_string(target.join("account")).unwrap(),
            "original"
        );
    }
}
#[test]
fn binary_pipe_drains_beyond_capacity_and_cleans_only_the_launch_container() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let mut child = launch(&root).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let bytes: Vec<_> = (0..512 * 1024).map(|n| (n % 251) as u8).collect();
    let sent = bytes.clone();
    let writer = std::thread::spawn(move || {
        stdin.write_all(&sent).unwrap();
    });
    let mut received = vec![];
    stdout.read_to_end(&mut received).unwrap();
    writer.join().unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(received, bytes);
    assert!(profile(&root).join(".aiphone-runtime.json").exists());
    assert!(!record(&root).exists());
    assert!(!root.join("engine.json").exists());
    let events: Vec<Vec<String>> =
        serde_json::from_slice(&fs::read(root.join("engine.events")).unwrap()).unwrap();
    let create = events.iter().find(|a| a[0] == "create").unwrap();
    assert!(create.windows(2).any(|a| a == ["--entrypoint", "/bin/sh"]));
    assert!(
        !create
            .iter()
            .any(|a| a.contains("python") || a == "--publish")
    );
}
#[test]
fn cancellation_while_input_is_blocked_reaps_attach_and_retains_account() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    owned(&root, OWNER);
    let mut child = launch(&root).spawn().unwrap();
    wait_for(&root, "\"start\"");
    let start = Instant::now();
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(child.id() as i32).unwrap(),
        rustix::process::Signal::TERM,
    )
    .unwrap();
    assert!(child.wait().unwrap().success());
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(!record(&root).exists());
    assert!(!root.join("engine.json").exists());
    assert_eq!(
        fs::read_to_string(profile(&root).join("account")).unwrap(),
        "retain existing account"
    );
}
#[test]
fn unknown_create_is_observed_and_removed_without_replaying_or_erasing_account() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    owned(&root, OWNER);
    let result = launch(&root)
        .env("BROWSER_TEST_UNKNOWN_CREATE", "1")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!record(&root).exists());
    assert!(!root.join("engine.json").exists());
    assert!(profile(&root).join("account").exists());
    let events: Vec<Vec<String>> =
        serde_json::from_slice(&fs::read(root.join("engine.events")).unwrap()).unwrap();
    assert_eq!(events.iter().filter(|a| a[0] == "create").count(), 1);
}
#[test]
fn sdk_socket_pairs_preserve_binary_frames_and_eof() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let (mut input, child_in) = UnixStream::pair().unwrap();
    let (child_out, mut output) = UnixStream::pair().unwrap();
    let child = command(&root)
        .args([
            format!("--user-data-dir={}", profile(&root).display()),
            "--remote-debugging-pipe".into(),
        ])
        .stdin(Stdio::from(OwnedFd::from(child_in)))
        .stdout(Stdio::from(OwnedFd::from(child_out)))
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let bytes: Vec<_> = (0..512 * 1024).map(|n| (n % 251) as u8).collect();
    let sent = bytes.clone();
    let writer = std::thread::spawn(move || input.write_all(&sent).unwrap());
    let mut received = vec![];
    output.read_to_end(&mut received).unwrap();
    writer.join().unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(received, bytes);
    assert!(!record(&root).exists());
}
fn crashed(root: &Path) {
    owned(root, OWNER);
    let path = record(root);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let key = path.file_stem().unwrap().to_str().unwrap();
    fs::write(&path,serde_json::to_vec(&json!({"format":1,"profile_hash":key,"image_id":format!("sha256:{}","a".repeat(64)),"token":OWNER,"container_id":"b".repeat(64)})).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join("engine.json"),serde_json::to_vec(&json!({"Id":"b".repeat(64),"Name":format!("/ai-phone-browser-{}",&key[..24]),"Image":format!("sha256:{}","a".repeat(64)),"Config":{"Labels":{"dev.aiphone.component":"channel-browser","dev.aiphone.profile":key,"dev.aiphone.launch":OWNER}},"State":{"Running":true}})).unwrap()).unwrap();
}
#[test]
fn crash_recovery_stops_before_removing_and_preserves_account() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    crashed(&root);
    assert_eq!(
        ok(command(&root).arg("--stop").output().unwrap())["stopped"],
        true
    );
    assert!(profile(&root).join("account").exists());
    assert!(!record(&root).exists());
    assert!(!root.join("engine.json").exists());
    let events: Vec<Vec<String>> =
        serde_json::from_slice(&fs::read(root.join("engine.events")).unwrap()).unwrap();
    assert!(
        events.iter().position(|a| a[0] == "stop").unwrap()
            < events.iter().position(|a| a[0] == "rm").unwrap()
    );
    assert_eq!(
        events.iter().filter(|a| a[0] == "inspect").count(),
        1,
        "the named immutable owner is already observed"
    );
}
#[test]
fn foreign_container_changes_are_not_adopted_or_deleted() {
    for field in ["Image", "Id", "Name", "token"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        crashed(&root);
        let path = root.join("engine.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if field == "token" {
            value["Config"]["Labels"]["dev.aiphone.launch"] = json!("f".repeat(64));
        } else {
            value[field] = json!(if field == "Id" {
                "f".repeat(64)
            } else {
                "foreign".into()
            });
        }
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            !command(&root)
                .arg("--stop")
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(path.exists());
        assert!(record(&root).exists());
        assert!(profile(&root).join("account").exists());
    }
}
#[test]
fn corrupt_private_records_are_retained_with_the_account() {
    for value in ["{}".to_owned(), "x".repeat(4097)] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        crashed(&root);
        fs::write(record(&root), &value).unwrap();
        assert!(
            !command(&root)
                .arg("--erase")
                .output()
                .unwrap()
                .status
                .success()
        );
        assert_eq!(fs::read_to_string(record(&root)).unwrap(), value);
        assert!(profile(&root).join("account").exists());
    }
    for mode in [0o644, 0o600] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        crashed(&root);
        let record = record(&root);
        let outside = root.join("original.json");
        fs::rename(&record, &outside).unwrap();
        if mode == 0o644 {
            fs::rename(&outside, &record).unwrap();
            fs::set_permissions(&record, fs::Permissions::from_mode(mode)).unwrap();
        } else {
            symlink(&outside, &record).unwrap();
        }
        assert!(
            !command(&root)
                .arg("--erase")
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(profile(&root).join("account").exists());
        assert!(root.join("engine.json").exists());
    }
}
#[test]
fn unknown_ipc_prevents_partial_cleanup_but_known_links_can_be_replaced() {
    for unknown in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        owned(&root, OWNER);
        symlink("abcdef012345-42", profile(&root).join("SingletonLock")).unwrap();
        symlink(
            "/tmp/.org.chromium.Chromium.fixture/SingletonSocket",
            profile(&root).join("SingletonSocket"),
        )
        .unwrap();
        if unknown {
            fs::write(profile(&root).join("SingletonCookie"), "unknown user file").unwrap();
        } else {
            symlink("123456", profile(&root).join("SingletonCookie")).unwrap();
        }
        let result = launch(&root).output().unwrap();
        assert_eq!(result.status.success(), !unknown);
        if unknown {
            assert_eq!(
                fs::read_link(profile(&root).join("SingletonLock")).unwrap(),
                Path::new("abcdef012345-42")
            );
            assert!(!root.join("engine.json").exists());
        } else {
            assert!(fs::symlink_metadata(profile(&root).join("SingletonLock")).is_err());
        }
    }
}
#[test]
fn launch_never_retries_a_busy_profile_and_manage_can_wait_for_its_owner() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let mut first = launch(&root).spawn().unwrap();
    wait_for(&root, "\"start\"");
    let start = Instant::now();
    assert!(!launch(&root).output().unwrap().status.success());
    assert!(start.elapsed() < Duration::from_secs(1));
    let waiting = command(&root)
        .arg("--stop")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(first.id() as i32).unwrap(),
        rustix::process::Signal::TERM,
    )
    .unwrap();
    assert!(first.wait().unwrap().success());
    assert_eq!(ok(waiting.wait_with_output().unwrap())["stopped"], true);
}
#[test]
fn daemon_failure_and_ambiguous_observation_keep_recovery_intent() {
    for value in [
        None,
        Some("invalid"),
        Some(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\ncccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        crashed(&root);
        let mut cmd = command(&root);
        if let Some(output) = value {
            cmd.env("BROWSER_TEST_OUTPUT_FOR", "container")
                .env("BROWSER_TEST_OUTPUT", output);
        } else {
            cmd.env("BROWSER_TEST_FAIL", "container");
        }
        assert!(!cmd.arg("--stop").output().unwrap().status.success());
        assert!(record(&root).exists());
        assert!(root.join("engine.json").exists());
        assert!(profile(&root).join("account").exists());
    }
}
#[test]
fn malformed_inspection_does_not_count_as_absence() {
    for value in ["{}", "[]", "[null]", "[{\"Id\":\"wrong\",\"Config\":null}]"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        crashed(&root);
        assert!(
            !command(&root)
                .env("BROWSER_TEST_OUTPUT_FOR", "inspect")
                .env("BROWSER_TEST_OUTPUT", value)
                .arg("--erase")
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(record(&root).exists());
        assert!(profile(&root).join("account").exists());
    }
}
#[test]
fn browser_version_and_command_output_budget_are_verified() {
    for value in ["not a browser", "Chromium x.1.2.3", "Chromium 1.2.3"] {
        let temp = tempfile::tempdir().unwrap();
        assert!(
            !command(temp.path())
                .env("BROWSER_TEST_OUTPUT_FOR", "run")
                .env("BROWSER_TEST_OUTPUT", value)
                .arg("--check")
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(!temp.path().join("data").exists());
    }
    let temp = tempfile::tempdir().unwrap();
    assert!(
        !command(temp.path())
            .env("BROWSER_TEST_OUTPUT_FOR", "run")
            .env("BROWSER_TEST_OUTPUT_BYTES", "1048577")
            .arg("--check")
            .output()
            .unwrap()
            .status
            .success()
    );
}
#[test]
fn unknown_profile_uid_mapping_never_creates_a_browser() {
    for value in [
        "",
        "0",
        "true:1000",
        "-1:0",
        "4294967295:0",
        "0:1.5",
        "0:0:0",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        owned(&root, OWNER);
        assert!(
            !launch(&root)
                .env("BROWSER_TEST_OUTPUT_FOR", "run")
                .env("BROWSER_TEST_OUTPUT", value)
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(!root.join("engine.json").exists());
        assert!(!record(&root).exists());
        assert!(profile(&root).join("account").exists());
    }
}
#[test]
fn profile_paths_with_unicode_comma_and_quotes_keep_their_exact_mount() {
    let temp = tempfile::Builder::new()
        .prefix("aiphone,空间\"-")
        .tempdir()
        .unwrap();
    let root = temp.path().canonicalize().unwrap();
    assert!(launch(&root).output().unwrap().status.success());
    let events: Vec<Vec<String>> =
        serde_json::from_slice(&fs::read(root.join("engine.events")).unwrap()).unwrap();
    let create = events.iter().find(|a| a[0] == "create").unwrap();
    let mount = &create[create.iter().position(|v| v == "--mount").unwrap() + 1];
    assert_eq!(
        mount,
        &format!(
            "type=bind,\"source={}\",target=/profile",
            profile(&root).to_str().unwrap().replace('"', "\"\"")
        )
    );
}
#[test]
fn unsafe_launch_options_are_rejected_before_opening_a_profile() {
    for options in [
        vec!["about:blank".into()],
        vec!["--remote-debugging-port=9222".into()],
        vec!["--x".into(); 65],
        vec![format!("--x={}", "x".repeat(16384))],
        vec!["--user-data-dir=/another-profile".into()],
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        assert!(
            !command(&root)
                .arg(format!("--user-data-dir={}", profile(&root).display()))
                .arg("--remote-debugging-pipe")
                .args(options)
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(!root.join("data").exists());
        assert!(!root.join("engine.events").exists());
    }
}
#[test]
fn restored_account_marker_is_independent_of_the_code_or_data_path() {
    let original = tempfile::tempdir().unwrap();
    let restored = tempfile::tempdir().unwrap();
    owned(original.path(), OWNER);
    fs::create_dir_all(profile(restored.path())).unwrap();
    for name in [".aiphone-runtime.json", "account"] {
        fs::copy(
            profile(original.path()).join(name),
            profile(restored.path()).join(name),
        )
        .unwrap();
    }
    assert_eq!(
        ok(command(restored.path()).arg("--stop").output().unwrap())["stopped"],
        true
    );
    assert!(profile(original.path()).join("account").exists());
    assert!(profile(restored.path()).join("account").exists());
    assert_ne!(
        record(original.path()).file_name(),
        record(restored.path()).file_name()
    );
}
