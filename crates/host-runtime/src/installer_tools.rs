//! Immutable, manifest-bound installer tools. No host Python or package manager.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const LIMIT: u64 = 512 * 1024 * 1024;
const RESERVE: u64 = 4 * 1024 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    path: String,
    size: u64,
    sha256: String,
    mode: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tool {
    version: String,
    executable: String,
    #[serde(rename = "source")]
    _source: serde_json::Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    host: String,
    tools: BTreeMap<String, Tool>,
    files: Vec<Record>,
}
struct Component {
    manifest: Manifest,
    bytes: Vec<u8>,
    identity: String,
}
pub struct Runtime {
    pub directory: PathBuf,
    pub python: PathBuf,
}

#[cfg(test)]
thread_local! {
    static HASHED_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn hash(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let count = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
        #[cfg(test)]
        HASHED_BYTES.with(|value| value.set(value.get() + count as u64));
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn invalid() -> String {
    "invalid_installer_tools".into()
}
fn identity(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn relative(value: &str) -> bool {
    !value.contains(['\\', ':'])
        && !value.chars().any(char::is_control)
        && value.split('/').all(|p| !matches!(p, "" | "." | ".."))
        && (value.starts_with("python/") || value.starts_with("node/"))
}
fn read_manifest(source: &Path, expected: &str, host: &str) -> Result<Component, String> {
    let bytes = manifest_bytes(source, expected)?;
    let value: Manifest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if value.format != 1 || value.host != host {
        return Err("installer_tools_host".into());
    }
    validate(&value)?;
    Ok(Component {
        manifest: value,
        bytes,
        identity: expected.into(),
    })
}
fn manifest_bytes(source: &Path, expected: &str) -> Result<Vec<u8>, String> {
    let path = source.join("manifest.json");
    let meta = fs::symlink_metadata(&path).map_err(|_| invalid())?;
    if !meta.is_file() || meta.len() > 4 * 1024 * 1024 || !identity(expected) {
        return Err(invalid());
    }
    let bytes = fs::read(path).map_err(|_| invalid())?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err("installer_tools_changed".into());
    }
    Ok(bytes)
}
fn validate(value: &Manifest) -> Result<(), String> {
    if value.files.is_empty() || value.files.len() > 12000 {
        return Err(invalid());
    }
    let mut names = BTreeMap::new();
    let mut size = 0u64;
    for row in &value.files {
        validate_record(row)?;
        size = size.checked_add(row.size).ok_or_else(invalid)?;
        if size > LIMIT || names.insert(row.path.as_str(), row).is_some() {
            return Err(invalid());
        }
    }
    validate_tools(&value.tools, &names)
}
fn validate_record(row: &Record) -> Result<(), String> {
    if !relative(&row.path) || !identity(&row.sha256) || ![0o644, 0o755].contains(&row.mode) {
        return Err(invalid());
    }
    Ok(())
}
fn validate_tools(
    tools: &BTreeMap<String, Tool>,
    names: &BTreeMap<&str, &Record>,
) -> Result<(), String> {
    if !tools.contains_key("python")
        || tools
            .keys()
            .any(|role| !matches!(role.as_str(), "python" | "node"))
    {
        return Err(invalid());
    }
    for (role, tool) in tools {
        let row = names.get(tool.executable.as_str()).ok_or_else(invalid)?;
        let version = tool.version.split('.').collect::<Vec<_>>();
        if !tool.executable.starts_with(&format!("{role}/"))
            || row.mode != 0o755
            || version.len() != 3
            || !version
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn tree_budget(root: &Path, path: &Path, visited: &mut usize) -> Result<(), String> {
    *visited += 1;
    if *visited > 24000
        || path
            .strip_prefix(root)
            .map_err(|_| invalid())?
            .components()
            .count()
            > 64
    {
        return Err(invalid());
    }
    Ok(())
}
fn tree_files(
    root: &Path,
    at: &Path,
    names: &mut BTreeSet<String>,
    visited: &mut usize,
) -> Result<(), String> {
    if fs::symlink_metadata(at)
        .map_err(|_| invalid())?
        .file_type()
        .is_symlink()
    {
        return Err(invalid());
    }
    for entry in fs::read_dir(at).map_err(|_| invalid())? {
        let entry = entry.map_err(|_| invalid())?;
        visit(root, &entry, names, visited)?;
    }
    Ok(())
}
fn visit(
    root: &Path,
    entry: &fs::DirEntry,
    names: &mut BTreeSet<String>,
    visited: &mut usize,
) -> Result<(), String> {
    tree_budget(root, &entry.path(), visited)?;
    let kind = entry.file_type().map_err(|_| invalid())?;
    if kind.is_dir() {
        tree_files(root, &entry.path(), names, visited)?;
    } else if kind.is_file() {
        names.insert(file_name(root, &entry.path())?);
        if names.len() > 12001 {
            return Err(invalid());
        }
    } else {
        return Err(invalid());
    }
    Ok(())
}
fn file_name(root: &Path, path: &Path) -> Result<String, String> {
    Ok(path
        .strip_prefix(root)
        .map_err(|_| invalid())?
        .to_str()
        .ok_or_else(invalid)?
        .replace('\\', "/"))
}
fn verify(
    root: &Path,
    component: &Component,
    cancel: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    if hash(&root.join("manifest.json"))? != component.identity {
        return Err("installer_tools_changed".into());
    }
    verify_payload(root, component, cancel)
}
fn verify_payload(
    root: &Path,
    component: &Component,
    cancel: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    verify_inventory(root, component)?;
    for row in &component.manifest.files {
        cancel()?;
        let path = record_path(root, row)?;
        if hash(&path)? != row.sha256 {
            return Err("installer_tools_changed".into());
        }
    }
    Ok(())
}

/// Runtime selection uses the same manifest and payload owner as client preparation.
#[cfg(unix)]
pub(crate) fn selected_environment(
    root: &Path,
    host: &str,
    base: &str,
) -> Result<BTreeMap<String, String>, String> {
    #[derive(Deserialize)]
    struct Selection {
        format: u32,
        path: PathBuf,
        sha256: String,
    }
    let bytes = match fs::read(root.join("data/deployment/tool-runtime.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.to_string()),
    };
    let record: Selection = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if record.format != 1 || !record.path.is_absolute() {
        return Err(invalid());
    }
    let component = read_manifest(&record.path, &record.sha256, host)?;
    // read_manifest already binds the selected manifest bytes to its saved identity.
    verify_payload(&record.path, &component, &|| Ok(()))?;
    let directory = record.path.canonicalize().map_err(|_| invalid())?;
    let mut env = BTreeMap::from([
        (
            "AI_PHONE_TOOL_RUNTIME".into(),
            directory.to_str().ok_or_else(invalid)?.into(),
        ),
        ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
    ]);
    let mut paths = vec![];
    for role in ["python", "node"] {
        if let Some(tool) = component.manifest.tools.get(role) {
            let program = directory.join(&tool.executable);
            paths.push(
                program
                    .parent()
                    .ok_or_else(invalid)?
                    .to_str()
                    .ok_or_else(invalid)?
                    .to_owned(),
            );
            env.insert(
                format!("AI_PHONE_{}", role.to_uppercase()),
                program.to_str().ok_or_else(invalid)?.to_owned(),
            );
        }
    }
    paths.push(base.into());
    env.insert("PATH".into(), paths.join(":"));
    Ok(env)
}
fn verify_inventory(root: &Path, component: &Component) -> Result<(), String> {
    let mut actual = BTreeSet::new();
    let mut visited = 0;
    tree_files(root, root, &mut actual, &mut visited)?;
    let mut expected: BTreeSet<_> = component
        .manifest
        .files
        .iter()
        .map(|r| r.path.clone())
        .collect();
    expected.insert("manifest.json".into());
    if actual != expected {
        return Err("installer_tools_inventory".into());
    }
    Ok(())
}
fn record_path(root: &Path, row: &Record) -> Result<PathBuf, String> {
    let path = root.join(&row.path);
    let meta = fs::metadata(&path).map_err(|_| invalid())?;
    if meta.len() != row.size {
        return Err("installer_tools_changed".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o777 != row.mode {
            return Err("installer_tools_permissions".into());
        }
    }
    Ok(path)
}
fn directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir(path).map_err(|_| invalid())?;
    }
    let meta = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !meta.is_dir() {
        return Err("installer_tools_directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| invalid())?;
    }
    Ok(())
}
fn copy_record(
    source: &Path,
    stage: &Path,
    row: &Record,
    check: &mut impl FnMut(u64) -> Result<(), String>,
) -> Result<(), String> {
    let target = stage.join(&row.path);
    fs::create_dir_all(target.parent().ok_or_else(invalid)?).map_err(|_| invalid())?;
    let mut input = fs::File::open(record_path(source, row)?).map_err(|_| invalid())?;
    let mut output = fs::File::create_new(&target).map_err(|_| invalid())?;
    copy_body(&mut input, &mut output, row, check)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(target, fs::Permissions::from_mode(row.mode)).map_err(|_| invalid())?;
    }
    Ok(())
}
fn copy_body(
    input: &mut fs::File,
    output: &mut fs::File,
    row: &Record,
    check: &mut impl FnMut(u64) -> Result<(), String>,
) -> Result<(), String> {
    let mut input = input.take(row.size + 1);
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = [0; 65536];
    loop {
        let length = input.read(&mut buffer).map_err(|_| invalid())?;
        if length == 0 {
            break;
        }
        size += length as u64;
        check(length as u64)?;
        output.write_all(&buffer[..length]).map_err(|_| invalid())?;
        hash.update(&buffer[..length]);
        #[cfg(test)]
        HASHED_BYTES.with(|bytes| bytes.set(bytes.get() + length as u64));
    }
    if size != row.size || format!("{:x}", hash.finalize()) != row.sha256 {
        return Err("installer_tools_changed".into());
    }
    output.sync_all().map_err(|_| invalid())?;
    Ok(())
}

struct Staging(PathBuf);
impl Staging {
    fn new(folder: &Path) -> Result<Self, String> {
        let path = folder.join(format!(
            ".stage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .map_err(|_| invalid())?;
        }
        #[cfg(not(unix))]
        fs::create_dir(&path).map_err(|_| invalid())?;
        Ok(Self(path))
    }
    fn cleanup(&self) -> Result<(), String> {
        if self.0.exists() {
            fs::remove_dir_all(&self.0).map_err(|_| "installer_tools_cleanup".to_string())?;
        }
        Ok(())
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn write_component(
    source: &Path,
    stage: &Path,
    component: &Component,
    check: &mut impl FnMut(u64) -> Result<(), String>,
) -> Result<(), String> {
    for row in &component.manifest.files {
        copy_record(source, stage, row, check)?;
    }
    let mut manifest = fs::File::create_new(stage.join("manifest.json")).map_err(|_| invalid())?;
    manifest
        .write_all(&component.bytes)
        .map_err(|_| invalid())?;
    manifest.sync_all().map_err(|_| invalid())?;
    Ok(())
}
fn copy_component(
    source: &Path,
    stage: &Path,
    component: &Component,
    free: &impl Fn(&Path) -> u64,
    cancel: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let mut since = 0;
    let mut check = |length| {
        cancel()?;
        since += length;
        if since >= 32 * 1024 * 1024 {
            since = 0;
            if free(stage) < RESERVE {
                return Err("installer_tools_space".into());
            }
        }
        Ok(())
    };
    write_component(source, stage, component, &mut check)
}
fn activate(
    stage: &Path,
    target: &Path,
    component: &Component,
    cancel: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    cancel()?;
    if fs::rename(stage, target).is_err() {
        verify(target, component, cancel)?;
    }
    #[cfg(unix)]
    {
        fs::File::open(target.parent().ok_or_else(invalid)?)
            .and_then(|f| f.sync_all())
            .map_err(|_| invalid())?;
    }
    Ok(())
}
fn publish(
    source: &Path,
    folder: &Path,
    target: &Path,
    component: &Component,
    free: &impl Fn(&Path) -> u64,
    cancel: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let size = component.manifest.files.iter().map(|r| r.size).sum::<u64>();
    if free(folder) < RESERVE + size {
        return Err("installer_tools_space".into());
    }
    let stage = Staging::new(folder)?;
    let result = copy_component(source, &stage.0, component, free, cancel)
        .and_then(|()| activate(&stage.0, target, component, cancel));
    stage.cleanup()?;
    result
}

pub fn prepare(
    source: &Path,
    home: &Path,
    expected: &str,
    host: &str,
    free: &impl Fn(&Path) -> u64,
    cancel: &impl Fn() -> Result<(), String>,
) -> Result<Runtime, String> {
    cancel()?;
    let component = read_manifest(source, expected, host)?;
    let components = home.join("components");
    directory(&components)?;
    let folder = components.join("installer-tools");
    directory(&folder)?;
    let target = folder.join(expected);
    if target.exists() {
        verify(&target, &component, cancel)?;
    } else {
        verify_inventory(source, &component)?;
        publish(source, &folder, &target, &component, free, cancel)?;
    }
    let python = target.join(&component.manifest.tools["python"].executable);
    Ok(Runtime {
        directory: target,
        python,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn fixture(home: &Path) -> (PathBuf, String) {
        let source = home.join("source");
        fs::create_dir_all(source.join("python/bin")).unwrap();
        fs::create_dir_all(source.join("node/bin")).unwrap();
        let mut files = Vec::new();
        for role in ["python", "node"] {
            let path = format!("{role}/bin/{role}");
            fs::write(source.join(&path), b"owned fixture").unwrap();
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(source.join(&path), fs::Permissions::from_mode(0o755)).unwrap();
            files.push(serde_json::json!({"path":path,"size":13,"mode":493,"sha256":hash(&source.join(path)).unwrap()}));
        }
        let value = serde_json::json!({"format":1,"host":"fixture-host","tools":{"python":{"version":"3.13.18","executable":"python/bin/python","source":{}},"node":{"version":"22.23.4","executable":"node/bin/node","source":{}}},"files":files});
        fs::write(
            source.join("manifest.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        let hash = hash(&source.join("manifest.json")).unwrap();
        (source, hash)
    }
    fn home() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "aiphone-tools-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn persistent_tools_reuse_exact_content_and_preserve_user_files() {
        let home = home();
        let (source, hash) = fixture(&home);
        fs::write(home.join("user-chat"), b"keep").unwrap();
        let runtime = prepare(
            &source,
            &home,
            &hash,
            "fixture-host",
            &|_| u64::MAX,
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(fs::read(&runtime.python).unwrap(), b"owned fixture");
        assert_eq!(
            prepare(&source, &home, &hash, "fixture-host", &|_| 0, &|| Ok(()))
                .unwrap()
                .directory,
            runtime.directory
        );
        fs::write(&runtime.python, b"changed").unwrap();
        assert!(
            prepare(
                &source,
                &home,
                &hash,
                "fixture-host",
                &|_| u64::MAX,
                &|| Ok(())
            )
            .is_err()
        );
        assert_eq!(fs::read(home.join("user-chat")).unwrap(), b"keep");
        fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn python_only_runtime_activates_and_reuses_without_host_node() {
        let home = home();
        let (source, _) = fixture(&home);
        fs::remove_dir_all(source.join("node")).unwrap();
        let path = source.join("manifest.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["tools"].as_object_mut().unwrap().remove("node");
        value["files"]
            .as_array_mut()
            .unwrap()
            .retain(|row| row["path"].as_str().unwrap().starts_with("python/"));
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let hash = hash(&path).unwrap();
        fs::write(home.join("user-chat"), b"keep").unwrap();
        let runtime = prepare(
            &source,
            &home,
            &hash,
            "fixture-host",
            &|_| u64::MAX,
            &|| Ok(()),
        )
        .unwrap();
        assert!(!runtime.directory.join("node").exists());
        assert_eq!(fs::read(&runtime.python).unwrap(), b"owned fixture");
        assert_eq!(
            prepare(&source, &home, &hash, "fixture-host", &|_| 0, &|| Ok(()))
                .unwrap()
                .directory,
            runtime.directory
        );
        assert_eq!(fs::read(home.join("user-chat")).unwrap(), b"keep");
        fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn tool_payload_hashes_are_not_repeated_before_copy_or_against_unused_sources() {
        let home = home();
        let (source, _) = fixture(&home);
        let path = source.join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let body = vec![b'x'; 16 * 1024 * 1024];
        for row in manifest["files"].as_array_mut().unwrap() {
            let file = source.join(row["path"].as_str().unwrap());
            fs::write(&file, &body).unwrap();
            row["size"] = serde_json::json!(body.len());
            row["sha256"] = serde_json::json!(hash(&file).unwrap());
        }
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let identity = hash(&path).unwrap();
        let metadata_bytes = fs::metadata(&path).unwrap().len();
        let payload_bytes = body.len() as u64 * 2;
        HASHED_BYTES.with(|bytes| bytes.set(0));
        let started = std::time::Instant::now();
        let runtime = prepare(
            &source,
            &home,
            &identity,
            "fixture-host",
            &|_| u64::MAX,
            &|| Ok(()),
        )
        .unwrap();
        let fresh = HASHED_BYTES.with(std::cell::Cell::get);
        let fresh_ms = started.elapsed().as_millis();
        HASHED_BYTES.with(|bytes| bytes.set(0));
        let started = std::time::Instant::now();
        prepare(
            &source,
            &home,
            &identity,
            "fixture-host",
            &|_| 0,
            &|| Ok(()),
        )
        .unwrap();
        let reused = HASHED_BYTES.with(std::cell::Cell::get);
        let reused_ms = started.elapsed().as_millis();
        fs::write(source.join("python/bin/python"), b"unused source changed").unwrap();
        let unaffected = prepare(
            &source,
            &home,
            &identity,
            "fixture-host",
            &|_| 0,
            &|| Ok(()),
        );
        assert_eq!(
            fs::metadata(runtime.python).unwrap().len(),
            body.len() as u64
        );
        fs::remove_dir_all(home).unwrap();
        println!(
            "payload={payload_bytes}; metadata={metadata_bytes}; fresh_hashed={fresh}; reused_hashed={reused}; fresh_ms={fresh_ms}; reused_ms={reused_ms}"
        );
        assert_eq!(
            fresh, payload_bytes,
            "copy must hash each payload byte only once"
        );
        assert_eq!(
            reused,
            payload_bytes + metadata_bytes,
            "reuse must hash only the installed copy"
        );
        assert!(
            unaffected.is_ok(),
            "unused source binaries must not block an intact installed copy"
        );
    }
    #[test]
    fn cancellation_during_copy_reclaims_only_its_staging_directory() {
        let home = home();
        let (source, hash) = fixture(&home);
        let cancel = || {
            let folder = home.join("components/installer-tools");
            if folder.is_dir()
                && fs::read_dir(folder).unwrap().any(|r| {
                    r.unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".stage-")
                })
            {
                Err("installation_cancelled".into())
            } else {
                Ok(())
            }
        };
        assert!(
            prepare(
                &source,
                &home,
                &hash,
                "fixture-host",
                &|_| u64::MAX,
                &cancel
            )
            .is_err()
        );
        assert_eq!(
            fs::read_dir(home.join("components/installer-tools"))
                .unwrap()
                .count(),
            0
        );
        assert!(source.exists());
        fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn changed_sources_symlinks_foreign_hosts_and_low_space_are_refused() {
        let home = home();
        let (source, hash) = fixture(&home);
        assert!(prepare(&source, &home, &hash, "foreign", &|_| u64::MAX, &|| Ok(())).is_err());
        assert!(prepare(&source, &home, &hash, "fixture-host", &|_| 0, &|| Ok(())).is_err());
        std::os::unix::fs::symlink("/tmp", source.join("outside")).unwrap();
        assert!(
            prepare(
                &source,
                &home,
                &hash,
                "fixture-host",
                &|_| u64::MAX,
                &|| Ok(())
            )
            .is_err()
        );
        fs::remove_file(source.join("outside")).unwrap();
        fs::write(source.join("node/bin/node"), b"tampered").unwrap();
        assert!(
            prepare(
                &source,
                &home,
                &hash,
                "fixture-host",
                &|_| u64::MAX,
                &|| Ok(())
            )
            .is_err()
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn directory_depth_is_bounded_before_activation() {
        let home = home();
        let (source, hash) = fixture(&home);
        let mut deep = source.clone();
        for _ in 0..65 {
            deep = deep.join("nested");
        }
        fs::create_dir_all(deep).unwrap();
        assert!(
            prepare(
                &source,
                &home,
                &hash,
                "fixture-host",
                &|_| u64::MAX,
                &|| Ok(())
            )
            .is_err()
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn concurrent_publication_checks_the_existing_manifest_and_cleans_its_stage() {
        let home = home();
        let (source, hash) = fixture(&home);
        let runtime = prepare(
            &source,
            &home,
            &hash,
            "fixture-host",
            &|_| u64::MAX,
            &|| Ok(()),
        )
        .unwrap();
        let component = read_manifest(&source, &hash, "fixture-host").unwrap();
        let folder = runtime.directory.parent().unwrap();
        for changed in [false, true] {
            let stage = Staging::new(folder).unwrap();
            copy_component(&source, &stage.0, &component, &|_| u64::MAX, &|| Ok(())).unwrap();
            if changed {
                fs::write(runtime.directory.join("manifest.json"), b"{}").unwrap();
            }
            assert_eq!(
                activate(&stage.0, &runtime.directory, &component, &|| Ok(())).is_err(),
                changed
            );
            let path = stage.0.clone();
            drop(stage);
            assert!(!path.exists());
        }
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_copy_panic_also_reclaims_the_owned_stage() {
        let home = home();
        let (source, hash) = fixture(&home);
        let cancel = || {
            let folder = home.join("components/installer-tools");
            if folder.is_dir()
                && fs::read_dir(folder).unwrap().any(|v| {
                    v.unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".stage-")
                })
            {
                panic!("fixture interrupted copy");
            }
            Ok(())
        };
        assert!(
            std::panic::catch_unwind(|| prepare(
                &source,
                &home,
                &hash,
                "fixture-host",
                &|_| u64::MAX,
                &cancel
            ))
            .is_err()
        );
        assert_eq!(
            fs::read_dir(home.join("components/installer-tools"))
                .unwrap()
                .count(),
            0
        );
        assert!(source.exists());
        fs::remove_dir_all(home).unwrap();
    }
}
