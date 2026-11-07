//! Select installed guest APK metadata; installation owns APK byte verification.
use crate::{Error, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const PACKAGE: &str = "com.github.metacubex.clash.meta";
const SOURCE: [&str; 3] = [
    "config/clash-apps.json",
    "scripts/prebuilt_guest_apps.py",
    "scripts/apk_review.py",
];
fn invalid() -> Error {
    Error::Invalid("guest_application_component")
}
fn identity(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Deserialize)]
struct Pin {
    sha256: String,
    signer_sha256: String,
    size_bytes: u64,
    file: String,
    version: String,
}
impl Pin {
    fn validate(&self) -> Result<()> {
        let version = self.version.strip_prefix('v').ok_or_else(invalid)?;
        let fields = version.split('.').collect::<Vec<_>>();
        if !identity(&self.sha256)
            || !identity(&self.signer_sha256)
            || !(1..=64 * 1024 * 1024).contains(&self.size_bytes)
            || self.file.len() < 5
            || !self.file.ends_with(".apk")
            || !self
                .file
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            || fields.len() != 3
            || !fields
                .iter()
                .all(|field| !field.is_empty() && field.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct App {
    package: String,
    version: String,
    signer_sha256: String,
}
#[derive(Deserialize, PartialEq, Eq)]
struct Record {
    path: String,
    size: u64,
    sha256: String,
    mode: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    source_sha256: String,
    apps: BTreeMap<String, App>,
    files: Vec<Record>,
}
pub(crate) fn environment(root: &Path, abi: &str) -> Result<BTreeMap<String, String>> {
    let folder = root.join("runtime/guest-apps");
    let meta = match fs::symlink_metadata(&folder) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.into()),
    };
    if !meta.is_dir() {
        return Err(invalid());
    }
    let path = folder.join("manifest.json");
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || meta.len() > 65536 {
        return Err(invalid());
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(&path)?).map_err(|_| invalid())?;
    let mut sources = vec![];
    for name in SOURCE {
        sources.push((
            name,
            format!("{:x}", Sha256::digest(fs::read(root.join(name))?)),
        ));
    }
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&sources).map_err(|_| invalid())?)
    );
    if manifest.format != 1
        || manifest.source_sha256 != fingerprint
        || manifest.apps.is_empty()
        || manifest.files.len() != manifest.apps.len()
    {
        return Err(invalid());
    }
    let pins: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("config/clash-apps.json"))?)
            .map_err(|_| invalid())?;
    let mut expected = BTreeSet::from(["manifest.json".to_owned()]);
    let mut records = vec![];
    for (abi, app) in &manifest.apps {
        let value = match abi.as_str() {
            "arm64-v8a" => &pins["android"],
            "x86_64" => &pins["alternatives"]["android-x86_64"],
            _ => return Err(invalid()),
        };
        let pin: Pin = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
        pin.validate()?;
        if app
            != &(App {
                package: PACKAGE.into(),
                version: pin.version,
                signer_sha256: pin.signer_sha256,
            })
        {
            return Err(invalid());
        }
        let file = format!("clash-{abi}.apk");
        expected.insert(file.clone());
        records.push(Record {
            path: format!("runtime/guest-apps/{file}"),
            size: pin.size_bytes,
            sha256: pin.sha256,
            mode: 0o644,
        });
    }
    let mut listed = manifest.files;
    listed.sort_by(|a, b| a.path.cmp(&b.path));
    if listed != records || !manifest.apps.contains_key(abi) {
        return Err(invalid());
    }
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(&folder)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err(invalid());
        }
        actual.insert(entry.file_name().into_string().map_err(|_| invalid())?);
    }
    if actual != expected {
        return Err(invalid());
    }
    let path = folder.canonicalize()?.join(format!("clash-{abi}.apk"));
    Ok(BTreeMap::from([(
        "AI_PHONE_CLASH_ANDROID_APK".into(),
        path.to_str().ok_or_else(invalid)?.into(),
    )]))
}
