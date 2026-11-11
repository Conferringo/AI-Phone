//! Durable per-action records. The in-memory ledger is only a bounded cache.
//! Outcomes are never deleted to make room, including unfinished/unknown actions.
use crate::{Ledger, Record};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct Entry {
    id: String,
    record: Record,
}
const MAX_RECORD_BYTES: u64 = 1024 * 1024;
pub const CACHE_SIZE: usize = 128;

fn path(ledger: &Path, id: &str) -> PathBuf {
    let hash = format!("{:x}", Sha256::digest(id.as_bytes()));
    ledger
        .with_extension("actions")
        .join(&hash[..2])
        .join(format!("{hash}.json"))
}
fn directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("Action storage is not an owned directory".into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}
pub fn lock(ledger: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(ledger.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&file)
        .map_err(|_| "Another runner owns this device ledger".to_string())?;
    Ok(file)
}
pub fn read(ledger: &Path, id: &str) -> Result<Option<Record>, String> {
    let file = path(ledger, id);
    let meta = match fs::symlink_metadata(&file) {
        Ok(value) => value,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_RECORD_BYTES {
        return Err("Archived action cannot be verified; refusing replay".into());
    }
    let value: Entry = serde_json::from_slice(&fs::read(file).map_err(|e| e.to_string())?)
        .map_err(|_| "Archived action is corrupt; refusing replay".to_string())?;
    if value.id != id {
        return Err("Archived action identity mismatch; refusing replay".into());
    }
    Ok(Some(value.record))
}
pub fn write(ledger: &Path, id: &str, record: &Record) -> Result<(), String> {
    let file = path(ledger, id);
    let base = ledger.with_extension("actions");
    directory(&base)?;
    directory(file.parent().ok_or("Missing action directory")?)?;
    let bytes = serde_json::to_vec(&Entry {
        id: id.into(),
        record: record.clone(),
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("Action record exceeds storage limit".into());
    }
    let temp = file.with_extension("tmp");
    let mut out = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    out.write_all(&bytes)
        .and_then(|_| out.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(temp, &file).map_err(|e| e.to_string())?;
    // Also persist directory entries: a crash must retain the initial unknown record.
    File::open(file.parent().unwrap())
        .and_then(|d| d.sync_all())
        .map_err(|e| e.to_string())?;
    File::open(base)
        .and_then(|d| d.sync_all())
        .map_err(|e| e.to_string())?;
    File::open(ledger.parent().ok_or("Missing ledger directory")?)
        .and_then(|d| d.sync_all())
        .map_err(|e| e.to_string())
}
pub fn migrate(ledger_path: &Path, ledger: &mut Ledger) -> Result<(), String> {
    if ledger.archive_version > 1 {
        return Err("Unsupported action archive version".into());
    }
    if ledger.archive_version == 0 {
        let backup = ledger_path.with_extension("pre-archive.json");
        if ledger_path.exists() && !backup.exists() {
            fs::copy(ledger_path, &backup).map_err(|e| e.to_string())?;
            fs::set_permissions(&backup, fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
            File::open(&backup)
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
        for (id, record) in &ledger.actions {
            match read(ledger_path, id)? {
                Some(old) if old != *record => {
                    return Err("Legacy action conflicts with archive; migration stopped".into());
                }
                Some(_) => {}
                None => write(ledger_path, id, record)?,
            }
        }
        ledger.archive_version = 1;
    } else {
        // Canonical archive wins over stale cache snapshots after a crash.
        for (id, record) in &ledger.actions {
            match read(ledger_path, id)? {
                Some(old) if old.hash == record.hash => {}
                _ => {
                    return Err(
                        "Cached action archive missing or inconsistent; refusing startup".into(),
                    );
                }
            }
        }
    }
    ledger.actions.clear();
    Ok(())
}
pub fn trim(ledger: &mut Ledger) {
    while ledger.actions.len() > CACHE_SIZE {
        ledger.actions.pop_first();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_reader_cannot_silently_ignore_the_durable_archive() {
        #[derive(Deserialize)]
        struct LegacyLedger {
            #[allow(dead_code)]
            actions: std::collections::BTreeMap<String, Record>,
        }
        let current = serde_json::to_vec(&Ledger::default()).unwrap();
        assert!(serde_json::from_slice::<LegacyLedger>(&current).is_err());
        let legacy = br#"{"epoch":1,"controller":"manual","sequence":0,"actions":{}}"#;
        let decoded: Ledger = serde_json::from_slice(legacy).unwrap();
        assert_eq!(decoded.archive_version, 0);
    }
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let dir =
                std::env::temp_dir().join(format!("aiphone-actions-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&dir).unwrap();
            Self(dir)
        }
        fn ledger(&self) -> PathBuf {
            self.0.join("ledger.json")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn old_completed_and_unknown_actions_survive_migration_and_more_than_2000_new_records() {
        let dir = Temp::new();
        let path = dir.ledger();
        let mut ledger = Ledger {
            archive_version: 0,
            ..Default::default()
        };
        for n in 0..2101 {
            let record = Record {
                hash: format!("request-{n}"),
                completed: n % 2 == 0,
                success: n % 2 == 0,
                unknown: n % 2 != 0,
                output: format!("result-{n}"),
                ..Default::default()
            };
            ledger.actions.insert(format!("action-{n}"), record);
        }
        let original = serde_json::to_vec(&ledger).unwrap();
        fs::write(&path, &original).unwrap();
        migrate(&path, &mut ledger).unwrap();
        assert!(ledger.actions.is_empty());
        assert_eq!(
            fs::read(path.with_extension("pre-archive.json")).unwrap(),
            original
        );
        for n in [0, 1, 2000, 2100] {
            let r = read(&path, &format!("action-{n}")).unwrap().unwrap();
            assert_eq!(r.hash, format!("request-{n}"));
            assert_eq!(r.completed, n % 2 == 0);
            assert_eq!(r.output, format!("result-{n}"));
        }
        write(
            &path,
            "next",
            &Record {
                hash: "next".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(read(&path, "next").unwrap().is_some());
        assert!(read(&path, "never-seen").unwrap().is_none());
    }
    #[test]
    fn interrupted_migration_can_repeat_and_canonical_outcome_survives_stale_cache() {
        let dir = Temp::new();
        let path = dir.ledger();
        let pending = Record {
            hash: "same".into(),
            ..Default::default()
        };
        let finished = Record {
            completed: true,
            success: true,
            ..pending.clone()
        };
        write(&path, "a", &pending).unwrap();
        let mut old = Ledger {
            archive_version: 0,
            ..Default::default()
        };
        old.actions.insert("a".into(), pending.clone());
        migrate(&path, &mut old).unwrap();
        write(&path, "a", &finished).unwrap();
        old.actions.insert("a".into(), pending);
        migrate(&path, &mut old).unwrap();
        assert!(read(&path, "a").unwrap().unwrap().completed);
    }
    #[test]
    fn corrupt_record_and_missing_cached_record_fail_closed() {
        let dir = Temp::new();
        let path = dir.ledger();
        write(&path, "a", &Record::default()).unwrap();
        fs::write(super::path(&path, "a"), b"truncated").unwrap();
        assert!(read(&path, "a").is_err());
        let mut ledger = Ledger::default();
        ledger.actions.insert("missing".into(), Record::default());
        assert!(migrate(&path, &mut ledger).is_err());
    }
    #[test]
    fn one_runner_owns_a_ledger_and_cache_eviction_does_not_delete_outcomes() {
        let dir = Temp::new();
        let path = dir.ledger();
        let guard = lock(&path).unwrap();
        assert!(lock(&path).is_err());
        drop(guard);
        assert!(lock(&path).is_ok());
        let mut ledger = Ledger::default();
        for n in 0..130 {
            let id = format!("{n:04}");
            let record = Record {
                hash: id.clone(),
                ..Default::default()
            };
            write(&path, &id, &record).unwrap();
            ledger.actions.insert(id, record);
        }
        trim(&mut ledger);
        assert_eq!(ledger.actions.len(), CACHE_SIZE);
        assert!(read(&path, "0000").unwrap().is_some());
    }
}
