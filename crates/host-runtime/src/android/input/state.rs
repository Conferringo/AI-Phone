use crate::{Error, Result, filesystem};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Stage {
    InstallUnknown,
    Configure,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    format: u8,
    package: String,
    serial: String,
    user: String,
    stage: Stage,
}
pub(super) struct State {
    pub directory: PathBuf,
    pub serial: String,
    _lease: filesystem::Lease,
}
impl State {
    pub fn lock(root: &Path, serial: &str) -> Result<Self> {
        if serial.is_empty()
            || serial.len() > 128
            || !serial
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
        {
            return Err(Error::Invalid("phone_input_serial"));
        }
        filesystem::existing_directory(root)?;
        let data = root.join("data");
        if !data.try_exists()? {
            filesystem::directory(&data, false)?;
        }
        // The source publisher owns root/data -> installation/user/data.
        let data = data.canonicalize()?;
        filesystem::existing_directory(&data)?;
        let directory = data.join("android");
        filesystem::directory(&directory, false)?;
        let lease = filesystem::lease(&directory.join("input-setup.lock"))?.ok_or(Error::Busy)?;
        Ok(Self {
            directory,
            serial: serial.into(),
            _lease: lease,
        })
    }
    fn path(&self) -> PathBuf {
        self.directory.join("input-setup.pending.json")
    }
    pub fn load(&self) -> Result<Option<Stage>> {
        let path = self.path();
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
            Ok(_) => {}
        }
        let value: Pending =
            serde_json::from_slice(&filesystem::bounded_file_with_limit(&path, 4096)?)
                .map_err(|_| Error::Invalid("phone_input_pending_owner"))?;
        if value.format != 1
            || value.package != super::PACKAGE
            || value.serial != self.serial
            || value.user != "0"
        {
            return Err(Error::Invalid("phone_input_pending_owner"));
        }
        Ok(Some(value.stage))
    }
    pub fn save(&self, stage: Stage) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        let value = Pending {
            format: 1,
            package: super::PACKAGE.into(),
            serial: self.serial.clone(),
            user: "0".into(),
            stage,
        };
        file.write_all(
            &serde_json::to_vec(&value).map_err(|_| Error::Invalid("phone_input_pending_owner"))?,
        )?;
        file.as_file().sync_all()?;
        file.persist(self.path()).map_err(|e| Error::Io(e.error))?;
        filesystem::sync_directory(&self.directory)
    }
    pub fn finish(&self) -> Result<()> {
        if self.path().try_exists()? {
            std::fs::remove_file(self.path())?;
            filesystem::sync_directory(&self.directory)?;
        }
        Ok(())
    }
}
