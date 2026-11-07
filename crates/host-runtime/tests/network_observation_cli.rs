#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};
const DEVICE: &str = "11111111-1111-4111-8111-111111111111";
fn call(root: &Path, op: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_host-resources"))
        .args([
            "--observe-device-network",
            root.to_str().unwrap(),
            "desktop",
            DEVICE,
            op,
        ])
        .env("AI_PHONE_DOCKER", root.join("docker"))
        .env("NETWORK_FIXTURE_LOG", root.join("commands"))
        .output()
        .unwrap()
}
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("data/device-network").join(DEVICE);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.json"),r#"{"revision":7,"enabled":true,"subscriptionUrl":"https://fixture.invalid/private","nodeCount":4}"#).unwrap();
    fs::write(
        root.path().join("docker"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$NETWORK_FIXTURE_LOG\"\nprintf 'app\\ntun\\n'\n",
    )
    .unwrap();
    fs::set_permissions(
        root.path().join("docker"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    root
}
fn parsed(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn credentials_are_read_without_a_guest_process_or_private_metadata_writes() {
    let root = fixture();
    let file = root
        .path()
        .join("data/device-network")
        .join(DEVICE)
        .join("source.json");
    let before = fs::read(&file).unwrap();
    assert_eq!(
        parsed(call(root.path(), "credential")),
        json!({"subscriptionUrl":"https://fixture.invalid/private","revision":7})
    );
    assert!(!root.path().join("commands").exists());
    assert_eq!(fs::read(file).unwrap(), before);
}
#[test]
fn application_owned_state_uses_one_guest_read_and_never_exposes_the_saved_subscription() {
    let root = fixture();
    let dir = root.path().join("data/device-network").join(DEVICE);
    fs::write(dir.join("app-managed.json"), "{}").unwrap();
    let output = parsed(call(root.path(), "state"));
    assert_eq!(output["managedBy"], "application");
    assert_eq!(output["status"], "running");
    assert_eq!(output["revision"], 7);
    assert_eq!(output["tunActive"], true);
    assert_eq!(output["enabled"], true);
    assert_eq!(output["hasSubscription"], true);
    assert!(!output.to_string().contains("fixture.invalid/private"));
    assert_eq!(
        fs::read_to_string(root.path().join("commands"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(!dir.join("job.json").exists());
    assert!(!dir.join("checks.json").exists());
}
#[test]
fn unknown_application_status_is_not_reported_as_stopped_and_corrupt_state_is_not_replaced() {
    let root = fixture();
    let dir = root.path().join("data/device-network").join(DEVICE);
    fs::write(dir.join("app-managed.json"), "{}").unwrap();
    fs::write(root.path().join("docker"), "#!/bin/sh\nexit 8\n").unwrap();
    let output = parsed(call(root.path(), "state"));
    assert_eq!(output["status"], "unknown");
    assert_eq!(output["error"], "设备状态暂未确认");
    fs::write(dir.join("source.json"), "broken-private-json").unwrap();
    let failed = call(root.path(), "credential");
    assert!(!failed.status.success());
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("broken-private-json"));
    assert_eq!(
        fs::read_to_string(dir.join("source.json")).unwrap(),
        "broken-private-json"
    );
}
fn legacy_engine(root: &Path) {
    fs::write(root.join("docker"),r##"#!/bin/sh
printf '%s\n' "$*" >> "$NETWORK_FIXTURE_LOG"
case "$*" in
 *"curl --config -"*)
  config=$(cat)
  printf '%s\n' "$config" >> "$NETWORK_FIXTURE_LOG.stdin"
  case "$config" in *'/version'*) printf '{"version":"native-fixture"}';; *) printf '{"tun":{"enable":true}}';; esac;;
 *'ip link show aiphone0'*) printf active;;
 *) printf stopped;;
esac
"##).unwrap();
}
#[test]
fn legacy_journal_keeps_interrupted_running_and_committed_results_distinct() {
    let root = fixture();
    legacy_engine(root.path());
    let dir = root.path().join("data/device-network").join(DEVICE);
    fs::write(
        dir.join("config.json"),
        r#"{"secret":"fixture-private-core-secret"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("checks.json"),
        r#"{"revision":6,"at":11,"items":[{"name":"old"}]}"#,
    )
    .unwrap();
    fs::write(
        dir.join("job.json"),
        format!(
            r#"{{"status":"applying","requestId":"request","pid":{}}}"#,
            std::process::id()
        ),
    )
    .unwrap();
    let running = parsed(call(root.path(), "state"));
    assert_eq!(running["status"], "applying");
    assert_eq!(running["coreVersion"], "native-fixture");
    assert_eq!(running["checks"], json!([]));
    fs::write(
        dir.join("job.json"),
        r#"{"status":"applying","requestId":"request","pid":2147483647}"#,
    )
    .unwrap();
    assert_eq!(parsed(call(root.path(), "state"))["status"], "interrupted");
    let source = r#"{"revision":7,"enabled":true,"requestId":"request","saved":"untouched"}"#;
    fs::write(dir.join("source.json"), source).unwrap();
    let committed = parsed(call(root.path(), "state"));
    assert_eq!(committed["status"], "running");
    assert_eq!(committed["error"], "");
    assert_eq!(fs::read_to_string(dir.join("source.json")).unwrap(), source);
    let commands = fs::read_to_string(root.path().join("commands")).unwrap();
    assert!(!commands.contains("fixture-private-core-secret"));
    let configs = fs::read_to_string(root.path().join("commands.stdin")).unwrap();
    assert!(configs.contains("noproxy = \"*\""));
    assert!(configs.contains("Authorization: Bearer fixture-private-core-secret"));
}
#[test]
fn probe_records_each_result_atomically_and_retains_configuration() {
    let root = fixture();
    let dir = root.path().join("data/device-network").join(DEVICE);
    fs::write(dir.join("app-managed.json"), "{}").unwrap();
    fs::create_dir(root.path().join("config")).unwrap();
    fs::write(
        root.path().join("config/mihomo-release.json"),
        json!({"verification_download":{"sha256":"a".repeat(64),"size_bytes":38}}).to_string(),
    )
    .unwrap();
    fs::write(root.path().join("docker"),format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$NETWORK_FIXTURE_LOG\"\ncase \"$*\" in *example.com*) exit 9;; *sha256sum*) printf '38\\n{}  fixture\\n';; *) printf 'app\\ntun\\n';; esac\n","a".repeat(64))).unwrap();
    let before = fs::read(dir.join("source.json")).unwrap();
    let output = parsed(call(root.path(), "probe"));
    assert_eq!(output["checks"].as_array().unwrap().len(), 3);
    assert_eq!(output["checks"][0]["passed"], true);
    assert_eq!(output["checks"][1]["passed"], false);
    assert_eq!(output["checks"][2]["passed"], true);
    let published: Value =
        serde_json::from_slice(&fs::read(dir.join("checks.json")).unwrap()).unwrap();
    assert_eq!(published["revision"], 7);
    assert!(published["at"].as_u64().unwrap() > 0);
    assert_eq!(fs::read(dir.join("source.json")).unwrap(), before);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 3);
    let commands = fs::read_to_string(root.path().join("commands")).unwrap();
    assert_eq!(commands.lines().count(), 4);
    assert!(!commands.contains("python"));
    assert!(commands.contains("curl --noproxy '*'"));
    assert!(commands.contains("rm -f '/etc/ai-phone-network/probe-"));
    fs::remove_file(dir.join("source.json")).unwrap();
    parsed(call(root.path(), "probe"));
    let fresh: Value = serde_json::from_slice(&fs::read(dir.join("checks.json")).unwrap()).unwrap();
    assert!(fresh.get("revision").is_none());
    fs::remove_file(root.path().join("config/mihomo-release.json")).unwrap();
    let missing_pin = parsed(call(root.path(), "probe"));
    assert_eq!(missing_pin["checks"][0]["passed"], true);
    assert_eq!(missing_pin["checks"][2]["passed"], false);
}
#[test]
fn phone_credentials_and_rejected_targets_require_no_adb_server() {
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("data/device-network/phone-local-01");
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("source.json"),
        r#"{"revision":2,"subscriptionUrl":"https://fixture.invalid/phone"}"#,
    )
    .unwrap();
    let run = |id: &str| {
        Command::new(env!("CARGO_BIN_EXE_host-resources"))
            .args([
                "--observe-device-network",
                root.path().to_str().unwrap(),
                "phone",
                id,
                "credential",
            ])
            .env_clear()
            .output()
            .unwrap()
    };
    assert_eq!(
        parsed(run("phone-local-01")),
        json!({"revision":2,"subscriptionUrl":"https://fixture.invalid/phone"})
    );
    assert!(!run("../outside").status.success());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}
#[test]
fn controller_uses_real_http_and_passes_quoted_auth_only_over_stdin() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::{Duration, Instant},
    };
    let root = fixture();
    let dir = root.path().join("data/device-network").join(DEVICE);
    let secret = r#"qa-quoted-"x\y"#;
    fs::write(
        dir.join("config.json"),
        json!({"secret":secret}).to_string(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(5);
        let mut requests = vec![];
        while requests.len() < 2 {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = vec![];
                    let mut part = [0; 1024];
                    while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                        let n = stream.read(&mut part).unwrap();
                        assert!(n > 0 && request.len() + n <= 8192);
                        request.extend_from_slice(&part[..n]);
                    }
                    let request = String::from_utf8(request).unwrap();
                    assert!(request.contains(&format!("Authorization: Bearer {secret}\r\n")));
                    let path = request.split_whitespace().nth(1).unwrap().to_owned();
                    let body = if path == "/version" {
                        r#"{"version":"real-http"}"#
                    } else {
                        r#"{"tun":{"enable":true}}"#
                    };
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .unwrap();
                    requests.push(path);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < until, "controller HTTP request missing");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("{e}"),
            }
        }
        requests.sort();
        requests
    });
    fs::write(
        root.path().join("docker"),
        format!(
            r##"#!/bin/sh
printf '%s\n' "$*" >> "$NETWORK_FIXTURE_LOG"
case "$*" in
 *'curl --config -'*) sed 's/127.0.0.1:19090/127.0.0.1:{port}/g' | /usr/bin/curl --config -;;
 *'ip link show aiphone0'*) printf active;;
 *) printf stopped;;
esac
"##
        ),
    )
    .unwrap();
    let result = parsed(call(root.path(), "health"));
    assert_eq!(server.join().unwrap(), ["/configs", "/version"]);
    assert_eq!(
        result,
        json!({"running":true,"confirmed":true,"version":"real-http","tun":true})
    );
    assert!(
        !fs::read_to_string(root.path().join("commands"))
            .unwrap()
            .contains(secret)
    );
}
