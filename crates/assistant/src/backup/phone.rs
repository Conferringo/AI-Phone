use super::{Result, command, digest, environment, text};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub(super) fn dependencies(root: &Path) -> Result<(Value, Value)> {
    let env = environment(root)?;
    let sdk =
        Path::new(env.get("AI_PHONE_SDK_ROOT").ok_or("Android SDK missing")?).canonicalize()?;
    let data = root.join("data").canonicalize()?;
    let avds = data.join("android/avd");
    let mut dependencies = Map::new();
    let mut chains = vec![];
    let mut seen = BTreeSet::new();
    if avds.try_exists()? {
        for avd in fs::read_dir(&avds)? {
            let avd = avd?;
            if !avd.file_type()?.is_dir() || !avd.file_name().to_string_lossy().ends_with(".avd") {
                continue;
            }
            for entry in fs::read_dir(avd.path())? {
                let file = entry?.path();
                if file
                    .extension()
                    .is_none_or(|extension| extension != "qcow2")
                {
                    continue;
                }
                let chain: Vec<Value> = serde_json::from_slice(&command(
                    text(&sdk.join("emulator/qemu-img"))?,
                    &["info", "--backing-chain", "--output=json", text(&file)?],
                    None,
                )?)?;
                for record in chain {
                    let source =
                        Path::new(record["filename"].as_str().ok_or("Phone image missing")?);
                    let source = if source.is_absolute() {
                        source.to_path_buf()
                    } else {
                        file.parent().ok_or("Phone parent missing")?.join(source)
                    }
                    .canonicalize()?;
                    let Some(backing) = record["full-backing-filename"]
                        .as_str()
                        .or_else(|| record["backing-filename"].as_str())
                    else {
                        continue;
                    };
                    let backing = Path::new(backing);
                    let backing = if backing.is_absolute() {
                        backing.to_path_buf()
                    } else {
                        source
                            .parent()
                            .ok_or("Backing parent missing")?
                            .join(backing)
                    }
                    .canonicalize()?;
                    if !source.starts_with(&data) {
                        return Err("Writable phone backing leaves user state".into());
                    }
                    if backing.starts_with(&data) {
                        dependencies.insert(
                            text(backing.strip_prefix(&data)?)?.into(),
                            Value::Bool(true),
                        );
                    } else if backing.starts_with(&sdk) {
                        let name = format!("sdk:{}", text(backing.strip_prefix(&sdk)?)?);
                        if !dependencies.contains_key(&name) {
                            dependencies.insert(name,json!({"size":fs::metadata(&backing)?.len(),"sha256":digest(&backing)?}));
                        }
                    } else {
                        return Err("Phone backing leaves this installation".into());
                    }
                    let image = text(source.strip_prefix(&data)?)?.to_owned();
                    let format = record["backing-filename-format"]
                        .as_str()
                        .unwrap_or("raw")
                        .to_owned();
                    if seen.insert((image.clone(), backing.clone(), format.clone())) {
                        chains.push(json!({"image":image,"backing":backing,"format":format}));
                    }
                }
            }
        }
    }
    Ok((Value::Object(dependencies), Value::Array(chains)))
}
fn replace_paths(input: &str, old_root: &str, old_data: &str, final_root: &Path) -> Result<String> {
    let final_root = text(final_root)?;
    let final_data = format!("{final_root}/data");
    let mut pairs = [
        (old_root, final_root),
        (old_data, final_data.as_str()),
        (final_root, final_root),
        (final_data.as_str(), final_data.as_str()),
    ];
    pairs.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    let mut remaining = input;
    let mut output = String::with_capacity(input.len());
    while !remaining.is_empty() {
        if let Some((from, to)) = pairs
            .iter()
            .find(|(from, _)| !from.is_empty() && remaining.starts_with(*from))
        {
            output.push_str(to);
            remaining = &remaining[from.len()..];
        } else {
            let next = remaining.chars().next().ok_or("Path text missing")?;
            output.push(next);
            remaining = &remaining[next.len_utf8()..];
        }
    }
    Ok(output)
}
fn ini_files(
    folder: &Path,
    root: &Path,
    old_root: &str,
    old_data: &str,
    final_root: &Path,
    changed: &mut Vec<String>,
) -> Result<()> {
    if !folder.try_exists()? {
        return Ok(());
    }
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            ini_files(&path, root, old_root, old_data, final_root, changed)?;
        } else if entry.file_type()?.is_file()
            && path.extension().is_some_and(|extension| extension == "ini")
        {
            let original = fs::read_to_string(&path)?;
            let next = replace_paths(&original, old_root, old_data, final_root)?;
            if next != original {
                fs::write(&path, next)?;
                changed.push(text(path.strip_prefix(root)?)?.into());
            }
        }
    }
    Ok(())
}
pub(super) fn relocate(
    root: &Path,
    manifest: &Value,
    final_root: &Path,
    sdk: &Path,
) -> Result<Vec<String>> {
    let old_root = manifest["source_root"]
        .as_str()
        .ok_or("Backup source root missing")?;
    let old_data = manifest["source_data"]
        .as_str()
        .ok_or("Backup source data missing")?;
    let mut changed = vec![];
    ini_files(
        &root.join("data/android/avd"),
        root,
        old_root,
        old_data,
        final_root,
        &mut changed,
    )?;
    let network = root.join("data/device-network");
    if network.try_exists()? {
        for entry in fs::read_dir(network)? {
            let entry = entry?;
            if !entry.file_type()?.is_symlink() {
                continue;
            }
            let original = fs::read_link(entry.path())?;
            let next = replace_paths(text(&original)?, old_root, old_data, final_root)?;
            if Path::new(&next) != original {
                fs::remove_file(entry.path())?;
                std::os::unix::fs::symlink(next, entry.path())?;
                changed.push(text(entry.path().strip_prefix(root)?)?.into());
            }
        }
    }
    let dependencies = manifest["disk_dependencies"]
        .as_object()
        .ok_or("Backup disk dependencies missing")?;
    for (name, row) in dependencies {
        if let Some(name) = name.strip_prefix("sdk:") {
            let file = sdk.join(name);
            if fs::metadata(&file)?.len()
                != row["size"].as_u64().ok_or("SDK dependency size missing")?
                || digest(&file)? != row["sha256"]
            {
                return Err("Matching Android SDK image required for restore".into());
            }
        }
    }
    for chain in manifest["disk_chains"]
        .as_array()
        .ok_or("Backup disk chains missing")?
    {
        let image = chain["image"].as_str().ok_or("Phone image missing")?;
        let original = chain["backing"].as_str().ok_or("Phone backing missing")?;
        let format = chain["format"]
            .as_str()
            .ok_or("Phone backing format missing")?;
        let mut backing = PathBuf::from(original);
        if let Ok(path) = backing.strip_prefix(old_data) {
            backing = root.join("data").join(path);
        } else if let Ok(path) = backing.strip_prefix(old_root) {
            backing = root.join(path);
        }
        for name in dependencies
            .keys()
            .filter_map(|key| key.strip_prefix("sdk:"))
        {
            if backing.ends_with(name) {
                backing = sdk.join(name);
                break;
            }
        }
        let target = if let Ok(relative) = backing.strip_prefix(root) {
            changed.push(format!("data/{image}"));
            final_root.join(relative)
        } else {
            backing
        };
        command(
            text(&sdk.join("emulator/qemu-img"))?,
            &[
                "rebase",
                "-u",
                "-f",
                "qcow2",
                "-F",
                format,
                "-b",
                text(&target)?,
                text(&root.join("data").join(image))?,
            ],
            None,
        )?;
    }
    Ok(changed)
}
