//! Partial ownership effects are modeled; every real permission/account call is denied.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RootFailure {
    None,
    BeforeChange,
    AfterChange,
}

struct Repair {
    home_uid: u32,
    children: [u32; 2],
    fail_child_once: bool,
    root_failure: RootFailure,
    find_calls: usize,
    root_calls: usize,
}

struct RepairFixture {
    files: Fixture,
    state: Mutex<Repair>,
}

impl RepairFixture {
    fn new(fail_child_once: bool, root_failure: RootFailure) -> Self {
        let mut files = Fixture::new();
        files.uid = 0;
        files.gid = 0;
        Self {
            files,
            state: Mutex::new(Repair {
                home_uid: 0,
                children: [0, 0],
                fail_child_once,
                root_failure,
                find_calls: 0,
                root_calls: 0,
            }),
        }
    }
    async fn initialize(&self) -> Result<(u32, u32)> {
        initialize(
            &self.files.home,
            &self.files.runtime,
            self,
            0,
            &|path| {
                let mut value = self.files.identity(path)?;
                if path == self.files.home {
                    let uid = self.state.lock().unwrap().home_uid;
                    value.uid = uid;
                    value.gid = uid;
                }
                Ok(value)
            },
            None,
        )
        .await
    }
    fn descendant_batch(&self, request: &process::Request) -> Result<process::Output> {
        assert!(request.timeout.is_none());
        assert!(matches!(request.capture, process::Capture::Bounded(16384)));
        let args: Vec<_> = request.args.iter().map(|v| v.to_str().unwrap()).collect();
        let old = [
            self.files.home.to_str().unwrap(),
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
            "+",
        ];
        let children_only = [
            self.files.home.to_str().unwrap(),
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
            "+",
        ];
        assert!(
            args == old || args == children_only,
            "Unexpected mutation command"
        );
        let mut state = self.state.lock().unwrap();
        state.find_calls += 1;
        if args == old {
            // The old batch includes the completion marker, which may change
            // before another operand fails. No actual chown is performed.
            state.home_uid = 1000;
        }
        state.children[0] = 1000;
        if std::mem::take(&mut state.fail_child_once) {
            return Err("fixture: partial child repair failed".into());
        }
        state.children[1] = 1000;
        Ok(output(Some(String::new())))
    }
    fn commit_home(&self, request: &process::Request) -> Result<process::Output> {
        assert_eq!(request.timeout, Some(Duration::from_secs(15)));
        let args: Vec<_> = request.args.iter().map(|v| v.to_str().unwrap()).collect();
        assert_eq!(args, ["1000:1000", self.files.home.to_str().unwrap()]);
        let mut state = self.state.lock().unwrap();
        assert_eq!(
            state.children,
            [1000, 1000],
            "Home must be committed only after every descendant batch succeeds"
        );
        state.root_calls += 1;
        if state.root_failure == RootFailure::BeforeChange {
            return Err("fixture: home commit failed before change".into());
        }
        state.home_uid = 1000;
        if state.root_failure == RootFailure::AfterChange {
            return Err("fixture: home commit result unknown after change".into());
        }
        Ok(output(Some(String::new())))
    }
}

impl process::Runner for RepairFixture {
    async fn execute(&self, request: process::Request) -> Result<process::Output> {
        assert!(request.cwd.is_none() && request.env.is_empty() && request.input.is_empty());
        match request.program.to_str().unwrap() {
            "find" => self.descendant_batch(&request),
            "chown" => self.commit_home(&request),
            // Reuse the original denying fake account boundary for every other call.
            _ => self.files.execute(request).await,
        }
    }
}

#[tokio::test]
async fn partial_child_failure_keeps_root_marker_and_retries_remaining_children() {
    let fixture = RepairFixture::new(true, RootFailure::None);
    let before = fixture.files.snapshot();
    assert!(
        fixture
            .initialize()
            .await
            .unwrap_err()
            .to_string()
            .contains("partial child repair failed")
    );
    {
        let state = fixture.state.lock().unwrap();
        assert_eq!(state.children, [1000, 0]);
        assert_eq!(state.find_calls, 1);
        assert_eq!(state.root_calls, 0);
        assert_eq!(
            state.home_uid, 0,
            "Partial repair consumed the only retry marker"
        );
    }
    assert_eq!(fixture.files.snapshot(), before);
    assert_eq!(fixture.initialize().await.unwrap(), (1000, 1000));
    {
        let state = fixture.state.lock().unwrap();
        assert_eq!(state.children, [1000, 1000]);
        assert_eq!(state.home_uid, 1000);
        assert_eq!((state.find_calls, state.root_calls), (2, 1));
    }
    assert_eq!(fixture.initialize().await.unwrap(), (1000, 1000));
    let state = fixture.state.lock().unwrap();
    assert_eq!((state.find_calls, state.root_calls), (2, 1));
    assert_eq!(fixture.files.snapshot(), before);
}

#[tokio::test]
async fn final_home_result_preserves_the_correct_retry_boundary() {
    for (failure, error, uid_after_failure, calls_after_retry) in [
        (RootFailure::BeforeChange, "before change", 0, (2, 2)),
        (RootFailure::AfterChange, "result unknown", 1000, (1, 1)),
    ] {
        let fixture = RepairFixture::new(false, failure);
        let before = fixture.files.snapshot();
        assert!(
            fixture
                .initialize()
                .await
                .unwrap_err()
                .to_string()
                .contains(error)
        );
        {
            let mut state = fixture.state.lock().unwrap();
            assert_eq!(state.children, [1000, 1000]);
            assert_eq!(state.home_uid, uid_after_failure);
            assert_eq!((state.find_calls, state.root_calls), (1, 1));
            state.root_failure = RootFailure::None;
        }
        assert_eq!(fixture.files.snapshot(), before);
        assert_eq!(fixture.initialize().await.unwrap(), (1000, 1000));
        let state = fixture.state.lock().unwrap();
        assert_eq!(state.home_uid, 1000);
        assert_eq!(state.children, [1000, 1000]);
        assert_eq!((state.find_calls, state.root_calls), calls_after_retry);
        assert_eq!(fixture.files.snapshot(), before);
    }
}
