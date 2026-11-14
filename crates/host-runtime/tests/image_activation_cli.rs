#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

#[path = "fixtures/image_capsule.rs"]
mod capsule;

fn fixture(home: &Path, xz: bool) -> (PathBuf, Value, Vec<u8>) {
    let result = capsule::create(home, xz, "arm64", "fixture", true);
    let bin = home.join("bin");
    fs::create_dir(&bin).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_image-test-engine"), bin.join("docker")).unwrap();
    result
}
fn invoke(home: &Path, payload: &Path) -> Output {
    invoke_with_digest(home, payload, None)
}
fn invoke_with_digest(home: &Path, payload: &Path, digest: Option<&str>) -> Output {
    Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .args(["--activate-images"])
        .arg(home)
        .arg(payload)
        .args(digest)
        .env_clear()
        .env("PATH", home.join("bin"))
        .env("AI_PHONE_IMAGE_TEST_STATE", home)
        .output()
        .unwrap()
}
#[test]
fn native_import_streams_each_codec_and_resumes_without_loading_again_or_replacing_user_files() {
    for xz in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let (payload, manifest, raw) = fixture(home, xz);
        fs::create_dir_all(home.join("data")).unwrap();
        fs::write(home.join("data/chat"), "original chat").unwrap();
        let bytes = fs::read(&payload).unwrap();
        let digest = capsule::hash(&bytes);
        fs::write(&payload, [&bytes[..], &[1]].concat()).unwrap();
        let rejected = invoke_with_digest(home, &payload, Some(&digest));
        assert!(!rejected.status.success());
        assert!(!home.join("queried").exists());
        fs::write(&payload, bytes).unwrap();
        let output = invoke_with_digest(home, &payload, Some(&digest));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let environment: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(environment["AI_PHONE_IMAGE_PULL_POLICY"], "never");
        assert_eq!(
            environment["AI_PHONE_DESKTOP_IMAGE"],
            manifest["images"]["desktop"]["ref"]
        );
        let imported = fs::read(home.join("imported")).unwrap();
        if xz {
            assert_eq!(imported, raw);
        } else {
            let mut decoder = flate2::read::GzDecoder::new(imported.as_slice());
            let mut actual = Vec::new();
            std::io::Read::read_to_end(&mut decoder, &mut actual).unwrap();
            assert_eq!(actual, raw);
        }
        let record = home.join("data/deployment/image-runtime.json");
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&record).unwrap()).unwrap(),
            manifest
        );
        assert_eq!(inspection_count(home), 2 * capsule::ROLES.len());
        fs::remove_file(home.join("imported")).unwrap();
        fs::write(home.join("events"), "").unwrap();
        let output = invoke(home, &payload);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!home.join("imported").exists());
        assert_eq!(inspection_count(home), capsule::ROLES.len());
        assert_eq!(fs::read(home.join("data/chat")).unwrap(), b"original chat");
    }
}

fn inspection_count(home: &Path) -> usize {
    fs::read_to_string(home.join("events"))
        .unwrap()
        .lines()
        .filter(|event| event.starts_with("inspect "))
        .count()
}
#[test]
fn native_wsl_image_import_uses_the_selected_socket_and_preserves_graph_and_chat() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (payload, manifest, _) = capsule::create_layered(home, false, "amd64", "wsl-socket", false);
    fs::create_dir(home.join("bin")).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_image-test-engine"),
        home.join("bin/docker"),
    )
    .unwrap();
    fs::create_dir(home.join("data")).unwrap();
    fs::write(home.join("data/chat"), b"keep-chat").unwrap();
    let socket = "unix:///run/aiphone/docker.sock";
    let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .arg("--activate-images")
        .arg(home)
        .arg(&payload)
        .env_clear()
        .env("PATH", home.join("bin"))
        .env("DOCKER_HOST", socket)
        .env("AI_PHONE_IMAGE_TEST_SOCKET", socket)
        .env("AI_PHONE_IMAGE_TEST_STATE", home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(home.join("socket-verified")).unwrap(), b"yes");
    assert_eq!(fs::read(home.join("data/chat")).unwrap(), b"keep-chat");
    let record: Value =
        serde_json::from_slice(&fs::read(home.join("data/deployment/image-runtime.json")).unwrap())
            .unwrap();
    assert_eq!(record["archive"], manifest["archive"]);
    assert_eq!(record["import_layout"], "oci-v1");
    let environment: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(environment["AI_PHONE_IMAGE_PULL_POLICY"], "never");
    for role in capsule::ROLES {
        assert_eq!(
            record["images"][role]["config_digest"],
            manifest["images"][role]["config_digest"]
        );
        assert_eq!(
            environment[format!("AI_PHONE_{}_IMAGE", role.to_uppercase())],
            record["images"][role]["ref"]
        );
    }
}

#[test]
fn invalid_capsule_fails_before_contacting_the_engine() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (payload, _, _) = fixture(home, false);
    fs::write(&payload, b"not a capsule").unwrap();
    assert!(!invoke(home, &payload).status.success());
    assert!(!home.join("queried").exists());
    assert!(!home.join("data/deployment/image-runtime.json").exists());
}

#[test]
fn legacy_archives_with_real_layers_are_normalized_to_stable_oci_targets_without_overwriting_old_aliases()
 {
    for xz in [false, true] {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path();
        fixture(home, xz);
        let (payload, source, _) = capsule::create_layered(home, xz, "arm64", "layered", false);
        let expected = home.join("expected-import");
        fs::create_dir(&expected).unwrap();
        let (_, expected, _) = capsule::create_layered(&expected, true, "arm64", "layered", true);
        let old_ref = source["images"]["desktop"]["ref"].as_str().unwrap();
        let foreign =
            json!({"Id":format!("sha256:{}","f".repeat(64)),"Architecture":"arm64","Os":"linux"});
        let mut ready = json!({});
        ready[old_ref] = foreign.clone();
        fs::write(home.join("ready.json"), serde_json::to_vec(&ready).unwrap()).unwrap();
        let output = invoke(home, &payload);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let selected: Value = serde_json::from_slice(
            &fs::read(home.join("data/deployment/image-runtime.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(selected["images"], expected["images"]);
        assert_eq!(selected["archive"], source["archive"]);
        assert_eq!(selected["import_layout"], "oci-v1");
        let ready: Value =
            serde_json::from_slice(&fs::read(home.join("ready.json")).unwrap()).unwrap();
        assert_eq!(ready[old_ref], foreign);
        fs::remove_file(home.join("imported")).unwrap();
        assert!(invoke(home, &payload).status.success());
        assert!(!home.join("imported").exists());
    }
}

fn ready(manifest: &Value) -> Value {
    let mut values = json!({});
    for row in manifest["images"].as_object().unwrap().values() {
        values[row["ref"].as_str().unwrap()] =
            json!({"Id":row["config_digest"],"Architecture":manifest["architecture"],"Os":"linux"});
    }
    values
}
#[test]
fn atomic_replacement_uses_the_open_capsule_but_inplace_changes_never_import() {
    for mode in ["rename", "inplace"] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let (payload, manifest, _) = fixture(home, false);
        let original = fs::read(&payload).unwrap();
        fs::write(home.join("mode"), mode).unwrap();
        let output = invoke(home, &payload);
        if mode == "rename" {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                capsule::hash(&fs::read(home.join("imported")).unwrap()),
                manifest["archive"]["sha256"]
            );
            assert_ne!(fs::read(payload).unwrap(), original);
        } else {
            assert!(!output.status.success());
            assert_eq!(output.stderr, b"image_capsule_changed\n");
            assert!(!home.join("imported").exists());
            assert!(!home.join("data/deployment/image-runtime.json").exists());
        }
    }
}
#[test]
fn failed_import_is_observed_before_resume_and_already_completed_import_is_not_replayed() {
    for mode in ["unknown-full", "unknown-partial", "reject"] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let (payload, _, _) = fixture(home, true);
        fs::write(home.join("mode"), mode).unwrap();
        assert!(!invoke(home, &payload).status.success());
        assert!(!home.join("data/deployment/image-runtime.json").exists());
        fs::write(home.join("mode"), "").unwrap();
        fs::write(home.join("events"), "").unwrap();
        if home.join("imported").exists() {
            fs::remove_file(home.join("imported")).unwrap();
        }
        let output = invoke(home, &payload);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let events = fs::read_to_string(home.join("events")).unwrap();
        let lines: Vec<_> = events.lines().collect();
        assert!(lines[1..5].iter().all(|line| line.starts_with("inspect ")));
        assert_eq!(home.join("imported").exists(), mode != "unknown-full");
    }
}
#[test]
fn wrong_engine_or_existing_tag_is_rejected_and_platform_indexes_are_explicitly_verified() {
    for mode in [
        "bad-info",
        "wrong-arch",
        "index",
        "index-missing",
        "wrong-id",
        "wrong-os",
        "wrong-architecture",
        "malformed-inspect",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let (payload, manifest, _) = fixture(home, false);
        let mut present = ready(&manifest);
        let row = present
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        match mode {
            "wrong-id" => row["Id"] = json!(format!("sha256:{}", "0".repeat(64))),
            "wrong-os" => row["Os"] = json!("windows"),
            "wrong-architecture" => row["Architecture"] = json!("amd64"),
            _ => {}
        }
        fs::write(
            home.join("ready.json"),
            serde_json::to_vec(&present).unwrap(),
        )
        .unwrap();
        fs::write(home.join("mode"), mode).unwrap();
        let output = invoke(home, &payload);
        assert_eq!(
            output.status.success(),
            mode == "index",
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!home.join("imported").exists());
        if mode.starts_with("index") {
            assert!(home.join("platform-query").exists());
        }
    }
}
#[test]
fn publication_failure_preserves_old_state_and_a_later_resume_needs_no_reimport() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (payload, _, _) = fixture(home, false);
    let destination = home.join("data/deployment/image-runtime.json");
    fs::create_dir_all(&destination).unwrap();
    fs::write(destination.join("keep"), "original state").unwrap();
    assert!(!invoke(home, &payload).status.success());
    assert_eq!(
        fs::read(destination.join("keep")).unwrap(),
        b"original state"
    );
    assert_eq!(
        fs::read_dir(destination.parent().unwrap()).unwrap().count(),
        1
    );
    fs::remove_file(destination.join("keep")).unwrap();
    fs::remove_dir(&destination).unwrap();
    fs::remove_file(home.join("imported")).unwrap();
    let output = invoke(home, &payload);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!home.join("imported").exists());
}
#[test]
fn cancellation_reaps_import_process_even_when_it_ignores_termination() {
    for mode in ["block", "block-ignore-term"] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let (payload, _, _) = fixture(home, true);
        fs::write(home.join("mode"), mode).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_host-resources"))
            .arg("--activate-images")
            .arg(home)
            .arg(payload)
            .env_clear()
            .env("PATH", home.join("bin"))
            .env("AI_PHONE_IMAGE_TEST_STATE", home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !home.join("load.pid").exists() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("native import did not start");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let engine = fs::read_to_string(home.join("load.pid"))
            .unwrap()
            .parse::<i32>()
            .unwrap();
        let engine = rustix::process::Pid::from_raw(engine).unwrap();
        let begin = Instant::now();
        rustix::process::kill_process(
            rustix::process::Pid::from_raw(child.id() as i32).unwrap(),
            rustix::process::Signal::TERM,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                let _ = rustix::process::kill_process(engine, rustix::process::Signal::KILL);
                panic!("cancel did not reap import");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(!status.success());
        assert!(rustix::process::test_kill_process(engine).is_err());
        assert!(!home.join("data/deployment/image-runtime.json").exists());
        if mode == "block-ignore-term" {
            assert!(begin.elapsed() >= Duration::from_secs(9));
        }
    }
}
fn entries(raw: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = tar::Archive::new(raw);
    archive
        .entries()
        .unwrap()
        .map(|entry| {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().to_str().unwrap().to_owned();
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}
#[test]
fn archive_content_identity_aliases_layers_and_oci_binding_are_enforced_before_engine_access() {
    for mode in [
        "config-digest",
        "extra-alias",
        "missing-layer",
        "duplicate",
        "repositories",
        "oci-binding",
        "oci-subject",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let (_, manifest, raw) = fixture(home, true);
        let (mut manifest, raw) = if mode.starts_with("oci") {
            let (_, manifest, raw) = capsule::create(home, true, "arm64", "fixture", true);
            (manifest, raw)
        } else {
            (manifest, raw)
        };
        let mut objects = entries(&raw);
        match mode {
            "config-digest" => objects[0].1.push(b' '),
            "duplicate" => objects.push(objects[0].clone()),
            "repositories" => objects.push(("repositories".into(), b"{}".to_vec())),
            "extra-alias" | "missing-layer" => {
                let (_, body) = objects
                    .iter_mut()
                    .find(|(name, _)| name == "manifest.json")
                    .unwrap();
                let mut rows: Value = serde_json::from_slice(body).unwrap();
                if mode == "extra-alias" {
                    rows[0]["RepoTags"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("foreign/image:latest"));
                } else {
                    rows[0]["Layers"] = json!(["missing/layer.tar"]);
                }
                *body = serde_json::to_vec(&rows).unwrap();
            }
            "oci-binding" => {
                let key = manifest["images"]["workspace"]["export_digest"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("sha256:")
                    .unwrap()
                    .to_owned();
                let (name, body) = objects
                    .iter_mut()
                    .find(|(name, _)| name == &format!("blobs/sha256/{key}"))
                    .unwrap();
                let mut graph: Value = serde_json::from_slice(body).unwrap();
                graph["config"]["digest"] = manifest["images"]["desktop"]["config_digest"].clone();
                *body = serde_json::to_vec(&graph).unwrap();
                let new_digest = capsule::hash(body);
                *name = format!("blobs/sha256/{new_digest}");
                let old_ref = manifest["images"]["workspace"]["ref"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                let new_ref = format!("ai-phone-bundle/workspace:sha-{new_digest}");
                manifest["images"]["workspace"]["ref"] = json!(new_ref);
                manifest["images"]["workspace"]["export_digest"] =
                    json!(format!("sha256:{new_digest}"));
                for (name, body) in &mut objects {
                    if name == "manifest.json" {
                        let mut rows: Value = serde_json::from_slice(body).unwrap();
                        for row in rows.as_array_mut().unwrap() {
                            if row["RepoTags"][0] == old_ref {
                                row["RepoTags"] = json!([new_ref]);
                            }
                        }
                        *body = serde_json::to_vec(&rows).unwrap();
                    }
                    if name == "index.json" {
                        let mut index: Value = serde_json::from_slice(body).unwrap();
                        for row in index["manifests"].as_array_mut().unwrap() {
                            if row["annotations"]["io.containerd.image.name"]
                                == format!("docker.io/{old_ref}")
                            {
                                row["digest"] = json!(format!("sha256:{new_digest}"));
                                row["annotations"]["io.containerd.image.name"] =
                                    json!(format!("docker.io/{new_ref}"));
                            }
                        }
                        *body = serde_json::to_vec(&index).unwrap();
                    }
                }
            }
            "oci-subject" => {
                let (_, body) = objects
                    .iter_mut()
                    .find(|(name, _)| name == "index.json")
                    .unwrap();
                let mut index: Value = serde_json::from_slice(body).unwrap();
                index["manifests"].as_array_mut().unwrap().push(json!({"annotations":{"io.containerd.manifest.subject":format!("sha256:{}","0".repeat(64))}}));
                *body = serde_json::to_vec(&index).unwrap();
            }
            _ => unreachable!(),
        }
        manifest["extra-preserved"] = json!(true);
        let (payload, _) = capsule::write(home, manifest, &capsule::tar(objects), true);
        let output = invoke(home, &payload);
        assert!(!output.status.success(), "{mode}");
        if mode == "oci-binding" {
            assert_eq!(output.stderr, b"image_oci_binding\n");
        }
        assert!(!home.join("queried").exists());
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    fixture(home, true);
    let (payload, _, _) = capsule::create(home, true, "arm64", "fixture", true);
    let output = invoke(home, &payload);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
