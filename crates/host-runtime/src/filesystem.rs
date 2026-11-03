use crate::{
    Error, Result,
    control::{CHUNK, Control},
    manifest::{MAX_MANIFEST, Record},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(crate) fn alias(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

pub(crate) fn directory(path: &Path, recursive: bool) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !alias(&metadata) => return Ok(()),
        Ok(_) => return Err(Error::Invalid("source_resource_directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(recursive);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}

pub(crate) fn existing_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || alias(&metadata) {
        return Err(Error::Invalid("source_resource_directory"));
    }
    Ok(())
}

pub(crate) fn lock_file(path: &Path) -> Result<File> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (alias(&metadata) || !metadata.is_file())
    {
        return Err(Error::Invalid("source_resource_lock"));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::Invalid("source_resource_lock"));
    }
    Ok(file)
}

pub(crate) fn regular(path: &Path) -> Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    if alias(&metadata) || !metadata.is_file() {
        return Err(Error::Invalid("source_resource_file"));
    }
    Ok(File::open(path)?)
}

pub(crate) fn checked_parents(root: &Path, path: &Path) -> Result<()> {
    let mut current = Some(path);
    while let Some(path) = current.filter(|path| path.starts_with(root)) {
        if alias(&fs::symlink_metadata(path)?) {
            return Err(Error::Invalid("source_resource_alias"));
        }
        if path == root {
            break;
        }
        current = path.parent();
    }
    Ok(())
}

pub(crate) fn mode(path: &Path, value: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(value))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, value);
    }
    Ok(())
}

fn correct_mode(metadata: &Metadata, expected: u32) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o777 == expected
    }
    #[cfg(not(unix))]
    {
        let _ = expected;
        !metadata.permissions().readonly()
    }
}

pub(crate) fn verify_record(root: &Path, row: &Record, control: &Control<'_>) -> Result<()> {
    let path = root.join(&row.path);
    checked_parents(root, &path)?;
    let mut input = regular(&path)?;
    let metadata = input.metadata()?;
    if metadata.len() != row.size || !correct_mode(&metadata, row.mode) {
        return Err(Error::Invalid("source_resource_changed"));
    }
    if crate::capsule::hash(&mut input, control)? != row.sha256 {
        return Err(Error::Invalid("source_resource_changed"));
    }
    Ok(())
}

pub(crate) fn bounded_file(path: &Path) -> Result<Vec<u8>> {
    let mut input = regular(path)?;
    if input.metadata()?.len() > MAX_MANIFEST {
        return Err(Error::Invalid("source_resource_metadata_size"));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut input)
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err(Error::Invalid("source_resource_metadata_size"));
    }
    Ok(bytes)
}

pub(crate) fn copy_record(
    root: &Path,
    row: &Record,
    source: &mut dyn Read,
    control: &Control<'_>,
    written: &mut u64,
) -> Result<()> {
    let (path, mut output) = record_file(root, row)?;
    copy_content(root, row, source, &mut output, control, written)?;
    output.sync_all()?;
    mode(&path, row.mode)
}

fn record_file(root: &Path, row: &Record) -> Result<(PathBuf, File)> {
    let path = root.join(&row.path);
    let parent = path
        .parent()
        .ok_or(Error::Invalid("source_resource_path"))?;
    directory(parent, true)?;
    checked_parents(root, parent)?;
    let output = File::create_new(&path)?;
    Ok((path, output))
}

struct RecordWriter<'a> {
    output: &'a mut File,
    row: &'a Record,
    digest: Sha256,
    actual: u64,
}

impl RecordWriter<'_> {
    fn append(
        &mut self,
        bytes: &[u8],
        root: &Path,
        control: &Control<'_>,
        written: &mut u64,
    ) -> Result<()> {
        self.actual += bytes.len() as u64;
        if self.actual > self.row.size {
            return Err(Error::Invalid("source_resource_changed"));
        }
        check_written(root, control, written, bytes.len() as u64)?;
        self.output.write_all(bytes)?;
        self.digest.update(bytes);
        Ok(())
    }

    fn verify(self) -> Result<()> {
        if self.actual != self.row.size
            || format!("{:x}", self.digest.finalize()) != self.row.sha256
        {
            return Err(Error::Invalid("source_resource_changed"));
        }
        Ok(())
    }
}

fn copy_content(
    root: &Path,
    row: &Record,
    source: &mut dyn Read,
    output: &mut File,
    control: &Control<'_>,
    written: &mut u64,
) -> Result<()> {
    let mut writer = RecordWriter {
        output,
        row,
        digest: Sha256::new(),
        actual: 0,
    };
    let mut buffer = [0; CHUNK];
    loop {
        control.check()?;
        let length = source.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        writer.append(&buffer[..length], root, control, written)?;
    }
    writer.verify()
}

fn check_written(path: &Path, control: &Control<'_>, written: &mut u64, size: u64) -> Result<()> {
    *written += size;
    if *written >= 32 * 1024 * 1024 {
        control.space(path, 0)?;
        *written = 0;
    }
    Ok(())
}

pub(crate) fn write_manifest(root: &Path, bytes: &[u8]) -> Result<()> {
    let path = root.join(crate::manifest::HOST_MANIFEST);
    let mut output = File::create_new(&path)?;
    output.write_all(bytes)?;
    output.sync_all()?;
    mode(&path, 0o644)
}

pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(crate) fn sync_tree(path: &Path, control: &Control<'_>) -> Result<()> {
    sync_children(path, control, 0, &mut 0)
}

fn sync_children(
    path: &Path,
    control: &Control<'_>,
    depth: usize,
    visited: &mut usize,
) -> Result<()> {
    control.check()?;
    *visited += 1;
    if depth > 64 || *visited > crate::manifest::MAX_FILES * 64 {
        return Err(Error::Invalid("source_resource_directory_budget"));
    }
    for item in fs::read_dir(path)? {
        sync_descendant(&item?, control, depth, visited)?;
    }
    sync_directory(path)
}

fn sync_descendant(
    item: &fs::DirEntry,
    control: &Control<'_>,
    depth: usize,
    visited: &mut usize,
) -> Result<()> {
    let metadata = fs::symlink_metadata(item.path())?;
    if metadata.is_dir() && !alias(&metadata) {
        sync_children(&item.path(), control, depth + 1, visited)?;
    }
    Ok(())
}

pub(crate) fn normalized_target(path: &Path, depth: usize) -> Result<PathBuf> {
    if depth > 64 {
        return Err(Error::Invalid("source_resource_link"));
    }
    if let Ok(value) = path.canonicalize() {
        return Ok(value);
    }
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        let target = link_destination(path)?;
        return normalized_target(&target, depth + 1);
    }
    let parent = path
        .parent()
        .ok_or(Error::Invalid("source_resource_link"))?
        .canonicalize()?;
    Ok(parent.join(
        path.file_name()
            .ok_or(Error::Invalid("source_resource_link"))?,
    ))
}

fn link_destination(path: &Path) -> Result<PathBuf> {
    let target = fs::read_link(path)?;
    if target.is_absolute() {
        return Ok(target);
    }
    Ok(path
        .parent()
        .ok_or(Error::Invalid("source_resource_link"))?
        .join(target))
}
