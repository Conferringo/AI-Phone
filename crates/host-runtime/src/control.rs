use crate::{Error, Result};
use std::path::Path;

pub(crate) const CHUNK: usize = 65_536;
pub(crate) const RESERVE: u64 = 4 * 1024 * 1024 * 1024 + 32 * 1024 * 1024;

pub(crate) struct Control<'a> {
    pub available: &'a dyn Fn(&Path) -> u64,
    pub cancelled: &'a dyn Fn() -> bool,
}

impl Control<'_> {
    pub fn check(&self) -> Result<()> {
        if (self.cancelled)() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn space(&self, path: &Path, needed: u64) -> Result<()> {
        self.check()?;
        let required = RESERVE.checked_add(needed).ok_or(Error::LowSpace)?;
        if (self.available)(path) < required {
            Err(Error::LowSpace)
        } else {
            Ok(())
        }
    }
}
