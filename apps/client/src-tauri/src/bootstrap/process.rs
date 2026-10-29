//! Child-process supervision and installation progress receipts.
use super::{ManagerState, Progress};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};

fn terminate(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill")
            .args(["-TERM", "--", &format!("-{}", child.id())])
            .status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn parse_progress(event: &str) -> Result<Progress, String> {
    let mut progress: Progress =
        serde_json::from_str(event).map_err(|_| "invalid_install_checkpoint")?;
    if progress.state == "completed" {
        progress.state = "running".into();
    }
    Ok(progress)
}

fn read_events(
    manager: &ManagerState,
    output: impl Read,
    folder: &Path,
    log: &Path,
) -> Result<Option<Progress>, String> {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(log)
        .map_err(|_| "process_output_unavailable")?;
    let mut ready = None;
    for line in BufReader::new(output).lines() {
        let line = line.map_err(|_| "process_output_unavailable")?;
        if let Some(event) = line
            .strip_prefix("AI_PHONE_SETUP ")
            .filter(|value| value.len() <= 150000)
        {
            let progress = parse_progress(event)?;
            if progress.state == "ready" {
                ready = Some(progress);
            } else {
                ready = None;
                manager.publish(folder, progress)?;
            }
        } else {
            writeln!(file, "{line}").map_err(|_| "process_output_unavailable")?;
        }
    }
    Ok(ready)
}

fn wait_for_installer(
    manager: &ManagerState,
    child: &mut std::process::Child,
    errors: &std::sync::mpsc::Receiver<String>,
) -> Result<(), String> {
    loop {
        if let Ok(error) = errors.try_recv() {
            terminate(child);
            return Err(error);
        }
        if manager.check_cancelled().is_err() {
            terminate(child);
            return Err("installation_cancelled".into());
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err("installation_failed".into()),
            Ok(None) => std::thread::sleep(Duration::from_millis(150)),
            Err(_) => {
                terminate(child);
                return Err("installation_failed".into());
            }
        }
    }
}

fn spawn(
    manager: &ManagerState,
    mut command: Command,
    log: &Path,
) -> Result<(std::process::Child, std::process::ChildStdout), String> {
    manager.check_cancelled()?;
    let error_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::{fs::PermissionsExt, process::CommandExt};
        fs::set_permissions(log, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        command.process_group(0);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(error_file)
        .spawn()
        .map_err(|_| "process_start_failed")?;
    *manager.input.lock().map_err(|_| "installer_state")? = child.stdin.take();
    let output = child.stdout.take().ok_or("process_output_unavailable")?;
    Ok((child, output))
}

pub(super) fn execute(
    manager: &Arc<ManagerState>,
    command: Command,
    folder: &Path,
) -> Result<(), String> {
    let log = folder.join("install.log");
    let (mut child, output) = spawn(manager, command, &log)?;
    let state = manager.clone();
    let reader_folder = folder.to_owned();
    let log = log.to_owned();
    let (error_sender, errors) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let result = read_events(&state, output, &reader_folder, &log);
        if let Err(error) = &result {
            let _ = error_sender.send(error.clone());
        }
        result
    });
    let result = wait_for_installer(manager, &mut child, &errors);
    let completed = reader.join().map_err(|_| "process_output_unavailable");
    *manager.input.lock().map_err(|_| "installer_state")? = None;
    result?;
    // A ready line alone is not a successful installer exit.
    if let Some(progress) = completed?? {
        manager.publish(folder, progress)?;
    }
    Ok(())
}
