//! Only owned files plus a denying fake account/process boundary; no OS tools.
use super::*;
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    Group,
    First,
    Both,
    Restore,
    IgnoredRestore,
    UnexpectedChild,
    ReplacedTemporary,
    Lookup,
    WrongIdentity,
}

struct State {
    account: Account,
    users: BTreeMap<u32, String>,
    groups: BTreeMap<u32, String>,
    extra_group_name: bool,
    calls: Vec<Vec<String>>,
    fault: Fault,
}

struct Fixture {
    _root: tempfile::TempDir,
    home: PathBuf,
    runtime: PathBuf,
    uid: u32,
    gid: u32,
    parent_mode: u32,
    state: Mutex<State>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let runtime = root.path().join("run");
        fs::DirBuilder::new().mode(0o750).create(&home).unwrap();
        fs::DirBuilder::new().mode(0o700).create(&runtime).unwrap();
        fs::write(home.join("private-note"), b"fixture user content").unwrap();
        Self {
            state: Mutex::new(State {
                account: Account {
                    name: "ai".into(),
                    uid: 1000,
                    gid: 1000,
                    home: home.clone(),
                },
                users: BTreeMap::new(),
                groups: BTreeMap::from([(1000, "ai".into()), (20, "dialout".into())]),
                extra_group_name: false,
                calls: vec![],
                fault: Fault::None,
            }),
            _root: root,
            home,
            runtime,
            uid: 1001,
            gid: 1001,
            parent_mode: 0o700,
        }
    }
    fn identity(&self, path: &Path) -> Result<Directory> {
        // lstat/type/dev/inode/mode are real. Only fixture account ownership is fake.
        let mut value = directory(path)?;
        if path == self.home {
            value.uid = self.uid;
            value.gid = self.gid;
        } else if path == self.runtime || path.parent() == Some(self.runtime.as_path()) {
            value.uid = 0;
            if path == self.runtime {
                value.mode = self.parent_mode;
            }
        }
        Ok(value)
    }
    fn snapshot(&self) -> Vec<(Directory, Vec<u8>)> {
        vec![
            (directory(&self.home).unwrap(), vec![]),
            (
                {
                    let m = fs::symlink_metadata(self.home.join("private-note")).unwrap();
                    Directory {
                        device: m.dev(),
                        inode: m.ino(),
                        uid: m.uid(),
                        gid: m.gid(),
                        mode: m.mode(),
                    }
                },
                fs::read(self.home.join("private-note")).unwrap(),
            ),
        ]
    }
    async fn initialize(&self) -> Result<(u32, u32)> {
        initialize(
            &self.home,
            &self.runtime,
            self,
            0,
            &|path| self.identity(path),
            None,
        )
        .await
    }
    fn commands(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|v| v[0] != "getent")
            .map(|v| v[0].clone())
            .collect()
    }
    fn fault(&self, fault: Fault) {
        self.state.lock().unwrap().fault = fault;
    }
    fn clean(&self) {
        assert_eq!(fs::read_dir(&self.runtime).unwrap().count(), 0);
    }
}

fn output(value: Option<String>) -> process::Output {
    process::Output {
        code: if value.is_some() { 0 } else { 2 },
        stdout: value.unwrap_or_default().into_bytes(),
        stderr: vec![],
    }
}

impl process::Runner for Fixture {
    async fn execute(&self, request: process::Request) -> Result<process::Output> {
        assert!(request.cwd.is_none() && request.env.is_empty() && request.input.is_empty());
        assert!(matches!(request.capture, process::Capture::Bounded(16384)));
        let mut argv = vec![request.program.to_str().unwrap().to_owned()];
        argv.extend(
            request
                .args
                .iter()
                .map(|arg| arg.to_str().unwrap().to_owned()),
        );
        let mut state = self.state.lock().unwrap();
        state.calls.push(argv.clone());
        if argv[0] == "find" {
            assert!(request.timeout.is_none());
            assert_eq!(
                self.uid, 0,
                "A nonroot home must never be recursively chowned"
            );
            assert_eq!(
                argv,
                [
                    "find",
                    self.home.to_str().unwrap(),
                    "-mindepth",
                    "1",
                    "(",
                    "-type",
                    "d",
                    "-o",
                    "-type",
                    "f",
                    ")",
                    "-exec",
                    "chown",
                    "1000:1000",
                    "{}",
                    "+"
                ]
            );
            return Ok(output(Some(String::new())));
        }
        assert_eq!(request.timeout, Some(Duration::from_secs(15)));
        match argv[0].as_str() {
            "chown" => {
                assert_eq!(self.uid, 0, "A nonroot home must never be chowned");
                assert_eq!(argv, ["chown", "1000:1000", self.home.to_str().unwrap()]);
                assert_eq!(state.calls[state.calls.len() - 2][0], "find");
            }
            "getent" => {
                assert_eq!(argv.len(), 3);
                if state.fault == Fault::Lookup {
                    return Ok(process::Output {
                        code: 1,
                        stdout: vec![],
                        stderr: b"lookup failed".to_vec(),
                    });
                }
                let value = match argv[1].as_str() {
                    "passwd" => {
                        if argv[2] == "ai" || argv[2] == state.account.uid.to_string() {
                            let account = &state.account;
                            Some(format!(
                                "{}:x:{}:{}::{}:/bin/bash\n",
                                account.name,
                                account.uid,
                                account.gid,
                                account.home.display()
                            ))
                        } else {
                            state
                                .users
                                .get(&argv[2].parse::<u32>().unwrap())
                                .map(|name| {
                                    format!("{name}:x:{}:1000::/other:/bin/bash\n", argv[2])
                                })
                        }
                    }
                    "group" => {
                        if state.extra_group_name && argv[2] == "ai-home-1001" {
                            Some("ai-home-1001:x:2000:\n".into())
                        } else {
                            state
                                .groups
                                .iter()
                                .find(|(gid, name)| gid.to_string() == argv[2] || **name == argv[2])
                                .map(|(gid, name)| format!("{name}:x:{gid}:\n"))
                        }
                    }
                    _ => panic!("Unexpected mocked lookup database"),
                };
                return Ok(output(value));
            }
            "groupadd" => {
                assert_eq!(
                    argv,
                    [
                        "groupadd",
                        "--gid",
                        &self.gid.to_string(),
                        &format!("ai-home-{}", self.gid)
                    ]
                );
                if state.fault == Fault::Group {
                    return Err("group creation failure".into());
                }
                state.groups.insert(self.gid, argv[3].clone());
            }
            "usermod" if argv.contains(&"--uid".into()) => {
                assert_eq!(argv.len(), 8);
                let absent = Path::new(&argv[2]);
                assert_eq!(absent.file_name().unwrap(), "absent-home");
                assert_eq!(
                    absent.parent().unwrap().parent(),
                    Some(self.runtime.as_path())
                );
                assert!(!absent.exists());
                let (uid, gid) = if self.uid == 0 {
                    (1000, 1000)
                } else {
                    (self.uid, self.gid)
                };
                assert_eq!(
                    argv[3..],
                    ["--uid", &uid.to_string(), "--gid", &gid.to_string(), "ai"]
                );
                assert!(
                    !argv
                        .iter()
                        .any(|v| ["-m", "--move-home", "-o", "--non-unique"].contains(&v.as_str()))
                );
                if state.fault == Fault::First {
                    return Err("account change failure".into());
                }
                state.account.uid = if state.fault == Fault::WrongIdentity {
                    777
                } else {
                    uid
                };
                state.account.gid = gid;
                state.account.home = absent.into();
                if state.fault == Fault::Both {
                    return Err("account change failure".into());
                }
                if state.fault == Fault::UnexpectedChild {
                    fs::create_dir(absent).unwrap();
                }
                if state.fault == Fault::ReplacedTemporary {
                    let folder = absent.parent().unwrap();
                    fs::rename(folder, folder.with_extension("preserved")).unwrap();
                    fs::DirBuilder::new().mode(0o700).create(folder).unwrap();
                }
            }
            "usermod" => {
                assert_eq!(
                    argv,
                    ["usermod", "--home", self.home.to_str().unwrap(), "ai"]
                );
                if [Fault::Both, Fault::Restore].contains(&state.fault) {
                    return Err("home restoration failure".into());
                }
                if state.fault != Fault::IgnoredRestore {
                    state.account.home = self.home.clone();
                }
            }
            _ => panic!("Unmocked external operation: process execution is forbidden"),
        }
        Ok(output(Some(String::new())))
    }
}

#[tokio::test]
async fn nonroot_home_aligns_account_without_changing_user_files() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    assert_eq!(fixture.initialize().await.unwrap(), (1001, 1001));
    assert_eq!(fixture.commands(), ["groupadd", "usermod", "usermod"]);
    let state = fixture.state.lock().unwrap();
    assert_eq!(
        (state.account.uid, state.account.gid, &state.account.home),
        (1001, 1001, &fixture.home)
    );
    drop(state);
    fixture.clean();
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn existing_group_is_reused_without_renumbering() {
    let mut fixture = Fixture::new();
    fixture.gid = 20;
    let before = fixture.snapshot();
    assert_eq!(fixture.initialize().await.unwrap(), (1001, 20));
    assert_eq!(fixture.commands(), ["usermod", "usermod"]);
    let state = fixture.state.lock().unwrap();
    assert_eq!(state.groups[&20], "dialout");
    assert_eq!(state.groups[&1000], "ai");
    drop(state);
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn already_aligned_home_needs_no_mutation() {
    let mut fixture = Fixture::new();
    fixture.uid = 1000;
    fixture.gid = 1000;
    let before = fixture.snapshot();
    assert_eq!(fixture.initialize().await.unwrap(), (1000, 1000));
    assert!(fixture.commands().is_empty());
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn root_home_keeps_legacy_repair_even_after_previous_mapping() {
    for previous in [1000, 1001] {
        let mut fixture = Fixture::new();
        fixture.uid = 0;
        fixture.gid = 0;
        {
            let mut state = fixture.state.lock().unwrap();
            state.account.uid = previous;
            state.account.gid = previous;
        }
        let before = fixture.snapshot();
        assert_eq!(fixture.initialize().await.unwrap(), (1000, 1000));
        assert_eq!(
            fixture.commands(),
            if previous == 1000 {
                vec!["find", "chown"]
            } else {
                vec!["usermod", "usermod", "find", "chown"]
            }
        );
        assert_eq!(fixture.snapshot(), before);
    }
}

#[tokio::test]
async fn repeated_initialization_reuses_identity_and_preserves_home() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    fixture.initialize().await.unwrap();
    fixture.state.lock().unwrap().calls.clear();
    assert_eq!(fixture.initialize().await.unwrap(), (1001, 1001));
    assert!(fixture.commands().is_empty());
    fixture.clean();
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn conflicting_uid_is_rejected_before_mutation() {
    let fixture = Fixture::new();
    fixture
        .state
        .lock()
        .unwrap()
        .users
        .insert(1001, "another".into());
    let before = fixture.snapshot();
    assert!(
        fixture
            .initialize()
            .await
            .unwrap_err()
            .to_string()
            .contains("another container account")
    );
    assert!(fixture.commands().is_empty());
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn conflicting_group_name_is_not_adopted() {
    let fixture = Fixture::new();
    fixture.state.lock().unwrap().extra_group_name = true;
    assert!(
        fixture
            .initialize()
            .await
            .unwrap_err()
            .to_string()
            .contains("group name is occupied")
    );
    assert!(fixture.commands().is_empty());
}

#[tokio::test]
async fn failed_group_does_not_change_account_or_home() {
    let fixture = Fixture::new();
    fixture.fault(Fault::Group);
    let before = fixture.snapshot();
    assert!(
        fixture
            .initialize()
            .await
            .unwrap_err()
            .to_string()
            .contains("group creation failure")
    );
    assert_eq!(fixture.state.lock().unwrap().account.home, fixture.home);
    fixture.clean();
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn account_failure_still_restores_home_and_retains_error() {
    let fixture = Fixture::new();
    fixture.fault(Fault::First);
    let before = fixture.snapshot();
    assert!(
        fixture
            .initialize()
            .await
            .unwrap_err()
            .to_string()
            .contains("account change failure")
    );
    assert_eq!(fixture.commands(), ["groupadd", "usermod", "usermod"]);
    assert_eq!(fixture.state.lock().unwrap().account.home, fixture.home);
    fixture.clean();
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn primary_and_restoration_failures_are_both_retained() {
    let fixture = Fixture::new();
    fixture.fault(Fault::Both);
    let before = fixture.snapshot();
    let error = fixture.initialize().await.unwrap_err().to_string();
    assert!(error.contains("account change failure"));
    assert!(error.contains("home restoration failure"));
    fixture.clean();
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn restoration_failure_or_unverified_success_refuses_initialization() {
    for fault in [Fault::Restore, Fault::IgnoredRestore, Fault::WrongIdentity] {
        let fixture = Fixture::new();
        fixture.fault(fault);
        let before = fixture.snapshot();
        assert!(fixture.initialize().await.is_err());
        assert_eq!(fixture.snapshot(), before);
    }
}

#[tokio::test]
async fn unexpected_temporary_contents_are_preserved() {
    let fixture = Fixture::new();
    fixture.fault(Fault::UnexpectedChild);
    let before = fixture.snapshot();
    assert!(fixture.initialize().await.is_err());
    let folders: Vec<_> = fs::read_dir(&fixture.runtime)
        .unwrap()
        .map(|v| v.unwrap().path())
        .collect();
    assert_eq!(folders.len(), 1);
    assert!(folders[0].join("absent-home").is_dir());
    assert_eq!(fixture.state.lock().unwrap().account.home, fixture.home);
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn replaced_temporary_directory_is_never_removed() {
    let fixture = Fixture::new();
    fixture.fault(Fault::ReplacedTemporary);
    let before = fixture.snapshot();
    assert!(
        fixture
            .initialize()
            .await
            .unwrap_err()
            .to_string()
            .contains("temporary directory changed")
    );
    assert_eq!(fs::read_dir(&fixture.runtime).unwrap().count(), 2);
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn uncontrolled_runtime_is_rejected_before_account_changes() {
    let mut fixture = Fixture::new();
    fixture.gid = 20;
    fixture.parent_mode = 0o777;
    assert!(
        fixture
            .initialize()
            .await
            .unwrap_err()
            .to_string()
            .contains("privately controlled")
    );
    assert!(fixture.commands().is_empty());
    fixture.clean();
}

#[tokio::test]
async fn nonroot_caller_does_not_read_accounts_or_mutate_files() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    assert!(
        initialize(
            &fixture.home,
            &fixture.runtime,
            &fixture,
            1001,
            &|_| panic!("No filesystem access allowed"),
            None,
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("container root")
    );
    assert!(fixture.state.lock().unwrap().calls.is_empty());
    assert_eq!(fixture.snapshot(), before);
}

#[tokio::test]
async fn unexpected_fixed_home_is_not_adopted() {
    let fixture = Fixture::new();
    fixture.state.lock().unwrap().account.home = "/unexpected-home".into();
    assert!(
        fixture
            .initialize()
            .await
            .unwrap_err()
            .to_string()
            .contains("fixed directory")
    );
    assert!(fixture.commands().is_empty());
}

#[tokio::test]
async fn lookup_failure_is_not_absence_or_permission_to_create_accounts() {
    let fixture = Fixture::new();
    fixture.fault(Fault::Lookup);
    assert!(
        fixture
            .initialize()
            .await
            .unwrap_err()
            .to_string()
            .contains("lookup failed")
    );
    assert!(fixture.commands().is_empty());
    fixture.clean();
}

#[tokio::test]
async fn symlink_home_and_unsupported_ids_refuse_all_commands() {
    let mut fixture = Fixture::new();
    let alias = fixture._root.path().join("alias");
    std::os::unix::fs::symlink(&fixture.home, &alias).unwrap();
    assert!(
        initialize(
            &alias,
            &fixture.runtime,
            &fixture,
            0,
            &|path| fixture.identity(path),
            None
        )
        .await
        .is_err()
    );
    for (uid, gid) in [(1 << 31, 1000), (1000, 1 << 31)] {
        fixture.uid = uid;
        fixture.gid = gid;
        assert!(fixture.initialize().await.is_err());
    }
    assert!(fixture.state.lock().unwrap().calls.is_empty());
}

#[path = "home_identity_legacy_tests.rs"]
mod legacy_repair;
