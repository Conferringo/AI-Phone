use super::process::{Context, Failure, Result};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub(super) const STEPS: [&str; 14] = [
    "not_started",
    "preflight",
    "source",
    "tools",
    "profile",
    "engine_supervisor",
    "engine_ready",
    "container_ownership",
    "images",
    "environment",
    "database_start",
    "database_prepare",
    "services_start",
    "complete",
];
const LIMIT: u64 = 65536;
const SIGNALS: [(&str, &[&str]); 8] = [
    (
        "executable_missing",
        &["exec: not found", "executable file not found"],
    ),
    (
        "library_load_failure",
        &[
            "error loading shared library",
            "error while loading shared libraries",
        ],
    ),
    (
        "permission_message",
        &["permission denied", "operation not permitted"],
    ),
    ("cgroup_message", &["cgroup"]),
    ("iptables_message", &["iptables", "nftables"]),
    (
        "storage_message",
        &["failed to mount overlay", "error initializing graphdriver"],
    ),
    ("address_in_use", &["address already in use"]),
    ("out_of_space", &["no space left on device"]),
];
#[derive(Serialize)]
pub(super) struct Initialization {
    pub step: String,
    pub error: &'static str,
    pub returncode: Option<i32>,
    pub engine: LogReport,
    pub database: LogReport,
}
impl Initialization {
    pub fn new() -> Self {
        Self {
            step: "not_started".into(),
            error: "none",
            returncode: None,
            engine: LogReport::new("not_sampled"),
            database: LogReport::new("not_sampled"),
        }
    }
}
#[derive(Serialize)]
pub(super) struct LogReport {
    state: &'static str,
    flags: Vec<&'static str>,
    bytes_read: usize,
    truncated: bool,
}
impl LogReport {
    pub fn new(state: &'static str) -> Self {
        Self {
            state,
            flags: vec![],
            bytes_read: 0,
            truncated: false,
        }
    }
}
pub(super) enum Cursor {
    Absent,
    Observed {
        device: u64,
        inode: u64,
        offset: u64,
    },
    Unverified,
}
fn path(root: &Path) -> Result<PathBuf> {
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err(Failure::invalid("unclassified"));
    }
    let data = root.join("data");
    if fs::symlink_metadata(&data)?.file_type().is_symlink() {
        let expected = root
            .parent()
            .and_then(Path::parent)
            .expect("installation root")
            .join("user/data");
        if fs::symlink_metadata(expected.parent().expect("user directory"))?
            .file_type()
            .is_symlink()
            || fs::symlink_metadata(&expected)?.file_type().is_symlink()
            || data.canonicalize()? != expected.canonicalize()?
        {
            return Err(Failure::invalid("unclassified"));
        }
    }
    let services = data.canonicalize()?.join("services");
    if fs::symlink_metadata(&services).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(Failure::invalid("unclassified"));
    }
    Ok(services.join("wsl-engine.log"))
}
fn owned(metadata: &fs::Metadata) -> bool {
    metadata.is_file() && metadata.uid() == rustix::process::getuid().as_raw()
}
pub(super) fn engine_cursor(root: &Path) -> Cursor {
    let result = path(root).and_then(|path| Ok(fs::symlink_metadata(path)?));
    match result {
        Ok(meta) if owned(&meta) => Cursor::Observed {
            device: meta.dev(),
            inode: meta.ino(),
            offset: meta.len(),
        },
        Err(error) if error.error == "resource_missing" => Cursor::Absent,
        _ => Cursor::Unverified,
    }
}
fn read(root: &Path, cursor: Cursor) -> Result<LogReport> {
    use rustix::fs::{Mode, OFlags};
    let mut file = File::from(
        rustix::fs::open(
            path(root)?,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    let meta = file.metadata()?;
    if !owned(&meta) {
        return Ok(LogReport::new("unverified"));
    }
    let offset = match cursor {
        Cursor::Absent => 0,
        Cursor::Observed {
            device,
            inode,
            offset,
        } if device == meta.dev() && inode == meta.ino() && meta.len() >= offset => offset,
        _ => return Ok(LogReport::new("unverified")),
    };
    let count = meta.len() - offset;
    file.seek(SeekFrom::Start(
        offset.max(meta.len().saturating_sub(LIMIT)),
    ))?;
    let mut bytes = vec![];
    file.take(count.min(LIMIT)).read_to_end(&mut bytes)?;
    let lowered = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    let mut flags: Vec<_> = SIGNALS
        .iter()
        .filter_map(|(name, patterns)| {
            patterns
                .iter()
                .any(|pattern| lowered.contains(pattern))
                .then_some(*name)
        })
        .collect();
    flags.sort_unstable();
    Ok(LogReport {
        state: if bytes.is_empty() {
            "empty"
        } else {
            "observed"
        },
        flags,
        bytes_read: bytes.len(),
        truncated: count > LIMIT,
    })
}
pub(super) async fn engine_report(context: &Context, cursor: Cursor) -> LogReport {
    if matches!(cursor, Cursor::Unverified) {
        return LogReport::new("unverified");
    }
    let state = context
        .execute(
            [
                context.bootstrap_str(),
                "--service-control",
                context.root_str(),
                "wsl-engine",
                "status",
            ],
            &[],
            120,
        )
        .await;
    match state.and_then(|body| Ok(serde_json::from_slice::<serde_json::Value>(&body)?)) {
        Ok(value) if !value.is_null() && value != false => {
            read(&context.root, cursor).unwrap_or_else(|_| LogReport::new("unavailable"))
        }
        Ok(_) => LogReport::new("unverified"),
        Err(_) => LogReport::new("unavailable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_evidence_reads_only_new_owned_bytes_and_keeps_closed_flags() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("data/services")).unwrap();
        let log = root.join("data/services/wsl-engine.log");
        fs::write(&log, b"old iptables line\n").unwrap();
        let cursor = engine_cursor(root);
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .open(&log)
            .unwrap()
            .write_all(b"NO SPACE LEFT ON DEVICE private-secret\n")
            .unwrap();
        let value = serde_json::to_value(read(root, cursor).unwrap()).unwrap();
        assert_eq!(value["flags"], serde_json::json!(["out_of_space"]));
        assert!(!value.to_string().contains("private-secret"));
        let cursor = engine_cursor(root);
        fs::remove_file(&log).unwrap();
        fs::write(&log, b"replacement").unwrap();
        assert_eq!(
            serde_json::to_value(read(root, cursor).unwrap()).unwrap()["state"],
            "unverified"
        );
    }
    #[test]
    fn log_budget_empty_phase_aliases_and_nonregular_paths_keep_the_contract() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("releases").join("source");
        fs::create_dir_all(&root).unwrap();
        let data = temp.path().join("user/data");
        fs::create_dir_all(data.join("services")).unwrap();
        std::os::unix::fs::symlink(&data, root.join("data")).unwrap();
        let log = data.join("services/wsl-engine.log");
        for size in [0, 1, LIMIT, LIMIT + 1] {
            fs::write(&log, vec![b'x'; usize::try_from(size).unwrap()]).unwrap();
            let value = read(&root, Cursor::Absent).unwrap();
            assert_eq!(u64::try_from(value.bytes_read).unwrap(), size.min(LIMIT));
            assert_eq!(value.truncated, size > LIMIT);
            assert!(value.flags.is_empty());
            let value = read(&root, engine_cursor(&root)).unwrap();
            assert_eq!(value.state, "empty");
            assert_eq!(value.bytes_read, 0);
        }
        fs::remove_file(&log).unwrap();
        let foreign = temp.path().join("foreign");
        fs::write(&foreign, b"private permission denied").unwrap();
        std::os::unix::fs::symlink(&foreign, &log).unwrap();
        assert!(matches!(engine_cursor(&root), Cursor::Unverified));
        assert!(read(&root, Cursor::Absent).is_err());
        fs::remove_file(&log).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&log)
                .status()
                .unwrap()
                .success()
        );
        assert!(matches!(engine_cursor(&root), Cursor::Unverified));
        assert_eq!(read(&root, Cursor::Absent).unwrap().state, "unverified");
        fs::remove_file(root.join("data")).unwrap();
        std::os::unix::fs::symlink(temp.path(), root.join("data")).unwrap();
        assert!(read(&root, Cursor::Absent).is_err());
    }
}
