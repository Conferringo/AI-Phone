//! A checkpoint describes installation, never proof that services are still running.
use super::{INTEGRITY, Progress};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Deserialize, Serialize)]
struct Checkpoint {
    format: u8,
    payload: String,
    progress: Progress,
}

fn payload() -> Result<String, String> {
    let integrity: serde_json::Value =
        serde_json::from_str(INTEGRITY).map_err(|_| "invalid_payload")?;
    integrity["source.tar.gz"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "invalid_payload".into())
}

pub(super) fn save(folder: &Path, progress: &Progress) -> Result<(), String> {
    let checkpoint = Checkpoint {
        format: 1,
        payload: payload()?,
        progress: progress.clone(),
    };
    let bytes = serde_json::to_vec(&checkpoint).map_err(|_| "invalid_install_checkpoint")?;
    if bytes.len() > 160_000 {
        return Err("invalid_install_checkpoint".into());
    }
    atomic_write(folder, &bytes)
}

fn atomic_write(folder: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = folder.join(format!(
        ".deployment-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "checkpoint_write_failed")?
            .as_nanos()
    ));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, folder.join("deployment.json"))?;
        #[cfg(unix)]
        fs::File::open(folder)?.sync_all()?;
        Ok::<_, std::io::Error>(())
    })();
    let _ = fs::remove_file(temp);
    result.map_err(|_| "checkpoint_write_failed".into())
}

fn read(folder: &Path) -> Result<Option<Progress>, String> {
    let file = folder.join("deployment.json");
    let metadata = match fs::symlink_metadata(&file) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("invalid_install_checkpoint".into()),
    };
    if !metadata.is_file() || metadata.len() > 160_000 {
        return Err("invalid_install_checkpoint".into());
    }
    decode(&fs::read(file).map_err(|_| "invalid_install_checkpoint")?)
}

fn decode(bytes: &[u8]) -> Result<Option<Progress>, String> {
    let checkpoint: Checkpoint =
        serde_json::from_slice(bytes).map_err(|_| "invalid_install_checkpoint")?;
    if checkpoint.format != 1 || checkpoint.payload != payload()? {
        return Ok(None);
    }
    Ok(Some(checkpoint.progress))
}

pub(super) fn completed(folder: &Path) -> Result<bool, String> {
    Ok(read(folder)?.is_some_and(|progress| progress.state == "ready"))
}
pub(super) fn admitted(folder: &Path) -> bool {
    read(folder)
        .ok()
        .flatten()
        .is_some_and(|progress| !progress.state.is_empty())
}

pub(super) fn restore(folder: &Path) -> Result<Progress, String> {
    let Some(mut progress) = read(folder)? else {
        return Ok(Progress::default());
    };
    progress.license.clear();
    match progress.state.as_str() {
        "running" | "license" => {
            progress.phase = "interrupted".into();
            progress.state = "interrupted".into();
            progress.error = "installation_interrupted".into();
        }
        "ready" => {
            progress.phase = "installed".into();
            progress.state = "installed".into();
        }
        _ => {}
    }
    Ok(progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_requires_live_readiness_and_never_reuses_license_approval() {
        let folder =
            std::env::temp_dir().join(format!("ai-phone-checkpoint-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        for state in ["running", "license"] {
            save(
                &folder,
                &Progress {
                    phase: "sdk-licenses".into(),
                    state: state.into(),
                    license: "Fixture terms".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let recovered = restore(&folder).unwrap();
            assert_eq!(recovered.state, "interrupted");
            assert!(recovered.license.is_empty());
            assert!(!completed(&folder).unwrap());
        }
        save(
            &folder,
            &Progress {
                phase: "ready".into(),
                state: "ready".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(completed(&folder).unwrap());
        assert_eq!(
            restore(&folder).unwrap().state,
            "installed",
            "a saved receipt is not a fresh service probe"
        );
        let file = folder.join("deployment.json");
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        record["payload"] = serde_json::json!("another-release");
        fs::write(&file, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(
            !completed(&folder).unwrap(),
            "another source version cannot auto-activate"
        );
        fs::write(&file, b"partial checkpoint").unwrap();
        assert!(restore(&folder).is_err());
        fs::remove_dir_all(folder).unwrap();
    }
}
