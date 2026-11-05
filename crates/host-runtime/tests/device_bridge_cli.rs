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
fn source_staging_is_available_in_the_shared_native_component() {
    let output = invoke(&["--source-staging-contract"], b"");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"ai-phone-source-staging-v1\n");
    assert!(
        !invoke(&["--source-staging-contract", "extra"], b"")
            .status
            .success()
    );
    let folder = tempfile::tempdir().unwrap();
    let home = folder.path().join("uncreated home");
    for args in [
        vec!["--stage-source"],
        vec![
            "--stage-source",
            "payload",
            home.to_str().unwrap(),
            "invalid-sha",
        ],
    ] {
        assert!(!invoke(&args, b"").status.success());
        assert!(!home.exists());
    }
}

#[cfg(unix)]
#[test]
fn source_staging_termination_waits_for_the_writer_and_leaves_no_partial_release() {
    use flate2::{Compression, write::GzEncoder};
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        time::{Duration, Instant},
    };

    struct OwnedChild(std::process::Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }
    let temporary = tempfile::tempdir().unwrap();
    let payload = temporary.path().join("source.payload");
    let mut archive = tar::Builder::new(GzEncoder::new(
        fs::File::create(&payload).unwrap(),
        Compression::fast(),
    ));
    let body = vec![b'x'; 12 * 1024 * 1024];
    let checksum = format!("{:x}", Sha256::digest(&body));
    let mut records = Vec::new();
    for i in 0..4 {
        let name = format!("scripts/fixture-{i}.rs");
        let mut header = tar::Header::new_ustar();
        header.set_mode(0o644);
        header.set_size(body.len() as u64);
        header.set_cksum();
        archive
            .append_data(&mut header, &name, body.as_slice())
            .unwrap();
        records.push(
            serde_json::json!({"path":name,"size":body.len(),"sha256":checksum,"mode":0o644}),
        );
    }
    let manifest = serde_json::to_vec(&serde_json::json!({"format":1,"files":records})).unwrap();
    let mut header = tar::Header::new_ustar();
    header.set_mode(0o644);
    header.set_size(manifest.len() as u64);
    header.set_cksum();
    archive
        .append_data(&mut header, "HOST-MANIFEST.json", manifest.as_slice())
        .unwrap();
    archive.into_inner().unwrap().finish().unwrap();
    let identity = format!("{:x}", Sha256::digest(fs::read(&payload).unwrap()));
    let home = temporary.path().join("owned installation");
    fs::create_dir_all(home.join("user/data")).unwrap();
    fs::write(home.join("user/data/chat"), b"preserve fixture history").unwrap();
    let mut child = OwnedChild(
        Command::new(env!("CARGO_BIN_EXE_host-resources"))
            .arg("--stage-source")
            .arg(&payload)
            .arg(&home)
            .arg(&identity)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "staging did not become observable"
        );
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "staging finished before the cancellation injection"
        );
        if fs::read_dir(home.join("releases")).is_ok_and(|mut entries| {
            entries.any(|entry| entry.is_ok_and(|entry| entry.file_name() != identity.as_str()))
        }) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        Command::new("/bin/kill")
            .args(["-TERM", &child.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.0.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "source writer did not settle after cancellation"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    use std::io::Read;
    let mut output = String::new();
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    assert!(
        output.is_empty(),
        "cancellation must not emit a successful publication receipt"
    );
    let mut errors = String::new();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut errors)
        .unwrap();
    assert!(errors.contains("installation_cancelled"));
    assert!(!child.0.wait().unwrap().success());
    assert_eq!(fs::read_dir(home.join("releases")).unwrap().count(), 0);
    assert_eq!(
        fs::read(home.join("user/data/chat")).unwrap(),
        b"preserve fixture history"
    );
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

#[test]
fn android_session_rejects_bad_frames_and_reports_typed_startup_failure() {
    use ai_phone_api::wire::{
        NativeAndroidSessionReply, native_android_session_reply::Result as Reply,
    };
    use prost::Message;
    let output = invoke(&["--android-session-contract"], b"");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"ai-phone-native-android-session-v1\n");
    for (arguments, input) in [
        (vec!["--android-session-contract", "extra"], vec![]),
        (vec!["--android-session", "extra"], vec![]),
        (vec!["--android-session"], vec![0, 0, 0, 1, 255]),
        (vec!["--android-session"], vec![0, 0, 64, 1]),
    ] {
        let output = invoke(&arguments, &input);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
    let output = invoke(&["--android-session"], &[0, 0, 0, 0]);
    assert!(!output.status.success());
    assert_eq!(
        u32::from_be_bytes(output.stdout[..4].try_into().unwrap()) as usize,
        output.stdout.len() - 4
    );
    let reply = NativeAndroidSessionReply::decode(&output.stdout[4..]).unwrap();
    let Some(Reply::Failure(failure)) = reply.result else {
        panic!("no readiness for a missing phone")
    };
    assert_eq!(failure.code, "android_session_phone");
    assert!(!failure.outcome_unknown);
}

#[test]
fn phone_peer_has_a_fixed_contract_and_never_emits_ready_for_bad_input() {
    let output = invoke(&["--phone-peer-contract"], b"");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"ai-phone-phone-peer-v1\n");
    for (args, input) in [
        (vec!["--phone-peer-contract", "extra"], vec![]),
        (vec!["--phone-peer", "extra"], vec![]),
        (vec!["--phone-peer"], vec![0, 0, 0, 0]),
        (vec!["--phone-peer"], vec![0, 0, 64, 1]),
        (vec!["--phone-peer"], vec![0, 0, 0, 1, 255]),
    ] {
        let output = invoke(&args, &input);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
