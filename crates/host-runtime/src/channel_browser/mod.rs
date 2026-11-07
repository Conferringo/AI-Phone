//! The native owner of the bundled WhatsApp Chromium process and account lease.
mod engine;
mod pipe;
use crate::{Error, Result, filesystem};
use engine::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

fn invalid() -> Error {
    Error::Invalid("channel_browser_unavailable")
}
fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn variable(name: &str) -> Result<String> {
    env::var(name).map_err(|_| invalid())
}
struct Context {
    profile: PathBuf,
    key: String,
    image: String,
    name: String,
    record: PathBuf,
    session: String,
}
impl Context {
    fn from_env(check: bool) -> Result<Self> {
        let root = PathBuf::from(variable("AI_PHONE_PROJECT_ROOT")?);
        let session = if check {
            "bootstrap-check".into()
        } else {
            variable("AI_PHONE_BROWSER_SESSION")?
        };
        let image = variable("AI_PHONE_DESKTOP_IMAGE")?;
        let digest = image
            .strip_prefix("ai-phone-bundle/desktop:sha-")
            .filter(|v| hex(v, 64))
            .ok_or_else(invalid)?;
        if !root.is_absolute()
            || session.is_empty()
            || session.len() > 128
            || !session
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || b"_-".contains(&v))
        {
            return Err(invalid());
        }
        let base = crate::runtime_environment::resolved(&root.join("data/channels/whatsapp"))?;
        let profile = base.join(format!("session-{session}"));
        let text = profile
            .to_str()
            .filter(|v| !v.chars().any(char::is_control))
            .ok_or_else(invalid)?;
        let key = format!("{:x}", Sha256::digest(text));
        Ok(Self {
            name: format!("ai-phone-browser-{}", &key[..24]),
            record: base.join(".browser-runtime").join(format!("{key}.json")),
            profile,
            key,
            image: format!("sha256:{digest}"),
            session,
        })
    }
    fn marker(&self) -> PathBuf {
        self.profile.join(".aiphone-runtime.json")
    }
    fn arguments(&self, args: &[OsString]) -> Result<Vec<String>> {
        let args: Vec<_> = args
            .iter()
            .map(|v| v.to_str().ok_or_else(invalid))
            .collect::<Result<_>>()?;
        if args.len() > 64 || args.iter().map(|v| v.len()).sum::<usize>() > 16384 {
            return Err(invalid());
        }
        let profiles: Vec<_> = args
            .iter()
            .filter_map(|v| v.strip_prefix("--user-data-dir="))
            .collect();
        if profiles.len() != 1
            || crate::runtime_environment::resolved(Path::new(profiles[0]))? != self.profile
            || !args.contains(&"--remote-debugging-pipe")
        {
            return Err(invalid());
        }
        let mut result = vec![];
        for arg in args {
            if !arg.starts_with("--")
                || arg.contains('\0')
                || arg.starts_with("--remote-debugging-") && arg != "--remote-debugging-pipe"
            {
                return Err(invalid());
            }
            if arg != "--remote-debugging-pipe" && !arg.starts_with("--user-data-dir=") {
                result.push(arg.to_owned());
            }
        }
        Ok(result)
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u32,
    profile_hash: String,
    image_id: String,
    token: String,
    container_id: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    format: u32,
    mode: String,
    session_id: String,
    owner_token: String,
}
fn private<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    if fs::symlink_metadata(path)?.permissions().mode() & 0o077 != 0 {
        return Err(invalid());
    }
    serde_json::from_slice(&filesystem::bounded_file_with_limit(path, 4096)?).map_err(|_| invalid())
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
fn publish(path: &Path, value: &impl Serialize) -> Result<()> {
    if exists(path)? && fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(invalid());
    }
    let parent = path.parent().ok_or_else(invalid)?;
    filesystem::directory(parent, true)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    filesystem::publish_private(
        parent,
        path.file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(invalid)?,
        &serde_json::to_vec(value).map_err(|_| invalid())?,
        false,
    )
}
struct Lease<'a> {
    context: &'a Context,
    engine: &'a Engine,
    owner: String,
    _lock: filesystem::Lease,
}
impl<'a> Lease<'a> {
    async fn acquire(
        context: &'a Context,
        engine: &'a Engine,
        owner: String,
        wait: Duration,
    ) -> Result<Self> {
        let parent = context.record.parent().ok_or_else(invalid)?;
        filesystem::directory(parent, true)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            if let Some(lock) = filesystem::lease(&context.record.with_extension("lock"))? {
                return Ok(Self {
                    context,
                    engine,
                    owner,
                    _lock: lock,
                });
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::Busy);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    fn record(&self) -> Result<Option<Record>> {
        if !exists(&self.context.record)? {
            return Ok(None);
        }
        let r: Record = private(&self.context.record)?;
        if r.format != 1
            || r.profile_hash != self.context.key
            || !hex(&r.token, 64)
            || !r
                .image_id
                .strip_prefix("sha256:")
                .is_some_and(|v| hex(v, 64))
            || !r.container_id.is_empty() && !hex(&r.container_id, 64)
        {
            return Err(invalid());
        }
        Ok(Some(r))
    }
    fn profile(&self) -> Result<Option<Marker>> {
        if exists(&self.context.profile)?
            && fs::symlink_metadata(&self.context.profile)?
                .file_type()
                .is_symlink()
        {
            return Err(invalid());
        }
        let marker = self.context.marker();
        if exists(&marker)? {
            let value: Marker = private(&marker)?;
            if value.format != 1
                || value.mode != "container"
                || value.session_id != self.context.session
                || !hex(&value.owner_token, 64)
            {
                return Err(invalid());
            }
            return Ok(Some(value));
        }
        if exists(&self.context.profile)? && fs::read_dir(&self.context.profile)?.next().is_some() {
            return Err(invalid());
        }
        Ok(None)
    }
    fn superseded(&self) -> Result<bool> {
        let record = self.record()?;
        let marker = self.profile()?;
        Ok(record.is_some_and(|r| r.token != self.owner)
            || marker.is_some_and(|m| m.owner_token != self.owner))
    }
    async fn recover(&self) -> Result<()> {
        let record = self.record()?;
        let named = self
            .engine
            .inspect(
                &format!("name=^/{}$", self.context.name),
                Some(&self.context.name),
            )
            .await?;
        let container = match (&record, named) {
            (Some(r), Some(container))
                if !r.container_id.is_empty() && container.id != r.container_id =>
            {
                return Err(invalid());
            }
            (Some(r), None) if !r.container_id.is_empty() => {
                self.engine
                    .inspect(&format!("id={}", r.container_id), None)
                    .await?
            }
            (_, observed) => observed,
        };
        if let Some(container) = container {
            let record = record.as_ref().ok_or_else(invalid)?;
            container.owned(record, &self.context.key)?;
            self.engine.remove(&container).await?;
            if self
                .engine
                .inspect(&format!("id={}", container.id), None)
                .await?
                .is_some()
            {
                return Err(invalid());
            }
        }
        if record.is_some() {
            fs::remove_file(&self.context.record)?;
        }
        Ok(())
    }
    async fn create(&self, args: &[String]) -> Result<String> {
        if self.record()?.is_some()
            || self
                .engine
                .inspect(
                    &format!("name=^/{}$", self.context.name),
                    Some(&self.context.name),
                )
                .await?
                .is_some()
        {
            return Err(invalid());
        }
        self.profile()?;
        filesystem::directory(&self.context.profile, false)?;
        publish(
            &self.context.marker(),
            &Marker {
                format: 1,
                mode: "container".into(),
                session_id: self.context.session.clone(),
                owner_token: self.owner.clone(),
            },
        )?;
        let links = stale_links(&self.context.profile)?;
        for path in links {
            fs::remove_file(path)?;
        }
        let mut record = Record {
            format: 1,
            profile_hash: self.context.key.clone(),
            image_id: self.context.image.clone(),
            token: self.owner.clone(),
            container_id: String::new(),
        };
        publish(&self.context.record, &record)?;
        let id = self.engine.create(self.context, &record, args).await?;
        record.container_id = id.clone();
        publish(&self.context.record, &record)?;
        Ok(id)
    }
}
fn stale_links(profile: &Path) -> Result<Vec<PathBuf>> {
    let mut links = vec![];
    for name in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        let path = profile.join(name);
        if !exists(&path)? {
            continue;
        }
        let target = fs::read_link(&path)?;
        let target = target.to_str().ok_or_else(invalid)?;
        let valid = match name {
            "SingletonCookie" => !target.is_empty() && target.bytes().all(|v| v.is_ascii_digit()),
            "SingletonSocket" => target
                .strip_prefix("/tmp/")
                .and_then(|v| {
                    v.strip_prefix(".org.chromium.Chromium.")
                        .or_else(|| v.strip_prefix("org.chromium.Chromium."))
                })
                .and_then(|v| v.strip_suffix("/SingletonSocket"))
                .is_some_and(|v| !v.is_empty() && !v.contains('/')),
            _ => target.rsplit_once('-').is_some_and(|(host, pid)| {
                (hex(host, 12)
                    || host
                        .strip_prefix("ai-phone-browser-")
                        .is_some_and(|v| hex(v, 24)))
                    && !pid.is_empty()
                    && !pid.starts_with('0')
                    && pid.bytes().all(|v| v.is_ascii_digit())
            }),
        };
        if !valid {
            return Err(invalid());
        }
        links.push(path);
    }
    Ok(links)
}
pub(crate) async fn run(args: &[OsString]) -> Result<()> {
    let check = args == [OsString::from("--check")];
    let context = Context::from_env(check)?;
    let engine = Engine::from_env()?;
    if check {
        println!(
            "{}",
            json!({"available":true,"version":engine.check(&context.image).await?})
        );
        return Ok(());
    }
    let owner = variable("AI_PHONE_BROWSER_OWNER")?;
    if !hex(&owner, 64) {
        return Err(invalid());
    }
    if args == [OsString::from("--stop")] || args == [OsString::from("--erase")] {
        let erase = args[0] == "--erase";
        let lease = Lease::acquire(&context, &engine, owner, Duration::from_secs(8)).await?;
        if lease.superseded()? {
            if erase {
                return Err(invalid());
            }
            println!("{}", json!({"stopped":true,"superseded":true}));
            return Ok(());
        }
        lease.recover().await?;
        if erase {
            lease.profile()?;
            if exists(&context.profile)? {
                fs::remove_dir_all(&context.profile)?;
            }
        }
        println!("{}", json!({"stopped":true,"erased":erase}));
        return Ok(());
    }
    let args = context.arguments(args)?;
    let (input, output) = pipe::descriptors()?;
    let lease = Lease::acquire(&context, &engine, owner, Duration::ZERO).await?;
    lease.recover().await?;
    let result = async {
        let id = lease.create(&args).await?;
        pipe::relay(engine.attach(&id)?, input, output).await
    }
    .await;
    let cleanup = lease.recover().await;
    result?;
    cleanup
}
