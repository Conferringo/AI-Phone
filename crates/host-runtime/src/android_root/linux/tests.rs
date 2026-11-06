use super::*;
use flate2::{Compression, write::GzEncoder};
use std::os::unix::fs::{MetadataExt, PermissionsExt};

fn payload(folder: &Path) -> PathBuf {
    let path = folder.join("runtime.tar.gz");
    let mut archive = tar::Builder::new(GzEncoder::new(
        File::create(&path).unwrap(),
        Compression::fast(),
    ));
    for name in [
        "usr/bin/qemu-system-aarch64",
        "opt/ai-phone/guest/kernel",
        "opt/ai-phone/guest/initramfs.gz",
        "etc/marker",
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o755);
        header.set_size(7);
        header.set_cksum();
        archive
            .append_data(&mut header, name, &b"payload"[..])
            .unwrap();
    }
    let mut link = tar::Header::new_gnu();
    link.set_entry_type(tar::EntryType::Symlink);
    link.set_size(0);
    link.set_mode(0o777);
    archive
        .append_link(&mut link, "etc/link", "marker")
        .unwrap();
    archive.into_inner().unwrap().finish().unwrap();
    path
}

fn request(folder: &Path) -> NativeAndroidRootInstall {
    NativeAndroidRootInstall {
        version: 1,
        payload_path: payload(folder).display().to_string(),
        installation: folder.join("installation").display().to_string(),
    }
}

#[test]
fn extraction_preserves_runtime_files_links_and_private_user_data() {
    let temporary = tempfile::tempdir().unwrap();
    let request = request(temporary.path());
    let user = temporary.path().join("installation/user");
    fs::create_dir_all(&user).unwrap();
    fs::write(user.join("keep"), b"existing account and disk sentinel").unwrap();
    let root = extract(request, Arc::new(AtomicBool::new(false))).unwrap();
    assert_eq!(fs::read(root.join("etc/link")).unwrap(), b"payload");
    assert_eq!(
        fs::read_link(root.join("etc/link")).unwrap(),
        Path::new("marker")
    );
    assert_eq!(
        fs::metadata(root.join("usr/bin/qemu-system-aarch64"))
            .unwrap()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        fs::read(user.join("keep")).unwrap(),
        b"existing account and disk sentinel"
    );
    assert_eq!(
        fs::metadata(root).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn repeated_install_does_not_replace_existing_runtime_or_user_data() {
    let temporary = tempfile::tempdir().unwrap();
    let request = request(temporary.path());
    let root = extract(request.clone(), Arc::new(AtomicBool::new(false))).unwrap();
    fs::write(root.join("etc/marker"), b"preserved").unwrap();
    let error = extract(request, Arc::new(AtomicBool::new(false))).unwrap_err();
    assert_eq!(error.code(), "android_root_already_installed");
    assert_eq!(fs::read(root.join("etc/marker")).unwrap(), b"preserved");
}

#[test]
fn cancelled_or_invalid_extraction_does_not_publish_or_keep_partial_files() {
    let temporary = tempfile::tempdir().unwrap();
    let request = request(temporary.path());
    let installation = PathBuf::from(&request.installation);
    let cancelled = extract(request.clone(), Arc::new(AtomicBool::new(true))).unwrap_err();
    assert_eq!(cancelled.code(), "installation_cancelled");
    assert!(!installation.join("rootfs").exists());
    fs::write(&request.payload_path, b"not an archive").unwrap();
    assert!(extract(request, Arc::new(AtomicBool::new(false))).is_err());
    assert!(!installation.join("rootfs").exists());
    assert!(
        fs::read_dir(installation)
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == "bootstrap.lock")
    );
}

#[test]
fn guest_command_keeps_kernel_arguments_together_and_never_substitutes_acceleration() {
    let mut request = NativeAndroidRootStart {
        version: 1,
        runtime_root: "/data/AI Phone/runtime".into(),
        cpus: 2,
        memory_mb: 2048,
        diagnostic_tcg: false,
    };
    let args = qemu_arguments(&request).unwrap();
    assert_eq!(args[1], "/data/AI Phone/runtime");
    assert!(
        args.windows(2)
            .any(|pair| pair == ["-append", "console=ttyAMA0 panic=-1"])
    );
    assert!(args.windows(2).any(|pair| pair == ["-accel", "kvm"]));
    assert!(!args.iter().any(|arg| arg == "tcg"));
    request.diagnostic_tcg = true;
    assert!(
        qemu_arguments(&request)
            .unwrap()
            .windows(2)
            .any(|pair| pair == ["-accel", "tcg"])
    );
    request.cpus = 0;
    assert_eq!(
        qemu_arguments(&request).unwrap_err().code(),
        "android_root_guest_request"
    );
}
