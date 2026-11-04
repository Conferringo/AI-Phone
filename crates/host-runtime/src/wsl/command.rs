//! Bounded native command ownership; output is not included in public errors.
use crate::{Error, Result};
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: u64 = 1024 * 1024;

fn reader(
    mut pipe: impl Read + Send + 'static,
    overflow: Arc<AtomicBool>,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.by_ref()
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > OUTPUT_LIMIT {
            overflow.store(true, Ordering::Relaxed);
        }
        Ok(bytes)
    })
}

fn finish(reader: thread::JoinHandle<std::io::Result<Vec<u8>>>) -> Result<Vec<u8>> {
    let bytes = reader
        .join()
        .map_err(|_| Error::Invalid("wsl_command_output"))??;
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(Error::Invalid("wsl_command_output"));
    }
    Ok(bytes)
}

fn wait(
    child: &mut Child,
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
    overflow: &AtomicBool,
) -> Result<()> {
    let until = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(Error::Invalid("wsl_command_failed"))
                };
            }
            Err(error) => return Err(Error::Io(error)),
            Ok(None) => {}
        }
        if cancelled() {
            return Err(Error::Cancelled);
        }
        if overflow.load(Ordering::Relaxed) {
            return Err(Error::Invalid("wsl_command_output"));
        }
        if Instant::now() >= until {
            return Err(Error::Invalid("wsl_command_unknown"));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn collected(
    outcome: Result<()>,
    output: Result<Vec<u8>>,
    diagnostic: Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    if let Err(error @ (Error::Cancelled | Error::Cleanup { .. })) = outcome {
        return Err(error);
    }
    // A child may fail while its output reader is discovering the size limit.
    // Select the bounded-output error deterministically after both readers join.
    let bytes = output?;
    diagnostic?;
    outcome?;
    Ok(bytes)
}

pub(super) fn run(
    command: Command,
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>> {
    execute(command, None, timeout, cancelled)
}

pub(super) fn run_input(
    command: Command,
    input: &[u8],
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>> {
    execute(command, Some(input.to_vec()), timeout, cancelled)
}

fn execute(
    mut command: Command,
    input: Option<Vec<u8>>,
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let overflow = Arc::new(AtomicBool::new(false));
    let input = input.map(|bytes| {
        let mut pipe = child.stdin.take().expect("piped child stdin");
        thread::spawn(move || pipe.write_all(&bytes))
    });
    // Both handles are guaranteed by the requested Stdio::piped configuration.
    let output = reader(
        child.stdout.take().expect("piped child stdout"),
        overflow.clone(),
    );
    let error = reader(
        child.stderr.take().expect("piped child stderr"),
        overflow.clone(),
    );
    let outcome = wait(&mut child, timeout, cancelled, &overflow);
    let outcome = reap(&mut child, outcome);
    let output = finish(output);
    let diagnostic = finish(error);
    let written = input
        .map(|writer| {
            writer
                .join()
                .map_err(|_| Error::Invalid("wsl_command_input"))?
                .map_err(Error::Io)
        })
        .transpose();
    let bytes = collected(outcome, output, diagnostic)?;
    written?;
    Ok(bytes)
}

fn reap(child: &mut Child, outcome: Result<()>) -> Result<()> {
    let killed = if outcome.is_err() {
        child.kill()
    } else {
        Ok(())
    };
    let reaped = child.wait();
    if let Err(cleanup) = reaped {
        return Err(Error::Cleanup {
            primary: Box::new(
                outcome
                    .err()
                    .unwrap_or(Error::Invalid("wsl_command_unknown")),
            ),
            cleanup,
        });
    }
    if let Err(cleanup) = killed
        && cleanup.kind() != std::io::ErrorKind::InvalidInput
    {
        return Err(Error::Cleanup {
            primary: Box::new(
                outcome
                    .err()
                    .unwrap_or(Error::Invalid("wsl_command_unknown")),
            ),
            cleanup,
        });
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture() {
        let Ok(mode) = std::env::var("AI_PHONE_WSL_COMMAND_FIXTURE") else {
            return;
        };
        if mode == "failure" {
            std::process::exit(7);
        }
        if mode == "large" {
            use std::io::Write;
            std::io::stdout()
                .write_all(&vec![b'x'; OUTPUT_LIMIT as usize + 1])
                .unwrap();
        }
        if mode == "success" {
            println!("owned-wsl-output");
            return;
        }
        fs_ready();
        loop {
            thread::park();
        }
    }

    fn fs_ready() {
        if let Ok(path) = std::env::var("AI_PHONE_WSL_READY_FIXTURE") {
            std::fs::write(path, b"ready").unwrap();
        }
    }
    fn command(mode: &str) -> Command {
        let mut value = Command::new(std::env::current_exe().unwrap());
        value
            .args(["--exact", "wsl::command::tests::fixture", "--nocapture"])
            .env("AI_PHONE_WSL_COMMAND_FIXTURE", mode);
        value
    }
    #[test]
    fn native_child_success_failure_and_output_limit_are_observable() {
        let output = run(command("success"), Duration::from_secs(5), &|| false).unwrap();
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("owned-wsl-output")
        );
        assert_eq!(
            run(command("failure"), Duration::from_secs(5), &|| false)
                .unwrap_err()
                .code(),
            "wsl_command_failed"
        );
        assert_eq!(
            run(command("large"), Duration::from_secs(5), &|| false)
                .unwrap_err()
                .code(),
            "wsl_command_output"
        );
    }
    #[test]
    fn cancellation_and_timeout_reap_the_owned_child_and_join_output_readers() {
        let folder = tempfile::tempdir().unwrap();
        let ready = folder.path().join("ready");
        let mut child = command("wait");
        child.env("AI_PHONE_WSL_READY_FIXTURE", &ready);
        assert!(matches!(
            run(child, Duration::from_secs(5), &|| ready.is_file()),
            Err(Error::Cancelled)
        ));
        assert!(ready.is_file());
        assert_eq!(
            run(command("wait"), Duration::from_millis(100), &|| false)
                .unwrap_err()
                .code(),
            "wsl_command_unknown"
        );
    }
}
