use std::{fs, process::Command};

#[test]
fn native_service_control_contract_needs_no_interpreter_or_installation() {
    let fixture = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .arg("--service-supervision-contract")
        .env_clear()
        .env("PATH", fixture.path())
        .current_dir(fixture.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"ai-phone-linux-supervision-v2\n");
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read_dir(fixture.path()).unwrap().count(), 0);
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use fs2::FileExt;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{
        fs::OpenOptions,
        io::Write,
        os::unix::fs::OpenOptionsExt,
        process::{Child, Output, Stdio},
        time::{Duration, Instant},
    };

    struct Fixture {
        root: tempfile::TempDir,
    }
    impl Fixture {
        fn new() -> Self {
            Self {
                root: tempfile::tempdir().unwrap(),
            }
        }
        fn path(&self) -> &std::path::Path {
            self.root.path()
        }
        fn request(&self, mode: &str) -> Value {
            json!({"command":[env!("CARGO_BIN_EXE_service-test-worker"),mode,"PRIVATE_ARGUMENT_DO_NOT_REPORT"],
                "environment":{"AI_PHONE_SERVICE_TEST_ROOT":self.path(),"PATH":"PRIVATE_PATH_DO_NOT_REPORT","LANG":"中文"}})
        }
        fn command(&self, operation: &str) -> Command {
            let mut command = Command::new(env!("CARGO_BIN_EXE_host-resources"));
            command
                .args(["--service-control"])
                .arg(self.path())
                .args(["fixture", operation])
                .env_clear()
                .env("PATH", self.path())
                .current_dir(self.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command
        }
        fn invoke(&self, operation: &str, request: Option<&Value>) -> Output {
            let mut child = self.command(operation).spawn().unwrap();
            if let Some(request) = request {
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(&serde_json::to_vec(request).unwrap())
                    .unwrap();
            } else {
                drop(child.stdin.take());
            }
            finish(child)
        }
        fn success(&self, operation: &str, request: Option<&Value>) -> Value {
            let output = self.invoke(operation, request);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                output.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            serde_json::from_slice(&output.stdout).unwrap()
        }
        fn worker(&self, previous: Option<u32>) -> u32 {
            wait(|| {
                fs::read_to_string(self.path().join("worker.pid"))
                    .ok()
                    .and_then(|text| text.parse::<u32>().ok())
                    .filter(|pid| {
                        Some(*pid) != previous
                            && std::path::Path::new(&format!("/proc/{pid}")).exists()
                    })
            })
        }
        fn owned_status(&self) -> Value {
            self.success("status", None)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let output = self.invoke("stop", None);
            assert!(
                output.status.success(),
                "owned fixture cleanup failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    fn finish(mut child: Child) -> Output {
        let until = Instant::now() + Duration::from_secs(12);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= until {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("native service controller did not return");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.wait_with_output().unwrap()
    }
    fn wait<T>(mut observe: impl FnMut() -> Option<T>) -> T {
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(value) = observe() {
                return value;
            }
            assert!(
                Instant::now() < until,
                "owned process did not reach the expected state"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn absent_status_and_stop_do_not_create_installation_state() {
        let f = Fixture::new();
        assert!(f.owned_status().is_null());
        assert_eq!(f.success("stop", None), false);
        assert_eq!(fs::read_dir(f.path()).unwrap().count(), 0);
    }

    #[test]
    fn real_start_resume_recovery_and_stop_preserve_user_files_and_release_the_lock() {
        let f = Fixture::new();
        fs::write(f.path().join("work.txt"), b"keep").unwrap();
        let request = f.request("loop");
        assert_eq!(f.success("start", Some(&request)), true);
        let worker = f.worker(None);
        let first = f.owned_status();
        assert_eq!(
            fs::read(f.path().join("worker.stdin")).unwrap(),
            b"/dev/null"
        );
        for row in fs::read_dir(format!("/proc/{worker}/fd")).unwrap() {
            assert_ne!(
                fs::read_link(row.unwrap().path()).unwrap(),
                f.path().join("data/services/fixture.lock")
            );
        }
        let spec = fs::read(f.path().join("data/services/fixture.spec.json")).unwrap();
        assert_eq!(f.success("start", Some(&request)), false);
        assert_eq!(f.owned_status()["pid"], first["pid"]);
        assert_eq!(
            fs::read(f.path().join("data/services/fixture.spec.json")).unwrap(),
            spec
        );
        rustix::process::kill_process(
            rustix::process::Pid::from_raw(worker as i32).unwrap(),
            rustix::process::Signal::KILL,
        )
        .unwrap();
        let replacement = f.worker(Some(worker));
        assert_ne!(worker, replacement);
        assert_eq!(f.owned_status()["pid"], first["pid"]);
        assert_eq!(f.success("stop", None), true);
        assert!(f.owned_status().is_null());
        assert!(!std::path::Path::new(&format!("/proc/{replacement}")).exists());
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(f.path().join("data/services/fixture.lock"))
            .unwrap();
        FileExt::try_lock_exclusive(&lock).unwrap();
        assert_eq!(fs::read(f.path().join("work.txt")).unwrap(), b"keep");
    }

    #[test]
    fn concurrent_starts_have_one_supervisor_and_same_spec_does_not_restart_it() {
        let f = Fixture::new();
        let request = f.request("loop");
        let results = std::thread::scope(|scope| {
            let jobs: Vec<_> = (0..4)
                .map(|_| scope.spawn(|| f.success("start", Some(&request))))
                .collect();
            jobs.into_iter()
                .map(|job| job.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|value| **value == true).count(), 1);
        f.worker(None);
        let pid = f.owned_status()["pid"].clone();
        assert_eq!(f.success("start", Some(&request)), false);
        assert_eq!(f.owned_status()["pid"], pid);
    }

    #[test]
    fn earlier_recorded_json_encoding_reuses_the_live_supervisor_without_rewriting_spec() {
        let f = Fixture::new();
        let request = f.request("loop");
        let folder = f.path().join("data/services");
        fs::create_dir_all(&folder).unwrap();
        let bytes = serde_json::to_vec_pretty(&request).unwrap();
        fs::write(folder.join("fixture.spec.json"), &bytes).unwrap();
        let identity = format!("{:x}", Sha256::digest(&bytes));
        let logical = format!("{:x}", Sha256::digest(b"earlier-recorded-encoding"));
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(folder.join("fixture.lock"))
            .unwrap();
        FileExt::try_lock_exclusive(&lock).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_host-resources"))
            .arg("--supervise")
            .arg(f.path())
            .args(["fixture", &logical, &identity])
            .current_dir(f.path())
            .stdin(Stdio::from(lock))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        f.worker(None);
        wait(|| f.owned_status().get("pid").cloned());
        assert_eq!(f.success("start", Some(&request)), false);
        assert_eq!(f.owned_status()["pid"], pid);
        assert_eq!(fs::read(folder.join("fixture.spec.json")).unwrap(), bytes);
        f.success("stop", None);
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn changed_configuration_stays_rejected_and_diagnostics_never_expose_values() {
        let f = Fixture::new();
        let original = f.request("loop");
        f.success("start", Some(&original));
        f.worker(None);
        let pid = f.owned_status()["pid"].clone();
        let spec = f.path().join("data/services/fixture.spec.json");
        let saved = fs::read(&spec).unwrap();
        let mut requested = original.clone();
        requested["environment"]["PATH"] = json!("PRIVATE_NEW_VALUE_DO_NOT_REPORT");
        requested["environment"]["PRIVATE_UNKNOWN_KEY_DO_NOT_REPORT"] =
            json!("PRIVATE_NEW_VALUE_DO_NOT_REPORT");
        let output = f.invoke("start", Some(&requested));
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("service_configuration_changed"));
        let report = error
            .lines()
            .find_map(|line| line.strip_prefix("AI_PHONE_SERVICE_SPEC "))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(report).unwrap(),
            json!({"format":1,"service":"other","evidence":"verified_spec","command_changed":false,"environment_fields":["PATH"],"other_environment_changes":1})
        );
        for secret in [
            "PRIVATE_PATH_DO_NOT_REPORT",
            "PRIVATE_ARGUMENT_DO_NOT_REPORT",
            "PRIVATE_UNKNOWN_KEY_DO_NOT_REPORT",
            "PRIVATE_NEW_VALUE_DO_NOT_REPORT",
        ] {
            assert!(!error.contains(secret));
        }
        assert_eq!(fs::read(&spec).unwrap(), saved);
        assert_eq!(f.owned_status()["pid"], pid);
        for body in [b"{PRIVATE_PATH_DO_NOT_REPORT".as_slice(), b"[]"] {
            fs::write(&spec, body).unwrap();
            let output = f.invoke("start", Some(&requested));
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(error.contains("unverified_spec"));
            assert!(!error.contains("PRIVATE_"));
        }
    }

    #[test]
    fn stopping_waits_for_process_exit_even_after_receipt_disappears() {
        let f = Fixture::new();
        f.success("start", Some(&f.request("hold")));
        let worker = f.worker(None);
        let mut stopping = f.command("stop").spawn().unwrap();
        drop(stopping.stdin.take());
        wait(|| f.path().join("term-seen").exists().then_some(()));
        fs::remove_file(f.path().join("data/services/fixture.json")).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        assert!(stopping.try_wait().unwrap().is_none());
        fs::write(f.path().join("release"), b"go").unwrap();
        let result = finish(stopping);
        assert!(result.status.success());
        assert!(!std::path::Path::new(&format!("/proc/{worker}")).exists());
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(f.path().join("data/services/fixture.lock"))
            .unwrap();
        FileExt::try_lock_exclusive(&lock).unwrap();
    }

    #[test]
    fn restart_backoff_and_term_resistant_children_obey_existing_stop_contract() {
        for mode in ["exit", "ignore"] {
            let f = Fixture::new();
            f.success("start", Some(&f.request(mode)));
            let worker = if mode == "exit" {
                wait(|| f.path().join("exits").exists().then_some(()));
                None
            } else {
                Some(f.worker(None))
            };
            assert_eq!(f.success("stop", None), true);
            if mode == "exit" {
                assert_eq!(
                    fs::read_to_string(f.path().join("exits")).unwrap(),
                    "exit\n"
                );
            } else {
                assert!(!std::path::Path::new(&format!("/proc/{}", worker.unwrap())).exists());
            }
        }
    }

    #[test]
    fn invalid_requests_never_publish_a_spec_or_launch_a_service() {
        let f = Fixture::new();
        for request in [
            json!([]),
            json!({"command":[],"environment":{}}),
            json!({"command":["worker"],"environment":{"PATH":null}}),
            json!({"command":["worker\0bad"],"environment":{}}),
        ] {
            let output = f.invoke("start", Some(&request));
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
            assert_eq!(fs::read_dir(f.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn a_held_lock_with_no_authenticated_owner_is_not_replayed_or_overwritten() {
        let f = Fixture::new();
        let folder = f.path().join("data/services");
        fs::create_dir_all(&folder).unwrap();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(folder.join("fixture.lock"))
            .unwrap();
        FileExt::try_lock_exclusive(&lock).unwrap();
        let output = f.invoke("start", Some(&f.request("loop")));
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("service_owner_unknown")
        );
        assert!(!folder.join("fixture.spec.json").exists());
        assert!(!f.path().join("worker.pid").exists());
    }

    #[test]
    fn forged_or_reused_receipts_cannot_stop_a_real_unrelated_process() {
        let f = Fixture::new();
        let mut child = Command::new(env!("CARGO_BIN_EXE_service-test-worker"))
            .arg("loop")
            .env_clear()
            .env("AI_PHONE_SERVICE_TEST_ROOT", f.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        f.worker(None);
        let stat = fs::read_to_string(format!("/proc/{}/stat", child.id())).unwrap();
        let ticks = stat
            .rsplit_once(')')
            .unwrap()
            .1
            .split_whitespace()
            .nth(19)
            .unwrap();
        let folder = f.path().join("data/services");
        fs::create_dir_all(&folder).unwrap();
        let value = json!({"format":1,"supervisor":"rust-v1","program":env!("CARGO_BIN_EXE_host-resources"),"pid":child.id(),"start_ticks":ticks,"root":f.path(),"spec_sha256":"fake","spec_content_sha256":"fake"});
        fs::write(
            folder.join("fixture.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        let status = f.owned_status();
        let stopped = f.success("stop", None);
        let still_alive = child.try_wait().unwrap().is_none();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(status.is_null());
        assert_eq!(stopped, false);
        assert!(still_alive);
    }

    #[test]
    fn closed_diagnostic_output_cannot_turn_a_configuration_conflict_into_success() {
        use std::os::{fd::OwnedFd, unix::net::UnixStream};
        let f = Fixture::new();
        let original = f.request("loop");
        f.success("start", Some(&original));
        f.worker(None);
        let pid = f.owned_status()["pid"].clone();
        let mut changed = original;
        changed["environment"]["PATH"] = json!("changed-private-path");
        let (reader, writer) = UnixStream::pair().unwrap();
        drop(reader);
        let descriptor: OwnedFd = writer.into();
        let mut child = f
            .command("start")
            .stderr(Stdio::from(descriptor))
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&changed).unwrap())
            .unwrap();
        let output = finish(child);
        assert!(!output.status.success());
        assert_eq!(f.owned_status()["pid"], pid);
    }
}
