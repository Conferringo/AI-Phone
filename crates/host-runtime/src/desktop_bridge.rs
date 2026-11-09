//! Apply the source release's desktop helper without rebuilding the guest or touching its home.
use crate::Result;
use sha2::{Digest, Sha256};
use std::{io::ErrorKind, path::Path};

const INSTALL: &str = r#"
target=/usr/local/bin/desktop-bridge
temporary=$(mktemp /usr/local/bin/.desktop-bridge.XXXXXXXX)
trap 'rm -f "$temporary"' EXIT
cat > "$temporary"
checksum=$(sha256sum < "$temporary")
test "${checksum%% *}" = "$1"
if cmp -s "$temporary" "$target"; then exit 0; fi
chmod 0755 "$temporary"
"$temporary" --version >/dev/null
mv -f "$temporary" "$target"
"#;

pub struct DesktopBridge {
    pub bytes: Vec<u8>,
    sha256: String,
}

impl DesktopBridge {
    pub async fn staged(root: &Path) -> Result<Option<Self>> {
        match tokio::fs::read(root.join("runtime/services/bin/desktop-bridge")).await {
            Ok(bytes) => Ok(Some(Self {
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes,
            })),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn install_args<'a>(&'a self, container: &'a str) -> [&'a str; 11] {
        [
            "exec",
            "-i",
            "-u",
            "0",
            container,
            "/bin/sh",
            "-eu",
            "-c",
            INSTALL,
            "sh",
            &self.sha256,
        ]
    }
}
