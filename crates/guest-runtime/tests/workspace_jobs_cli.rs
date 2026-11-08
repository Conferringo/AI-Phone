use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
use uuid::Uuid;
fn command(root: &Path, operation: &str, job: Uuid) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_desktop-bridge"));
    cmd.current_dir(root).args([operation, &job.to_string()]);
    cmd
}
fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn ready(path: &Path) {
    let end = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < end, "job did not become ready");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn runs_in_the_workspace_streams_output_and_preserves_artifacts() {
    let root = tempfile::Builder::new()
        .prefix("workspace 空间,\"-")
        .tempdir()
        .unwrap();
    let job = Uuid::new_v4();
    let output = command(root.path(), "workspace-run", job)
        .arg("printf 'done\\n'; printf 'artifact' > result.txt")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(output.stdout, b"done\n");
    assert_eq!(
        fs::read(root.path().join("result.txt")).unwrap(),
        b"artifact"
    );
    assert_eq!(fs::read_dir(root.path().join(".jobs")).unwrap().count(), 0);
}
#[test]
fn shell_failure_keeps_its_exit_status() {
    let root = tempfile::tempdir().unwrap();
    let output = command(root.path(), "workspace-run", Uuid::new_v4())
        .arg("printf 'failed' >&2; exit 7")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stderr, b"failed");
}
#[test]
fn cancellation_before_start_prevents_execution_and_consumes_its_intent() {
    let root = tempfile::tempdir().unwrap();
    let job = Uuid::new_v4();
    success(
        command(root.path(), "workspace-cancel", job)
            .output()
            .unwrap(),
    );
    let output = command(root.path(), "workspace-run", job)
        .arg("touch unexpected")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(130));
    assert!(!root.path().join("unexpected").exists());
    assert_eq!(fs::read_dir(root.path().join(".jobs")).unwrap().count(), 0);
}
#[test]
fn cancellation_is_per_job_and_waits_for_the_process_to_stop() {
    let root = tempfile::tempdir().unwrap();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let mut first = command(root.path(), "workspace-run", a)
        .arg("/bin/sleep 20")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut second = command(root.path(), "workspace-run", b)
        .arg("while [ ! -f release ]; do /bin/sleep 0.02; done; printf B > result")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    ready(&root.path().join(format!(".jobs/{a}.pid")));
    ready(&root.path().join(format!(".jobs/{b}.pid")));
    success(
        command(root.path(), "workspace-cancel", a)
            .output()
            .unwrap(),
    );
    assert_eq!(first.wait().unwrap().code(), Some(130));
    assert!(second.try_wait().unwrap().is_none());
    fs::write(root.path().join("release"), b"go").unwrap();
    assert!(second.wait().unwrap().success());
    assert_eq!(fs::read(root.path().join("result")).unwrap(), b"B");
    assert_eq!(fs::read_dir(root.path().join(".jobs")).unwrap().count(), 0);
}

fn stopped(pid: &str) {
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        match fs::read_to_string(format!("/proc/{}/stat", pid.trim())) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Ok(value)
                if value
                    .split_once(") ")
                    .is_some_and(|(_, tail)| tail.starts_with('Z')) =>
            {
                return;
            }
            result => {
                assert!(Instant::now() < end, "job process still alive: {result:?}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
#[test]
fn completion_terminates_background_children_before_returning() {
    let root = tempfile::tempdir().unwrap();
    let script = "/bin/sh -c 'echo $$ > background; sleep 20; printf wrong > late' & while [ ! -s background ]; do sleep 0.01; done; printf complete";
    let output = command(root.path(), "workspace-run", Uuid::new_v4())
        .arg(script)
        .output()
        .unwrap();
    success(output);
    stopped(&fs::read_to_string(root.path().join("background")).unwrap());
    assert!(!root.path().join("late").exists());
}
#[test]
fn terminating_the_owner_stops_its_group_and_cleans_its_records() {
    let root = tempfile::tempdir().unwrap();
    let job = Uuid::new_v4();
    let mut child = command(root.path(), "workspace-run", job)
        .arg("echo $$ > leader; sleep 20")
        .spawn()
        .unwrap();
    ready(&root.path().join("leader"));
    ready(&root.path().join(format!(".jobs/{job}.pid")));
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(child.id() as i32).unwrap(),
        rustix::process::Signal::TERM,
    )
    .unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(130));
    stopped(&fs::read_to_string(root.path().join("leader")).unwrap());
    assert_eq!(fs::read_dir(root.path().join(".jobs")).unwrap().count(), 0);
}
#[tokio::test]
async fn timeout_stops_the_group_without_removing_work_files() {
    let root = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let job = ai_phone_guest_runtime::workspace_job::Job::new(root.path(), id);
    let result = job
        .run(
            root.path(),
            "echo $$ > leader; printf kept > artifact; sleep 20",
            Duration::from_millis(200),
        )
        .await
        .unwrap();
    assert_eq!(result, 124);
    stopped(&fs::read_to_string(root.path().join("leader")).unwrap());
    assert_eq!(fs::read(root.path().join("artifact")).unwrap(), b"kept");
    assert_eq!(fs::read_dir(root.path().join(".jobs")).unwrap().count(), 0);
}
#[test]
fn output_streams_beyond_pipe_capacity_and_invalid_ids_cannot_create_jobs() {
    let root = tempfile::tempdir().unwrap();
    let output = command(root.path(), "workspace-run", Uuid::new_v4())
        .arg("head -c 131072 /dev/zero")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, vec![0; 131072]);
    let empty = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_desktop-bridge"))
        .current_dir(empty.path())
        .args(["workspace-run", "../outside", "touch unexpected"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!empty.path().join(".jobs").exists());
    assert!(!empty.path().join("unexpected").exists());
}
