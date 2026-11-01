//! CLI lease/cleanup tests use only this test executable as the fake FFmpeg.
//! They do not exercise real PCM, X11, PulseAudio, Docker, or a user's desktop.
use ai_phone_guest_runtime::Result;
use std::{
    fs,
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const MODE: &str = "AI_PHONE_MEDIA_CONTRACT_MODE";
const ROOT: &str = "AI_PHONE_MEDIA_CONTRACT_ROOT";

// A private socket also lets a failing test release its own orphan fixture without
// signalling a PID that might have been reaped/reused by the real product owner.
#[tokio::test]
async fn media_child_fixture() {
    let Ok(mode) = std::env::var(MODE) else {
        return;
    };
    let root = PathBuf::from(std::env::var(ROOT).unwrap());
    let _term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    let listener = UnixListener::bind(root.join(format!("{mode}.sock"))).unwrap();
    listener.set_nonblocking(true).unwrap();
    fs::write(
        root.join(format!("{mode}.pid")),
        std::process::id().to_string(),
    )
    .unwrap();
    if mode == "leader" {
        let mut child = fixture_command(&root, "descendant").spawn().unwrap();
        wait_file(&root.join("descendant.pid"), Duration::from_secs(4));
        assert!(child.try_wait().unwrap().is_none());
        std::process::exit(0);
    }
    loop {
        fs::write(
            root.join(format!("{mode}.beat")),
            format!("{:?}", Instant::now()),
        )
        .unwrap();
        if listener.accept().is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn fixture_command(root: &Path, mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "media_child_fixture", "--nocapture"])
        .env(MODE, mode)
        .env(ROOT, root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn wait_file(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "fixture did not create {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn wait_exit(child: &mut Child, timeout: Duration) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "owned capture did not exit");
        std::thread::sleep(Duration::from_millis(15));
    }
}

struct Capture {
    root: tempfile::TempDir,
    child: Child,
}

impl Capture {
    fn start(mode: &str) -> Self {
        let root = tempfile::Builder::new().prefix("mg-").tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .replace('\'', "'\\''");
        let shim = bin.join("ffmpeg");
        fs::write(
            &shim,
            format!("#!/bin/sh\nexec '{executable}' --exact media_child_fixture --nocapture\n"),
        )
        .unwrap();
        fs::set_permissions(&shim, fs::Permissions::from_mode(0o700)).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_desktop-bridge"))
            .args(["capture", "audio"])
            .env("PATH", bin)
            .env(MODE, mode)
            .env(ROOT, root.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap();
        let result = Self { root, child };
        wait_file(
            &result.root.path().join(format!("{mode}.pid")),
            Duration::from_secs(4),
        );
        result
    }

    fn wait(&mut self) {
        assert!(wait_exit(&mut self.child, Duration::from_secs(8)).success());
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        for role in ["leader", "descendant", "capture"] {
            let _ = UnixStream::connect(self.root.path().join(format!("{role}.sock")));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        // Child is a direct owned handle, never a free PID or global process group.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn actual_cli_eof_lost_heartbeat_and_term_reap_the_capture() -> Result<()> {
    for reason in ["eof", "heartbeat", "term"] {
        let mut capture = Capture::start("capture");
        match reason {
            "eof" => drop(capture.child.stdin.take()),
            "term" => rustix::process::kill_process(
                rustix::process::Pid::from_raw(i32::try_from(capture.child.id())?).unwrap(),
                rustix::process::Signal::TERM,
            )?,
            _ => (),
        }
        capture.wait();
        let path = capture.root.path().join("capture.beat");
        let before = fs::read(&path)?;
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(
            fs::read(path)?,
            before,
            "capture continued after its lease owner exited"
        );
    }
    Ok(())
}

#[test]
fn early_leader_exit_reaps_its_descendant_without_signalling_an_external_sentinel() -> Result<()> {
    let sentinel_root = tempfile::Builder::new().prefix("ms-").tempdir()?;
    let mut sentinel = fixture_command(sentinel_root.path(), "sentinel");
    sentinel.process_group(0);
    let mut sentinel = Sentinel(sentinel.spawn()?);
    let sentinel_beat = sentinel_root.path().join("sentinel.beat");
    wait_file(&sentinel_beat, Duration::from_secs(4));
    let mut capture = Capture::start("leader");
    let descendant_beat = capture.root.path().join("descendant.beat");
    wait_file(&descendant_beat, Duration::from_secs(4));
    capture.wait();
    let before_descendant = fs::read(&descendant_beat)?;
    let before_sentinel = fs::read(&sentinel_beat)?;
    std::thread::sleep(Duration::from_millis(180));
    assert_eq!(
        fs::read(descendant_beat)?,
        before_descendant,
        "orphan capture remained active"
    );
    assert!(
        sentinel.0.try_wait()?.is_none(),
        "unrelated sentinel was signalled"
    );
    assert_ne!(fs::read(sentinel_beat)?, before_sentinel);
    drop(UnixStream::connect(
        sentinel_root.path().join("sentinel.sock"),
    )?);
    assert!(wait_exit(&mut sentinel.0, Duration::from_secs(4)).success());
    Ok(())
}

struct Sentinel(Child);
impl Drop for Sentinel {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
