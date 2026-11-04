use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, io::Write};
use xz2::write::XzEncoder;

fn platform() -> manifest::Platform {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        manifest::Platform::MacArm64
    } else if cfg!(windows) {
        manifest::Platform::WindowsX64
    } else {
        manifest::Platform::LinuxX64
    }
}
// Portable archive/lifecycle cases explicitly select their fixture platform.
// Native host support itself is asserted separately; ARM Linux is not promoted.
fn prepare(
    install: &mut Installation,
    payload: &Path,
    identity: &str,
    resources: Resources,
    available: &dyn Fn(&Path) -> u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<Prepared> {
    install.prepare_android_for(
        payload,
        identity,
        resources,
        platform(),
        &Control {
            available,
            cancelled,
        },
    )
}
fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn resources() -> Resources {
    Resources {
        cpus: 2,
        memory_mb: 2048,
        storage_gb: 16,
    }
}
fn fixture(root: &Path) -> (PathBuf, String) {
    fixture_changed(root, |_, _| {})
}
fn fixture_changed(
    root: &Path,
    change: impl FnOnce(&mut serde_json::Value, &mut Vec<(String, Vec<u8>)>),
) -> (PathBuf, String) {
    let platform = platform().spec().host;
    let abi = self::platform().spec().abi;
    let suffix = if platform == "windows-x64" {
        ".exe"
    } else {
        ""
    };
    let mut names = vec![
        format!("sdk/emulator/emulator{suffix}"),
        format!("sdk/platform-tools/adb{suffix}"),
    ];
    for name in [
        "system.img",
        "vendor.img",
        "kernel-ranchu",
        "ramdisk.img",
        "NOTICE.txt",
    ] {
        names.push(format!("sdk/system-images/android-35/default/{abi}/{name}"));
    }
    let rows: Vec<_> = names.iter().map(|name| json!({"path":name,"size":7,"sha256":digest(b"factory"),"mode":if name.contains("system-images") {420} else {493}})).collect();
    let mut manifest = json!({"format":1,"host":platform,"api":35,"abi":abi,"redistribution":"review-pending","files":rows});
    let mut files: Vec<_> = names
        .into_iter()
        .map(|name| (name, b"factory".to_vec()))
        .collect();
    change(&mut manifest, &mut files);
    let mut tar = tar::Builder::new(XzEncoder::new(Vec::new(), 1));
    for (name, bytes) in std::iter::once((
        "ANDROID-MANIFEST.json".to_string(),
        serde_json::to_vec(&manifest).unwrap(),
    ))
    .chain(files)
    {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, bytes.as_slice())
            .unwrap();
    }
    let bytes = tar.into_inner().unwrap().finish().unwrap();
    let path = root.join("android.payload");
    fs::write(&path, &bytes).unwrap();
    (path, digest(&bytes))
}
#[test]
fn sealed_sdk_creates_one_device_and_reuses_it_without_resizing_or_touching_user_files() {
    let folder = tempfile::tempdir().unwrap();
    let (payload, identity) = fixture(folder.path());
    let home = folder.path().join("home");
    let mut install = Installation::lock(&home).unwrap();
    let first = prepare(
        &mut install,
        &payload,
        &identity,
        resources(),
        &|_| u64::MAX,
        &|| false,
    )
    .unwrap();
    assert!(!first.reused_sdk && !first.reused_device);
    assert_eq!(
        fs::read(
            first
                .sdk_root
                .join("platform-tools")
                .join(if cfg!(windows) { "adb.exe" } else { "adb" })
        )
        .unwrap(),
        b"factory"
    );
    let config = first.avd_home.join("phone-local-01.avd/config.ini");
    let original = fs::read(&config).unwrap();
    assert!(String::from_utf8_lossy(&original).contains("disk.dataPartition.size=16G\n"));
    fs::write(
        first.avd_home.join("phone-local-01.avd/userdata-qemu.img"),
        b"account-and-chat",
    )
    .unwrap();
    let mut next = resources();
    next.storage_gb = 32;
    let second = prepare(
        &mut install,
        &payload,
        &identity,
        next,
        &|_| u64::MAX,
        &|| false,
    )
    .unwrap();
    assert!(second.reused_sdk && second.reused_device);
    assert_eq!(fs::read(config).unwrap(), original);
    assert_eq!(
        fs::read(second.avd_home.join("phone-local-01.avd/userdata-qemu.img")).unwrap(),
        b"account-and-chat"
    );
}
#[test]
fn changed_bundle_and_existing_sdk_are_never_accepted() {
    let folder = tempfile::tempdir().unwrap();
    let (payload, identity) = fixture(folder.path());
    let mut install = Installation::lock(&folder.path().join("home")).unwrap();
    assert!(
        prepare(
            &mut install,
            &payload,
            &"0".repeat(64),
            resources(),
            &|_| u64::MAX,
            &|| false
        )
        .is_err()
    );
    let prepared = prepare(
        &mut install,
        &payload,
        &identity,
        resources(),
        &|_| u64::MAX,
        &|| false,
    )
    .unwrap();
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(prepared.sdk_root.join("emulator").join(if cfg!(windows) {
            "emulator.exe"
        } else {
            "emulator"
        }))
        .unwrap();
    file.write_all(b"changed").unwrap();
    drop(file);
    assert!(
        prepare(
            &mut install,
            &payload,
            &identity,
            resources(),
            &|_| u64::MAX,
            &|| false
        )
        .is_err()
    );
}

#[test]
fn cancellation_after_directory_or_record_publication_recovers_the_original_resources() {
    for boundary in ["phone-local-01.avd", "ai_phone_p2.ini"] {
        let temp = tempfile::tempdir().unwrap();
        let (payload, identity) = fixture(temp.path());
        let home = temp.path().join("home");
        let mut install = Installation::lock(&home).unwrap();
        let avd = home.join("user/data/android/avd");
        let cancel = || avd.join(boundary).exists();
        assert!(matches!(
            prepare(
                &mut install,
                &payload,
                &identity,
                resources(),
                &|_| u64::MAX,
                &cancel
            ),
            Err(Error::Cancelled)
        ));
        assert!(avd.join(".create.pb").is_file());
        let mut changed = resources();
        changed.storage_gb = 32;
        let result = prepare(
            &mut install,
            &payload,
            &identity,
            changed,
            &|_| u64::MAX,
            &|| false,
        )
        .unwrap();
        assert!(result.reused_sdk && result.reused_device);
        assert!(!avd.join(".create.pb").exists());
        assert!(
            fs::read_to_string(avd.join("phone-local-01.avd/config.ini"))
                .unwrap()
                .contains("disk.dataPartition.size=16G\n")
        );
    }
}
#[test]
fn unknown_partial_device_is_preserved_without_adoption() {
    let temp = tempfile::tempdir().unwrap();
    let (payload, identity) = fixture(temp.path());
    let home = temp.path().join("home");
    let mut install = Installation::lock(&home).unwrap();
    let avd = home.join("user/data/android/avd/phone-local-01.avd");
    fs::create_dir_all(&avd).unwrap();
    fs::write(avd.join("config.ini"), b"foreign incomplete state").unwrap();
    assert_eq!(
        prepare(
            &mut install,
            &payload,
            &identity,
            resources(),
            &|_| u64::MAX,
            &|| false
        )
        .err()
        .unwrap()
        .code(),
        "android_device_creation_unknown"
    );
    assert_eq!(
        fs::read(avd.join("config.ini")).unwrap(),
        b"foreign incomplete state"
    );
    assert!(!avd.parent().unwrap().join(".create.pb").exists());
}
#[test]
fn resource_limits_and_cancelled_admission_do_not_publish_devices() {
    let temp = tempfile::tempdir().unwrap();
    let (payload, identity) = fixture(temp.path());
    let home = temp.path().join("home");
    let mut install = Installation::lock(&home).unwrap();
    assert!(matches!(
        prepare(
            &mut install,
            &payload,
            &identity,
            resources(),
            &|_| 0,
            &|| false
        ),
        Err(Error::LowSpace)
    ));
    assert!(matches!(
        prepare(
            &mut install,
            &payload,
            &identity,
            resources(),
            &|_| u64::MAX,
            &|| true
        ),
        Err(Error::Cancelled)
    ));
    let mut invalid = resources();
    invalid.memory_mb = 0;
    assert_eq!(
        prepare(
            &mut install,
            &payload,
            &identity,
            invalid,
            &|_| u64::MAX,
            &|| false
        )
        .err()
        .unwrap()
        .code(),
        "invalid_device_resources"
    );
    assert!(!home.join("user/data/android/avd").exists());
}
#[test]
fn native_platform_support_does_not_claim_arm_linux_or_ordinary_android() {
    let supported = matches!(
        (std::env::consts::OS, std::env::consts::ARCH),
        ("macos", "aarch64") | ("linux", "x86_64") | ("windows", "x86_64")
    );
    assert_eq!(manifest::Platform::current().is_ok(), supported);
}
#[cfg(unix)]
#[test]
fn linked_component_or_device_parent_is_never_followed() {
    use std::os::unix::fs::symlink;
    for entry in ["components", "user"] {
        let temp = tempfile::tempdir().unwrap();
        let (payload, identity) = fixture(temp.path());
        let home = temp.path().join("home");
        let mut install = Installation::lock(&home).unwrap();
        let other = temp.path().join("other");
        fs::create_dir(&other).unwrap();
        symlink(&other, home.join(entry)).unwrap();
        assert!(
            prepare(
                &mut install,
                &payload,
                &identity,
                resources(),
                &|_| u64::MAX,
                &|| false
            )
            .is_err()
        );
        assert!(fs::read_dir(other).unwrap().next().is_none());
    }
}

#[test]
fn malformed_manifest_body_and_archive_tail_do_not_publish_resources() {
    for mode in [
        "host",
        "abi",
        "format",
        "revision",
        "review",
        "path",
        "hash",
        "missing",
        "extra",
        "duplicate",
        "truncated",
        "tail",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (payload, mut identity) = fixture_changed(temp.path(), |manifest, files| match mode {
            "host" => manifest["host"] = "foreign".into(),
            "abi" => manifest["abi"] = "foreign".into(),
            "format" => manifest["format"] = true.into(),
            "revision" => manifest["api"] = 36.into(),
            "review" => manifest["redistribution"] = "cleared".into(),
            "path" => {
                manifest["files"][0]["path"] = "sdk/../outside".into();
            }
            "hash" => files[0].1 = b"corrupt".to_vec(),
            "missing" => {
                files.pop();
            }
            "extra" => files.push(("sdk/emulator/extra".into(), b"extra".to_vec())),
            "duplicate" => files.push(files[0].clone()),
            _ => {}
        });
        if mode == "truncated" || mode == "tail" {
            let mut data = fs::read(&payload).unwrap();
            if mode == "truncated" {
                data.truncate(data.len() - 5);
            } else {
                data.extend_from_slice(b"trailing content");
            }
            fs::write(&payload, &data).unwrap();
            identity = digest(&data);
        }
        let home = temp.path().join("home");
        let mut install = Installation::lock(&home).unwrap();
        assert!(
            prepare(
                &mut install,
                &payload,
                &identity,
                resources(),
                &|_| u64::MAX,
                &|| false
            )
            .is_err(),
            "{mode}"
        );
        assert!(
            !home.join("components/android").join(identity).exists(),
            "{mode}"
        );
        assert!(!home.join("user/data/android/avd").exists(), "{mode}");
    }
}
#[test]
fn windows_inventory_rejects_reserved_names_and_case_collisions_on_every_test_host() {
    fn raw(extra: Option<&str>) -> crate::archive::RawInventory {
        let mut names = vec![
            "sdk/emulator/emulator.exe".to_string(),
            "sdk/platform-tools/adb.exe".to_string(),
        ];
        for file in [
            "system.img",
            "vendor.img",
            "kernel-ranchu",
            "ramdisk.img",
            "NOTICE.txt",
        ] {
            names.push(format!(
                "sdk/system-images/android-35/default/x86_64/{file}"
            ));
        }
        if let Some(name) = extra {
            names.push(name.to_string());
        }
        let files:Vec<_>=names.iter().map(|name|json!({"path":name,"size":0,"sha256":"0".repeat(64),"mode":if name.ends_with(".exe"){493}else{420}})).collect();
        let raw=serde_json::to_vec(&json!({"format":1,"host":"windows-x64","api":35,"abi":"x86_64","redistribution":"review-pending","files":files})).unwrap();
        let mut headers: std::collections::BTreeMap<_, _> =
            names.into_iter().map(|n| (n, 0)).collect();
        headers.insert("ANDROID-MANIFEST.json".into(), raw.len() as u64);
        crate::archive::RawInventory { raw, headers }
    }
    assert!(manifest::parse(&raw(None), manifest::Platform::WindowsX64).is_ok());
    for path in [
        "sdk/emulator/CON.txt",
        "sdk/emulator/nul",
        "sdk/emulator/com¹.dat",
        "sdk/emulator/name.",
        "sdk/emulator/end ",
        "sdk/emulator/x:y",
        "sdk/platform-tools/ADB.exe",
    ] {
        assert!(
            manifest::parse(&raw(Some(path)), manifest::Platform::WindowsX64).is_err(),
            "{path}"
        );
    }
}

#[cfg(windows)]
#[test]
fn native_windows_junction_is_not_a_component_directory() {
    let temp = tempfile::tempdir().unwrap();
    let (payload, identity) = fixture(temp.path());
    let home = temp.path().join("home");
    let mut install = Installation::lock(&home).unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let status = std::process::Command::new(
        std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/cmd.exe"),
    )
    .args(["/C", "mklink", "/J"])
    .arg(home.join("components"))
    .arg(&outside)
    .status()
    .unwrap();
    assert!(status.success());
    assert!(
        prepare(
            &mut install,
            &payload,
            &identity,
            resources(),
            &|_| u64::MAX,
            &|| false
        )
        .is_err()
    );
    assert!(fs::read_dir(outside).unwrap().next().is_none());
    fs::remove_dir(home.join("components")).unwrap();
}
