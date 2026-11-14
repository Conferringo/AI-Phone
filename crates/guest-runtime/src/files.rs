use crate::Result;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub fn work_path(home: &Path, value: &str) -> Result<PathBuf> {
    let root = home.canonicalize()?.join("Files");
    if root.is_symlink() {
        return Err("只允许工作文件目录".into());
    }
    let path = root.join(value).canonicalize()?;
    if !path.starts_with(&root) {
        return Err("只允许工作文件目录".into());
    }
    Ok(path)
}

pub fn list(home: &Path, value: &str) -> Result<Value> {
    let root = home.canonicalize()?.join("Files");
    let path = work_path(home, value)?;
    let mut entries = std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let files = entries
        .iter()
        .take(100)
        .map(|entry| file_record(entry, &root))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({"files": files}))
}

fn file_record(entry: &std::fs::DirEntry, root: &Path) -> Result<Value> {
    let metadata = std::fs::metadata(entry.path())?;
    let name = entry.file_name();
    let path = entry.path();
    let relative = path.strip_prefix(root)?;
    Ok(json!({
        "name": name.to_str().ok_or("文件名不是有效的UTF-8")?,
        "path": relative.to_str().ok_or("文件路径不是有效的UTF-8")?,
        "directory": metadata.is_dir(),
        "bytes": metadata.len(),
    }))
}
