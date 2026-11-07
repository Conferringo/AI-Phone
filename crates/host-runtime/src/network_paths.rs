//! Owned device-network directories. Preparation never installs or starts a proxy.
use crate::{Error, Result};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, symlink};
#[cfg(windows)]
use std::os::windows::fs::symlink_dir as symlink;
use std::{ffi::OsString, fs, path::Path};

fn directory(path: &Path, mode: u32) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(mode);
    #[cfg(not(unix))]
    let _ = mode;
    builder.create(path)?;
    Ok(())
}

pub(crate) fn validate(kind: &str, id: &str) -> Result<()> {
    match kind {
        "phone" if id == "phone-local-01" => {}
        "desktop"
            if id.len() == 36
                && id.bytes().all(|byte| {
                    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte) || byte == b'-'
                }) => {}
        _ => return Err(Error::Invalid("device_network_device")),
    }
    Ok(())
}
pub fn prepare(root: &Path, kind: &str, id: &str) -> Result<()> {
    validate(kind, id)?;
    let root = root.canonicalize()?;
    let parent = root.join("data/device-network");
    let folder = parent.join(id);
    directory(&folder, 0o700)?;
    if kind == "desktop" {
        let alias = parent.join("desktop");
        match fs::symlink_metadata(&alias) {
            Ok(_) => {
                if alias.canonicalize()? != folder.canonicalize()? {
                    return Err(Error::Invalid("device_network_alias_owner"));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => symlink(&folder, &alias)?,
            Err(error) => return Err(error.into()),
        }
        // This optional legacy bind remains empty unless the user already owns a core.
        directory(&root.join(".tools/mihomo/linux-arm64"), 0o755)?;
    }
    Ok(())
}
pub(crate) fn run(arguments: &[OsString]) -> Result<()> {
    let [root, kind, id] = arguments else {
        return Err(Error::Invalid("device_network_arguments"));
    };
    prepare(
        Path::new(root),
        kind.to_str()
            .ok_or(Error::Invalid("device_network_device"))?,
        id.to_str().ok_or(Error::Invalid("device_network_device"))?,
    )?;
    println!("prepared");
    Ok(())
}
