use super::{Device, REMOTE_PROBE};
use crate::{Error, Result, filesystem};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) struct Probe<'a> {
    path: &'a str,
    digest: String,
}

impl<'a> Probe<'a> {
    pub fn load(path: &'a Path) -> Result<Self> {
        let bytes = filesystem::bounded_file_with_limit(path, 64 * 1024)?;
        if bytes.is_empty() {
            return Err(Error::Invalid("phone_input_resource"));
        }
        Ok(Self {
            path: path
                .to_str()
                .ok_or(Error::Invalid("phone_input_resource"))?,
            digest: format!("{:x}", Sha256::digest(&bytes)),
        })
    }

    async fn current(&self, adb: &mut impl Device) -> Result<bool> {
        // The static script receives the fixed path as a positional argument.
        // Absence is distinct from a failed or malformed observation.
        let value = adb
            .execute(
                &[
                    "shell",
                    "sh",
                    "-c",
                    "if [ -f \"$1\" ]; then sha256sum \"$1\"; else printf missing; fi",
                    "sh",
                    REMOTE_PROBE,
                ],
                10,
            )
            .await?;
        if value == "missing" {
            return Ok(false);
        }
        let (digest, name) = value
            .split_once("  ")
            .ok_or(Error::Invalid("phone_input_probe_unknown"))?;
        if name != REMOTE_PROBE
            || digest.len() != 64
            || !digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::Invalid("phone_input_probe_unknown"));
        }
        Ok(digest == self.digest)
    }

    pub async fn prepare(&self, adb: &mut impl Device) -> Result<bool> {
        if self.current(adb).await? {
            return Ok(false);
        }
        adb.execute(&["push", self.path, REMOTE_PROBE], 10).await?;
        if !self.current(adb).await? {
            return Err(Error::Invalid("phone_input_probe_unknown"));
        }
        Ok(true)
    }
}
