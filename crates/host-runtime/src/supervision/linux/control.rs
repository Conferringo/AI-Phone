//! Native start/observe/stop. Version-one running supervisors retain their PID and disk state.
mod state;

use super::{Spec, service_name};
use crate::{Error, Result, filesystem};
use fs2::FileExt;
use rustix::process::{Pid, Signal, kill_process};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::process::Command;

const KNOWN_SERVICES: [&str; 7] = [
    "emulator",
    "runner",
    "media",
    "assistant",
    "channels",
    "web",
    "lan",
];
const ENV_FIELDS: [&str; 6] = [
    "PATH",
    "JAVA_HOME",
    "AI_PHONE_JAVA_HOME",
    "AI_PHONE_SDK_ROOT",
    "AI_PHONE_PROJECT_ROOT",
    "AI_PHONE_DATA_ROOT",
];

fn input() -> Result<Spec> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Invalid("service_spec"));
    }
    let spec: Spec = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("service_spec"))?;
    spec.validate()?;
    Ok(spec)
}

fn saved_spec(root: &Path, name: &str, receipt: &state::Receipt) -> Option<Spec> {
    let folder = state::directory(&root.join("data/services")).ok()?;
    let bytes = state::private_bytes(&folder, &format!("{name}.spec.json"), 8192)?;
    if format!("{:x}", Sha256::digest(&bytes)) != receipt.spec_content_sha256 {
        return None;
    }
    let spec: Spec = serde_json::from_slice(&bytes).ok()?;
    spec.validate().ok()?;
    Some(spec)
}

fn conflict_summary(name: &str, previous: Option<&Spec>, requested: &Spec) -> serde_json::Value {
    let mut report = json!({"format":1,"service":if KNOWN_SERVICES.contains(&name){name}else{"other"},"evidence":"unverified_spec"});
    if let Some(previous) = previous {
        let changed: BTreeSet<_> = previous
            .environment
            .keys()
            .chain(requested.environment.keys())
            .filter(|key| previous.environment.get(*key) != requested.environment.get(*key))
            .collect();
        report["evidence"] = json!("verified_spec");
        report["command_changed"] = json!(previous.command != requested.command);
        report["environment_fields"] = json!(
            ENV_FIELDS
                .iter()
                .filter(|key| changed.iter().any(|value| value.as_str() == **key))
                .collect::<Vec<_>>()
        );
        report["other_environment_changes"] = json!(
            changed
                .iter()
                .filter(|key| !ENV_FIELDS.contains(&key.as_str()))
                .count()
        );
    }
    report
}

fn report_conflict(name: &str, previous: Option<&Spec>, requested: &Spec) {
    // A closed diagnostic stream must not turn a rejected configuration into a successful resume.
    let _ = writeln!(
        std::io::stderr(),
        "AI_PHONE_SERVICE_SPEC {}",
        conflict_summary(name, previous, requested)
    );
}

async fn running(root: &Path, name: &str) -> Result<state::Receipt> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(receipt) = state::observe(root, name, Path::new("/proc")) {
            return Ok(receipt);
        }
        if Instant::now() >= until {
            return Err(state::missing());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn persist(folder: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let path = folder.join(format!("{name}.spec.json"));
    if fs::symlink_metadata(&path).is_ok_and(|row| filesystem::alias(&row)) {
        return Err(Error::Invalid("service_spec_file"));
    }
    let mut output = tempfile::NamedTempFile::new_in(folder)?;
    output.write_all(bytes)?;
    output.as_file().sync_all()?;
    output
        .persist(path)
        .map_err(|error| Error::Io(error.error))?;
    File::open(folder)?.sync_all()?;
    Ok(())
}

async fn start(root: &Path, name: &str, spec: Spec) -> Result<bool> {
    let folder = root.join("data/services");
    filesystem::directory(&folder, true)?;
    let lock = filesystem::lock_file(&folder.join(format!("{name}.lock")))?;
    let bytes = serde_json::to_vec(&spec).map_err(|_| Error::Invalid("service_spec"))?;
    let identity = format!("{:x}", Sha256::digest(&bytes));
    match FileExt::try_lock_exclusive(&lock) {
        Ok(()) => {}
        Err(error)
            if error.kind() == std::io::ErrorKind::WouldBlock
                || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
        {
            let receipt = running(root, name).await?;
            if receipt.spec_sha256 == identity {
                return Ok(false);
            }
            // Existing Python-produced spec encodings have a different byte identity;
            // their authenticated contents must still resume without restarting the unit.
            let previous = saved_spec(root, name, &receipt);
            if previous.as_ref() == Some(&spec) {
                return Ok(false);
            }
            report_conflict(name, previous.as_ref(), &spec);
            return Err(Error::Invalid("service_configuration_changed"));
        }
        Err(error) => return Err(error.into()),
    }
    persist(&folder, name, &bytes)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(folder.join(format!("{name}.log")))?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--supervise")
        .arg(root)
        .arg(name)
        .arg(&identity)
        .arg(&identity)
        .current_dir(root)
        .stdin(Stdio::from(lock.try_clone()?))
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .process_group(0)
        .spawn()?;
    // The child inherits this open file description. Closing the parent copy must not unlock it.
    drop(lock);
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    running(root, name).await?;
    Ok(true)
}

async fn stop(root: &Path, name: &str) -> Result<bool> {
    let Some(receipt) = state::observe(root, name, Path::new("/proc")) else {
        return Ok(false);
    };
    let pid = i32::try_from(receipt.pid)
        .ok()
        .and_then(Pid::from_raw)
        .ok_or_else(state::missing)?;
    match kill_process(pid, Signal::TERM) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
        Err(error) => return Err(std::io::Error::from(error).into()),
    }
    let until = Instant::now() + Duration::from_secs(8);
    while state::process_start(Path::new("/proc"), receipt.pid).as_deref()
        == Some(&receipt.start_ticks)
    {
        if Instant::now() >= until {
            return Err(Error::Invalid("service_stop_unknown"));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(true)
}

pub(in crate::supervision) async fn run(arguments: &[OsString]) -> Result<()> {
    let [root, name, operation] = arguments else {
        return Err(Error::Invalid("service_control_arguments"));
    };
    let root = Path::new(root).canonicalize()?;
    let name = service_name(name)?;
    let result = match operation.to_str() {
        Some("start") => json!(start(&root, &name, input()?).await?),
        Some("stop") => json!(stop(&root, &name).await?),
        Some("status") => serde_json::to_value(state::observe(&root, &name, Path::new("/proc")))
            .map_err(|_| Error::Invalid("service_receipt"))?,
        _ => return Err(Error::Invalid("service_control_arguments")),
    };
    println!("{result}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Spec {
        serde_json::from_value(json!({"command":["native-worker","PRIVATE_ARGUMENT_DO_NOT_REPORT"],"environment":{"PATH":"PRIVATE_PATH_DO_NOT_REPORT","JAVA_HOME":"PRIVATE_JAVA_DO_NOT_REPORT"}})).unwrap()
    }

    #[test]
    fn conflict_summary_reports_only_fixed_names_and_counts_private_environment_changes() {
        let old = spec();
        for field in ENV_FIELDS {
            let mut requested = spec();
            requested
                .environment
                .insert(field.into(), "PRIVATE_NEW_VALUE_DO_NOT_REPORT".into());
            let report = conflict_summary("emulator", Some(&old), &requested);
            assert_eq!(
                report,
                json!({"format":1,"service":"emulator","evidence":"verified_spec","command_changed":false,"environment_fields":[field],"other_environment_changes":0})
            );
            assert!(!report.to_string().contains("PRIVATE_"));
        }
        for (before, after) in [(false, true), (true, false), (true, true)] {
            let mut old = spec();
            let mut requested = spec();
            if before {
                old.environment.insert(
                    "PRIVATE_UNKNOWN_KEY_DO_NOT_REPORT".into(),
                    "PRIVATE_OLD".into(),
                );
            }
            if after {
                requested.environment.insert(
                    "PRIVATE_UNKNOWN_KEY_DO_NOT_REPORT".into(),
                    "PRIVATE_NEW".into(),
                );
            }
            let report = conflict_summary("emulator", Some(&old), &requested);
            assert_eq!(report["other_environment_changes"], 1);
            assert_eq!(report["environment_fields"], json!([]));
            assert!(!report.to_string().contains("PRIVATE_"));
        }
        let mut requested = spec();
        requested.command[1] = "PRIVATE_CHANGED_ARGUMENT".into();
        requested.environment.insert(
            "AI_PHONE_PRIVATE_NAME_DO_NOT_REPORT".into(),
            "PRIVATE_NEW".into(),
        );
        let report = conflict_summary("private-service-name", Some(&old), &requested);
        assert_eq!(report["service"], "other");
        assert_eq!(report["command_changed"], true);
        assert!(!report.to_string().contains("private-service-name"));
        assert!(!report.to_string().contains("PRIVATE_"));
        assert_eq!(
            conflict_summary("emulator", None, &requested),
            json!({"format":1,"service":"emulator","evidence":"unverified_spec"})
        );
    }

    #[test]
    fn stored_spec_requires_the_bound_bytes_and_shape_without_following_aliases() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("data/services");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("emulator.spec.json");
        let valid = serde_json::to_vec(&spec()).unwrap();
        fs::write(&path, &valid).unwrap();
        let mut receipt:state::Receipt=serde_json::from_value(json!({"format":1,"supervisor":"rust-v1","program":"/fixture","pid":42,"start_ticks":"123","root":root.path(),"spec_sha256":"earlier-logical-digest","spec_content_sha256":format!("{:x}",Sha256::digest(&valid))})).unwrap();
        assert!(saved_spec(root.path(), "emulator", &receipt).is_some());
        let mut oversized = valid.clone();
        oversized.extend_from_slice(&[b' '; 8193]);
        fs::write(&path, &oversized).unwrap();
        receipt.spec_content_sha256 = format!("{:x}", Sha256::digest(&oversized));
        assert!(saved_spec(root.path(), "emulator", &receipt).is_none());
        receipt.spec_content_sha256 = format!("{:x}", Sha256::digest(&valid));
        fs::write(&path, b"changed").unwrap();
        assert!(saved_spec(root.path(), "emulator", &receipt).is_none());
        for value in [
            json!([]),
            json!({"command":"private","environment":{}}),
            json!({"command":[null],"environment":{}}),
            json!({"command":["worker"],"environment":[]}),
            json!({"command":["worker"],"environment":{"PATH":null}}),
        ] {
            let bytes = serde_json::to_vec(&value).unwrap();
            fs::write(&path, &bytes).unwrap();
            receipt.spec_content_sha256 = format!("{:x}", Sha256::digest(&bytes));
            assert!(saved_spec(root.path(), "emulator", &receipt).is_none());
        }
        fs::remove_file(&path).unwrap();
        assert!(saved_spec(root.path(), "emulator", &receipt).is_none());
        fs::write(root.path().join("foreign"), &valid).unwrap();
        std::os::unix::fs::symlink(root.path().join("foreign"), &path).unwrap();
        receipt.spec_content_sha256 = format!("{:x}", Sha256::digest(&valid));
        assert!(saved_spec(root.path(), "emulator", &receipt).is_none());
    }
}
