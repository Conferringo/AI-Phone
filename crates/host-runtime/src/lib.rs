//! Application-owned native resources, lifecycle and paired device transports.
//! Device permissions and action receipts remain owned by their callers.
pub mod android;
mod archive;
mod capsule;
#[cfg(unix)]
mod channel_browser;
pub mod component;
mod control;
pub mod device_bridge;
mod filesystem;
#[cfg(unix)]
mod guest_apps;
#[cfg(unix)]
mod images;
pub mod installer_tools;
mod links;
#[cfg(unix)]
mod managed_engine;
mod manifest;
pub mod native_ipc;
pub mod network_paths;
pub mod package;
mod profile;
#[cfg(unix)]
mod runtime_environment;
mod source_cli;
mod staging;
pub mod supervision;
#[cfg(unix)]
mod unix_initializer;
pub mod wsl;

use std::{
    fmt,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Invalid(&'static str),
    Io(std::io::Error),
    Cancelled,
    Busy,
    LowSpace,
    Cleanup {
        primary: Box<Error>,
        cleanup: std::io::Error,
    },
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid(code) => code,
            Self::Io(_) => "source_resource_io",
            Self::Cancelled => "installation_cancelled",
            Self::Busy => "installation_running",
            Self::LowSpace => "source_resource_space",
            Self::Cleanup { primary, .. } => primary.code(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(self.code())
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        if error.get_ref().is_some_and(|inner| inner.is::<Self>()) {
            if let Some(inner) = error.into_inner() {
                return match inner.downcast::<Self>() {
                    Ok(error) => *error,
                    Err(error) => Self::Io(std::io::Error::other(error)),
                };
            }
            return Self::Invalid("source_resource_io");
        }
        Self::Io(error)
    }
}

/// This guard must remain alive for the complete installation transaction.
pub struct Installation {
    home: PathBuf,
    _lock: filesystem::Lease,
}

#[derive(Debug, Clone)]
pub struct StagedSource {
    pub directory: PathBuf,
    pub data: PathBuf,
    pub identity: String,
    pub components: Vec<String>,
    pub reused: bool,
}

impl Installation {
    pub fn lock(home: &Path) -> Result<Self> {
        filesystem::directory(home, true)?;
        let home = home.canonicalize()?;
        let lock = filesystem::lease(&home.join("bootstrap.lock"))?.ok_or(Error::Busy)?;
        Ok(Self { home, _lock: lock })
    }

    pub fn stage_source(
        &mut self,
        payload: &Path,
        expected_sha256: &str,
        available: &dyn Fn(&Path) -> u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<StagedSource> {
        let control = control::Control {
            available,
            cancelled,
        };
        staging::stage(payload, &self.home, expected_sha256, &control)
    }
}

#[cfg(test)]
mod tests;
