use super::*;
use std::os::unix::process::ExitStatusExt;
use std::{cell::RefCell, path::PathBuf, rc::Rc};

#[derive(Default)]
struct Fixture {
    calls: Vec<(String, Vec<String>, Vec<u8>)>,
    container: Option<(String, bool)>,
    assistant_running: bool,
    fail_images: bool,
    trace: Rc<RefCell<Vec<String>>>,
    fail: Option<String>,
    nonzero: Option<String>,
    inspection: Option<Value>,
    socket: Option<String>,
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
        let event = match args.first().map(String::as_str) {
            Some("--service-control") => format!("{}-{}", args[2], args[3]),
            Some("container") => args[1].clone(),
            Some(value) => value.into(),
            None => "none".into(),
        };
        self.trace.borrow_mut().push(event.clone());
        if let Some(socket) = &self.socket {
            let selected = command
                .as_std()
                .get_envs()
                .find(|(key, _)| *key == "DOCKER_HOST")
                .and_then(|(_, value)| value);
            assert_eq!(selected, Some(OsStr::new(socket)));
        }
        if self.fail.as_ref() == Some(&event) {
            return Err(Error::Invalid("injected_initialization_failure"));
        }
        if self.nonzero.as_ref() == Some(&event) {
            return Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(256),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        }
        let bytes = match args.first().map(String::as_str) {
            Some("--stage-source") => {
                let root = Path::new(&args[2]).join("releases").join(&args[3]);
                fs::create_dir_all(root.join("data")).unwrap();
                format!("{} {}\n", args[3], root.canonicalize().unwrap().display()).into_bytes()
            }
            Some("container") if args.get(1).map(String::as_str) == Some("ls") => {
                if args.iter().any(|arg| arg == "name=^/ai-phone-desktop$")
                    && (self.container.is_some() || self.inspection.is_some())
                {
                    "a".repeat(64).into_bytes()
                } else {
                    Vec::new()
                }
            }
            Some("container") => {
                if let Some(value) = &self.inspection {
                    return Ok(std::process::Output {
                        status: std::process::ExitStatus::from_raw(0),
                        stdout: serde_json::to_vec(value).unwrap(),
                        stderr: Vec::new(),
                    });
                }
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

const REQUEST:&[u8]=br#"{"locale":"en","phone":{"cpus":2,"memory_mb":2048,"storage_gb":16},"desktop":{"cpus":2,"memory_mb":2560}}"#;
struct Init {
    _folder: tempfile::TempDir,
    bundle: PathBuf,
    home: PathBuf,
    run: PathBuf,
    root: PathBuf,
}
impl Init {
    fn new() -> Self {
        let folder = tempfile::tempdir().unwrap();
        let bundle = folder.path().join("bundle");
        fs::create_dir(&bundle).unwrap();
        fs::write(bundle.join("source.sha256"), "a".repeat(64)).unwrap();
        let home = folder.path().join("user/installation");
        let root = home.join("releases").join("a".repeat(64));
        let run = folder.path().join("run");
        Self {
            _folder: folder,
            bundle,
            home,
            run,
            root,
        }
    }
    async fn initialize(&self, fixture: Fixture, free: u64) -> Result<()> {
        initialize(
            &self.bundle,
            &self.home,
            &self.run,
            REQUEST,
            fixture,
            &|_| Ok(free),
        )
        .await
    }
}

#[test]
fn owned_environment_removes_foreign_context_without_changing_the_callers_state() {
    let parent = Path::new("/var/lib/aiphone/installation");
    let inherited: Vec<_> = FOREIGN_ENGINE
        .map(|key| (key.into(), "private-foreign-value".into()))
        .into();
    let original = inherited.clone();
    let env = environment(parent, Path::new("/run/aiphone"), inherited.clone()).unwrap();
    assert_eq!(inherited, original);
    assert_eq!(env["DOCKER_HOST"], "unix:///run/aiphone/docker.sock");
    assert_eq!(env["DOCKER_CONFIG"], "/var/lib/aiphone/docker-cli");
    for key in FOREIGN_ENGINE {
        if !["DOCKER_HOST", "DOCKER_CONFIG"].contains(&key) {
            assert!(!env.contains_key(key));
        }
    }
}

#[tokio::test]
async fn first_boot_runs_every_native_phase_with_one_owned_socket() {
    let init = Init::new();
    let fixture = Fixture {
        socket: Some(format!("unix://{}", init.run.join("docker.sock").display())),
        ..Fixture::default()
    };
    let trace = fixture.trace.clone();
    init.initialize(fixture, u64::MAX).await.unwrap();
    assert_eq!(
        *trace.borrow(),
        [
            "--stage-source",
            "wsl-engine-start",
            "info",
            "ls",
            "ls",
            "ls",
            "ls",
            "--activate-images",
            "compose",
            "assistant-status",
            "--prepare-database",
            "--migrate",
            "runner-start",
            "assistant-start",
            "web-start",
            "channels-start",
            "media-start"
        ]
    );
    assert!(init.root.join("data/deployment/profile.json").is_file());
    assert!(init.root.join("data/assistant/token").is_file());
}

#[tokio::test]
async fn each_failed_initialization_phase_stops_before_later_effects() {
    let stages = [
        "--stage-source",
        "wsl-engine-start",
        "info",
        "ls",
        "--activate-images",
        "compose",
        "assistant-status",
        "--prepare-database",
        "--migrate",
        "runner-start",
        "assistant-start",
        "web-start",
        "channels-start",
        "media-start",
    ];
    for stage in stages {
        let init = Init::new();
        let fixture = Fixture {
            fail: Some(stage.into()),
            ..Fixture::default()
        };
        let trace = fixture.trace.clone();
        assert_eq!(
            init.initialize(fixture, u64::MAX).await.unwrap_err().code(),
            "injected_initialization_failure"
        );
        assert_eq!(trace.borrow().last().unwrap(), stage);
    }
}

#[tokio::test]
async fn initialization_lock_and_low_space_never_start_source_or_services() {
    let init = Init::new();
    let fixture = Fixture::default();
    let trace = fixture.trace.clone();
    assert_eq!(
        init.initialize(fixture, 4 * 1024 * 1024 * 1024 - 1)
            .await
            .unwrap_err()
            .code(),
        "source_resource_space"
    );
    assert!(trace.borrow().is_empty());
    assert!(!init.root.exists());
    let parent = init.home.parent().unwrap();
    let lock = filesystem::lease(&parent.join("initialize.lock"))
        .unwrap()
        .unwrap();
    let fixture = Fixture::default();
    let trace = fixture.trace.clone();
    assert_eq!(
        init.initialize(fixture, u64::MAX).await.unwrap_err().code(),
        "installation_running"
    );
    assert!(trace.borrow().is_empty());
    drop(lock);
    init.initialize(Fixture::default(), u64::MAX).await.unwrap();
}

#[tokio::test]
async fn quarantined_and_lima_owned_state_stop_before_engine_or_profile_changes() {
    for (name, code) in [
        ("data/restore-quarantine.json", "restored_state_quarantined"),
        (
            "data/deployment/container-host.json",
            "foreign_installation",
        ),
    ] {
        let init = Init::new();
        let marker = init.root.join(name);
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        fs::write(&marker, b"keep-owned-state").unwrap();
        let fixture = Fixture::default();
        let trace = fixture.trace.clone();
        assert_eq!(
            init.initialize(fixture, u64::MAX).await.unwrap_err().code(),
            code
        );
        assert_eq!(*trace.borrow(), ["--stage-source"]);
        assert_eq!(fs::read(marker).unwrap(), b"keep-owned-state");
        assert!(!init.root.join("data/deployment/profile.json").exists());
    }
}

#[tokio::test]
async fn resumed_initialization_keeps_existing_profile_password_tokens_and_chats() {
    let init = Init::new();
    fs::create_dir_all(init.root.join("data")).unwrap();
    profile::prepare(
        &init.root,
        Path::new("/existing/sdk"),
        "android35-x86_64",
        REQUEST,
        "/existing/path",
        "",
    )
    .unwrap();
    for name in [
        "data/assistant/token",
        "data/phone-runner/token",
        "data/channels/token",
        "data/assistant/chat",
    ] {
        let path = init.root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"keep-existing").unwrap();
    }
    let paths = [
        ".env",
        "data/deployment/profile.json",
        "data/assistant/token",
        "data/phone-runner/token",
        "data/channels/token",
        "data/assistant/chat",
    ];
    let before: Vec<_> = paths
        .map(|path| fs::read(init.root.join(path)).unwrap())
        .into();
    let fixture = Fixture {
        assistant_running: true,
        ..Fixture::default()
    };
    let trace = fixture.trace.clone();
    init.initialize(fixture, u64::MAX).await.unwrap();
    for (path, bytes) in paths.iter().zip(before) {
        assert_eq!(fs::read(init.root.join(path)).unwrap(), bytes);
    }
    assert!(
        !trace
            .borrow()
            .iter()
            .any(|step| step == "--prepare-database" || step == "--migrate")
    );
}

#[tokio::test]
async fn malformed_container_receipts_and_failed_listing_stop_before_import() {
    for inspection in [
        Value::Null,
        json!({}),
        json!({"id":1,"running":false,"labels":{}}),
        json!({"id":"a".repeat(64),"running":"false","labels":{}}),
    ] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().canonicalize().unwrap();
        let mut backend = fixture(
            &root,
            Fixture {
                inspection: Some(inspection),
                ..Fixture::default()
            },
        );
        assert_eq!(
            backend
                .start(Path::new("images.payload"), Path::new("/root"))
                .await
                .unwrap_err()
                .code(),
            "container_ownership"
        );
        assert!(
            backend
                .execute
                .calls
                .iter()
                .all(|(_, args, _)| args[0] == "container")
        );
    }
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    let mut backend = fixture(
        &root,
        Fixture {
            nonzero: Some("ls".into()),
            ..Fixture::default()
        },
    );
    assert_eq!(
        backend
            .start(Path::new("images.payload"), Path::new("/root"))
            .await
            .unwrap_err()
            .code(),
        "wsl_initializer_command"
    );
    assert_eq!(backend.execute.calls.len(), 1);
}

#[tokio::test]
async fn unavailable_engine_reaches_its_deadline_instead_of_claiming_ready() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    let mut backend = fixture(
        &root,
        Fixture {
            nonzero: Some("info".into()),
            ..Fixture::default()
        },
    );
    assert_eq!(
        backend
            .wait_engine(Instant::now())
            .await
            .unwrap_err()
            .code(),
        "wsl_engine_not_ready"
    );
    assert_eq!(backend.execute.calls.len(), 1);
    let mut backend = fixture(&root, Fixture::default());
    backend.wait_engine(Instant::now()).await.unwrap();
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
    backend
        .start(Path::new("images.payload"), Path::new("/root"))
        .await
        .unwrap();
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
        backend
            .start(Path::new("images.payload"), Path::new("/root"))
            .await
            .unwrap();
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
            .start(Path::new("images.payload"), Path::new("/root"))
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
            .start(Path::new("images.payload"), Path::new("/root"))
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
async fn invalid_resources_are_rejected_before_creating_installation_state() {
    let folder = tempfile::tempdir().unwrap();
    let bundle = folder.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    fs::write(bundle.join("source.sha256"), "a".repeat(64)).unwrap();
    let parent = folder.path().join("owned");
    let home = parent.join("installation");
    let error = initialize(
        &bundle,
        &home,
        &folder.path().join("run"),
        b"{}",
        Fixture::default(),
        &|_| Ok(u64::MAX),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code(), "wsl_initialization_request");
    assert!(!parent.exists());
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
