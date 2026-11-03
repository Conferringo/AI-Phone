use super::*;
use sha2::{Digest, Sha256};
use std::{cell::Cell, fs, io::Write};

#[derive(Default)]
struct Fixture {
    registered: Vec<Registration>,
    imports: usize,
    fail_after_effect: bool,
    fail_before_effect: bool,
}
impl Backend for Fixture {
    fn registrations(&mut self) -> Result<Vec<Registration>> {
        Ok(self.registered.clone())
    }
    fn import(&mut self, name: &str, directory: &Path, _: &Path) -> Result<()> {
        self.imports += 1;
        if self.fail_before_effect {
            return Err(Error::Invalid("wsl_import_unknown"));
        }
        fs::write(
            directory.join("ext4.vhdx"),
            b"synthetic disk; not an actual WSL boot",
        )?;
        self.registered.push(Registration {
            name: name.into(),
            base_path: directory.into(),
            version: 2,
        });
        if self.fail_after_effect {
            Err(Error::Invalid("wsl_import_unknown"))
        } else {
            Ok(())
        }
    }
}

fn resources<'a>(path: &'a Path, identity: &'a str) -> Rootfs<'a> {
    Rootfs {
        path,
        sha256: identity,
        expanded_bytes: 1024,
    }
}

fn rootfs(folder: &Path) -> (PathBuf, String) {
    let path = folder.join("rootfs.tar.gz");
    fs::File::create(&path)
        .unwrap()
        .write_all(b"synthetic rootfs fixture")
        .unwrap();
    let identity = format!("{:x}", Sha256::digest(fs::read(&path).unwrap()));
    (path, identity)
}

#[test]
fn imports_once_and_resumes_the_exact_owned_distro_without_readiness_claims() {
    let folder = tempfile::tempdir().unwrap();
    let (path, identity) = rootfs(folder.path());
    let home = folder.path().join("home");
    let mut installation = Installation::lock(&home).unwrap();
    fs::write(home.join("chat.keep"), b"keep").unwrap();
    let mut backend = Fixture::default();
    let reservations = Cell::new(0);
    let first = installation
        .ensure_wsl(
            resources(&path, &identity),
            &mut backend,
            &|_| {
                reservations.set(reservations.get() + 1);
                u64::MAX
            },
            &|| false,
        )
        .unwrap();
    assert!(!first.reused);
    assert_eq!(
        reservations.get(),
        1,
        "verify and reserve the fresh base image once"
    );
    assert!(!first.backend_initialized && !first.phone_ready);
    let disk = first.directory.join("ext4.vhdx");
    fs::write(&disk, b"existing guest data").unwrap();
    fs::remove_file(&path).unwrap();
    drop(installation);
    let mut reopened = Installation::lock(&home).unwrap();
    let second = reopened
        .ensure_wsl(
            resources(&path, &identity),
            &mut backend,
            &|_| panic!("resuming a registered guest must not reserve or rehash its base image"),
            &|| false,
        )
        .unwrap();
    assert!(second.reused);
    assert_eq!(second.distribution, first.distribution);
    assert_eq!(backend.imports, 1);
    assert_eq!(fs::read(disk).unwrap(), b"existing guest data");
    assert_eq!(fs::read(home.join("chat.keep")).unwrap(), b"keep");
}

#[test]
fn prepared_checkpoint_revalidates_the_base_image_before_the_first_os_import() {
    let folder = tempfile::tempdir().unwrap();
    let (path, identity) = rootfs(folder.path());
    let original = fs::read(&path).unwrap();
    let home = folder.path().join("home");
    let mut installation = Installation::lock(&home).unwrap();
    fs::create_dir(home.join("wsl-retry01")).unwrap();
    let owner = Owner {
        format: 1,
        directory: "wsl-retry01".into(),
        distribution: "AIPhone-wsl-retry01".into(),
        rootfs_sha256: identity.clone(),
        phase: Phase::Prepared,
    };
    save(&home, &owner, true).unwrap();
    fs::write(&path, b"changed base image").unwrap();
    let mut backend = Fixture::default();
    assert_eq!(
        installation
            .ensure_wsl(
                resources(&path, &identity),
                &mut backend,
                &|_| u64::MAX,
                &|| false
            )
            .err()
            .unwrap()
            .code(),
        "wsl_rootfs_changed"
    );
    assert_eq!(backend.imports, 0);
    fs::write(&path, original).unwrap();
    let resumed = installation
        .ensure_wsl(
            resources(&path, &identity),
            &mut backend,
            &|_| u64::MAX,
            &|| false,
        )
        .unwrap();
    assert!(!resumed.reused);
    assert_eq!(backend.imports, 1);
}

#[test]
fn unknown_import_with_an_empty_directory_is_not_replayed() {
    let folder = tempfile::tempdir().unwrap();
    let (path, identity) = rootfs(folder.path());
    let mut installation = Installation::lock(&folder.path().join("home")).unwrap();
    let mut backend = Fixture {
        fail_before_effect: true,
        ..Fixture::default()
    };
    assert!(
        installation
            .ensure_wsl(
                resources(&path, &identity),
                &mut backend,
                &|_| u64::MAX,
                &|| false
            )
            .is_err()
    );
    backend.fail_before_effect = false;
    fs::remove_file(&path).unwrap();
    assert_eq!(
        installation
            .ensure_wsl(
                resources(&path, &identity),
                &mut backend,
                &|_| panic!("unknown result must be observed, not verified for replay"),
                &|| false
            )
            .err()
            .unwrap()
            .code(),
        "wsl_import_unknown"
    );
    assert_eq!(backend.imports, 1);
}

#[test]
fn missing_registered_distro_changed_image_and_wrong_wsl_version_preserve_guest_data() {
    let folder = tempfile::tempdir().unwrap();
    let (path, identity) = rootfs(folder.path());
    let mut installation = Installation::lock(&folder.path().join("home")).unwrap();
    let mut backend = Fixture::default();
    let registered = installation
        .ensure_wsl(
            resources(&path, &identity),
            &mut backend,
            &|_| u64::MAX,
            &|| false,
        )
        .unwrap();
    fs::write(registered.directory.join("chat.keep"), b"keep").unwrap();
    assert_eq!(
        installation
            .ensure_wsl(
                resources(&path, &"0".repeat(64)),
                &mut backend,
                &|_| u64::MAX,
                &|| false
            )
            .err()
            .unwrap()
            .code(),
        "wsl_rootfs_upgrade_required"
    );
    backend.registered[0].version = 1;
    assert_eq!(
        installation
            .ensure_wsl(
                resources(&path, &identity),
                &mut backend,
                &|_| u64::MAX,
                &|| false
            )
            .err()
            .unwrap()
            .code(),
        "wsl_foreign_distribution"
    );
    backend.registered.clear();
    assert_eq!(
        installation
            .ensure_wsl(
                resources(&path, &identity),
                &mut backend,
                &|_| u64::MAX,
                &|| false
            )
            .err()
            .unwrap()
            .code(),
        "wsl_import_unknown"
    );
    assert_eq!(backend.imports, 1);
    assert_eq!(
        fs::read(registered.directory.join("chat.keep")).unwrap(),
        b"keep"
    );
}

#[test]
fn malformed_owner_and_path_escape_stop_before_native_effects() {
    let folder = tempfile::tempdir().unwrap();
    let (path, identity) = rootfs(folder.path());
    let home = folder.path().join("home");
    let mut installation = Installation::lock(&home).unwrap();
    let mut backend = Fixture::default();
    fs::write(home.join("wsl-owner.json"), b"{}").unwrap();
    assert_eq!(
        installation
            .ensure_wsl(
                resources(&path, &identity),
                &mut backend,
                &|_| u64::MAX,
                &|| false
            )
            .err()
            .unwrap()
            .code(),
        "wsl_owner_invalid"
    );
    let value = serde_json::json!({"format":1,"directory":"../foreign","distribution":"AIPhone-../foreign","rootfs_sha256":identity,"phase":"prepared"});
    fs::write(
        home.join("wsl-owner.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    assert_eq!(
        installation
            .ensure_wsl(
                resources(&path, &identity),
                &mut backend,
                &|_| u64::MAX,
                &|| false
            )
            .err()
            .unwrap()
            .code(),
        "wsl_owner_invalid"
    );
    assert_eq!(backend.imports, 0);
}

#[test]
fn unknown_import_recovers_from_matching_registration_without_repeating_it() {
    let folder = tempfile::tempdir().unwrap();
    let (path, identity) = rootfs(folder.path());
    let mut installation = Installation::lock(&folder.path().join("home")).unwrap();
    let mut backend = Fixture {
        fail_after_effect: true,
        ..Fixture::default()
    };
    assert!(
        installation
            .ensure_wsl(
                resources(&path, &identity),
                &mut backend,
                &|_| u64::MAX,
                &|| false
            )
            .is_err()
    );
    backend.fail_after_effect = false;
    let recovered = installation
        .ensure_wsl(
            resources(&path, &identity),
            &mut backend,
            &|_| u64::MAX,
            &|| false,
        )
        .unwrap();
    assert!(recovered.reused);
    assert_eq!(backend.imports, 1);
}

#[test]
fn absent_or_foreign_registration_never_recreates_an_attempted_import() {
    for foreign in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let (path, identity) = rootfs(folder.path());
        let mut installation = Installation::lock(&folder.path().join("home")).unwrap();
        let mut backend = Fixture {
            fail_after_effect: true,
            ..Fixture::default()
        };
        assert!(
            installation
                .ensure_wsl(
                    resources(&path, &identity),
                    &mut backend,
                    &|_| u64::MAX,
                    &|| false
                )
                .is_err()
        );
        if foreign {
            backend.registered[0].base_path = folder.path().into();
        } else {
            backend.registered.clear();
        }
        let error = installation
            .ensure_wsl(
                resources(&path, &identity),
                &mut backend,
                &|_| u64::MAX,
                &|| false,
            )
            .err()
            .unwrap();
        assert_eq!(
            error.code(),
            if foreign {
                "wsl_foreign_distribution"
            } else {
                "wsl_import_unknown"
            }
        );
        assert_eq!(backend.imports, 1);
    }
}

#[test]
fn changed_rootfs_low_space_and_cancel_are_rejected_before_import() {
    let folder = tempfile::tempdir().unwrap();
    let (path, identity) = rootfs(folder.path());
    let home = folder.path().join("home");
    let mut installation = Installation::lock(&home).unwrap();
    let mut backend = Fixture::default();
    assert!(
        installation
            .ensure_wsl(
                resources(&path, &"0".repeat(64)),
                &mut backend,
                &|_| u64::MAX,
                &|| false
            )
            .is_err()
    );
    assert!(matches!(
        installation.ensure_wsl(resources(&path, &identity), &mut backend, &|_| 0, &|| false),
        Err(Error::LowSpace)
    ));
    assert!(matches!(
        installation.ensure_wsl(
            resources(&path, &identity),
            &mut backend,
            &|_| u64::MAX,
            &|| true
        ),
        Err(Error::Cancelled)
    ));
    assert_eq!(backend.imports, 0);
    assert!(!home.join("wsl-owner.json").exists());
}
