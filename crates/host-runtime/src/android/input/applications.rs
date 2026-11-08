//! Offline applications share the input setup's serial lease, not a new owner.
use super::{Device, State, policy};
use crate::{Error, Result, filesystem};
use ai_phone_api::wire::NativeAndroidApplicationIntent;
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

const HOME: &str = "dev.aiphone.home/.HomeActivity";
pub(super) struct Application {
    package: &'static str,
    journal: &'static str,
    path: PathBuf,
    maximum: u64,
    pending: bool,
}
impl Application {
    fn admit(
        state: &State,
        package: &'static str,
        journal: &'static str,
        path: PathBuf,
        maximum: u64,
    ) -> Result<Self> {
        let input = filesystem::regular(&path)?;
        let size = input.metadata()?.len();
        if size == 0 || size > maximum {
            return Err(Error::Invalid("phone_application_resource"));
        }
        let pending_path = state.directory.join(journal);
        let pending = match std::fs::symlink_metadata(&pending_path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
            Ok(_) => {
                let value = NativeAndroidApplicationIntent::decode(
                    filesystem::bounded_file_with_limit(&pending_path, 512)?.as_slice(),
                )
                .map_err(|_| Error::Invalid("phone_application_pending_owner"))?;
                if value.version != 1
                    || value.serial != state.serial
                    || value.package_name != package
                    || value.apk_sha256.len() != 64
                    || !value.apk_sha256.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(Error::Invalid("phone_application_pending_owner"));
                }
                true
            }
        };
        if path.to_str().is_none() {
            return Err(Error::Invalid("phone_application_resource"));
        }
        Ok(Self {
            package,
            journal,
            path,
            maximum,
            pending,
        })
    }
    fn digest(&self) -> Result<String> {
        // Source admission owns the sealed inventory. Hash the APK only when a
        // new install needs a durable intent, never for an already installed app.
        let mut input = filesystem::regular(&self.path)?;
        let size = input.metadata()?.len();
        if size == 0 || size > self.maximum {
            return Err(Error::Invalid("phone_application_resource"));
        }
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut read = 0u64;
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            read += count as u64;
            if read > self.maximum {
                return Err(Error::Invalid("phone_application_resource"));
            }
            hash.update(&buffer[..count]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }
    async fn present(&self, adb: &mut impl Device) -> Result<bool> {
        policy::package(
            &adb.execute(
                &[
                    "shell",
                    "pm",
                    "list",
                    "packages",
                    "--user",
                    "0",
                    self.package,
                ],
                10,
            )
            .await?,
            self.package,
        )
    }
    async fn configure(&self, state: &State, adb: &mut impl Device, observe: bool) -> Result<bool> {
        let present = self.present(adb).await?;
        if !present && self.pending {
            return Err(Error::Invalid("phone_application_install_unknown"));
        }
        if !present && observe {
            return Err(Error::Invalid("phone_application_not_ready"));
        }
        if present && !self.pending {
            return Ok(false);
        }
        if observe {
            return Err(Error::Invalid("phone_application_not_ready"));
        }
        if !present {
            let intent = NativeAndroidApplicationIntent {
                version: 1,
                serial: state.serial.clone(),
                package_name: self.package.into(),
                apk_sha256: self.digest()?,
            };
            state.persist(self.journal, &intent.encode_to_vec())?;
            let path = self
                .path
                .to_str()
                .ok_or(Error::Invalid("phone_application_resource"))?;
            // No -r: even a concurrently installed user's app is never replaced.
            let _ = adb
                .execute(&["install", "--no-incremental", "--user", "0", path], 60)
                .await;
            if !self.present(adb).await? {
                return Err(Error::Invalid("phone_application_install_unknown"));
            }
        }
        let mut changed = !present;
        if self.package == "dev.aiphone.home" {
            changed |= select_home(adb).await?;
        }
        state.clear(self.journal)?;
        Ok(changed)
    }
}
async fn select_home(adb: &mut impl Device) -> Result<bool> {
    async fn selected(adb: &mut impl Device) -> Result<bool> {
        let output = adb
            .execute(
                &[
                    "shell",
                    "cmd",
                    "package",
                    "resolve-activity",
                    "--brief",
                    "--user",
                    "0",
                    "-a",
                    "android.intent.action.MAIN",
                    "-c",
                    "android.intent.category.HOME",
                ],
                10,
            )
            .await?;
        let value = output
            .lines()
            .last()
            .ok_or(Error::Invalid("phone_home_observation"))?;
        if !policy::component(value) {
            return Err(Error::Invalid("phone_home_observation"));
        }
        Ok(policy::contains(&[value], HOME))
    }
    if selected(adb).await? {
        return Ok(false);
    }
    adb.execute(
        &[
            "shell",
            "cmd",
            "package",
            "set-home-activity",
            "--user",
            "0",
            HOME,
        ],
        10,
    )
    .await?;
    if !selected(adb).await? {
        return Err(Error::Invalid("phone_home_selection_unknown"));
    }
    Ok(true)
}

pub(super) struct Applications(Vec<Application>);
impl Applications {
    pub async fn load(root: &Path, state: &State, adb: &mut impl Device) -> Result<Self> {
        let abi = adb
            .execute(&["shell", "getprop", "ro.product.cpu.abi"], 10)
            .await?;
        if !["arm64-v8a", "x86_64"].contains(&abi.as_str()) {
            return Err(Error::Invalid("phone_application_abi"));
        }
        // Resources and pending intents are admitted before configuring input.
        Ok(Self(vec![
            Application::admit(
                state,
                "dev.aiphone.home",
                "application-home.pending.pb",
                root.join("runtime/phone-apps/home.apk"),
                16 * 1024 * 1024,
            )?,
            Application::admit(
                state,
                "com.github.metacubex.clash.meta",
                "application-clash.pending.pb",
                root.join(format!("runtime/guest-apps/clash-{abi}.apk")),
                64 * 1024 * 1024,
            )?,
            Application::admit(
                state,
                "info.plateaukao.einkbro",
                "application-browser.pending.pb",
                root.join("runtime/phone-apps/browser.apk"),
                crate::manifest::MAX_FILE,
            )?,
        ]))
    }
    pub async fn configure(
        &self,
        state: &State,
        adb: &mut impl Device,
        observe: bool,
    ) -> Result<bool> {
        let mut changed = false;
        for app in &self.0 {
            changed |= app.configure(state, adb, observe).await?;
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    struct Phone {
        packages: BTreeSet<String>,
        home: String,
        calls: Vec<Vec<String>>,
        directory: PathBuf,
        visible: bool,
        install_unknown: bool,
        home_unknown: bool,
        abi: String,
    }
    impl Phone {
        fn new(state: &State) -> Self {
            Self {
                packages: BTreeSet::new(),
                home: "other/.Home".into(),
                calls: Vec::new(),
                directory: state.directory.clone(),
                visible: true,
                install_unknown: false,
                home_unknown: false,
                abi: "arm64-v8a".into(),
            }
        }
        fn count(&self, verb: &str) -> usize {
            self.calls
                .iter()
                .filter(|v| v.iter().any(|s| s == verb))
                .count()
        }
    }
    impl Device for Phone {
        fn cancelled(&self) -> bool {
            false
        }
        async fn execute(&mut self, args: &[&str], _: u64) -> Result<String> {
            self.calls
                .push(args.iter().map(|s| s.to_string()).collect());
            if args == ["shell", "getprop", "ro.product.cpu.abi"] {
                return Ok(self.abi.clone());
            }
            if args.starts_with(&["shell", "pm", "list", "packages"]) {
                let package = args.last().unwrap();
                return Ok(if self.packages.contains(*package) {
                    format!("package:{package}")
                } else {
                    String::new()
                });
            }
            if args.first() == Some(&"install") {
                assert!(!args.contains(&"-r"));
                let (journal, package) = match Path::new(args.last().unwrap())
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                {
                    "home.apk" => ("application-home.pending.pb", "dev.aiphone.home"),
                    "browser.apk" => ("application-browser.pending.pb", "info.plateaukao.einkbro"),
                    _ => (
                        "application-clash.pending.pb",
                        "com.github.metacubex.clash.meta",
                    ),
                };
                let path = self.directory.join(journal);
                let intent = NativeAndroidApplicationIntent::decode(
                    std::fs::read(&path).unwrap().as_slice(),
                )
                .unwrap();
                assert_eq!(intent.serial, "emulator-fixture");
                assert_eq!(intent.package_name, package);
                assert_eq!(
                    intent.apk_sha256,
                    format!(
                        "{:x}",
                        Sha256::digest(std::fs::read(args.last().unwrap()).unwrap())
                    )
                );
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(
                        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                }
                if self.visible {
                    self.packages.insert(package.into());
                }
                return if self.install_unknown {
                    Err(Error::Invalid("fixture_unknown"))
                } else {
                    Ok("Success".into())
                };
            }
            if args.get(3) == Some(&"resolve-activity") {
                return Ok(self.home.clone());
            }
            if args.get(3) == Some(&"set-home-activity") {
                self.home = HOME.into();
                return if self.home_unknown {
                    self.home_unknown = false;
                    Err(Error::Invalid("fixture_unknown"))
                } else {
                    Ok("Success".into())
                };
            }
            panic!("unexpected application command: {args:?}")
        }
    }
    fn resources(root: &Path) {
        for name in [
            "runtime/phone-apps/home.apk",
            "runtime/phone-apps/browser.apk",
            "runtime/guest-apps/clash-arm64-v8a.apk",
            "runtime/guest-apps/clash-x86_64.apk",
        ] {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"fixture application").unwrap();
        }
    }
    #[tokio::test]
    async fn fresh_offline_apps_install_once_and_preserve_unrelated_packages_on_resume() {
        let root = tempfile::tempdir().unwrap();
        resources(root.path());
        let state = State::lock(root.path(), "emulator-fixture").unwrap();
        let mut phone = Phone::new(&state);
        phone.packages.insert("user.application".into());
        let apps = Applications::load(root.path(), &state, &mut phone)
            .await
            .unwrap();
        assert!(apps.configure(&state, &mut phone, false).await.unwrap());
        assert_eq!(phone.count("install"), 3);
        assert_eq!(phone.home, HOME);
        let apps = Applications::load(root.path(), &state, &mut phone)
            .await
            .unwrap();
        assert!(!apps.configure(&state, &mut phone, false).await.unwrap());
        assert_eq!(phone.count("install"), 3);
        assert!(phone.packages.contains("user.application"));
    }
    #[tokio::test]
    async fn native_only_bundle_installs_matching_apps_without_an_unused_abi() {
        for (abi, other) in [("arm64-v8a", "x86_64"), ("x86_64", "arm64-v8a")] {
            let root = tempfile::tempdir().unwrap();
            resources(root.path());
            std::fs::remove_file(
                root.path()
                    .join(format!("runtime/guest-apps/clash-{other}.apk")),
            )
            .unwrap();
            let state = State::lock(root.path(), "emulator-fixture").unwrap();
            let mut phone = Phone::new(&state);
            phone.abi = abi.into();
            phone.packages.insert("user.application".into());
            let apps = Applications::load(root.path(), &state, &mut phone)
                .await
                .unwrap();
            assert!(apps.configure(&state, &mut phone, false).await.unwrap());
            assert_eq!(phone.count("install"), 3);
            assert!(phone.packages.contains("user.application"));
        }
    }
    #[tokio::test]
    async fn existing_apps_keep_the_users_home_and_are_not_reinstalled() {
        let root = tempfile::tempdir().unwrap();
        resources(root.path());
        let state = State::lock(root.path(), "emulator-fixture").unwrap();
        let mut phone = Phone::new(&state);
        phone.packages.extend([
            "dev.aiphone.home".into(),
            "com.github.metacubex.clash.meta".into(),
            "info.plateaukao.einkbro".into(),
        ]);
        let apps = Applications::load(root.path(), &state, &mut phone)
            .await
            .unwrap();
        assert!(!apps.configure(&state, &mut phone, false).await.unwrap());
        assert_eq!(phone.home, "other/.Home");
        assert_eq!(phone.count("install"), 0);
    }
    #[tokio::test]
    async fn unknown_install_observes_presence_but_absence_never_replays() {
        for visible in [true, false] {
            let root = tempfile::tempdir().unwrap();
            resources(root.path());
            let state = State::lock(root.path(), "emulator-fixture").unwrap();
            let mut phone = Phone::new(&state);
            phone.visible = visible;
            phone.install_unknown = true;
            let apps = Applications::load(root.path(), &state, &mut phone)
                .await
                .unwrap();
            let first = apps.configure(&state, &mut phone, false).await;
            assert_eq!(first.is_ok(), visible);
            let apps = Applications::load(root.path(), &state, &mut phone)
                .await
                .unwrap();
            let second = apps.configure(&state, &mut phone, false).await;
            assert_eq!(second.is_ok(), visible);
            assert_eq!(phone.count("install"), if visible { 3 } else { 1 });
        }
    }
    #[tokio::test]
    async fn a_lost_home_selection_reply_is_observed_on_resume_without_replay() {
        let root = tempfile::tempdir().unwrap();
        resources(root.path());
        let state = State::lock(root.path(), "emulator-fixture").unwrap();
        let mut phone = Phone::new(&state);
        phone.home_unknown = true;
        let apps = Applications::load(root.path(), &state, &mut phone)
            .await
            .unwrap();
        assert!(apps.configure(&state, &mut phone, false).await.is_err());
        let apps = Applications::load(root.path(), &state, &mut phone)
            .await
            .unwrap();
        assert!(apps.configure(&state, &mut phone, false).await.is_ok());
        assert_eq!(phone.count("set-home-activity"), 1);
        assert_eq!(phone.count("install"), 3);
    }
    #[tokio::test]
    async fn foreign_intent_or_missing_resource_fails_before_device_writes() {
        let root = tempfile::tempdir().unwrap();
        resources(root.path());
        let state = State::lock(root.path(), "emulator-fixture").unwrap();
        let mut phone = Phone::new(&state);
        let value = NativeAndroidApplicationIntent {
            version: 1,
            serial: "another".into(),
            package_name: "dev.aiphone.home".into(),
            apk_sha256: "a".repeat(64),
        };
        state
            .persist("application-home.pending.pb", &value.encode_to_vec())
            .unwrap();
        assert!(
            Applications::load(root.path(), &state, &mut phone)
                .await
                .is_err()
        );
        state.clear("application-home.pending.pb").unwrap();
        std::fs::remove_file(root.path().join("runtime/guest-apps/clash-arm64-v8a.apk")).unwrap();
        assert!(
            Applications::load(root.path(), &state, &mut phone)
                .await
                .is_err()
        );
        assert_eq!(phone.count("install"), 0);
        assert_eq!(phone.count("set-home-activity"), 0);
    }
    #[tokio::test]
    async fn unsupported_abi_and_read_only_absence_never_install() {
        let root = tempfile::tempdir().unwrap();
        resources(root.path());
        let state = State::lock(root.path(), "emulator-fixture").unwrap();
        let mut phone = Phone::new(&state);
        phone.abi = "unsupported".into();
        assert!(
            Applications::load(root.path(), &state, &mut phone)
                .await
                .is_err()
        );
        phone.abi = "x86_64".into();
        let apps = Applications::load(root.path(), &state, &mut phone)
            .await
            .unwrap();
        assert!(apps.configure(&state, &mut phone, true).await.is_err());
        assert_eq!(phone.count("install"), 0);
    }
    #[test]
    fn malformed_package_observation_is_not_known_absence() {
        for value in ["package:error", "package:com..example", "transport failure"] {
            assert!(policy::package(value, "dev.aiphone.home").is_err());
        }
    }
}
