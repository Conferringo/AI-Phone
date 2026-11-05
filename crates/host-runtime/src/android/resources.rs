use super::manifest::{self, Manifest, Platform};
use crate::{Error, Result, archive, capsule, control::Control, filesystem};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

fn inventory(
    root: &Path,
    folder: &Path,
    files: &mut BTreeSet<String>,
    visited: &mut usize,
    depth: usize,
) -> Result<()> {
    *visited += 1;
    if depth > 64 || *visited > crate::manifest::MAX_FILES * 64 {
        return Err(Error::Invalid("android_resource_inventory"));
    }
    for entry in fs::read_dir(folder)? {
        inventory_entry(root, &entry?, files, visited, depth)?;
    }
    Ok(())
}
fn inventory_entry(
    root: &Path,
    entry: &fs::DirEntry,
    files: &mut BTreeSet<String>,
    visited: &mut usize,
    depth: usize,
) -> Result<()> {
    let metadata = fs::symlink_metadata(entry.path())?;
    if filesystem::alias(&metadata) {
        return Err(Error::Invalid("android_resource_alias"));
    }
    if metadata.is_dir() {
        inventory(root, &entry.path(), files, visited, depth + 1)?;
    } else if metadata.is_file() {
        files.insert(
            entry
                .path()
                .strip_prefix(root)
                .map_err(|_| Error::Invalid("android_resource_path"))?
                .to_string_lossy()
                .replace('\\', "/"),
        );
    } else {
        return Err(Error::Invalid("android_resource_file"));
    }
    Ok(())
}
fn existing(
    root: &Path,
    raw: &archive::RawInventory,
    manifest: &Manifest,
    control: &Control<'_>,
) -> Result<()> {
    filesystem::existing_directory(root)?;
    if !crate::manifest::same_manifest(
        &filesystem::bounded_file_with_limit(
            &root.join(manifest::NAME),
            manifest::LIMITS.metadata,
        )?,
        &raw.raw,
    ) {
        return Err(Error::Invalid("android_resource_changed"));
    }
    let mut actual = BTreeSet::new();
    inventory(root, root, &mut actual, &mut 0, 0)?;
    let expected: BTreeSet<_> = raw.headers.keys().cloned().collect();
    if actual != expected {
        return Err(Error::Invalid("android_resource_inventory"));
    }
    for row in &manifest.files {
        control.check()?;
        filesystem::verify_record(root, row, control)?;
    }
    Ok(())
}
fn fill(
    file: &mut File,
    root: &Path,
    raw: &archive::RawInventory,
    manifest: &Manifest,
    control: &Control<'_>,
) -> Result<()> {
    filesystem::directory(root, false)?;
    let index: BTreeMap<_, _> = manifest
        .files
        .iter()
        .map(|r| (r.path.as_str(), r))
        .collect();
    let mut written = 0;
    let actual =
        archive::visit_component(file, control, &manifest::LIMITS, |name, size, source| {
            let row = index
                .get(name)
                .ok_or(Error::Invalid("android_resource_inventory"))?;
            if row.size != size {
                return Err(Error::Invalid("android_resource_inventory"));
            }
            filesystem::copy_record(root, row, source, control, &mut written)
        })?;
    if actual.headers != raw.headers || actual.raw != raw.raw {
        return Err(Error::Invalid("android_resource_changed"));
    }
    write_manifest(root, &raw.raw)?;
    filesystem::sync_tree(root, control)
}
fn write_manifest(root: &Path, bytes: &[u8]) -> Result<()> {
    let mut output = File::create_new(root.join(manifest::NAME))?;
    output.write_all(bytes)?;
    output.sync_all()?;
    filesystem::mode(&root.join(manifest::NAME), 0o644)
}
fn publish(
    file: &mut File,
    folder: &Path,
    destination: &Path,
    raw: &archive::RawInventory,
    manifest: &Manifest,
    control: &Control<'_>,
) -> Result<()> {
    control.space(folder, manifest.files.iter().map(|r| r.size).sum())?;
    let temporary = tempfile::Builder::new()
        .prefix(".android-")
        .tempdir_in(folder)?;
    let work = temporary.path().join("runtime");
    let result = (|| {
        fill(file, &work, raw, manifest, control)?;
        control.check()?;
        fs::rename(&work, destination)?;
        filesystem::sync_directory(folder)
    })();
    filesystem::finish_temporary(result, temporary)
}
fn inspect(
    payload: &Path,
    identity: &str,
    platform: Platform,
    control: &Control<'_>,
) -> Result<(File, archive::RawInventory, Manifest)> {
    let mut file = filesystem::regular(payload)?;
    capsule::verify_bounded_identity(&mut file, identity, control, manifest::LIMITS.total)?;
    let raw = archive::visit_component(&mut file, control, &manifest::LIMITS, |_, _, reader| {
        archive::drain(reader)
    })?;
    let manifest = manifest::parse(&raw, platform)?;
    Ok((file, raw, manifest))
}
pub(super) fn stage(
    home: &Path,
    payload: &Path,
    identity: &str,
    platform: Platform,
    control: &Control<'_>,
) -> Result<(PathBuf, bool)> {
    let (mut file, raw, manifest) = inspect(payload, identity, platform, control)?;
    for name in ["components", "components/android"] {
        filesystem::directory(&home.join(name), false)?;
    }
    let folder = home.join("components/android");
    let destination = folder.join(identity);
    let reused = fs::symlink_metadata(&destination).is_ok();
    if reused {
        existing(&destination, &raw, &manifest, control)?;
    } else {
        publish(&mut file, &folder, &destination, &raw, &manifest, control)?;
    }
    Ok((destination.join("sdk"), reused))
}
