//! Real SIGINT through the initializer; only this executable can be a child tool.
use super::*;
use std::{
    io::{Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
        process::CommandExt,
    },
    process::{Child, Command, Stdio},
    time::Instant,
};

const ROOT: &str = "AI_PHONE_HOME_TEST_ROOT";
const NONCE: &str = "AI_PHONE_HOME_TEST_NONCE";
const ROLE: &str = "AI_PHONE_HOME_TEST_ROLE";
const TARGET: &str = "AI_PHONE_HOME_TEST_TARGET";
const TOOL: &str = "home_identity::signal_tests::owned_command_fixture";
const INITIALIZER: &str = "home_identity::signal_tests::owned_initializer_fixture";

fn fixture_root() -> Option<(PathBuf, String)> {
    let root = std::env::var_os(ROOT)?;
    let root = PathBuf::from(root);
    let meta = directory(&root).unwrap();
    assert_eq!(meta.uid, rustix::process::geteuid().as_raw());
    assert_eq!(meta.mode, 0o700);
    let nonce = std::env::var(NONCE).unwrap();
    uuid::Uuid::parse_str(&nonce).unwrap();
    assert_eq!(fs::read_to_string(root.join("nonce")).unwrap(), nonce);
    Some((root, nonce))
}

#[test]
fn owned_command_fixture() {
    let Some((root, nonce)) = fixture_root() else {
        return;
    };
    let role = std::env::var(ROLE).unwrap();
    let target = PathBuf::from(std::env::var_os(TARGET).unwrap());
    if role == "restore" {
        assert_eq!(target, root.join("home"));
        fs::write(
            root.join("account"),
            format!("1001:20:{}", target.display()),
        )
        .unwrap();
        fs::write(root.join("restored"), b"yes").unwrap();
        return;
    }
    assert_eq!(role, "forward");
    assert_eq!(target.file_name().unwrap(), "absent-home");
    assert_eq!(
        target.parent().unwrap().parent(),
        Some(root.join("run").as_path())
    );
    assert!(!target.exists());
    let socket = UnixListener::bind(root.join("r.sock")).unwrap();
    socket.set_nonblocking(true).unwrap();
    fs::write(
        root.join("account"),
        format!("1001:20:{}", target.display()),
    )
    .unwrap();
    fs::write(root.join("ready"), std::process::id().to_string()).unwrap();
    // A nonce-authenticated private socket releases an orphan on a RED/panic path.
    // No cleanup signal is ever sent to a free PID read from a fixture file.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut beat = 0u64;
    while Instant::now() < deadline {
        fs::write(root.join("beat"), beat.to_string()).unwrap();
        beat += 1;
        if let Ok((stream, _)) = socket.accept() {
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut message = String::new();
            stream.take(64).read_to_string(&mut message).unwrap();
            if message == nonce {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("Owned fixture exceeded its failsafe deadline");
}

struct FakeAccounts {
    root: PathBuf,
    nonce: String,
}

impl process::Runner for FakeAccounts {
    async fn execute(&self, mut request: process::Request) -> Result<process::Output> {
        let program = request.program.to_str().unwrap();
        let args: Vec<_> = request.args.iter().map(|v| v.to_str().unwrap()).collect();
        if program == "getent" {
            assert_eq!(args.len(), 2);
            let text = fs::read_to_string(self.root.join("account"))?;
            let fields: Vec<_> = text.splitn(3, ':').collect();
            assert_eq!(fields.len(), 3);
            let value = match args.as_slice() {
                ["passwd", key] if *key == "ai" || *key == fields[0] => Some(format!(
                    "ai:x:{}:{}::{}:/bin/bash\n",
                    fields[0], fields[1], fields[2]
                )),
                ["passwd", "1001"] => None,
                ["group", "20"] => Some("dialout:x:20:\n".into()),
                _ => panic!("Unmocked lookup; real account tools are forbidden"),
            };
            return Ok(process::Output {
                code: if value.is_some() { 0 } else { 2 },
                stdout: value.unwrap_or_default().into_bytes(),
                stderr: vec![],
            });
        }
        assert_eq!(
            program, "usermod",
            "Only the exact owned fake command is allowed"
        );
        let (role, target) = match args.as_slice() {
            ["--home", path, "--uid", "1001", "--gid", "20", "ai"] => {
                ("forward", PathBuf::from(*path))
            }
            ["--home", path, "ai"] => ("restore", PathBuf::from(*path)),
            _ => panic!("Unexpected account mutation request"),
        };
        assert_eq!(request.timeout, Some(Duration::from_secs(15)));
        assert!(request.input.is_empty() && request.env.is_empty() && request.cwd.is_none());
        // Preserve the product's timeout/cancellation fields while replacing the
        // executable and argv with this same known test executable, never PATH.
        request.program = std::env::current_exe()?.into_os_string();
        request.args = ["--exact", TOOL, "--nocapture"].map(Into::into).into();
        request.env = vec![
            (ROOT.into(), self.root.as_os_str().into()),
            (NONCE.into(), self.nonce.clone().into()),
            (ROLE.into(), role.into()),
            (TARGET.into(), target.into_os_string()),
        ];
        let result = process::Runner::execute(&process::OsRunner, request).await;
        if role == "forward" && result.is_err() {
            let pid = fs::read_to_string(self.root.join("ready"))?.parse::<i32>()?;
            let pid = rustix::process::Pid::from_raw(pid).unwrap();
            // Called by the direct parent before restore creates another child.
            // ECHILD proves the process owner already waited/reaped this leader.
            assert!(matches!(
                rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::NOHANG),
                Err(rustix::io::Errno::CHILD)
            ));
            fs::write(self.root.join("reaped"), b"yes")?;
        }
        result
    }
}

#[tokio::test]
async fn owned_initializer_fixture() {
    let Some((root, nonce)) = fixture_root() else {
        return;
    };
    let home = root.join("home");
    let runtime = root.join("run");
    let runner = FakeAccounts {
        root: root.clone(),
        nonce,
    };
    let identity = |path: &Path| {
        let mut value = directory(path)?;
        if path == home {
            value.uid = 1001;
            value.gid = 20;
        } else if path == runtime || path.parent() == Some(runtime.as_path()) {
            value.uid = 0;
        }
        Ok(value)
    };
    let result = run_with(&home, &runtime, &runner, 0, &identity).await;
    assert!(
        result.is_err(),
        "Cancellation signal must refuse successful initialization"
    );
    fs::write(root.join("result"), result.unwrap_err().to_string()).unwrap();
}

struct OwnedInitializer {
    root: tempfile::TempDir,
    child: Child,
    nonce: String,
}
impl OwnedInitializer {
    fn start() -> Self {
        let root = tempfile::Builder::new().prefix("hi-").tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let nonce = uuid::Uuid::new_v4().to_string();
        fs::write(root.path().join("nonce"), &nonce).unwrap();
        let home = root.path().join("home");
        fs::DirBuilder::new().mode(0o750).create(&home).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.path().join("run"))
            .unwrap();
        fs::write(home.join("private-note"), b"unchanged fixture content").unwrap();
        fs::write(
            root.path().join("account"),
            format!("1000:1000:{}", home.display()),
        )
        .unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", INITIALIZER, "--nocapture"])
            .env(ROOT, root.path())
            .env(NONCE, &nonce)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap();
        Self { root, child, nonce }
    }
    fn wait_file(&mut self, name: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.root.path().join(name).exists() {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "Owned initializer exited before fixture readiness"
            );
            assert!(
                Instant::now() < deadline,
                "Owned fixture readiness timed out"
            );
            std::thread::sleep(Duration::from_millis(15));
        }
    }
    fn wait(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "Interrupted initializer did not finish compensation"
            );
            std::thread::sleep(Duration::from_millis(15));
        }
    }
}
impl Drop for OwnedInitializer {
    fn drop(&mut self) {
        if let Ok(mut socket) = UnixStream::connect(self.root.path().join("r.sock")) {
            let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));
            let _ = socket.write_all(self.nonce.as_bytes());
            let _ = socket.shutdown(std::net::Shutdown::Write);
            let _ = socket.take(64).read_to_end(&mut Vec::new());
        }
        // Only the held direct child handle is killed on failed-test cleanup.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn sigint_during_owned_usermod_reaps_then_restores_without_reusing_cancel_token() {
    cancellation_during_owned_usermod(rustix::process::Signal::INT);
}

#[test]
fn sigterm_during_owned_usermod_reaps_then_restores_without_reusing_cancel_token() {
    cancellation_during_owned_usermod(rustix::process::Signal::TERM);
}

fn cancellation_during_owned_usermod(signal: rustix::process::Signal) {
    let mut fixture = OwnedInitializer::start();
    let home = fixture.root.path().join("home");
    let original = directory(&home).unwrap();
    fixture.wait_file("ready");
    fixture.wait_file("beat");
    let changed = fs::read_to_string(fixture.root.path().join("account")).unwrap();
    assert!(changed.ends_with("/absent-home"));
    let pid = rustix::process::Pid::from_raw(fixture.child.id().try_into().unwrap()).unwrap();
    rustix::process::kill_process(pid, signal).unwrap();
    assert!(
        fixture.wait().success(),
        "{signal:?} killed the initializer before compensation"
    );
    assert!(
        fixture.root.path().join("reaped").is_file(),
        "Owned updater must be reaped before rollback"
    );
    assert!(
        fixture.root.path().join("restored").is_file(),
        "A stale cancellation token must not cancel rollback"
    );
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("account")).unwrap(),
        format!("1001:20:{}", home.display())
    );
    assert_eq!(
        fs::read_dir(fixture.root.path().join("run"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(directory(&home).unwrap(), original);
    assert_eq!(
        fs::read(home.join("private-note")).unwrap(),
        b"unchanged fixture content"
    );
    assert!(
        fs::read_to_string(fixture.root.path().join("result"))
            .unwrap()
            .contains("interrupt")
    );
    let beat = fs::read(fixture.root.path().join("beat")).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        fs::read(fixture.root.path().join("beat")).unwrap(),
        beat,
        "Updater continued after cancellation"
    );
}
