use super::*;
use std::collections::BTreeMap;

struct Phone {
    installed: bool,
    user: String,
    settings: BTreeMap<String, String>,
    granted: bool,
    register_after: usize,
    queries: usize,
    fail: Option<Vec<String>>,
    after_apply: bool,
    deny: bool,
    install_visible: bool,
    notification_output: Option<String>,
    calls: Vec<Vec<String>>,
    probe_hash: Option<String>,
    fail_on: usize,
    registration_output: Option<String>,
    ignore: Option<Vec<String>>,
    pending_at_install: Option<std::path::PathBuf>,
}
impl Phone {
    fn new(installed: bool) -> Self {
        Self {
            installed,
            user: "0".into(),
            settings: BTreeMap::from([
                ("enabled_input_methods".into(), "other/.Keyboard;7".into()),
                ("default_input_method".into(), "other/.Keyboard".into()),
                ("show_ime_with_hard_keyboard".into(), "0".into()),
                (
                    "enabled_accessibility_services".into(),
                    "other/.Reader".into(),
                ),
                ("accessibility_enabled".into(), "0".into()),
            ]),
            granted: false,
            register_after: 1,
            queries: 0,
            fail: None,
            after_apply: false,
            deny: false,
            install_visible: true,
            notification_output: None,
            calls: Vec::new(),
            probe_hash: None,
            fail_on: 1,
            registration_output: None,
            ignore: None,
            pending_at_install: None,
        }
    }
    fn ready() -> Self {
        let mut phone = Self::new(true);
        phone.granted = true;
        phone.settings.insert(
            "enabled_input_methods".into(),
            format!("other/.Keyboard;7:{IME}"),
        );
        phone
            .settings
            .insert("default_input_method".into(), IME.into());
        phone
            .settings
            .insert("show_ime_with_hard_keyboard".into(), "1".into());
        phone.settings.insert(
            "enabled_accessibility_services".into(),
            format!("other/.Reader:{ATTENTION}"),
        );
        phone
            .settings
            .insert("accessibility_enabled".into(), "1".into());
        phone
    }
    fn count(&self, command: &[&str]) -> usize {
        self.calls
            .iter()
            .filter(|v| v.iter().map(String::as_str).eq(command.iter().copied()))
            .count()
    }
}
impl Device for Phone {
    fn cancelled(&self) -> bool {
        false
    }
    async fn execute(&mut self, args: &[&str], _: u64) -> Result<String> {
        let owned = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        self.calls.push(owned.clone());
        let matched = self.fail.as_ref() == Some(&owned);
        let fail = matched && self.fail_on == 1;
        if matched && !fail {
            self.fail_on -= 1;
        }
        if fail {
            self.fail = None;
            if !self.after_apply {
                return Err(Error::Invalid("fixture_unknown"));
            }
        }
        if self.ignore.as_ref() == Some(&owned) {
            return Ok(String::new());
        }
        let output = if args == ["shell", "pm", "list", "packages", "--user", "0", PACKAGE] {
            if self.installed {
                format!("package:{PACKAGE}")
            } else {
                String::new()
            }
        } else if args == ["shell", "am", "get-current-user"] {
            self.user.clone()
        } else if args.first() == Some(&"install") {
            if let Some(path) = &self.pending_at_install {
                let pending: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
                assert_eq!(pending["stage"], "install_unknown");
                assert_eq!(pending["serial"], "emulator-fixture");
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(
                        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                }
            }
            if self.install_visible {
                self.installed = true;
            }
            "Success".into()
        } else if args.first() == Some(&"push") {
            use sha2::Digest;
            self.probe_hash = Some(format!(
                "{:x}",
                sha2::Sha256::digest(std::fs::read(args[1]).unwrap())
            ));
            String::new()
        } else if args.starts_with(&["shell", "sh", "-c"]) {
            self.probe_hash
                .as_ref()
                .map(|h| format!("{h}  {REMOTE_PROBE}"))
                .unwrap_or("missing".into())
        } else if args == ["shell", "ime", "list", "-a", "-s", "--user", "0"] {
            self.queries += 1;
            self.registration_output.clone().unwrap_or_else(|| {
                if self.queries >= self.register_after {
                    IME.into()
                } else {
                    String::new()
                }
            })
        } else if args == ["shell", "ime", "enable", "--user", "0", IME] {
            self.settings
                .entry("enabled_input_methods".into())
                .and_modify(|s| {
                    s.push(':');
                    s.push_str(IME);
                });
            String::new()
        } else if args == ["shell", "ime", "set", "--user", "0", IME] {
            self.settings
                .insert("default_input_method".into(), IME.into());
            String::new()
        } else if args.starts_with(&["shell", "settings", "--user", "0", "get", "secure"]) {
            self.settings.get(args[6]).cloned().unwrap_or("null".into())
        } else if args.starts_with(&["shell", "settings", "--user", "0", "put", "secure"]) {
            self.settings.insert(args[6].into(), args[7].into());
            String::new()
        } else if args
            == [
                "shell",
                "cmd",
                "notification",
                "allow_listener",
                NOTIFICATION,
                "0",
            ]
        {
            self.granted = !self.deny;
            String::new()
        } else if args.get(1) == Some(&"env") {
            self.notification_output
                .clone()
                .unwrap_or_else(|| self.granted.to_string())
        } else {
            panic!("unexpected fixture command: {args:?}")
        };
        if fail {
            return Err(Error::Invalid("fixture_unknown"));
        }
        Ok(output)
    }
}
struct Fixture {
    root: tempfile::TempDir,
    apk: std::path::PathBuf,
    probe: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let apk = root.path().join("input.apk");
        let probe = root.path().join("probe.jar");
        std::fs::write(&apk, b"fixture apk").unwrap();
        std::fs::write(&probe, b"fixture probe").unwrap();
        Self { root, apk, probe }
    }
    fn state(&self) -> State {
        State::lock(self.root.path(), "emulator-fixture").unwrap()
    }
    async fn run(&self, phone: &mut Phone, observe: bool) -> Result<bool> {
        let state = self.state();
        let pending = state.load()?;
        configure(&state, phone, pending, &self.apk, &self.probe, observe).await
    }
}

#[tokio::test]
async fn installed_package_repairs_missing_capabilities_without_reinstalling_or_removing_other_services()
 {
    let f = Fixture::new();
    let mut phone = Phone::new(true);
    assert!(f.run(&mut phone, false).await.unwrap());
    assert!(phone.granted);
    assert_eq!(phone.settings["default_input_method"], IME);
    assert!(phone.settings["enabled_input_methods"].starts_with("other/.Keyboard;7:"));
    assert_eq!(
        phone.settings["enabled_accessibility_services"],
        format!("other/.Reader:{ATTENTION}")
    );
    assert!(!phone.calls.iter().any(|a| a[0] == "install"));
    assert!(f.state().load().unwrap().is_none());
}
#[tokio::test]
async fn observe_only_is_read_only_and_never_claims_missing_permissions_ready() {
    let f = Fixture::new();
    let mut phone = Phone::ready();
    assert!(!f.run(&mut phone, true).await.unwrap());
    assert!(!phone.calls.iter().any(|a| a[0] == "push"
        || a[0] == "install"
        || a.get(4) == Some(&"put".into())
        || a.get(2) == Some(&"enable".into())));
    phone.granted = false;
    assert_eq!(
        f.run(&mut phone, true).await.unwrap_err().code(),
        "phone_input_not_ready"
    );
    assert!(!phone.granted);
    assert!(f.state().load().unwrap().is_none());
}
#[tokio::test]
async fn one_install_records_unknown_first_and_observes_success_without_replay() {
    let f = Fixture::new();
    let mut phone = Phone::new(false);
    phone.pending_at_install = Some(f.root.path().join("data/android/input-setup.pending.json"));
    phone.fail = Some(vec![
        "install".into(),
        "--no-incremental".into(),
        "-r".into(),
        "--user".into(),
        "0".into(),
        f.apk.to_str().unwrap().into(),
    ]);
    phone.after_apply = true;
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(phone.calls.iter().filter(|a| a[0] == "install").count(), 1);
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(phone.calls.iter().filter(|a| a[0] == "install").count(), 1);
}
#[tokio::test]
async fn absent_package_after_attempt_remains_unknown_and_is_not_replayed_on_resume() {
    let f = Fixture::new();
    let mut phone = Phone::new(false);
    phone.install_visible = false;
    assert_eq!(
        f.run(&mut phone, false).await.unwrap_err().code(),
        "phone_input_install_unknown"
    );
    assert!(f.state().load().unwrap() == Some(Stage::InstallUnknown));
    assert_eq!(
        f.run(&mut phone, false).await.unwrap_err().code(),
        "phone_input_install_unknown"
    );
    assert_eq!(phone.calls.iter().filter(|a| a[0] == "install").count(), 1);
}
#[tokio::test]
async fn known_package_resumes_a_legacy_unknown_install_record_without_reinstall() {
    let f = Fixture::new();
    let state = f.state();
    state.save(Stage::InstallUnknown).unwrap();
    drop(state);
    let mut phone = Phone::new(true);
    phone.register_after = 3;
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(phone.queries, 3);
    assert!(!phone.calls.iter().any(|a| a[0] == "install"));
    assert!(f.state().load().unwrap().is_none());
}
#[tokio::test]
async fn unknown_setting_write_is_observed_on_resume_instead_of_repeated() {
    let f = Fixture::new();
    let mut phone = Phone::ready();
    phone
        .settings
        .insert("accessibility_enabled".into(), "0".into());
    let command = [
        "shell",
        "settings",
        "--user",
        "0",
        "put",
        "secure",
        "accessibility_enabled",
        "1",
    ];
    phone.fail = Some(command.iter().map(|s| s.to_string()).collect());
    phone.after_apply = true;
    assert!(f.run(&mut phone, false).await.is_err());
    assert!(f.state().load().unwrap() == Some(Stage::Configure));
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(phone.count(&command), 1);
}
#[tokio::test]
async fn notification_authority_is_used_without_regrant_when_secure_mirror_lags() {
    let f = Fixture::new();
    let mut phone = Phone::ready();
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(
        phone.count(&[
            "shell",
            "cmd",
            "notification",
            "allow_listener",
            NOTIFICATION,
            "0"
        ]),
        0
    );
    phone.granted = false;
    phone.after_apply = true;
    phone.fail = Some(vec![
        "shell".into(),
        "cmd".into(),
        "notification".into(),
        "allow_listener".into(),
        NOTIFICATION.into(),
        "0".into(),
    ]);
    assert!(f.run(&mut phone, false).await.is_err());
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(
        phone.count(&[
            "shell",
            "cmd",
            "notification",
            "allow_listener",
            NOTIFICATION,
            "0"
        ]),
        1
    );
}
#[tokio::test]
async fn malformed_or_failed_notification_observation_does_not_authorize_a_grant() {
    for value in ["unexpected", "false\ntrue"] {
        let f = Fixture::new();
        let mut phone = Phone::ready();
        phone.notification_output = Some(value.into());
        assert!(f.run(&mut phone, false).await.is_err());
        assert_eq!(
            phone.count(&[
                "shell",
                "cmd",
                "notification",
                "allow_listener",
                NOTIFICATION,
                "0"
            ]),
            0
        );
    }
    let f = Fixture::new();
    let mut phone = Phone::ready();
    phone.granted = false;
    phone.deny = true;
    assert!(f.run(&mut phone, false).await.is_err());
    assert_eq!(
        phone.count(&[
            "shell",
            "cmd",
            "notification",
            "allow_listener",
            NOTIFICATION,
            "0"
        ]),
        1
    );
}
#[tokio::test]
async fn malformed_settings_never_authorize_a_destructive_rewrite() {
    for key in [
        "enabled_input_methods",
        "default_input_method",
        "show_ime_with_hard_keyboard",
        "enabled_accessibility_services",
        "accessibility_enabled",
    ] {
        let f = Fixture::new();
        let mut phone = Phone::ready();
        phone.settings.insert(key.into(), "broken;value".into());
        assert!(f.run(&mut phone, false).await.is_err());
        assert_eq!(phone.settings[key], "broken;value");
    }
}
#[tokio::test]
async fn expanded_components_are_recognized_without_duplicate_writes() {
    let f = Fixture::new();
    let mut phone = Phone::ready();
    phone.settings.insert(
        "default_input_method".into(),
        "dev.aiphone.input/dev.aiphone.input.UnicodeIme".into(),
    );
    phone.settings.insert(
        "enabled_accessibility_services".into(),
        "dev.aiphone.input/dev.aiphone.input.PhoneAttention".into(),
    );
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(phone.count(&["shell", "ime", "set", "--user", "0", IME]), 0);
    assert!(!phone.calls.iter().any(|a| a.get(6)
        == Some(&"enabled_accessibility_services".into())
        && a.get(4) == Some(&"put".into())));
}
#[test]
fn conflicting_lock_and_corrupt_foreign_or_wrong_serial_journals_are_preserved() {
    let f = Fixture::new();
    let state = f.state();
    assert!(matches!(
        State::lock(f.root.path(), "emulator-fixture"),
        Err(Error::Busy)
    ));
    let path = state.directory.join("input-setup.pending.json");
    for body in [
        "{broken",
        r#"{"format":1,"package":"other","serial":"emulator-fixture","user":"0","stage":"configure"}"#,
        r#"{"format":1,"package":"dev.aiphone.input","serial":"another","user":"0","stage":"configure"}"#,
    ] {
        std::fs::write(&path, body).unwrap();
        assert!(state.load().is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body);
    }
}
#[tokio::test]
async fn wrong_user_and_offline_observation_never_install_or_create_pending() {
    let f = Fixture::new();
    let mut phone = Phone::new(false);
    phone.user = "10".into();
    assert!(f.run(&mut phone, false).await.is_err());
    assert!(f.state().load().unwrap().is_none());
    assert!(!phone.calls.iter().any(|a| a[0] == "install"));
    phone.fail = Some(vec![
        "shell".into(),
        "pm".into(),
        "list".into(),
        "packages".into(),
        "--user".into(),
        "0".into(),
        PACKAGE.into(),
    ]);
    assert!(f.run(&mut phone, false).await.is_err());
    assert!(f.state().load().unwrap().is_none());
}

#[tokio::test]
async fn ready_device_resume_does_not_transfer_the_same_notification_probe_again() {
    let f = Fixture::new();
    let mut phone = Phone::ready();
    f.run(&mut phone, false).await.unwrap();
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(phone.calls.iter().filter(|a| a[0] == "push").count(), 1);
}

#[tokio::test]
async fn unavailable_probe_cannot_leave_a_partially_configured_device() {
    let f = Fixture::new();
    std::fs::remove_file(&f.probe).unwrap();
    let mut phone = Phone::new(true);
    let before = phone.settings.clone();
    assert!(f.run(&mut phone, false).await.is_err());
    assert_eq!(phone.settings, before);
    assert!(f.state().load().unwrap().is_none());
}

#[tokio::test]
async fn failed_registration_preserves_intent_and_resume_does_not_reinstall() {
    let f = Fixture::new();
    let mut phone = Phone::new(false);
    phone.registration_output = Some("malformed/component/extra".into());
    assert!(f.run(&mut phone, false).await.is_err());
    assert!(f.state().load().unwrap() == Some(Stage::Configure));
    phone.registration_output = None;
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(phone.calls.iter().filter(|a| a[0] == "install").count(), 1);
}

#[tokio::test]
async fn unconfirmed_setting_never_finishes_and_reopen_observes_before_writing() {
    let f = Fixture::new();
    let mut phone = Phone::ready();
    phone
        .settings
        .insert("accessibility_enabled".into(), "0".into());
    let command = [
        "shell",
        "settings",
        "--user",
        "0",
        "put",
        "secure",
        "accessibility_enabled",
        "1",
    ];
    phone.ignore = Some(command.iter().map(|s| s.to_string()).collect());
    assert_eq!(
        f.run(&mut phone, false).await.unwrap_err().code(),
        "phone_input_setting_unknown"
    );
    assert!(f.state().load().unwrap() == Some(Stage::Configure));
    phone
        .settings
        .insert("accessibility_enabled".into(), "1".into());
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(phone.count(&command), 1);
}

#[tokio::test]
async fn unknown_notification_confirmation_resumes_from_authority_without_regrant() {
    let f = Fixture::new();
    let mut phone = Phone::ready();
    phone.granted = false;
    phone.fail = Some(vec![
        "shell".into(),
        "env".into(),
        format!("CLASSPATH={REMOTE_PROBE}"),
        "app_process".into(),
        "/".into(),
        "dev.aiphone.input.probe.NotificationAccess".into(),
    ]);
    phone.fail_on = 2;
    assert!(f.run(&mut phone, false).await.is_err());
    assert!(phone.granted);
    f.run(&mut phone, false).await.unwrap();
    assert_eq!(
        phone.count(&[
            "shell",
            "cmd",
            "notification",
            "allow_listener",
            NOTIFICATION,
            "0"
        ]),
        1
    );
}

#[tokio::test]
async fn invalid_remote_probe_observation_never_authorizes_a_push() {
    let f = Fixture::new();
    let mut phone = Phone::ready();
    phone.probe_hash = Some("invalid".into());
    assert_eq!(
        f.run(&mut phone, false).await.unwrap_err().code(),
        "phone_input_probe_unknown"
    );
    assert!(!phone.calls.iter().any(|a| a[0] == "push"));
}

#[tokio::test]
async fn unknown_probe_transfer_is_observed_on_resume_instead_of_repeated() {
    let f = Fixture::new();
    let mut phone = Phone::ready();
    phone.fail = Some(vec![
        "push".into(),
        f.probe.to_str().unwrap().into(),
        REMOTE_PROBE.into(),
    ]);
    phone.after_apply = true;
    assert!(f.run(&mut phone, false).await.is_err());
    assert!(f.state().load().unwrap() == Some(Stage::Configure));
    assert!(!f.run(&mut phone, false).await.unwrap());
    assert_eq!(phone.calls.iter().filter(|a| a[0] == "push").count(), 1);
}

#[tokio::test]
async fn malformed_registration_cannot_be_treated_as_a_settings_list() {
    for value in [
        "null",
        "other/.Ime:dev.aiphone.input/.UnicodeIme",
        "dev.aiphone.input/.UnicodeIme;7",
    ] {
        let f = Fixture::new();
        let mut phone = Phone::ready();
        phone.registration_output = Some(value.into());
        assert_eq!(
            f.run(&mut phone, false).await.unwrap_err().code(),
            "phone_input_observation"
        );
    }
}

#[tokio::test]
async fn cancelled_setup_does_not_claim_ready_or_create_an_owner() {
    let f = Fixture::new();
    let (_sender, stop) = watch::channel(true);
    let request = NativeAndroidInputSetup {
        version: 1,
        project_root: f.root.path().to_str().unwrap().into(),
        serial: "emulator-fixture".into(),
        ..Default::default()
    };
    assert!(matches!(setup(request, stop).await, Err(Error::Cancelled)));
    assert!(!f.root.path().join("data").exists());
}

#[cfg(unix)]
#[test]
fn input_state_rejects_aliased_android_and_pending_paths_without_mutation() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let state = f.state();
    let path = state.directory.join("input-setup.pending.json");
    symlink(&f.apk, &path).unwrap();
    assert!(state.load().is_err());
    assert_eq!(std::fs::read(&f.apk).unwrap(), b"fixture apk");
    drop(state);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(f.root.path().join("data/android/input-setup.lock")).unwrap();
    std::fs::remove_dir(f.root.path().join("data/android")).unwrap();
    symlink(f.root.path(), f.root.path().join("data/android")).unwrap();
    assert!(State::lock(f.root.path(), "emulator-fixture").is_err());
}
