//! Application-owned WSL registration. Registration does not imply guest readiness.
use crate::{Error, Installation, Result, capsule, control::Control, filesystem, manifest};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
pub(crate) mod backend;
mod bundle;
pub(crate) mod devices;
mod initialize;
pub const BACKEND_INITIALIZER_CONTRACT: &str = "ai-phone-wsl-initializer-v1";
pub use initialize::{InitializeRequest, Initializer};
#[cfg(any(windows, test))]
mod command;
mod windows;
pub use windows::Native;
pub const DEVICE_SETUP_CONTRACT: &str = "ai-phone-device-setup-v2";
pub(crate) use windows::{PhoneOperation, phone_command};

#[derive(Clone)]
pub struct Registration {
    pub name: String,
    pub base_path: PathBuf,
    pub version: u32,
}

pub trait Backend {
    fn registrations(&mut self) -> Result<Vec<Registration>>;
    fn import(&mut self, name: &str, directory: &Path, rootfs: &Path) -> Result<()>;
}

pub struct Rootfs<'a> {
    pub path: &'a Path,
    pub sha256: &'a str,
    pub expanded_bytes: u64,
}

#[derive(Serialize)]
pub struct Registered {
    pub distribution: String,
    pub directory: PathBuf,
    pub reused: bool,
    pub backend_initialized: bool,
    pub phone_ready: bool,
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    Importing,
    Registered,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    format: u8,
    directory: String,
    distribution: String,
    rootfs_sha256: String,
    phase: Phase,
}

fn load(home: &Path) -> Result<Option<Owner>> {
    let path = home.join("wsl-owner.json");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let value: Owner = serde_json::from_slice(&filesystem::bounded_file(&path)?)
        .map_err(|_| Error::Invalid("wsl_owner_invalid"))?;
    validate_owner(value).map(Some)
}

fn validate_owner(value: Owner) -> Result<Owner> {
    if value.format != 1
        || !value.directory.starts_with("wsl-")
        || !(8..=64).contains(&value.directory.len())
        || !value
            .directory
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || value.distribution != format!("AIPhone-{}", value.directory)
        || !manifest::identity(&value.rootfs_sha256)
    {
        return Err(Error::Invalid("wsl_owner_invalid"));
    }
    Ok(value)
}

fn save(home: &Path, owner: &Owner, initial: bool) -> Result<()> {
    filesystem::publish_private(
        home,
        "wsl-owner.json",
        &serde_json::to_vec(owner).map_err(|_| Error::Invalid("wsl_owner_invalid"))?,
        initial,
    )
}

fn verify_rootfs(rootfs: &Rootfs<'_>, home: &Path, control: &Control<'_>) -> Result<()> {
    if !manifest::identity(rootfs.sha256)
        || !(1..=16 * 1024_u64.pow(3)).contains(&rootfs.expanded_bytes)
    {
        return Err(Error::Invalid("wsl_rootfs_identity"));
    }
    control.space(home, rootfs.expanded_bytes)?;
    let mut source = filesystem::regular(rootfs.path)?;
    if !(1..=16 * 1024_u64.pow(3)).contains(&source.metadata()?.len())
        || capsule::hash(&mut source, control)? != rootfs.sha256
    {
        return Err(Error::Invalid("wsl_rootfs_changed"));
    }
    Ok(())
}

fn prepare(
    home: &Path,
    rootfs: &Rootfs<'_>,
    backend: &mut dyn Backend,
    control: &Control<'_>,
) -> Result<Owner> {
    let directory = tempfile::Builder::new().prefix("wsl-").tempdir_in(home)?;
    let name = directory
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(Error::Invalid("wsl_owner_invalid"))?
        .to_owned();
    let owner = Owner {
        format: 1,
        distribution: format!("AIPhone-{name}"),
        directory: name,
        rootfs_sha256: rootfs.sha256.into(),
        phase: Phase::Prepared,
    };
    if backend
        .registrations()?
        .iter()
        .any(|r| r.name.eq_ignore_ascii_case(&owner.distribution))
    {
        return Err(Error::Invalid("wsl_foreign_distribution"));
    }
    control.check()?;
    save(home, &owner, true)?;
    let _owned = directory.keep();
    Ok(owner)
}

fn matching_registration(
    owner: &Owner,
    directory: &Path,
    registrations: Vec<Registration>,
) -> Result<bool> {
    let mut matching = registrations
        .into_iter()
        .filter(|r| r.name.eq_ignore_ascii_case(&owner.distribution));
    let Some(value) = matching.next() else {
        return Ok(false);
    };
    if matching.next().is_some()
        || value.version != 2
        || value.base_path.canonicalize().ok().as_deref() != Some(directory)
    {
        return Err(Error::Invalid("wsl_foreign_distribution"));
    }
    Ok(true)
}

fn import(
    home: &Path,
    owner: &mut Owner,
    directory: &Path,
    rootfs: &Rootfs<'_>,
    backend: &mut dyn Backend,
    control: &Control<'_>,
) -> Result<()> {
    control.check()?;
    // Persist uncertainty before the OS can register a distro. A failed or
    // interrupted import is observed on recovery and is never blindly repeated.
    owner.phase = Phase::Importing;
    save(home, owner, false)?;
    backend.import(&owner.distribution, directory, rootfs.path)?;
    control.check()?;
    if !matching_registration(owner, directory, backend.registrations()?)? {
        return Err(Error::Invalid("wsl_import_unknown"));
    }
    Ok(())
}

fn selected_owner(
    home: &Path,
    rootfs: &Rootfs<'_>,
    backend: &mut dyn Backend,
    control: &Control<'_>,
) -> Result<(Owner, bool)> {
    let (owner, verified) = match load(home)? {
        Some(owner) => (owner, false),
        None => {
            verify_rootfs(rootfs, home, control)?;
            (prepare(home, rootfs, backend, control)?, true)
        }
    };
    if owner.rootfs_sha256 != rootfs.sha256 {
        return Err(Error::Invalid("wsl_rootfs_upgrade_required"));
    }
    Ok((owner, verified))
}

fn owned_directory(home: &Path, owner: &Owner) -> Result<PathBuf> {
    let path = home.join(&owner.directory);
    filesystem::directory(&path, false)?;
    filesystem::checked_parents(home, &path)?;
    if owner.phase == Phase::Prepared && fs::read_dir(&path)?.next().is_some() {
        return Err(Error::Invalid("wsl_import_unknown"));
    }
    Ok(path)
}

fn import_pending(
    home: &Path,
    owner: &mut Owner,
    directory: &Path,
    rootfs: &Rootfs<'_>,
    backend: &mut dyn Backend,
    control: &Control<'_>,
    verified: bool,
) -> Result<()> {
    if owner.phase != Phase::Prepared {
        return Err(Error::Invalid("wsl_import_unknown"));
    }
    if !verified {
        verify_rootfs(rootfs, home, control)?;
    }
    import(home, owner, directory, rootfs, backend, control)
}

impl Installation {
    pub fn ensure_wsl(
        &mut self,
        rootfs: Rootfs<'_>,
        backend: &mut dyn Backend,
        available: &dyn Fn(&Path) -> u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Registered> {
        let control = Control {
            available,
            cancelled,
        };
        control.check()?;
        let (mut owner, verified) = selected_owner(&self.home, &rootfs, backend, &control)?;
        let directory = owned_directory(&self.home, &owner)?;
        let reused = matching_registration(&owner, &directory, backend.registrations()?)?;
        if !reused {
            import_pending(
                &self.home, &mut owner, &directory, &rootfs, backend, &control, verified,
            )?;
        }
        owner.phase = Phase::Registered;
        save(&self.home, &owner, false)?;
        Ok(Registered {
            distribution: owner.distribution,
            directory,
            reused,
            backend_initialized: false,
            phone_ready: false,
        })
    }
}

#[cfg(test)]
mod tests;
