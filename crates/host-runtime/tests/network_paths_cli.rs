#![cfg(unix)]
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::Path,
    process::{Command, Output},
};
const ID: &str = "11111111-1111-4111-8111-111111111111";
fn call(root: &Path, kind: &str, id: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .args(["--prepare-device-network"])
        .arg(root)
        .args([kind, id])
        .env_clear()
        .env("PATH", "/no-executables")
        .output()
        .unwrap()
}
fn ready(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"prepared\n");
}
#[test]
fn prepares_without_python_node_downloads_or_an_external_engine() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    ready(call(root, "desktop", ID));
    let core = root.join(".tools/mihomo/linux-arm64");
    let before = fs::metadata(&core).unwrap();
    assert!(core.is_dir());
    assert!(!core.join("mihomo").exists());
    assert_eq!(
        root.join("data/device-network/desktop")
            .canonicalize()
            .unwrap(),
        root.join(format!("data/device-network/{ID}"))
            .canonicalize()
            .unwrap()
    );
    ready(call(root, "desktop", ID));
    assert_eq!(fs::metadata(core).unwrap().ino(), before.ino());
}
#[test]
fn preserves_existing_core_subscription_and_modes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let core = root.join(".tools/mihomo/linux-arm64");
    fs::create_dir_all(&core).unwrap();
    fs::set_permissions(&core, fs::Permissions::from_mode(0o700)).unwrap();
    let binary = core.join("mihomo");
    fs::write(&binary, b"existing-core").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let info = fs::metadata(&binary).unwrap();
    let folder = root.join(format!("data/device-network/{ID}"));
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("source.json"), b"private-existing-subscription").unwrap();
    ready(call(root, "desktop", ID));
    assert_eq!(fs::read(&binary).unwrap(), b"existing-core");
    assert_eq!(fs::metadata(&binary).unwrap().ino(), info.ino());
    assert_eq!(fs::metadata(&binary).unwrap().mode(), info.mode());
    assert_eq!(fs::metadata(core).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        fs::read(folder.join("source.json")).unwrap(),
        b"private-existing-subscription"
    );
}
#[test]
fn foreign_or_dangling_alias_is_never_replaced() {
    for dangling in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let parent = root.join("data/device-network");
        fs::create_dir_all(&parent).unwrap();
        let other = root.join("other-device");
        if !dangling {
            fs::create_dir(&other).unwrap();
            fs::write(other.join("keep"), b"keep").unwrap();
        }
        let alias = parent.join("desktop");
        symlink(&other, &alias).unwrap();
        let before = fs::read_link(&alias).unwrap();
        let output = call(root, "desktop", ID);
        assert!(!output.status.success());
        assert!(!output.stdout.starts_with(b"prepared"));
        assert_eq!(fs::read_link(alias).unwrap(), before);
        if !dangling {
            assert_eq!(fs::read(other.join("keep")).unwrap(), b"keep");
        }
    }
}
#[test]
fn blocking_cache_file_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join(".tools")).unwrap();
    let path = root.join(".tools/mihomo");
    fs::write(&path, b"keep").unwrap();
    assert!(!call(root, "desktop", ID).status.success());
    assert_eq!(fs::read(path).unwrap(), b"keep");
}
#[test]
fn phone_has_no_desktop_core_cache_and_invalid_devices_cannot_write() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for (kind, id) in [
        ("desktop", "../../outside"),
        ("phone", "other-phone"),
        ("other", ID),
    ] {
        assert!(!call(root, kind, id).status.success());
        assert!(!root.join("data").exists());
    }
    ready(call(root, "phone", "phone-local-01"));
    assert!(root.join("data/device-network/phone-local-01").is_dir());
    assert!(!root.join(".tools").exists());
}
