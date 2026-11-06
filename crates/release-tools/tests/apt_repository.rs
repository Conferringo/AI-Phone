//! The real APT client uses only a private loopback repository and package database.
#[cfg(target_os = "linux")]
mod linux {
    use serde_json::Value;
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        net::{TcpListener, TcpStream},
        os::unix::fs::symlink,
        path::{Path, PathBuf},
        process::{Command, Output},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    };

    fn checked(command: &mut Command) -> Output {
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{command:?}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    struct Keys(PathBuf);
    impl Drop for Keys {
        fn drop(&mut self) {
            let _ = Command::new("gpgconf")
                .arg("--homedir")
                .arg(&self.0)
                .args(["--kill", "gpg-agent"])
                .status();
        }
    }

    struct Server {
        address: std::net::SocketAddr,
        stopping: Arc<AtomicBool>,
        worker: Option<thread::JoinHandle<()>>,
    }
    impl Server {
        fn new(root: PathBuf) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let stopping = Arc::new(AtomicBool::new(false));
            let flag = stopping.clone();
            let worker = thread::spawn(move || {
                for incoming in listener.incoming() {
                    let mut socket = incoming.unwrap();
                    if flag.load(Ordering::Acquire) {
                        break;
                    }
                    let mut request = BufReader::new(&socket);
                    let mut first = String::new();
                    request.read_line(&mut first).unwrap();
                    let mut header = String::new();
                    loop {
                        header.clear();
                        if request.read_line(&mut header).unwrap() == 0 || header == "\r\n" {
                            break;
                        }
                    }
                    let mut parts = first.split_whitespace();
                    let method = parts.next().unwrap();
                    let relative = parts.next().unwrap().trim_start_matches('/');
                    let (status, body) = match fs::read(root.join(relative)) {
                        Ok(body) => ("200 OK", body),
                        Err(_) => ("404 Not Found", Vec::new()),
                    };
                    write!(
                        socket,
                        "HTTP/1.0 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .unwrap();
                    if method != "HEAD" {
                        socket.write_all(&body).unwrap();
                    }
                }
            });
            Self {
                address,
                stopping,
                worker: Some(worker),
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stopping.store(true, Ordering::Release);
            let _ = TcpStream::connect(self.address);
            self.worker.take().unwrap().join().unwrap();
        }
    }

    fn tool(operation: &str, output: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_release-tools"));
        command.args(["apt", operation, "--output"]).arg(output);
        command
    }

    #[test]
    fn invalid_invocations_do_not_create_or_replace_output() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("keep absent");
        for arguments in [["apt", "index"], ["apt", "sign"], ["apt", "unknown"]] {
            let result = Command::new(env!("CARGO_BIN_EXE_release-tools"))
                .args(arguments)
                .arg("--output")
                .arg(&output)
                .output()
                .unwrap();
            assert!(!result.status.success());
            assert!(result.stdout.is_empty());
            assert!(!output.exists());
        }
    }

    #[test]
    fn real_signed_apt_upgrade_preserves_data_and_rejects_tampering() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let keys = Keys(root.join("keys"));
        fs::create_dir(&keys.0).unwrap();
        checked(Command::new("gpg").arg("--homedir").arg(&keys.0).args([
            "--batch",
            "--pinentry-mode",
            "loopback",
            "--passphrase",
            "",
            "--quick-generate-key",
            "AI Phone fixture",
            "ed25519",
            "sign",
            "1d",
        ]));
        let listing = checked(
            Command::new("gpg")
                .arg("--homedir")
                .arg(&keys.0)
                .args(["--with-colons", "--list-secret-keys"]),
        );
        let listing = String::from_utf8(listing.stdout).unwrap();
        let key = listing
            .lines()
            .find(|line| line.starts_with("fpr:"))
            .unwrap()
            .split(':')
            .nth(9)
            .unwrap();
        let architecture =
            String::from_utf8(checked(Command::new("dpkg").arg("--print-architecture")).stdout)
                .unwrap();
        let architecture = architecture.trim();
        let repos = root.join("repos");
        fs::create_dir(&repos).unwrap();
        let server = Server::new(repos.clone());
        let base = format!("http://{}/current/", server.address);
        let repository = repos.join("signed");
        symlink("signed", repos.join("current")).unwrap();
        let target = root.join("installed");
        let status = target.join("var/lib/dpkg/status");
        fs::create_dir_all(status.parent().unwrap()).unwrap();
        fs::write(&status, b"").unwrap();
        for folder in [
            "state/lists/partial",
            "cache/archives/partial",
            "logs",
            "installed/home/user",
            "downloads",
        ] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        let sentinel = target.join("home/user/work.txt");
        fs::write(&sentinel, b"keep").unwrap();
        let user = String::from_utf8(checked(Command::new("id").arg("-un")).stdout).unwrap();
        let apt = || {
            let mut command = Command::new("apt-get");
            for option in [
                format!(
                    "Dir::Etc::sourcelist={}",
                    root.join("ai-phone.sources").display()
                ),
                "Dir::Etc::sourceparts=-".into(),
                format!("Dir::State={}", root.join("state").display()),
                format!("Dir::State::status={}", status.display()),
                format!("Dir::Cache={}", root.join("cache").display()),
                format!("Dir::Log={}", root.join("logs").display()),
                "Debug::NoLocking=true".into(),
                format!("APT::Sandbox::User={}", user.trim()),
                format!("DPkg::Options::=--root={}", target.display()),
                "DPkg::Options::=--force-not-root".into(),
                "DPkg::Use-Pty=0".into(),
            ] {
                command.args(["-o", &option]);
            }
            command.arg("--assume-yes");
            command
        };
        let mut last_deb = PathBuf::new();
        for version in ["1.0", "2.0"] {
            let package = root.join(version);
            fs::create_dir_all(package.join("DEBIAN")).unwrap();
            fs::write(package.join("DEBIAN/control"), format!("Package: aiphone-apt-fixture\nVersion: {version}\nArchitecture: {architecture}\nMaintainer: AI Phone fixture\nDescription: Isolated package\n")).unwrap();
            let payload = package.join("usr/share/aiphone-apt-fixture/version");
            fs::create_dir_all(payload.parent().unwrap()).unwrap();
            fs::write(&payload, version).unwrap();
            let deb = root.join(format!("aiphone-apt-fixture_{version}_{architecture}.deb"));
            checked(
                Command::new("dpkg-deb")
                    .args(["--build", "--root-owner-group"])
                    .arg(&package)
                    .arg(&deb),
            );
            let operation = if version == "1.0" { "build" } else { "index" };
            if version == "2.0" {
                assert!(repository.join("InRelease").is_file());
                fs::write(repository.join("maintainer-note"), "keep").unwrap();
            }
            let mut command = tool(operation, &repository);
            command.arg(&deb);
            if operation == "build" {
                command
                    .args(["--base-url", &base, "--key", key, "--gnupg-home"])
                    .arg(&keys.0);
            }
            let result: Value = serde_json::from_slice(&checked(&mut command).stdout).unwrap();
            assert_eq!(
                result["packages"],
                serde_json::json!([deb.file_name().unwrap().to_str().unwrap()])
            );
            if version == "2.0" {
                assert_eq!(result["metadata"].as_array().unwrap().len(), 3);
                for name in [
                    "InRelease",
                    "Release.gpg",
                    "ai-phone-archive-key.asc",
                    "ai-phone.sources",
                ] {
                    assert!(!repository.join(name).exists());
                }
                assert_eq!(
                    fs::read_to_string(repository.join("maintainer-note")).unwrap(),
                    "keep"
                );
                checked(
                    tool("sign", &repository)
                        .args(["--base-url", &base, "--key", key, "--gnupg-home"])
                        .arg(&keys.0),
                );
            } else {
                assert_eq!(result["metadata"].as_array().unwrap().len(), 7);
            }
            symlink(&deb, repository.join(deb.file_name().unwrap())).unwrap();
            fs::copy(
                repository.join("ai-phone.sources"),
                root.join("ai-phone.sources"),
            )
            .unwrap();
            checked(apt().arg("update"));
            checked(apt().args(["install", "aiphone-apt-fixture"]));
            assert_eq!(
                fs::read_to_string(target.join("usr/share/aiphone-apt-fixture/version")).unwrap(),
                version
            );
            assert_eq!(fs::read(&sentinel).unwrap(), b"keep");
            last_deb = deb;
        }
        let mut body = fs::read(&last_deb).unwrap();
        body[0] ^= 1;
        fs::write(&last_deb, body).unwrap();
        let changed = apt()
            .args(["download", "aiphone-apt-fixture"])
            .current_dir(root.join("downloads"))
            .output()
            .unwrap();
        assert!(!changed.status.success());
        assert!(
            String::from_utf8_lossy(&changed.stderr)
                .to_lowercase()
                .contains("hash sum mismatch")
        );
        let signature = repository.join("InRelease");
        let body = fs::read_to_string(&signature)
            .unwrap()
            .replace("Origin: AI Phone", "Origin: Changed");
        fs::write(signature, body).unwrap();
        let changed = apt()
            .args(["-o", "APT::Update::Error-Mode=any", "update"])
            .output()
            .unwrap();
        assert!(!changed.status.success());
        assert!(
            String::from_utf8_lossy(&changed.stderr)
                .to_lowercase()
                .contains("signature")
        );
    }
}
