//! Lossless installer volumes. The manifest digest comes from the release owner.
use crate::{
    Error, Result,
    control::{CHUNK, Control},
    filesystem,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_PART_SIZE: u64 = 1024 * 1024 * 1024;
const MAX_PACKAGE_SIZE: u64 = 64 * MAX_PART_SIZE;
const MAX_PARTS: usize = 128;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u8,
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    pub part_size: u64,
    pub parts: Vec<Part>,
}

pub struct Sealed {
    pub path: PathBuf,
    pub sha256: String,
    pub manifest: Manifest,
}

fn valid_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && !value.ends_with('.')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn part_name(file_name: &str, index: usize) -> String {
    format!("{file_name}.part-{:04}", index + 1)
}

impl Manifest {
    fn validate_header(&self) -> Result<()> {
        if self.format != 1
            || !valid_name(&self.file_name)
            || !valid_sha(&self.sha256)
            || !(1..=MAX_PACKAGE_SIZE).contains(&self.size)
            || !(1..=MAX_PART_SIZE).contains(&self.part_size)
        {
            return Err(Error::Invalid("package_manifest"));
        }
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        self.validate_header()?;
        let count = self.size.div_ceil(self.part_size);
        if self.parts.is_empty() || self.parts.len() > MAX_PARTS || self.parts.len() as u64 != count
        {
            return Err(Error::Invalid("package_volume_count"));
        }
        for (index, part) in self.parts.iter().enumerate() {
            let expected = (self.size - index as u64 * self.part_size).min(self.part_size);
            if part.name != part_name(&self.file_name, index)
                || part.size != expected
                || !valid_sha(&part.sha256)
            {
                return Err(Error::Invalid("package_volume_record"));
            }
        }
        Ok(())
    }
}

fn identity(hash: Sha256) -> String {
    format!("{:x}", hash.finalize())
}

fn manifest_bytes(path: &Path, expected: &str) -> Result<Vec<u8>> {
    if !valid_sha(expected) {
        return Err(Error::Invalid("package_manifest_identity"));
    }
    let mut file = filesystem::regular(path)?;
    if file.metadata()?.len() > 1024 * 1024 {
        return Err(Error::Invalid("package_manifest_size"));
    }
    let mut body = Vec::new();
    Read::by_ref(&mut file)
        .take(1024 * 1024 + 1)
        .read_to_end(&mut body)?;
    if body.len() > 1024 * 1024 || format!("{:x}", Sha256::digest(&body)) != expected {
        return Err(Error::Invalid("package_manifest_identity"));
    }
    Ok(body)
}

fn read_manifest(path: &Path, expected: &str) -> Result<Manifest> {
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes(path, expected)?)
        .map_err(|_| Error::Invalid("package_manifest"))?;
    manifest.validate()?;
    Ok(manifest)
}

fn copy_volume(
    input: &mut impl Read,
    output: &mut impl Write,
    length: u64,
    whole: &mut Sha256,
    control: &Control<'_>,
    folder: Option<&Path>,
) -> Result<String> {
    let mut part = Sha256::new();
    let mut buffer = [0; CHUNK];
    let mut remaining = length;
    let mut since_space = 0;
    while remaining > 0 {
        control.check()?;
        let limit = remaining.min(CHUNK as u64) as usize;
        let count = input.read(&mut buffer[..limit])?;
        if count == 0 {
            return Err(Error::Invalid("package_volume_size"));
        }
        output.write_all(&buffer[..count])?;
        whole.update(&buffer[..count]);
        part.update(&buffer[..count]);
        remaining -= count as u64;
        since_space += count;
        if since_space >= 32 * 1024 * 1024 {
            if let Some(folder) = folder {
                control.space(folder, 0)?;
            }
            since_space = 0;
        }
    }
    Ok(identity(part))
}

fn eof(input: &mut impl Read) -> Result<()> {
    if input.read(&mut [0; 1])? != 0 {
        return Err(Error::Invalid("package_volume_size"));
    }
    Ok(())
}

fn open_part(folder: &Path, part: &Part) -> Result<File> {
    let file = filesystem::regular(&folder.join(&part.name))?;
    if file.metadata()?.len() != part.size {
        return Err(Error::Invalid("package_volume_size"));
    }
    Ok(file)
}

fn copy_package(
    path: &Path,
    manifest: &Manifest,
    output: &mut impl Write,
    control: &Control<'_>,
    destination: Option<&Path>,
) -> Result<()> {
    let folder = path.parent().ok_or(Error::Invalid("package_directory"))?;
    let mut whole = Sha256::new();
    for part in &manifest.parts {
        let mut source = open_part(folder, part)?;
        if copy_volume(
            &mut source,
            output,
            part.size,
            &mut whole,
            control,
            destination,
        )? != part.sha256
        {
            return Err(Error::Invalid("package_volume_digest"));
        }
        eof(&mut source)?;
    }
    if identity(whole) != manifest.sha256 {
        return Err(Error::Invalid("package_installer_digest"));
    }
    control.check()
}

/// Verify every byte, without allocating or writing a second installer copy.
pub fn verify(path: &Path, expected: &str, cancelled: &dyn Fn() -> bool) -> Result<Manifest> {
    let manifest = read_manifest(path, expected)?;
    let control = Control {
        available: &|_| u64::MAX,
        cancelled,
    };
    copy_package(path, &manifest, &mut std::io::sink(), &control, None)?;
    Ok(manifest)
}

/// Restore into an existing download directory; never replace an existing file.
pub fn restore(
    path: &Path,
    expected: &str,
    destination: &Path,
    available: &dyn Fn(&Path) -> u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<PathBuf> {
    let manifest = read_manifest(path, expected)?;
    filesystem::directory(destination, false)?;
    let control = Control {
        available,
        cancelled,
    };
    control.space(destination, manifest.size)?;
    let target = destination.join(&manifest.file_name);
    if fs::symlink_metadata(&target).is_ok() {
        return Err(Error::Invalid("package_destination_exists"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(destination)?;
    copy_package(path, &manifest, &mut temporary, &control, Some(destination))?;
    activate(temporary, &target, destination)?;
    Ok(target)
}

fn activate(temporary: tempfile::NamedTempFile, target: &Path, parent: &Path) -> Result<()> {
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(target)
        .map_err(|error| Error::Io(error.error))?;
    filesystem::sync_directory(parent)
}

fn write_parts(
    source: &mut File,
    folder: &Path,
    manifest: &mut Manifest,
    control: &Control<'_>,
) -> Result<()> {
    let mut whole = Sha256::new();
    let mut remaining = manifest.size;
    while remaining > 0 {
        let name = part_name(&manifest.file_name, manifest.parts.len());
        let size = remaining.min(manifest.part_size);
        let mut target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(folder.join(&name))?;
        let sha256 = copy_volume(source, &mut target, size, &mut whole, control, Some(folder))?;
        target.sync_all()?;
        manifest.parts.push(Part { name, size, sha256 });
        remaining -= size;
    }
    eof(source)?;
    if identity(whole) != manifest.sha256 {
        return Err(Error::Invalid("package_installer_digest"));
    }
    Ok(())
}

fn seal(
    source: &mut File,
    folder: &Path,
    mut manifest: Manifest,
    control: &Control<'_>,
) -> Result<Sealed> {
    write_parts(source, folder, &mut manifest, control)?;
    manifest.validate()?;
    control.check()?;
    let body =
        serde_json::to_vec_pretty(&manifest).map_err(|_| Error::Invalid("package_manifest"))?;
    let path = folder.join(format!("{}.volumes.json", manifest.file_name));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(&body)?;
    file.sync_all()?;
    filesystem::sync_directory(folder)?;
    Ok(Sealed {
        path,
        sha256: format!("{:x}", Sha256::digest(&body)),
        manifest,
    })
}

fn split_manifest(input: &Path, size: u64, part_size: u64, expected: &str) -> Result<Manifest> {
    let file_name = input
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(Error::Invalid("package_name"))?
        .to_owned();
    let manifest = Manifest {
        format: 1,
        file_name,
        size,
        sha256: expected.into(),
        part_size,
        parts: Vec::new(),
    };
    manifest.validate_header()?;
    if manifest.size.div_ceil(part_size) > MAX_PARTS as u64 {
        return Err(Error::Invalid("package_split_input"));
    }
    Ok(manifest)
}

/// Split one inspected installer; on failure remove only the newly owned directory.
pub fn split(
    input: &Path,
    folder: &Path,
    part_size: u64,
    expected: &str,
    available: &dyn Fn(&Path) -> u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<Sealed> {
    let mut source = filesystem::regular(input)?;
    let manifest = split_manifest(input, source.metadata()?.len(), part_size, expected)?;
    let parent = folder.parent().ok_or(Error::Invalid("package_directory"))?;
    filesystem::directory(parent, false)?;
    let control = Control {
        available,
        cancelled,
    };
    control.space(parent, manifest.size)?;
    fs::create_dir(folder)?;
    seal(&mut source, folder, manifest, &control).map_err(|primary| cleanup_split(folder, primary))
}

fn cleanup_split(folder: &Path, primary: Error) -> Error {
    match fs::remove_dir_all(folder) {
        Ok(()) => primary,
        Err(cleanup) => Error::Cleanup {
            primary: Box::new(primary),
            cleanup,
        },
    }
}

#[cfg(test)]
mod tests;
