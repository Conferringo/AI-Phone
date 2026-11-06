use crate::Error;
use rustix::{
    fs::{Mode, OFlags, open, openat},
    process::getuid,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    os::{
        fd::OwnedFd,
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
pub(super) struct Receipt {
    pub format: u8,
    pub supervisor: String,
    pub program: PathBuf,
    pub pid: u32,
    pub start_ticks: String,
    pub root: PathBuf,
    pub spec_sha256: String,
    pub spec_content_sha256: String,
}

pub(super) fn directory(path: &Path) -> std::io::Result<OwnedFd> {
    open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(Into::into)
}

pub(super) fn private_bytes(folder: &OwnedFd, name: &str, maximum: u64) -> Option<Vec<u8>> {
    let descriptor = openat(
        folder,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .ok()?;
    read_regular(File::from(descriptor), maximum)
}

fn read_regular(file: File, maximum: u64) -> Option<Vec<u8>> {
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= maximum).then_some(bytes)
}

pub(super) fn process_start(proc: &Path, pid: u32) -> Option<String> {
    let path = proc.join(pid.to_string());
    if fs::metadata(&path).ok()?.uid() != getuid().as_raw() {
        return None;
    }
    let stat = fs::read_to_string(path.join("stat")).ok()?;
    let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    (fields.first() != Some(&"Z"))
        .then(|| fields.get(19).map(|text| (*text).into()))
        .flatten()
}

pub(super) fn observe(root: &Path, name: &str, proc: &Path) -> Option<Receipt> {
    let folder = directory(&root.join("data/services")).ok()?;
    let bytes = private_bytes(&folder, &format!("{name}.json"), 8192)?;
    let receipt: Receipt = serde_json::from_slice(&bytes).ok()?;
    if receipt.format != 1
        || receipt.supervisor != "rust-v1"
        || receipt.root != root
        || process_start(proc, receipt.pid).as_deref() != Some(&receipt.start_ticks)
    {
        return None;
    }
    let process = proc.join(receipt.pid.to_string());
    if fs::read_link(process.join("exe")).ok()? != receipt.program {
        return None;
    }
    let mut expected = Vec::new();
    for part in [
        receipt.program.as_os_str(),
        "--supervise".as_ref(),
        root.as_os_str(),
        name.as_ref(),
        receipt.spec_sha256.as_ref(),
        receipt.spec_content_sha256.as_ref(),
    ] {
        expected.extend_from_slice(part.as_bytes());
        expected.push(0);
    }
    (fs::read(process.join("cmdline")).ok()? == expected).then_some(receipt)
}

pub(super) fn missing() -> Error {
    Error::Invalid("service_owner_unknown")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, os::unix::fs::symlink};

    #[test]
    fn descriptor_read_does_not_follow_a_replaced_directory_or_file() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("services");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("unit.json"), b"original").unwrap();
        let opened = directory(&folder).unwrap();
        let file = openat(
            &opened,
            "unit.json",
            OFlags::RDONLY | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .unwrap();
        fs::rename(&folder, root.path().join("moved")).unwrap();
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("unit.json"), b"replacement").unwrap();
        assert_eq!(
            private_bytes(&opened, "unit.json", 8192).unwrap(),
            b"original"
        );
        let moved = root.path().join("moved/unit.json");
        fs::remove_file(&moved).unwrap();
        symlink(folder.join("unit.json"), &moved).unwrap();
        assert!(private_bytes(&opened, "unit.json", 8192).is_none());
        assert_eq!(read_regular(File::from(file), 8192).unwrap(), b"original");
    }

    #[test]
    fn aliases_nonregular_and_oversized_inputs_are_not_receipts() {
        let root = tempfile::tempdir().unwrap();
        let folder = directory(root.path()).unwrap();
        let file = root.path().join("file");
        let mut output = File::create(&file).unwrap();
        output.write_all(&[b' '; 8193]).unwrap();
        assert!(private_bytes(&folder, "file", 8192).is_none());
        symlink(&file, root.path().join("alias")).unwrap();
        assert!(private_bytes(&folder, "alias", 8192).is_none());
        fs::create_dir(root.path().join("directory")).unwrap();
        assert!(private_bytes(&folder, "directory", 8192).is_none());
        rustix::fs::mknodat(
            &folder,
            "fifo",
            rustix::fs::FileType::Fifo,
            Mode::RUSR | Mode::WUSR,
            0,
        )
        .unwrap();
        assert!(private_bytes(&folder, "fifo", 8192).is_none());
    }

    #[test]
    fn receipt_requires_the_exact_root_start_time_executable_and_complete_arguments() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("app");
        let proc = fixture.path().join("proc");
        let folder = root.join("data/services");
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir_all(proc.join("42")).unwrap();
        let value = serde_json::json!({"format":1,"supervisor":"rust-v1","program":"/fixture/native-host","pid":42,"start_ticks":"123","root":root,"spec_sha256":"logical","spec_content_sha256":"content"});
        fs::write(
            folder.join("fixture.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        fs::write(
            proc.join("42/stat"),
            format!("42 (private command) S {} 123", "0 ".repeat(18)),
        )
        .unwrap();
        symlink("/fixture/native-host", proc.join("42/exe")).unwrap();
        let args = [
            "/fixture/native-host",
            "--supervise",
            root.to_str().unwrap(),
            "fixture",
            "logical",
            "content",
        ];
        let raw = format!("{}\0", args.join("\0"));
        fs::write(proc.join("42/cmdline"), raw.as_bytes()).unwrap();
        assert!(observe(&root, "fixture", &proc).is_some());
        for index in 0..args.len() {
            let mut changed = args;
            changed[index] = "unrelated";
            fs::write(proc.join("42/cmdline"), format!("{}\0", changed.join("\0"))).unwrap();
            assert!(observe(&root, "fixture", &proc).is_none());
        }
        fs::write(proc.join("42/cmdline"), raw.as_bytes()).unwrap();
        fs::remove_file(proc.join("42/exe")).unwrap();
        symlink("/other/program", proc.join("42/exe")).unwrap();
        assert!(observe(&root, "fixture", &proc).is_none());
        fs::remove_file(proc.join("42/exe")).unwrap();
        symlink("/fixture/native-host", proc.join("42/exe")).unwrap();
        for (state, ticks) in [("S", "reused"), ("Z", "123")] {
            fs::write(
                proc.join("42/stat"),
                format!("42 (command) {state} {} {ticks}", "0 ".repeat(18)),
            )
            .unwrap();
            assert!(observe(&root, "fixture", &proc).is_none());
        }
        fs::write(
            proc.join("42/stat"),
            format!("42 (command) S {} 123", "0 ".repeat(18)),
        )
        .unwrap();
        for field in [
            "pid",
            "start_ticks",
            "root",
            "program",
            "spec_sha256",
            "spec_content_sha256",
        ] {
            let mut changed = value.clone();
            changed.as_object_mut().unwrap().remove(field);
            fs::write(
                folder.join("fixture.json"),
                serde_json::to_vec(&changed).unwrap(),
            )
            .unwrap();
            assert!(observe(&root, "fixture", &proc).is_none());
        }
        fs::write(folder.join("fixture.json"), b"[]").unwrap();
        assert!(observe(&root, "fixture", &proc).is_none());
        fs::remove_file(folder.join("fixture.json")).unwrap();
        assert!(observe(&root, "fixture", &proc).is_none());
    }
}
