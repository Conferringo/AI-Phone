//! Atomic private-file publication shared by model settings and database credentials.
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

pub(crate) fn atomic_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        fs::File::open(path.parent().expect("private directory"))?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map_err(|_| "配置无法保存，请检查存储空间与权限".into())
}
