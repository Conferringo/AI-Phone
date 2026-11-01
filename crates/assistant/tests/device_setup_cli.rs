use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
use uuid::Uuid;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("aiphone-setup-cli-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("preserved"), b"user data").unwrap();
        Self(root)
    }

    fn invoke(&self, arguments: &[&str], with_root: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ai-phone-assistant"));
        command
            .args(arguments)
            .env_clear()
            .env("PATH", &self.0)
            .env("DATABASE_URL", "postgresql://unused@127.0.0.1:0/unused")
            .current_dir(&self.0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if with_root {
            command.env("AI_PHONE_PROJECT_ROOT", &self.0);
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                return child.wait_with_output().unwrap();
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("device setup CLI did not exit; no daemon may start");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn unchanged(&self) {
        assert_eq!(fs::read(self.0.join("preserved")).unwrap(), b"user data");
        assert_eq!(fs::read_dir(&self.0).unwrap().count(), 1);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn capability_is_read_only_and_does_not_require_an_installation_or_interpreter() {
    let fixture = Fixture::new();
    let output = fixture.invoke(&["--device-setup-contract"], false);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"ai-phone-device-setup-v1\n");
    assert!(output.stderr.is_empty());
    fixture.unchanged();
}

#[test]
fn setup_rejects_missing_root_and_mixed_arguments_before_service_startup() {
    let fixture = Fixture::new();
    for arguments in [
        vec!["--setup-devices"],
        vec!["--setup-devices", "--version"],
        vec!["--device-setup-contract", "--setup-devices"],
        vec!["--device-setup-contract", "--set-admin"],
    ] {
        let output = fixture.invoke(&arguments, false);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let expected = if arguments.len() == 1 {
            "device_setup_owned_root_required"
        } else {
            "device_setup_arguments"
        };
        assert!(String::from_utf8(output.stderr).unwrap().contains(expected));
        fixture.unchanged();
    }
}

#[test]
fn incomplete_installation_is_an_error_and_does_not_create_state() {
    let fixture = Fixture::new();
    let output = fixture.invoke(&["--setup-devices"], true);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("device_setup_storage")
    );
    fixture.unchanged();
}
