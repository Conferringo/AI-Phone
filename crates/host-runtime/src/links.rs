use crate::{Error, Result, filesystem, manifest};
use serde::Deserialize;
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    path: String,
    target: String,
}

#[derive(Deserialize)]
struct Links {
    #[serde(default)]
    links: Vec<Link>,
}

pub(crate) fn symbolic(target: &Path, link: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)?;
    #[cfg(windows)]
    {
        let resolved = if target.is_absolute() {
            target.to_owned()
        } else {
            link.parent()
                .ok_or(Error::Invalid("source_resource_link"))?
                .join(target)
        };
        if resolved.is_dir() {
            std::os::windows::fs::symlink_dir(target, link)?;
        } else {
            std::os::windows::fs::symlink_file(target, link)?;
        }
    }
    Ok(())
}

fn ensure(link: &Path, target: &Path, original: &Path, create: bool) -> Result<()> {
    match fs::symlink_metadata(link) {
        Ok(metadata) => verify_existing_link(link, target, &metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
            create_link(link, original)
        }
        Err(error) => Err(error.into()),
    }
}

fn verify_existing_link(link: &Path, target: &Path, metadata: &fs::Metadata) -> Result<()> {
    if !metadata.file_type().is_symlink() {
        return Err(Error::Invalid("source_resource_link_replaced"));
    }
    if filesystem::normalized_target(link, 0)? != target {
        return Err(Error::Invalid("source_resource_link_changed"));
    }
    Ok(())
}

fn create_link(link: &Path, original: &Path) -> Result<()> {
    filesystem::directory(
        link.parent()
            .ok_or(Error::Invalid("source_resource_link"))?,
        true,
    )?;
    symbolic(original, link)
}

fn component(root: &Path, metadata: &str, prefix: &str, create: bool) -> Result<()> {
    let value: Links = serde_json::from_slice(&filesystem::bounded_file(&root.join(metadata))?)
        .map_err(|_| Error::Invalid("source_resource_links"))?;
    if value.links.len() > manifest::MAX_FILES {
        return Err(Error::Invalid("source_resource_links"));
    }
    let mut seen = BTreeSet::new();
    for row in value.links {
        if !seen.insert(row.path.clone()) {
            return Err(Error::Invalid("source_resource_links"));
        }
        component_link(root, prefix, &row, create)?;
    }
    Ok(())
}

fn component_link(root: &Path, prefix: &str, row: &Link, create: bool) -> Result<()> {
    validate_component_link(prefix, row)?;
    let base = root.join(prefix).canonicalize()?;
    let link = root.join(&row.path);
    let parent = component_parent(root, &link, create)?;
    let target = component_target(&base, parent, &row.target)?;
    ensure(&link, &target, Path::new(&row.target), create)
}

fn validate_component_link(prefix: &str, row: &Link) -> Result<()> {
    manifest::source_name(&row.path, true)?;
    if !row.path.starts_with(&format!("{prefix}/"))
        || row.target.contains(['\\', ':'])
        || Path::new(&row.target).is_absolute()
    {
        return Err(Error::Invalid("source_resource_link"));
    }
    Ok(())
}

fn component_parent<'a>(root: &Path, link: &'a Path, create: bool) -> Result<&'a Path> {
    let parent = link
        .parent()
        .ok_or(Error::Invalid("source_resource_link"))?;
    if create {
        filesystem::directory(parent, true)?;
    }
    checked_component_parents(root, parent)?;
    Ok(parent)
}

fn component_target(base: &Path, parent: &Path, relative: &str) -> Result<std::path::PathBuf> {
    let target = parent.join(relative).canonicalize()?;
    if !target.starts_with(base) || parent.canonicalize()?.starts_with(&target) {
        return Err(Error::Invalid("source_resource_link_escape"));
    }
    Ok(target)
}

fn checked_component_parents(root: &Path, parent: &Path) -> Result<()> {
    let mut at = Some(parent);
    while let Some(path) = at.filter(|path| path.starts_with(root)) {
        match fs::symlink_metadata(path) {
            Ok(metadata) if filesystem::alias(&metadata) => {
                return Err(Error::Invalid("source_resource_alias"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if path == root {
            break;
        }
        at = path.parent();
    }
    Ok(())
}

pub(crate) fn runtime(root: &Path, components: &[String], create: bool) -> Result<()> {
    for (role, metadata, prefix) in [
        ("web", "runtime/manifest.json", "runtime/web"),
        (
            "js-services",
            "runtime/js-services/manifest.json",
            "runtime/js-services",
        ),
    ] {
        if components.iter().any(|name| name == role) {
            component(root, metadata, prefix, create)?;
        }
    }
    Ok(())
}

pub(crate) fn persistent(root: &Path, home: &Path, create: bool) -> Result<()> {
    for (name, target) in [
        ("data", home.join("user/data")),
        (".env", home.join("user/.env")),
    ] {
        let expected = filesystem::normalized_target(&target, 0)?;
        ensure(&root.join(name), &expected, &target, create)?;
    }
    Ok(())
}
