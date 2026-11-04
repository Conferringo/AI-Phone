//! Bound installer resources enter the same owned registration lifecycle as raw imports.
use super::{Backend, InitializeRequest, Initializer, Registered, Rootfs};
use crate::{Error, Installation, Result, control::Control, filesystem, manifest};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Deserialize)]
struct Bundle {
    format: u8,
    host: String,
    backend_host: String,
    runtime: String,
    rootfs: Image,
    inventory: Inventory,
}
#[derive(Deserialize)]
struct Image {
    path: String,
    size: u64,
    sha256: String,
}
#[derive(Deserialize)]
struct Inventory {
    uncompressed_bytes: u64,
}

fn read(source: &Path, expected: &str) -> Result<Bundle> {
    filesystem::existing_directory(source)?;
    let bytes = filesystem::bounded_file(&source.join("manifest.json"))?;
    if !manifest::identity(expected) || format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err(Error::Invalid("wsl_bundle_changed"));
    }
    let value: Bundle =
        serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("wsl_bundle_invalid"))?;
    if (
        value.format,
        value.host.as_str(),
        value.backend_host.as_str(),
        value.runtime.as_str(),
        value.rootfs.path.as_str(),
    ) != (
        1,
        "windows-x64",
        "linux-x64",
        "wsl2-backend",
        "rootfs.tar.gz",
    ) || !(1..=16 * 1024_u64.pow(3)).contains(&value.rootfs.size)
    {
        return Err(Error::Invalid("wsl_bundle_invalid"));
    }
    Ok(value)
}

impl Installation {
    pub fn start_wsl_bundle<B: Backend + Initializer>(
        &mut self,
        source: &Path,
        expected_manifest: &str,
        request: &InitializeRequest,
        backend: &mut B,
        available: &dyn Fn(&Path) -> u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Registered> {
        let mut registered =
            self.ensure_wsl_bundle(source, expected_manifest, backend, available, cancelled)?;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        backend.initialize(&registered.distribution, request)?;
        registered.backend_initialized = true;
        // The Windows phone bridge and live two-device probe own phone_ready.
        Ok(registered)
    }

    pub fn ensure_wsl_bundle(
        &mut self,
        source: &Path,
        expected_manifest: &str,
        backend: &mut dyn Backend,
        available: &dyn Fn(&Path) -> u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Registered> {
        Control {
            available,
            cancelled,
        }
        .check()?;
        let value = read(source, expected_manifest)?;
        let path = source.join("rootfs.tar.gz");
        self.ensure_wsl(
            Rootfs {
                path: &path,
                sha256: &value.rootfs.sha256,
                expanded_bytes: value.inventory.uncompressed_bytes,
            },
            backend,
            available,
            cancelled,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wsl::Registration;
    use serde_json::json;
    use std::{fs, path::PathBuf};

    #[derive(Default)]
    struct Host {
        registrations: Vec<Registration>,
        imports: usize,
        initializations: usize,
        fail_initialize: bool,
    }
    impl Initializer for Host {
        fn initialize(&mut self, distribution: &str, request: &InitializeRequest) -> Result<()> {
            assert!(
                self.registrations
                    .iter()
                    .any(|row| row.name == distribution)
            );
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(request.as_ref()).unwrap()["locale"],
                "en"
            );
            self.initializations += 1;
            if self.fail_initialize {
                return Err(Error::Invalid("wsl_initialization_unknown"));
            }
            Ok(())
        }
    }

    #[test]
    fn backend_initialization_follows_owned_registration_and_never_claims_phone_readiness() {
        let (root, source, identity) = fixture();
        let home = root.path().join("home");
        let request = InitializeRequest::from_json(br#"{"locale":"en"}"#.to_vec()).unwrap();
        let mut host = Host::default();
        let mut installation = Installation::lock(&home).unwrap();
        let first = installation
            .start_wsl_bundle(&source, &identity, &request, &mut host, &available, &|| {
                false
            })
            .unwrap();
        assert!(first.backend_initialized);
        assert!(!first.phone_ready);
        assert_eq!((host.imports, host.initializations), (1, 1));
        let user = home.join("user-work.txt");
        fs::write(&user, b"preserve user work").unwrap();
        let resumed = installation
            .start_wsl_bundle(&source, &identity, &request, &mut host, &available, &|| {
                false
            })
            .unwrap();
        assert!(resumed.reused && resumed.backend_initialized && !resumed.phone_ready);
        assert_eq!((host.imports, host.initializations), (1, 2));
        assert_eq!(fs::read(user).unwrap(), b"preserve user work");
    }

    #[test]
    fn unknown_initialization_retains_the_owned_import_for_observed_manual_recovery() {
        let (root, source, identity) = fixture();
        let home = root.path().join("home");
        let request = InitializeRequest::from_json(br#"{"locale":"en"}"#.to_vec()).unwrap();
        let mut host = Host {
            fail_initialize: true,
            ..Host::default()
        };
        let mut installation = Installation::lock(&home).unwrap();
        assert_eq!(
            installation
                .start_wsl_bundle(&source, &identity, &request, &mut host, &available, &|| {
                    false
                })
                .err()
                .unwrap()
                .code(),
            "wsl_initialization_unknown"
        );
        assert_eq!((host.imports, host.initializations), (1, 1));
        host.fail_initialize = false;
        let resumed = installation
            .start_wsl_bundle(&source, &identity, &request, &mut host, &available, &|| {
                false
            })
            .unwrap();
        assert!(resumed.reused && resumed.backend_initialized);
        assert_eq!((host.imports, host.initializations), (1, 2));
        assert!(matches!(
            installation.start_wsl_bundle(
                &source,
                &identity,
                &request,
                &mut host,
                &available,
                &|| true
            ),
            Err(Error::Cancelled)
        ));
        assert_eq!(host.initializations, 2);
    }

    #[test]
    fn altered_capsule_never_reaches_registration_or_initialization() {
        let (root, source, _identity) = fixture();
        let request = InitializeRequest::from_json(br#"{"locale":"en"}"#.to_vec()).unwrap();
        let mut host = Host::default();
        let mut installation = Installation::lock(&root.path().join("home")).unwrap();
        assert_eq!(
            installation
                .start_wsl_bundle(
                    &source,
                    &"0".repeat(64),
                    &request,
                    &mut host,
                    &available,
                    &|| false
                )
                .err()
                .unwrap()
                .code(),
            "wsl_bundle_changed"
        );
        assert_eq!((host.imports, host.initializations), (0, 0));
    }
    impl Backend for Host {
        fn registrations(&mut self) -> Result<Vec<Registration>> {
            Ok(self.registrations.clone())
        }
        fn import(&mut self, name: &str, directory: &Path, rootfs: &Path) -> Result<()> {
            assert_eq!(fs::read(rootfs).unwrap(), b"rootfs-fixture");
            self.imports += 1;
            self.registrations.push(Registration {
                name: name.into(),
                base_path: directory.into(),
                version: 2,
            });
            Ok(())
        }
    }
    fn fixture() -> (tempfile::TempDir, PathBuf, String) {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("资源 resources");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("rootfs.tar.gz"), b"rootfs-fixture").unwrap();
        let manifest = json!({"format":1,"host":"windows-x64","backend_host":"linux-x64","runtime":"wsl2-backend",
            "rootfs":{"path":"rootfs.tar.gz","size":14,"sha256":format!("{:x}",Sha256::digest(b"rootfs-fixture"))},
            "inventory":{"uncompressed_bytes":1024},"complete_device_bundle":false});
        let bytes = serde_json::to_vec(&manifest).unwrap();
        fs::write(source.join("manifest.json"), &bytes).unwrap();
        (root, source, format!("{:x}", Sha256::digest(&bytes)))
    }
    fn available(_: &Path) -> u64 {
        128 * 1024_u64.pow(3)
    }

    #[test]
    fn bundled_registration_uses_exact_resource_and_resumes_without_copy_or_reimport() {
        let (root, source, identity) = fixture();
        let home = root.path().join("installation");
        let mut installer = Installation::lock(&home).unwrap();
        let mut host = Host::default();
        let first = installer
            .ensure_wsl_bundle(&source, &identity, &mut host, &available, &|| false)
            .unwrap();
        assert!(!first.reused);
        assert!(!first.backend_initialized);
        assert!(!first.phone_ready);
        let user = home.join("user");
        fs::create_dir(&user).unwrap();
        fs::write(user.join("chat"), b"keep").unwrap();
        fs::remove_file(source.join("rootfs.tar.gz")).unwrap();
        let resumed = installer
            .ensure_wsl_bundle(&source, &identity, &mut host, &available, &|| false)
            .unwrap();
        assert!(resumed.reused);
        assert_eq!(host.imports, 1);
        assert_eq!(fs::read(user.join("chat")).unwrap(), b"keep");
    }

    #[test]
    fn missing_resources_are_rejected_without_creating_a_bundle_directory() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing resources");
        let mut installation = Installation::lock(&root.path().join("install")).unwrap();
        let mut host = Host::default();
        assert!(
            installation
                .ensure_wsl_bundle(&missing, &"0".repeat(64), &mut host, &available, &|| false)
                .is_err()
        );
        assert!(!missing.exists(), "resource admission must be read-only");
        assert_eq!(host.imports, 0);
    }

    #[test]
    fn malformed_paths_and_changed_rootfs_fail_before_import() {
        let (root, source, identity) = fixture();
        let mut installation = Installation::lock(&root.path().join("install")).unwrap();
        let mut host = Host::default();
        fs::write(source.join("rootfs.tar.gz"), b"changed-rootfs").unwrap();
        assert_eq!(
            installation
                .ensure_wsl_bundle(&source, &identity, &mut host, &available, &|| false)
                .err()
                .unwrap()
                .code(),
            "wsl_rootfs_changed"
        );
        let path = source.join("manifest.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["rootfs"]["path"] = json!("../../user/chat");
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(path, &bytes).unwrap();
        let changed = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            installation
                .ensure_wsl_bundle(&source, &changed, &mut host, &available, &|| false)
                .err()
                .unwrap()
                .code(),
            "wsl_bundle_invalid"
        );
        assert_eq!(host.imports, 0);
    }

    #[test]
    fn resource_changes_or_cancellation_never_import_a_distribution() {
        let (root, source, identity) = fixture();
        let mut installer = Installation::lock(&root.path().join("install")).unwrap();
        let mut host = Host::default();
        assert!(matches!(
            installer.ensure_wsl_bundle(&source, &identity, &mut host, &available, &|| true),
            Err(Error::Cancelled)
        ));
        fs::write(source.join("manifest.json"), b"changed").unwrap();
        assert_eq!(
            installer
                .ensure_wsl_bundle(&source, &identity, &mut host, &available, &|| false)
                .err()
                .unwrap()
                .code(),
            "wsl_bundle_changed"
        );
        assert_eq!(host.imports, 0);
    }
}
