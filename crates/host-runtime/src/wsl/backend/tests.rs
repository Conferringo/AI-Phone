use super::*;
use std::os::unix::process::ExitStatusExt;

#[derive(Default)]
struct Fixture {
    calls: Vec<(String, Vec<String>, Vec<u8>)>,
    container: Option<(String, bool)>,
    assistant_running: bool,
    fail_images: bool,
}
impl Execute for Fixture {
    async fn run(&mut self, command: Command, input: Vec<u8>) -> Result<std::process::Output> {
        let program = command.as_std().get_program().to_string_lossy().to_string();
        let args: Vec<_> = command
            .as_std()
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        self.calls.push((program.clone(), args.clone(), input));
        let bytes = match args.first().map(String::as_str) {
            Some("container") if args.get(1).map(String::as_str) == Some("ls") => {
                if args.iter().any(|arg| arg == "name=^/ai-phone-desktop$")
                    && self.container.is_some()
                {
                    "a".repeat(64).into_bytes()
                } else {
                    Vec::new()
                }
            }
            Some("container") => {
                let (root, running) = self.container.as_ref().unwrap();
                serde_json::to_vec(&json!({"id":"a".repeat(64),"running":running,"labels":{"com.docker.compose.project":"ai-phone","com.docker.compose.project.working_dir":root}})).unwrap()
            }
            Some("--activate-images") => {
                if self.fail_images {
                    return Err(Error::Invalid("image_import_unconfirmed"));
                }
                br#"{"AI_PHONE_IMAGE_PULL_POLICY":"never"}"#.to_vec()
            }
            Some("--service-control") if args[3] == "status" => {
                if self.assistant_running {
                    b"{}".to_vec()
                } else {
                    b"null".to_vec()
                }
            }
            Some("--service-control") => b"true".to_vec(),
            _ => Vec::new(),
        };
        Ok(std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: bytes,
            stderr: Vec::new(),
        })
    }
}
fn fixture(root: &Path, execute: Fixture) -> Backend<'_, Fixture> {
    fs::create_dir_all(root.join("data")).unwrap();
    profile::prepare(root,Path::new("/opt/aiphone/native-phone-bridge"),"android35-x86_64",br#"{"locale":"en","phone":{"cpus":2,"memory_mb":2048,"storage_gb":16},"desktop":{"cpus":2,"memory_mb":2560}}"#,PATH,"").unwrap();
    Backend {
        root,
        program: OsStr::new("native-bootstrap"),
        env: BTreeMap::new(),
        execute,
    }
}

#[tokio::test]
async fn fresh_backend_uses_native_services_and_never_creates_desktop_or_starts_phone() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    let mut backend = fixture(&root, Fixture::default());
    backend.start(Path::new("images.payload")).await.unwrap();
    let calls = &backend.execute.calls;
    let names: Vec<_> = calls
        .iter()
        .filter(|(_, args, _)| {
            args.first().map(String::as_str) == Some("--service-control") && args[3] == "start"
        })
        .map(|(_, args, _)| args[2].as_str())
        .collect();
    assert_eq!(names, ["runner", "assistant", "web", "channels", "media"]);
    assert!(calls.iter().any(|(_, args, _)| args.ends_with(&[
        "postgres".into(),
        "redis".into(),
        "ocr".into()
    ]) && args.contains(&"--no-build".into())
        && args.contains(&"never".into())));
    assert!(!calls.iter().any(|(program, args, _)| {
        program.contains("python")
            || args
                .iter()
                .any(|arg| arg.contains("android-start") || arg == "compose.desktop.yaml")
    }));
    let specifications: Vec<Value> = calls
        .iter()
        .filter(|(_, args, _)| {
            args.first().map(String::as_str) == Some("--service-control") && args[3] == "start"
        })
        .map(|(_, _, input)| serde_json::from_slice(input).unwrap())
        .collect();
    assert!(
        specifications[0]["environment"]["AI_PHONE_OCR"]
            .as_str()
            .unwrap()
            .ends_with("/runtime/services/bin/vision")
    );
    assert_eq!(specifications[2]["environment"]["PORT"], "17860");
    for index in [1, 3, 4] {
        assert!(specifications[index]["environment"].get("PORT").is_none());
        assert!(
            specifications[index]["environment"]
                .get("AI_PHONE_OCR")
                .is_none()
        );
    }
}

#[tokio::test]
async fn resume_keeps_tokens_skips_live_database_and_only_starts_exact_stopped_desktop() {
    for running in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().canonicalize().unwrap();
        let mut backend = fixture(
            &root,
            Fixture {
                container: Some((root.to_str().unwrap().into(), running)),
                assistant_running: true,
                ..Fixture::default()
            },
        );
        fs::create_dir_all(root.join("data/assistant")).unwrap();
        fs::write(root.join("data/assistant/token"), "keep-token").unwrap();
        backend.start(Path::new("images.payload")).await.unwrap();
        assert_eq!(
            fs::read(root.join("data/assistant/token")).unwrap(),
            b"keep-token"
        );
        assert!(
            !backend
                .execute
                .calls
                .iter()
                .any(|(_, args, _)| args.contains(&"--migrate".into())
                    || args.contains(&"--prepare-database".into()))
        );
        let starts: Vec<_> = backend
            .execute
            .calls
            .iter()
            .filter(|(_, args, _)| args.first().map(String::as_str) == Some("start"))
            .collect();
        assert_eq!(starts.len(), usize::from(!running));
        if !running {
            assert_eq!(starts[0].1, ["start".into(), "a".repeat(64)]);
        }
    }
}

#[tokio::test]
async fn foreign_container_rejects_before_import_database_or_service_changes() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    let mut backend = fixture(
        &root,
        Fixture {
            container: Some(("/another-installation".into(), false)),
            ..Fixture::default()
        },
    );
    assert_eq!(
        backend
            .start(Path::new("images.payload"))
            .await
            .unwrap_err()
            .code(),
        "foreign_installation"
    );
    assert!(
        backend
            .execute
            .calls
            .iter()
            .all(|(_, args, _)| args[0] == "container")
    );
}

#[tokio::test]
async fn unknown_image_import_does_not_run_database_or_services() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    let mut backend = fixture(
        &root,
        Fixture {
            fail_images: true,
            ..Fixture::default()
        },
    );
    assert_eq!(
        backend
            .start(Path::new("images.payload"))
            .await
            .unwrap_err()
            .code(),
        "image_import_unconfirmed"
    );
    assert!(
        !backend
            .execute
            .calls
            .iter()
            .any(|(_, args, _)| args[0] == "compose" || args[0] == "--service-control")
    );
}

#[tokio::test]
async fn cancelled_process_does_not_spawn() {
    let (_, stop) = watch::channel(true);
    let mut process = Process(stop);
    assert_eq!(
        process
            .run(Command::new("this-program-must-not-run"), Vec::new())
            .await
            .unwrap_err()
            .code(),
        "installation_cancelled"
    );
}

#[tokio::test]
async fn real_process_transports_unicode_input_and_closes_stdin() {
    let (_send, stop) = watch::channel(false);
    let mut process = Process(stop);
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "/bin/cat"]);
    let output = process
        .run(command, "真实输入\n".as_bytes().to_vec())
        .await
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, "真实输入\n".as_bytes());
}

#[tokio::test]
async fn real_cancel_reaps_the_owned_command() {
    let folder = tempfile::tempdir().unwrap();
    let pid_file = folder.path().join("child.pid");
    let (send, stop) = watch::channel(false);
    let mut process = Process(stop);
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "echo $$ > \"$1.tmp\"; /bin/mv \"$1.tmp\" \"$1\"; exec /bin/sleep 60",
            "fixture",
        ])
        .arg(&pid_file);
    let marker = pid_file.clone();
    let cancellation = tokio::spawn(async move {
        let until = Instant::now() + Duration::from_secs(5);
        while !marker.is_file() {
            assert!(Instant::now() < until);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let pid = rustix::process::Pid::from_raw(
            fs::read_to_string(&marker).unwrap().trim().parse().unwrap(),
        )
        .unwrap();
        rustix::process::test_kill_process(pid).unwrap();
        send.send_replace(true);
        pid
    });
    assert_eq!(
        process.run(command, Vec::new()).await.unwrap_err().code(),
        "installation_cancelled"
    );
    let pid = cancellation.await.unwrap();
    assert_eq!(
        rustix::process::test_kill_process(pid),
        Err(rustix::io::Errno::SRCH),
        "command remained alive after cancellation"
    );
}

#[tokio::test]
async fn output_overflow_closes_and_reaps_the_command_without_waiting_for_cancellation() {
    let (_send, stop) = watch::channel(false);
    let mut process = Process(stop);
    let mut command = Command::new("/bin/sh");
    command.args(["-c","while :; do printf '012345678901234567890123456789012345678901234567890123456789012345'; done"]);
    let started = Instant::now();
    let error = process.run(command, Vec::new()).await.unwrap_err();
    assert!(matches!(error, Error::Io(_)));
    assert!(started.elapsed() < Duration::from_secs(5));
}
