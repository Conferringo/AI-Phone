//! Align the guest account with its mounted home without migrating user files.
use crate::{Result, process};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Directory {
    device: u64,
    inode: u64,
    uid: u32,
    gid: u32,
    mode: u32,
}

fn directory(path: &Path) -> Result<Directory> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() {
        return Err("Desktop identity path is not a directory".into());
    }
    Ok(Directory {
        device: meta.dev(),
        inode: meta.ino(),
        uid: meta.uid(),
        gid: meta.gid(),
        mode: meta.mode() & 0o7777,
    })
}

#[derive(Clone, Debug)]
struct Account {
    name: String,
    uid: u32,
    gid: u32,
    home: PathBuf,
}

// Per-initialization command policy; process::OsRunner remains the only OS owner.
struct Commands<'a, R> {
    runner: &'a R,
    cancellation: Option<tokio::sync::watch::Receiver<bool>>,
}

impl<R: process::Runner> Commands<'_, R> {
    async fn execute(&self, mut request: process::Request) -> Result<process::Output> {
        request.cancellation = self.cancellation.clone();
        self.runner.execute(request).await
    }

    fn compensation(&self) -> Commands<'_, R> {
        Commands {
            runner: self.runner,
            cancellation: None,
        }
    }
}

fn request(program: &str, args: &[&str]) -> process::Request {
    let mut value = process::Request::new(program, args);
    value.timeout = Some(Duration::from_secs(15));
    value.capture = process::Capture::Bounded(16384);
    value
}

async fn lookup(
    runner: &Commands<'_, impl process::Runner>,
    database: &str,
    key: &str,
) -> Result<Option<String>> {
    let output = runner.execute(request("getent", &[database, key])).await?;
    // getent's exit 2 means the specific key is absent, not a tool failure.
    if output.code == 2 && output.stdout.is_empty() && output.stderr.is_empty() {
        return Ok(None);
    }
    let bytes = process::checked(output)?;
    let value = String::from_utf8(bytes)?;
    let value = value.strip_suffix('\n').unwrap_or(&value);
    if value.is_empty() || value.contains(['\n', '\r']) {
        return Err("Desktop account lookup returned an invalid record".into());
    }
    Ok(Some(value.into()))
}

async fn account(
    runner: &Commands<'_, impl process::Runner>,
    key: &str,
) -> Result<Option<Account>> {
    lookup(runner, "passwd", key)
        .await?
        .map(|line| {
            let fields: Vec<_> = line.split(':').collect();
            let [name, _, uid, gid, _, home, _] = fields.as_slice() else {
                return Err("Desktop account lookup returned an invalid passwd record".into());
            };
            Ok(Account {
                name: (*name).into(),
                uid: uid.parse()?,
                gid: gid.parse()?,
                home: (*home).into(),
            })
        })
        .transpose()
}

async fn ai(runner: &Commands<'_, impl process::Runner>) -> Result<Account> {
    let value = account(runner, "ai")
        .await?
        .ok_or("Desktop account is missing")?;
    if value.name != "ai" {
        return Err("Desktop account lookup returned a different identity".into());
    }
    Ok(value)
}

async fn group(
    runner: &Commands<'_, impl process::Runner>,
    key: &str,
) -> Result<Option<(String, u32)>> {
    lookup(runner, "group", key)
        .await?
        .map(|line| {
            let fields: Vec<_> = line.split(':').collect();
            let [name, _, gid, _] = fields.as_slice() else {
                return Err("Desktop account lookup returned an invalid group record".into());
            };
            Ok(((*name).into(), gid.parse()?))
        })
        .transpose()
}

async fn command(
    runner: &Commands<'_, impl process::Runner>,
    request: process::Request,
) -> Result<()> {
    process::checked(runner.execute(request).await?)?;
    Ok(())
}

async fn primary_group(runner: &Commands<'_, impl process::Runner>, gid: u32) -> Result<()> {
    let key = gid.to_string();
    if let Some((_, actual)) = group(runner, &key).await? {
        if actual != gid {
            return Err("Desktop group identity was not verified".into());
        }
        return Ok(());
    }
    let name = format!("ai-home-{gid}");
    if group(runner, &name).await?.is_some() {
        return Err("Desktop home group name is occupied".into());
    }
    command(runner, request("groupadd", &["--gid", &key, &name])).await?;
    if group(runner, &key).await? != Some((name, gid)) {
        return Err("Desktop home group creation was not verified".into());
    }
    Ok(())
}

fn temporary_home(
    runtime: &Path,
    identity: &impl Fn(&Path) -> Result<Directory>,
) -> Result<(PathBuf, Directory)> {
    let parent = identity(runtime)?;
    if parent.uid != 0 || parent.mode & 0o022 != 0 {
        return Err("Desktop identity runtime directory is not privately controlled".into());
    }
    let path = runtime.join(format!(".ai-phone-identity-{}", uuid::Uuid::new_v4()));
    fs::DirBuilder::new().mode(0o700).create(&path)?;
    let owned = identity(&path)?;
    if owned.uid != 0 || owned.mode != 0o700 {
        return Err("Desktop identity temporary directory is not owned; preserved".into());
    }
    Ok((path, owned))
}

async fn restore_home(runner: &Commands<'_, impl process::Runner>, home: &str) -> Result<()> {
    command(runner, request("usermod", &["--home", home, "ai"])).await?;
    if ai(runner).await?.home != Path::new(home) {
        return Err("Desktop account home restoration was not verified".into());
    }
    Ok(())
}

async fn change_account(
    runner: &Commands<'_, impl process::Runner>,
    home: &str,
    runtime: &Path,
    uid: u32,
    gid: u32,
    identity: &impl Fn(&Path) -> Result<Directory>,
) -> Result<()> {
    let (folder, owned) = temporary_home(runtime, identity)?;
    let result = async {
        let absent = folder.join("absent-home");
        match fs::symlink_metadata(&absent) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
            Ok(_) => return Err("Desktop identity temporary home is occupied".into()),
        }
        // No -m: shadow usermod must not walk the real old/new UID-owned home.
        let update = command(
            runner,
            request(
                "usermod",
                &[
                    "--home",
                    absent.to_str().ok_or("Invalid temporary home path")?,
                    "--uid",
                    &uid.to_string(),
                    "--gid",
                    &gid.to_string(),
                    "ai",
                ],
            ),
        )
        .await;
        let restore = restore_home(&runner.compensation(), home).await;
        process::preserve(update, restore)
    }
    .await;
    let cleanup = (|| {
        if identity(&folder)? != owned {
            return Err("Desktop identity temporary directory changed; preserved".into());
        }
        // Never recursively delete unexpected content, even after a failed update.
        fs::remove_dir(&folder)?;
        Ok(())
    })();
    process::preserve(result, cleanup)
}

struct DesiredIdentity {
    uid: u32,
    gid: u32,
    legacy: bool,
    account: Account,
}

// Immutable mount/account policy: no group or account changes occur here.
async fn desired_identity(
    home: &Path,
    owner: &Directory,
    runner: &Commands<'_, impl process::Runner>,
) -> Result<DesiredIdentity> {
    let legacy = owner.uid == 0;
    let (uid, gid) = if legacy {
        (1000, 1000)
    } else {
        (owner.uid, owner.gid)
    };
    if uid == 0 || uid >= 1 << 31 || gid >= 1 << 31 {
        return Err("Desktop home identity is unsupported".into());
    }
    let current = ai(runner).await?;
    if current.home != home {
        return Err("Desktop account home differs from its fixed directory".into());
    }
    if account(runner, &uid.to_string())
        .await?
        .is_some_and(|user| user.name != "ai")
    {
        return Err("Desktop home UID belongs to another container account".into());
    }
    Ok(DesiredIdentity {
        uid,
        gid,
        legacy,
        account: current,
    })
}

// Apply and verify only the approved account identity. User files are not touched.
async fn align_account(
    home: &str,
    runtime: &Path,
    desired: &DesiredIdentity,
    runner: &Commands<'_, impl process::Runner>,
    identity: &impl Fn(&Path) -> Result<Directory>,
) -> Result<()> {
    let (uid, gid) = (desired.uid, desired.gid);
    primary_group(runner, gid).await?;
    if (desired.account.uid, desired.account.gid) != (uid, gid) {
        change_account(runner, home, runtime, uid, gid, identity).await?;
    }
    let current = ai(runner).await?;
    if (current.uid, current.gid, current.home.as_path()) != (uid, gid, Path::new(home)) {
        return Err("Desktop account identity was not verified".into());
    }
    Ok(())
}

async fn initialize(
    home: &Path,
    runtime: &Path,
    runner: &impl process::Runner,
    euid: u32,
    identity: &impl Fn(&Path) -> Result<Directory>,
    cancellation: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<(u32, u32)> {
    if euid != 0 {
        return Err("Desktop account initialization requires container root".into());
    }
    let runner = &Commands {
        runner,
        cancellation,
    };
    let desired = desired_identity(home, &identity(home)?, runner).await?;
    let home = home.to_str().ok_or("Invalid desktop home path")?;
    align_account(home, runtime, &desired, runner, identity).await?;
    let (uid, gid, legacy) = (desired.uid, desired.gid, desired.legacy);
    if legacy {
        let mut repair = request(
            "find",
            &[
                home,
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
                &format!("{uid}:{gid}"),
                "{}",
                "+",
            ],
        );
        // Retain only the existing root-owned-home compatibility repair.
        repair.timeout = None;
        command(runner, repair).await?;
        // Root ownership is the retry marker. Commit it only after every child
        // batch succeeded; a partial failure must still look legacy on restart.
        command(runner, request("chown", &[&format!("{uid}:{gid}"), home])).await?;
    }
    Ok((uid, gid))
}

async fn run_with(
    home: &Path,
    runtime: &Path,
    runner: &impl process::Runner,
    euid: u32,
    identity: &impl Fn(&Path) -> Result<Directory>,
) -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut interrupts = signal(SignalKind::interrupt())?;
    let mut terminations = signal(SignalKind::terminate())?;
    let stopping = async {
        tokio::select! { _ = interrupts.recv() => (), _ = terminations.recv() => () }
    };
    let (cancel, receiver) = tokio::sync::watch::channel(false);
    let initialization = initialize(home, runtime, runner, euid, identity, Some(receiver));
    tokio::pin!(initialization);
    tokio::select! {
        biased;
        _ = stopping => {
            // Do not drop the initialization future: it must await owned-process
            // reaping, restore the fixed home without this token, then clean up.
            cancel.send_replace(true);
            initialization.await?;
            Err("Desktop identity initialization interrupted".into())
        }
        result = &mut initialization => result.map(|_| ()),
    }
}

pub async fn run() -> Result<()> {
    run_with(
        Path::new("/home/ai"),
        Path::new("/run"),
        &process::OsRunner,
        rustix::process::geteuid().as_raw(),
        &directory,
    )
    .await
}

#[cfg(test)]
#[path = "home_identity_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "home_identity_signal_tests.rs"]
mod signal_tests;
