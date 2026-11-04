use crate::{Error, Result};
use rustix::{
    fs::{FlockOperation, OFlags, flock},
    process::{Pid, Signal, getuid, kill_process_group},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    signal::unix::{SignalKind, signal},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    command: Vec<String>,
    environment: BTreeMap<String, String>,
}

impl Spec {
    fn validate(&self) -> Result<()> {
        if self.command.first().is_none_or(String::is_empty)
            || self.command.iter().any(|text| text.contains('\0'))
            || self.environment.iter().any(|(key, value)| {
                key.is_empty() || key.contains(['=', '\0']) || value.contains('\0')
            })
        {
            return Err(Error::Invalid("service_spec"));
        }
        Ok(())
    }
}

struct Invocation {
    root: PathBuf,
    name: String,
    spec_sha256: String,
    content_sha256: String,
}

impl Invocation {
    fn parse(arguments: &[OsString]) -> Result<Self> {
        let [root, name, spec, content] = arguments else {
            return Err(Error::Invalid("service_supervision_arguments"));
        };
        Ok(Self {
            root: Path::new(root).canonicalize()?,
            name: service_name(name)?,
            spec_sha256: identity(spec)?,
            content_sha256: identity(content)?,
        })
    }

    fn folder(&self) -> PathBuf {
        self.root.join("data/services")
    }

    fn read_spec(&self) -> Result<Spec> {
        let path = self.folder().join(format!("{}.spec.json", self.name));
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK).bits() as i32)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(Error::Invalid("service_spec_file"));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024
            || format!("{:x}", Sha256::digest(&bytes)) != self.content_sha256
        {
            return Err(Error::Invalid("service_spec_changed"));
        }
        let spec: Spec =
            serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("service_spec"))?;
        spec.validate()?;
        Ok(spec)
    }

    fn inherited_lock(&self) -> Result<()> {
        let stdin = std::io::stdin();
        let inherited = fs::metadata("/proc/self/fd/0")?;
        let expected = fs::symlink_metadata(self.folder().join(format!("{}.lock", self.name)))?;
        if !inherited.is_file()
            || !expected.is_file()
            || (inherited.dev(), inherited.ino()) != (expected.dev(), expected.ino())
            || inherited.uid() != getuid().as_raw()
        {
            return Err(Error::Invalid("service_supervision_lock"));
        }
        flock(stdin.as_fd(), FlockOperation::NonBlockingLockExclusive)
            .map_err(std::io::Error::from)?;
        Ok(())
    }
}

fn service_name(value: &OsString) -> Result<String> {
    let name = value.to_str().ok_or(Error::Invalid("service_name"))?;
    if name.len() > 40
        || !name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(Error::Invalid("service_name"));
    }
    Ok(name.into())
}

fn identity(value: &OsString) -> Result<String> {
    let text = value
        .to_str()
        .ok_or(Error::Invalid("service_spec_identity"))?;
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Invalid("service_spec_identity"));
    }
    Ok(text.into())
}

#[derive(Serialize)]
struct Receipt<'a> {
    format: u8,
    supervisor: &'static str,
    program: PathBuf,
    pid: u32,
    start_ticks: String,
    root: &'a Path,
    spec_sha256: &'a str,
    spec_content_sha256: &'a str,
}

struct Published {
    path: PathBuf,
    inode: u64,
}

impl Published {
    fn create(input: &Invocation) -> Result<Self> {
        let receipt = Receipt {
            format: 1,
            supervisor: "rust-v1",
            program: std::env::current_exe()?,
            pid: std::process::id(),
            start_ticks: process_start()?,
            root: &input.root,
            spec_sha256: &input.spec_sha256,
            spec_content_sha256: &input.content_sha256,
        };
        let path = input.folder().join(format!("{}.json", input.name));
        let mut output = tempfile::NamedTempFile::new_in(input.folder())?;
        serde_json::to_writer(&mut output, &receipt)
            .map_err(|_| Error::Invalid("service_receipt"))?;
        output.write_all(b"\n")?;
        output.as_file().sync_all()?;
        let file = output
            .persist(&path)
            .map_err(|error| Error::Io(error.error))?;
        Ok(Self {
            path,
            inode: file.metadata()?.ino(),
        })
    }
}

fn process_start() -> Result<String> {
    let status = fs::read_to_string("/proc/self/stat")?;
    status
        .rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .map(String::from)
        .ok_or(Error::Invalid("service_process_identity"))
}

impl Drop for Published {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.ino() == self.inode)
            && let Err(error) = fs::remove_file(&self.path)
        {
            eprintln!("service_receipt_cleanup: {error}");
        }
    }
}

async fn stop_child(child: &mut Child) -> Result<()> {
    let pid = child.id().and_then(|pid| Pid::from_raw(pid as i32));
    if let Some(pid) = pid {
        signal_group(pid, Signal::TERM)?;
        match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
            Ok(result) => {
                result?;
            }
            Err(_) => {
                signal_group(pid, Signal::KILL)?;
                child.wait().await?;
            }
        }
    }
    Ok(())
}

fn signal_group(pid: Pid, signal: Signal) -> Result<()> {
    match kill_process_group(pid, signal) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(std::io::Error::from(error).into()),
    }
}

pub(super) async fn run(arguments: &[OsString]) -> Result<()> {
    let input = Invocation::parse(arguments)?;
    input.inherited_lock()?;
    let spec = input.read_spec()?;
    let mut term = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let _receipt = Published::create(&input)?;
    while !run_child(&input, &spec, &mut term, &mut interrupt).await? {
        if !restart_delay(&mut term, &mut interrupt).await {
            break;
        }
    }
    Ok(())
}

async fn run_child(
    input: &Invocation,
    spec: &Spec,
    term: &mut tokio::signal::unix::Signal,
    interrupt: &mut tokio::signal::unix::Signal,
) -> Result<bool> {
    let mut command = Command::new(&spec.command[0]);
    command
        .args(&spec.command[1..])
        .envs(&spec.environment)
        .current_dir(&input.root)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .process_group(0);
    let mut child = command.spawn()?;
    let stopped = tokio::select! {
        biased;
        _ = term.recv() => true,
        _ = interrupt.recv() => true,
        result = child.wait() => { eprintln!("Owned child exited: {} {}", input.name, result?); false },
    };
    if stopped {
        stop_child(&mut child).await?;
    }
    Ok(stopped)
}

async fn restart_delay(
    term: &mut tokio::signal::unix::Signal,
    interrupt: &mut tokio::signal::unix::Signal,
) -> bool {
    tokio::select! {
        biased;
        _ = term.recv() => false,
        _ = interrupt.recv() => false,
        _ = tokio::time::sleep(Duration::from_secs(2)) => true,
    }
}
