//! Own the PulseAudio daemon independently of desktop applications.
use crate::{Result, process};
use fs2::FileExt;
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    future::Future,
    io,
    path::Path,
    time::Duration,
};
use tokio::{
    process::Command,
    signal::unix::{SignalKind, signal},
};

const RESTART_DELAY: Duration = Duration::from_secs(1);
const STOP_GRACE: Duration = Duration::from_secs(2);

pub async fn run(runtime: &Path, config: &Path) -> Result<()> {
    let Some(_lock) = audio_lock(runtime)? else {
        return Ok(());
    };
    let mut term = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let stop = async {
        tokio::select! {
            value = term.recv() => value.ok_or("audio termination signal stream closed"),
            value = interrupt.recv() => value.ok_or("audio interrupt signal stream closed"),
        }
        .map_err(Into::into)
    };
    supervise(pulse_command(config), stop).await
}

fn audio_lock(runtime: &Path) -> Result<Option<File>> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(runtime.join("audio-supervisor.lock"))?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(file)),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn pulse_command(config: &Path) -> Command {
    let mut file = OsString::from("--file=");
    file.push(config);
    let mut command = Command::new("pulseaudio");
    command
        .args([
            "--daemonize=no",
            "--exit-idle-time=-1",
            "--disallow-exit=yes",
            "--log-target=stderr",
            "-n",
        ])
        .arg(file)
        .env_remove("PULSE_SERVER")
        .kill_on_drop(true);
    command
}

async fn supervise(mut command: Command, stop: impl Future<Output = Result<()>>) -> Result<()> {
    tokio::pin!(stop);
    loop {
        tokio::select! { biased;
            result = &mut stop => return result,
            () = std::future::ready(()) => (),
        }
        let mut child = command.spawn()?;
        let result = tokio::select! { biased;
            result = &mut stop => result.map(|()| true),
            result = process::wait_observed(&child, Duration::from_millis(200)) => result.map(|()| false),
        };
        let cleanup = process::stop_owned(&mut child, false, STOP_GRACE).await;
        if process::preserve(result, cleanup)? {
            return Ok(());
        }
        tokio::select! { biased;
            result = &mut stop => return result,
            () = tokio::time::sleep(RESTART_DELAY) => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write, process::Stdio};

    const CHILD_ENV: &str = "AI_PHONE_AUDIO_FIXTURE_MODE";

    #[tokio::test]
    async fn daemon_fixture() {
        let Ok(mode) = std::env::var(CHILD_ENV) else {
            return;
        };
        let mut signal = (mode == "ignore-term").then(|| signal(SignalKind::terminate()).unwrap());
        let path = std::env::var("AI_PHONE_AUDIO_FIXTURE_LOG").unwrap();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(file, "{}", std::process::id()).unwrap();
        if mode == "exit" {
            std::process::exit(7);
        }
        if let Some(stream) = &mut signal {
            while stream.recv().await.is_some() {}
        }
        std::future::pending::<()>().await;
    }

    fn fixture(log: &Path, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "audio::tests::daemon_fixture", "--nocapture"])
            .env(CHILD_ENV, mode)
            .env("AI_PHONE_AUDIO_FIXTURE_LOG", log)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        command
    }

    async fn wait_lines(path: &Path, expected: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while fs::read_to_string(path).unwrap_or_default().lines().count() < expected {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn pulse_arguments_and_environment_are_preserved_without_shell() {
        let command = pulse_command(Path::new("/fixture/pulse.pa"));
        let command = command.as_std();
        assert_eq!(command.get_program(), "pulseaudio");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "--daemonize=no",
                "--exit-idle-time=-1",
                "--disallow-exit=yes",
                "--log-target=stderr",
                "-n",
                "--file=/fixture/pulse.pa"
            ]
        );
        assert_eq!(
            command
                .get_envs()
                .find(|(key, _)| *key == "PULSE_SERVER")
                .unwrap()
                .1,
            None
        );
    }

    #[tokio::test]
    async fn singleton_lock_does_not_spawn_or_change_existing_lock_identity() {
        const MARKER: &str = "AI_PHONE_AUDIO_LOCK_FIXTURE_MARKER";
        const NONCE: &str = "AI_PHONE_AUDIO_LOCK_FIXTURE_NONCE";
        if let Some(marker) = std::env::var_os(MARKER) {
            let nonce = std::env::var(NONCE).unwrap();
            assert!(!nonce.is_empty());
            singleton_lock_body().await;
            let mut receipt = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(marker)
                .unwrap();
            receipt.write_all(nonce.as_bytes()).unwrap();
            receipt.sync_all().unwrap();
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("completed");
        let nonce = uuid::Uuid::new_v4().to_string();
        assert!(!marker.exists());
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "audio::tests::singleton_lock_does_not_spawn_or_change_existing_lock_identity",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(MARKER, &marker)
            .env(NONCE, &nonce)
            .env("PATH", root.path())
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "isolated lock body failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(marker).unwrap(), nonce);
    }

    async fn singleton_lock_body() {
        use std::os::unix::fs::MetadataExt;
        let root = tempfile::tempdir().unwrap();
        let held = audio_lock(root.path()).unwrap().unwrap();
        let before = held.metadata().unwrap();
        run(root.path(), Path::new("/not-used.pa")).await.unwrap();
        let after = held.metadata().unwrap();
        assert_eq!(
            (before.ino(), before.uid(), before.gid(), before.mode()),
            (after.ino(), after.uid(), after.gid(), after.mode())
        );
        drop(held);
        assert!(audio_lock(root.path()).unwrap().is_some());
    }

    #[test]
    fn missing_runtime_is_not_created() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("missing");
        assert!(audio_lock(&path).is_err());
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn daemon_exit_restarts_after_delay_and_stop_during_backoff_does_not_restart() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("children");
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let start = tokio::time::Instant::now();
        let task = tokio::spawn(supervise(fixture(&log, "exit"), async {
            receiver.await?;
            Ok(())
        }));
        wait_lines(&log, 2).await;
        assert!(start.elapsed() >= Duration::from_millis(1000));
        sender.send(()).unwrap();
        task.await.unwrap().unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert_eq!(fs::read_to_string(log).unwrap().lines().count(), 2);
    }

    #[tokio::test]
    async fn termination_reaps_owned_daemon_including_term_resistant_child() {
        for mode in ["wait", "ignore-term"] {
            let root = tempfile::tempdir().unwrap();
            let log = root.path().join("children");
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(supervise(fixture(&log, mode), async {
                receiver.await?;
                Ok(())
            }));
            wait_lines(&log, 1).await;
            let pid = fs::read_to_string(&log)
                .unwrap()
                .trim()
                .parse::<i32>()
                .unwrap();
            sender.send(()).unwrap();
            tokio::time::timeout(Duration::from_secs(4), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let result = rustix::process::waitid(
                rustix::process::WaitId::Pid(rustix::process::Pid::from_raw(pid).unwrap()),
                rustix::process::WaitIdOptions::EXITED | rustix::process::WaitIdOptions::NOHANG,
            );
            assert_eq!(result.unwrap_err(), rustix::io::Errno::CHILD);
        }
    }
}
