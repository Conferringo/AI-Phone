use super::{Error, Result, policy};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Deserialize)]
struct Profile {
    desktop: Resources,
}
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct Resources {
    pub cpus: u32,
    pub memory_mb: u32,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    pub device_id: String,
    pub request_id: String,
    pub resources: Resources,
}
pub(super) struct Store {
    folder: PathBuf,
}
fn io(_: std::io::Error) -> Error {
    Error("device_setup_storage")
}

pub(super) fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit as u64 {
        return Err(Error("device_setup_file"));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > limit {
        return Err(Error("device_setup_file"));
    }
    Ok(bytes)
}
pub(super) fn token(root: &Path, role: &str) -> Result<String> {
    let value = String::from_utf8(read(&root.join(format!("data/{role}/token")), 4096)?)
        .map_err(|_| Error("device_setup_token"))?;
    let value = value.trim();
    if value.is_empty()
        || value
            .bytes()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
    {
        return Err(Error("device_setup_token"));
    }
    Ok(value.into())
}
impl Store {
    #[cfg(test)]
    pub fn folder_for_test(&self) -> &Path {
        &self.folder
    }
    pub fn new(root: &Path) -> Result<Self> {
        let folder = root.join("data/deployment").canonicalize().map_err(io)?;
        if !folder.is_dir() {
            return Err(Error("device_setup_storage"));
        }
        Ok(Self { folder })
    }
    pub fn resources(&self) -> Result<Resources> {
        let profile: Profile =
            serde_json::from_slice(&read(&self.folder.join("profile.json"), 128 * 1024)?)
                .map_err(|_| Error("device_setup_profile"))?;
        policy::resources(profile.desktop.cpus, profile.desktop.memory_mb).map_err(Error)?;
        Ok(profile.desktop)
    }
    pub fn load(&self) -> Result<Option<Intent>> {
        let path = self.folder.join("device-setup.json");
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io(error)),
            Ok(_) => {}
        }
        let intent: Intent = serde_json::from_slice(&read(&path, 4096)?)
            .map_err(|_| Error("device_setup_intent"))?;
        if Uuid::parse_str(&intent.request_id).is_err()
            || intent.device_id.is_empty()
            || intent.device_id.len() > 128
        {
            return Err(Error("device_setup_intent"));
        }
        policy::resources(intent.resources.cpus, intent.resources.memory_mb).map_err(Error)?;
        Ok(Some(intent))
    }
    pub fn select(&self, device_id: Option<&str>, resources: Resources) -> Result<Intent> {
        if let Some(intent) = self.load()? {
            return self.check(intent, device_id, resources);
        }
        let intent = Intent {
            device_id: device_id
                .map(str::to_owned)
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            request_id: Uuid::new_v4().to_string(),
            resources,
        };
        let bytes = serde_json::to_vec(&intent).map_err(|_| Error("device_setup_intent"))?;
        let temp = self
            .folder
            .join(format!(".device-setup-{}", Uuid::new_v4()));
        let mut file = private_new(&temp).map_err(io)?;
        let result = (|| {
            file.write_all(&bytes).map_err(io)?;
            file.sync_all().map_err(io)?;
            match fs::hard_link(&temp, self.folder.join("device-setup.json")) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io(error)),
            }
            self.sync()?;
            self.check(
                self.load()?.ok_or(Error("device_setup_intent"))?,
                device_id,
                resources,
            )
        })();
        let cleanup = fs::remove_file(temp).map_err(io);
        match (result, cleanup) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(value), Ok(())) => Ok(value),
        }
    }
    fn check(
        &self,
        intent: Intent,
        device_id: Option<&str>,
        resources: Resources,
    ) -> Result<Intent> {
        if device_id.is_some_and(|id| id != intent.device_id) || intent.resources != resources {
            return Err(Error("device_setup_intent_conflict"));
        }
        Ok(intent)
    }
    pub fn begin(&self, intent: &Intent, phase: &str) -> Result<bool> {
        if !["register", "provision"].contains(&phase) {
            return Err(Error("device_setup_phase"));
        }
        let path = self.folder.join(format!(
            "device-setup-{}-{phase}.attempt",
            intent.request_id
        ));
        match private_new(&path) {
            Ok(file) => {
                file.sync_all().map_err(io)?;
                self.sync()?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(error) => Err(io(error)),
        }
    }
    fn sync(&self) -> Result<()> {
        File::open(&self.folder).map_err(io)?.sync_all().map_err(io)
    }
}
fn private_new(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
