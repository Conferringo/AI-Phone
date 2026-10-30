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
pub(super) struct Runtime {
    pub directory: PathBuf,
    pub python: PathBuf,
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
    if tools.keys().map(String::as_str).collect::<Vec<_>>() != ["node", "python"] {
        return Err(invalid());
    }
    for (role, tool) in tools {
        let row = names.get(tool.executable.as_str()).ok_or_else(invalid)?;
        if !tool.executable.starts_with(&format!("{role}/"))
            || row.mode != 0o755
            || tool.version.is_empty()
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
    let value = &component.manifest;
    if super::download::hash(&root.join("manifest.json"))? != component.identity {
        return Err("installer_tools_changed".into());
    }
    let mut actual = BTreeSet::new();
    let mut visited = 0;
    tree_files(root, root, &mut actual, &mut visited)?;
    let mut expected: BTreeSet<_> = value.files.iter().map(|r| r.path.clone()).collect();
    expected.insert("manifest.json".into());
    if actual != expected {
        return Err("installer_tools_inventory".into());
    }
    for row in &value.files {
        cancel()?;
        verify_record(root, row)?;
    }
    Ok(())
}
fn verify_record(root: &Path, row: &Record) -> Result<(), String> {
    let path = root.join(&row.path);
    let meta = fs::metadata(&path).map_err(|_| invalid())?;
    if meta.len() != row.size || super::download::hash(&path)? != row.sha256 {
        return Err("installer_tools_changed".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o777 != row.mode {
            return Err("installer_tools_permissions".into());
        }
    }
    Ok(())
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
    let mut input = fs::File::open(source.join(&row.path)).map_err(|_| invalid())?;
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

pub(super) fn prepare(
    source: &Path,
    home: &Path,
    expected: &str,
    host: &str,
    free: &impl Fn(&Path) -> u64,
    cancel: &impl Fn() -> Result<(), String>,
) -> Result<Runtime, String> {
    cancel()?;
    let component = read_manifest(source, expected, host)?;
    verify(source, &component, cancel)?;
    let components = home.join("components");
    directory(&components)?;
    let folder = components.join("installer-tools");
    directory(&folder)?;
    let target = folder.join(expected);
    if target.exists() {
        verify(&target, &component, cancel)?;
    } else {
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
            files.push(serde_json::json!({"path":path,"size":13,"mode":493,"sha256":super::super::download::hash(&source.join(path)).unwrap()}));
        }
        let value = serde_json::json!({"format":1,"host":"fixture-host","tools":{"python":{"version":"3.13.18","executable":"python/bin/python","source":{}},"node":{"version":"22.23.4","executable":"node/bin/node","source":{}}},"files":files});
        fs::write(
            source.join("manifest.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        let hash = super::super::download::hash(&source.join("manifest.json")).unwrap();
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

    #[test]
    #[ignore = "requires reviewed installer tools and source payload"]
    fn real_portable_tools_stage_source_without_host_python_node_or_network() {
        let source = PathBuf::from(
            std::env::var_os("AI_PHONE_TEST_INSTALLER_TOOLS").expect("reviewed tools path"),
        );
        let payload = PathBuf::from(
            std::env::var_os("AI_PHONE_TEST_SOURCE_PAYLOAD").expect("reviewed source payload"),
        );
        let home = home();
        let result = (|| -> Result<(), String> {
            let identity = super::super::download::hash(&source.join("manifest.json"))?;
            let host = super::super::platform_key(std::env::consts::OS, std::env::consts::ARCH);
            let runtime = prepare(
                &source,
                &home,
                &identity,
                &host,
                &super::super::available_bytes,
                &|| Ok(()),
            )?;
            let code = r#"import sys,ssl,sqlite3,json,pathlib,subprocess
sys.path.insert(0,sys.argv[1])
from client_host_entry import stage_source
home=pathlib.Path(sys.argv[3]);(home/'user/data').mkdir(parents=True)
(home/'user/data/chat-fixture').write_text('keep')
root=stage_source(pathlib.Path(sys.argv[2]),home)
sys.path.insert(0,str(root/'scripts'))
from installer_tools import select,verify
from runtime_config import runtime_env
component=pathlib.Path(sys.argv[4]).resolve();manifest=verify(component)
select(root,component);environment=runtime_env(root)
assert environment['PATH'].split(':')[:2]==[str(component/'python/bin'),str(component/'node/bin')]
assert (home/'user/data/chat-fixture').read_text()=='keep'
assert pathlib.Path(sys.prefix).resolve()==(component/'python').resolve()
node=subprocess.check_output(['node','--version'],env={'PATH':str(component/'node/bin')},text=True).strip()
assert node=='v'+manifest['tools']['node']['version']
print(json.dumps({'python':sys.version.split()[0],'node':node,'ssl':ssl.OPENSSL_VERSION,'sqlite':sqlite3.sqlite_version,'staged_source':True,'chat_fixture_preserved':True}))
"#;
            let output = std::process::Command::new(&runtime.python)
                .env_clear()
                .env("PATH", "/no-host-tools")
                .args(["-I", "-S", "-B", "-c", code])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../scripts"))
                .arg(payload)
                .arg(home.join("deployment"))
                .arg(runtime.directory)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into());
            }
            println!("{}", String::from_utf8_lossy(&output.stdout));
            Ok(())
        })();
        fs::remove_dir_all(home).unwrap();
        assert!(result.is_ok(), "{result:?}");
    }
}
