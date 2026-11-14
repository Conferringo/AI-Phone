//! Each FFmpeg capture belongs to a live input lease, never to the desktop session.
use crate::{Error, Result, process};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use std::{
    future::Future,
    os::fd::{AsFd, OwnedFd},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    process::Command,
    signal::unix::{SignalKind, signal},
};

const LEASE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(200);
const STOP_GRACE: Duration = Duration::from_secs(1);
const AUDIO: &[&str] = &[
    "-f",
    "pulse",
    "-i",
    "aiphone.monitor",
    "-ac",
    "2",
    "-ar",
    "48000",
    "-f",
    "s16le",
    "pipe:1",
];
const SCREEN: &[&str] = &[
    "-f",
    "x11grab",
    "-framerate",
    "12",
    "-video_size",
    "1280x800",
    "-draw_mouse",
    "1",
    "-i",
    ":0",
    "-an",
    "-c:v",
    "mjpeg",
    "-q:v",
    "5",
    "-threads",
    "1",
    "-f",
    "image2pipe",
    "pipe:1",
];

pub async fn run(mode: &str) -> Result<()> {
    let command = capture_command(mode)?;
    let input = std::io::stdin().as_fd().try_clone_to_owned()?;
    let mut term = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let stop = async {
        tokio::select! {
            value = term.recv() => value.ok_or("capture termination signal stream closed"),
            value = interrupt.recv() => value.ok_or("capture interrupt signal stream closed"),
        }
        .map_err(Into::into)
    };
    capture(command, input, LEASE, stop).await
}

fn capture_command(mode: &str) -> Result<Command> {
    let args = match mode {
        "audio" => AUDIO,
        "screen" => SCREEN,
        _ => return Err("unknown capture mode".into()),
    };
    let mut command = Command::new("ffmpeg");
    command
        .args(["-nostdin", "-hide_banner", "-loglevel", "error"])
        .args(args)
        .stdin(Stdio::null())
        .process_group(0)
        .kill_on_drop(true);
    Ok(command)
}

struct LeaseStop(Arc<AtomicBool>);
impl Drop for LeaseStop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

// A bounded poll, unlike Tokio stdin's uncancellable blocking read, can always join.
fn watch_input(input: OwnedFd, stopped: &AtomicBool, lease: Duration) -> Result<()> {
    let mut deadline = Instant::now() + lease;
    while !stopped.load(Ordering::Acquire) {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(());
        };
        if input_ready(&input, remaining.min(POLL))? {
            let mut bytes = [0u8; 4096];
            if rustix::io::read(&input, &mut bytes[..])? == 0 {
                return Ok(());
            }
            deadline = Instant::now() + lease;
        }
    }
    Ok(())
}

fn input_ready(input: &OwnedFd, timeout: Duration) -> Result<bool> {
    let mut fds = [PollFd::new(input, PollFlags::IN)];
    let timeout = Timespec::try_from(timeout)?;
    match poll(&mut fds, Some(&timeout)) {
        Ok(0) | Err(rustix::io::Errno::INTR) => Ok(false),
        Ok(_) if fds[0].revents().contains(PollFlags::NVAL) => {
            Err("capture input descriptor is invalid".into())
        }
        Ok(_) => Ok(true),
        Err(error) => Err(error.into()),
    }
}

fn watcher_result(result: std::result::Result<Result<()>, tokio::task::JoinError>) -> Result<()> {
    result.map_err(Error::from)?
}

async fn capture(
    mut command: Command,
    input: OwnedFd,
    lease: Duration,
    stop: impl Future<Output = Result<()>>,
) -> Result<()> {
    let mut child = command.spawn()?;
    let mut group = process::GroupGuard::for_child(&child)?;
    let stopped = Arc::new(AtomicBool::new(false));
    let _stop_on_drop = LeaseStop(stopped.clone());
    let worker_stop = stopped.clone();
    let mut watcher = tokio::task::spawn_blocking(move || watch_input(input, &worker_stop, lease));
    let mut joined = false;
    let primary = tokio::select! { biased;
        result = stop => result,
        result = &mut watcher => { joined = true; watcher_result(result) },
        result = process::wait_observed(&child, POLL) => result,
    };
    stopped.store(true, Ordering::Release);
    let cleanup = process::stop_group(&mut child, &mut group, STOP_GRACE).await;
    let joined = if joined {
        Ok(())
    } else {
        watcher_result(watcher.await)
    };
    process::preserve(process::preserve(primary, cleanup), joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        os::unix::net::UnixStream,
        path::Path,
    };

    const FIXTURE_ENV: &str = "AI_PHONE_MEDIA_FIXTURE_MODE";

    #[tokio::test]
    async fn capture_fixture() {
        let Ok(mode) = std::env::var(FIXTURE_ENV) else {
            return;
        };
        let mut term = signal(SignalKind::terminate()).unwrap();
        let path = std::env::var("AI_PHONE_MEDIA_FIXTURE_LOG").unwrap();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(file, "{}", std::process::id()).unwrap();
        if mode == "exit" {
            std::process::exit(6);
        }
        // Keep the leader alive through TERM so the KILL fallback is exercised.
        loop {
            let _ = term.recv().await;
        }
    }

    fn fixture(log: &Path, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "media::tests::capture_fixture", "--nocapture"])
            .env(FIXTURE_ENV, mode)
            .env("AI_PHONE_MEDIA_FIXTURE_LOG", log)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .kill_on_drop(true);
        command
    }

    fn pair() -> (OwnedFd, UnixStream) {
        let (read, write) = UnixStream::pair().unwrap();
        (read.into(), write)
    }

    async fn wait_pid(path: &Path) -> i32 {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(value) = fs::read_to_string(path)
                    && let Ok(pid) = value.trim().parse::<i32>()
                {
                    return pid;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap()
    }

    fn assert_reaped(pid: i32) {
        let flags = rustix::process::WaitIdOptions::EXITED | rustix::process::WaitIdOptions::NOHANG;
        let result = rustix::process::waitid(
            rustix::process::WaitId::Pid(rustix::process::Pid::from_raw(pid).unwrap()),
            flags,
        );
        assert_eq!(result.unwrap_err(), rustix::io::Errno::CHILD);
    }

    #[test]
    fn exact_pcm_and_screen_arguments_and_unknown_mode_contract() {
        for (mode, args) in [("audio", AUDIO), ("screen", SCREEN)] {
            let command = capture_command(mode).unwrap();
            assert_eq!(command.as_std().get_program(), "ffmpeg");
            let expected = ["-nostdin", "-hide_banner", "-loglevel", "error"]
                .into_iter()
                .chain(args.iter().copied())
                .collect::<Vec<_>>();
            assert_eq!(command.as_std().get_args().collect::<Vec<_>>(), expected);
        }
        assert!(capture_command("other").is_err());
    }

    #[test]
    fn heartbeat_extends_lease_and_eof_releases_without_blocked_read() {
        let (input, mut output) = pair();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let start = Instant::now();
        let task = std::thread::spawn(move || {
            watch_input(input, &worker_stop, Duration::from_millis(300))
        });
        for _ in 0..3 {
            std::thread::sleep(Duration::from_millis(150));
            output.write_all(b"h").unwrap();
        }
        assert!(!task.is_finished());
        drop(output);
        task.join().unwrap().unwrap();
        assert!(start.elapsed() < Duration::from_millis(800));
    }

    #[test]
    fn lost_heartbeat_and_cancellation_join_a_bounded_input_watcher() {
        let (input, _output) = pair();
        let stopped = Arc::new(AtomicBool::new(false));
        let start = Instant::now();
        watch_input(input, &stopped, Duration::from_millis(250)).unwrap();
        assert!(start.elapsed() >= Duration::from_millis(250));
        let (input, _output) = pair();
        let worker_stop = stopped.clone();
        let task = std::thread::spawn(move || watch_input(input, &worker_stop, LEASE));
        stopped.store(true, Ordering::Release);
        task.join().unwrap().unwrap();
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn disconnect_expiry_and_signal_reap_only_the_owned_capture() {
        for mode in ["disconnect", "expiry", "signal", "exit"] {
            let root = tempfile::tempdir().unwrap();
            let log = root.path().join("child");
            let (input, output) = pair();
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let daemon = if mode == "exit" { "exit" } else { "wait" };
            let task = tokio::spawn(capture(
                fixture(&log, daemon),
                input,
                Duration::from_millis(350),
                async {
                    receiver.await?;
                    Ok(())
                },
            ));
            let pid = wait_pid(&log).await;
            if mode == "signal" {
                sender.send(()).unwrap();
            }
            if mode == "disconnect" {
                drop(output);
            }
            tokio::time::timeout(Duration::from_secs(3), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_reaped(pid);
        }
    }
}
