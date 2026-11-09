use crate::{
    Error, Result, StagedSource, archive, capsule,
    control::Control,
    filesystem, links,
    manifest::{self, Inventory},
};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::Path,
};

const MAX_COMPRESSED: u64 = 2 * 1024 * 1024 * 1024;

fn layout(home: &Path) -> Result<()> {
    for name in ["releases", "user", "user/data"] {
        filesystem::directory(&home.join(name), false)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn owns_release(root: &Path, candidate: &Path) -> Result<()> {
    let releases = root
        .parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == "releases"))
        .ok_or(Error::Invalid("foreign_installation"))?;
    let home = releases
        .parent()
        .ok_or(Error::Invalid("foreign_installation"))?;
    if candidate.parent() != Some(releases)
        || !candidate
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(manifest::identity)
        || !fs::symlink_metadata(candidate.join(manifest::HOST_MANIFEST))?.is_file()
    {
        return Err(Error::Invalid("foreign_installation"));
    }
    links::persistent(root, home, false)?;
    links::persistent(candidate, home, false)
}

fn verify_existing(
    root: &Path,
    index: &Inventory,
    home: &Path,
    control: &Control<'_>,
) -> Result<()> {
    filesystem::checked_parents(root, root)?;
    if !manifest::same_manifest(
        &filesystem::bounded_file(&root.join(manifest::HOST_MANIFEST))?,
        &index.raw,
    ) {
        return Err(Error::Invalid("source_resource_changed"));
    }
    for row in &index.manifest.files {
        control.check()?;
        filesystem::verify_record(root, row, control)?;
    }
    links::runtime(root, &index.manifest.components, false)?;
    links::persistent(root, home, false)
}

fn write_source(
    file: &mut File,
    work: &Path,
    index: &Inventory,
    control: &Control<'_>,
) -> Result<()> {
    let records: BTreeMap<_, _> = index
        .manifest
        .files
        .iter()
        .map(|row| (row.path.as_str(), row))
        .collect();
    let mut written = 0;
    let mut count = 0usize;
    let repeated = archive::visit(file, control, |name, _size, source| {
        if count.is_multiple_of(32) {
            control.space(work, 0)?;
        }
        count += 1;
        let row = records
            .get(name)
            .ok_or(Error::Invalid("source_resource_inventory"))?;
        filesystem::copy_record(work, row, source, control, &mut written)
    })?;
    if repeated.headers != index.headers || !manifest::same_manifest(&repeated.raw, &index.raw) {
        return Err(Error::Invalid("source_resource_changed"));
    }
    filesystem::write_manifest(work, &index.raw)
}

fn publish(
    work: &Path,
    destination: &Path,
    index: &Inventory,
    home: &Path,
    control: &Control<'_>,
) -> Result<()> {
    control.check()?;
    if fs::symlink_metadata(destination).is_ok() {
        return verify_existing(destination, index, home, control);
    }
    fs::rename(work, destination)?;
    filesystem::sync_directory(
        destination
            .parent()
            .ok_or(Error::Invalid("source_resource_directory"))?,
    )
}

fn new_source(
    file: &mut File,
    home: &Path,
    destination: &Path,
    index: &Inventory,
    control: &Control<'_>,
) -> Result<()> {
    let total = index.manifest.files.iter().map(|row| row.size).sum();
    control.space(&home.join("releases"), total)?;
    let temporary = tempfile::Builder::new()
        .prefix(".source-stage-")
        .tempdir_in(home.join("releases"))?;
    let work = temporary.path().join("source");
    let result = fill_source(file, &work, home, index, control)
        .and_then(|()| publish(&work, destination, index, home, control));
    filesystem::finish_temporary(result, temporary)
}

fn fill_source(
    file: &mut File,
    work: &Path,
    home: &Path,
    index: &Inventory,
    control: &Control<'_>,
) -> Result<()> {
    filesystem::directory(work, false)?;
    write_source(file, work, index, control)?;
    links::runtime(work, &index.manifest.components, true)?;
    links::persistent(work, home, true)?;
    filesystem::sync_tree(work, control)
}

fn inspect_payload(
    payload: &Path,
    expected: &str,
    control: &Control<'_>,
) -> Result<(File, Inventory)> {
    let mut file = filesystem::regular(payload)?;
    capsule::check_identity(&file, expected, MAX_COMPRESSED)?;
    let index = archive::visit(&mut file, control, |_name, _size, source| {
        archive::drain(source)
    })?;
    capsule::verify_bounded_identity(&mut file, expected, control, MAX_COMPRESSED)?;
    Ok((file, index))
}

pub(crate) fn stage(
    payload: &Path,
    home: &Path,
    expected: &str,
    control: &Control<'_>,
) -> Result<StagedSource> {
    control.check()?;
    let (mut file, index) = inspect_payload(payload, expected, control)?;
    layout(home)?;
    let destination = home.join("releases").join(expected);
    let reused = fs::symlink_metadata(&destination).is_ok();
    if reused {
        verify_existing(&destination, &index, home, control)?;
    } else {
        new_source(&mut file, home, &destination, &index, control)?;
    }
    Ok(StagedSource {
        directory: destination,
        data: home.join("user/data"),
        identity: expected.to_owned(),
        components: index.manifest.components,
        reused,
    })
}
