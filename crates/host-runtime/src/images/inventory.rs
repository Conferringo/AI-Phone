use super::manifest::{MAX_ARCHIVE, MAX_METADATA, MAX_RAW, Manifest, digest, identity};
use crate::{Error, Result};
use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read},
    path::Path,
};
use xz2::{read::XzDecoder, stream::Stream};

pub(super) fn decoder<'a>(
    input: impl Read + Send + 'a,
    xz: bool,
) -> Result<Box<dyn Read + Send + 'a>> {
    if xz {
        let stream = Stream::new_stream_decoder(128 * 1024 * 1024, 0)
            .map_err(|_| Error::Invalid("image_codec"))?;
        Ok(Box::new(XzDecoder::new_stream(input, stream)))
    } else {
        Ok(Box::new(GzDecoder::new(input)))
    }
}
pub(super) struct Bounded<R> {
    pub input: R,
    total: u64,
}
impl<R> Bounded<R> {
    pub fn new(input: R) -> Self {
        Self { input, total: 0 }
    }
}
impl<R: Read> Read for Bounded<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let limit = (MAX_RAW - self.total + 1).min(bytes.len() as u64) as usize;
        let count = self.input.read(&mut bytes[..limit])?;
        self.total += count as u64;
        if self.total > MAX_RAW {
            return Err(io::Error::other(Error::Invalid("image_raw_budget")));
        }
        Ok(count)
    }
}
pub(super) fn path(value: &str) -> Result<&str> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value.bytes().any(|byte| byte < 32)
        || (value != "." && value.split('/').any(|part| matches!(part, "" | "." | "..")))
    {
        return Err(Error::Invalid("image_archive_path"));
    }
    Ok(value)
}
pub(super) struct Found {
    pub config_path: String,
    pub layers: Vec<String>,
    config: String,
    export: String,
    architecture: String,
}

pub(super) struct Blob {
    pub size: u64,
    pub digest: String,
    pub media_type: &'static str,
}
struct Inventory {
    objects: BTreeMap<String, Value>,
    files: BTreeMap<String, Blob>,
}
fn objects(input: impl Read) -> Result<Inventory> {
    let mut archive = tar::Archive::new(input);
    let mut objects = BTreeMap::new();
    let mut files = BTreeMap::new();
    let mut total = 0;
    let mut metadata = 0;
    for (index, entry) in archive.entries()?.enumerate() {
        let mut entry = entry?;
        if index >= 20000 {
            return Err(Error::Invalid("image_entry_budget"));
        }
        let name = String::from_utf8(entry.path_bytes().into_owned())
            .map_err(|_| Error::Invalid("image_archive_path"))?;
        let name = if entry.header().entry_type().is_dir() {
            name.trim_end_matches('/')
        } else {
            &name
        };
        path(name)?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let size = entry.size();
        total += size;
        if !entry.header().entry_type().is_file()
            || name == "repositories"
            || files.contains_key(name)
            || size > MAX_ARCHIVE
            || total > MAX_ARCHIVE
        {
            return Err(Error::Invalid("image_archive_member"));
        }
        let filename = name.rsplit('/').next().unwrap_or(name);
        let content_name = filename.strip_suffix(".json");
        let addressed = name.starts_with("blobs/sha256/") || content_name.is_some_and(identity);
        let mut hash = Sha256::new();
        let mut data = Vec::new();
        let mut buffer = [0; 65536];
        let mut magic = Vec::new();
        loop {
            let count = entry.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            if magic.len() < 4 {
                magic.extend_from_slice(&buffer[..count.min(4 - magic.len())]);
            }
            hash.update(&buffer[..count]);
            if size <= MAX_METADATA {
                data.extend_from_slice(&buffer[..count]);
            }
        }
        let digest = format!("{:x}", hash.finalize());
        if addressed && filename.strip_suffix(".json").unwrap_or(filename) != digest {
            return Err(Error::Invalid("image_object_digest"));
        }
        let media_type = if magic.starts_with(&[0x1f, 0x8b]) {
            "application/vnd.oci.image.layer.v1.tar+gzip"
        } else if magic.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
            "application/vnd.oci.image.layer.v1.tar+zstd"
        } else {
            "application/vnd.oci.image.layer.v1.tar"
        };
        files.insert(
            name.to_owned(),
            Blob {
                size,
                digest,
                media_type,
            },
        );
        if size <= MAX_METADATA
            && let Ok(value) = serde_json::from_slice::<Value>(&data)
            && !value.is_null()
        {
            metadata += size;
            if metadata > MAX_METADATA {
                return Err(Error::Invalid("image_metadata_budget"));
            }
            objects.insert(name.to_owned(), value);
        }
    }
    io::copy(&mut archive.into_inner(), &mut io::sink())?;
    Ok(Inventory { objects, files })
}
fn bindings(
    objects: &BTreeMap<String, Value>,
    files: &BTreeMap<String, Blob>,
) -> Result<BTreeMap<String, Found>> {
    let rows = objects
        .get("manifest.json")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty() && rows.len() <= 4)
        .ok_or(Error::Invalid("image_inventory"))?;
    let mut found = BTreeMap::new();
    for row in rows {
        let config = row
            .get("Config")
            .and_then(Value::as_str)
            .ok_or(Error::Invalid("image_config"))?;
        path(config)?;
        let value = objects
            .get(config)
            .filter(|value| value.get("os").and_then(Value::as_str) == Some("linux"))
            .ok_or(Error::Invalid("image_config"))?;
        let filename = Path::new(config)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(Error::Invalid("image_config"))?;
        let identity = format!(
            "sha256:{}",
            filename.strip_suffix(".json").unwrap_or(filename)
        );
        if !digest(&identity) {
            return Err(Error::Invalid("image_config"));
        }
        let architecture = value
            .get("architecture")
            .and_then(Value::as_str)
            .ok_or(Error::Invalid("image_config"))?;
        let layers = row
            .get("Layers")
            .and_then(Value::as_array)
            .ok_or(Error::Invalid("image_layers"))?;
        for name in layers {
            let name = name.as_str().ok_or(Error::Invalid("image_layers"))?;
            path(name)?;
            if !files.contains_key(name) {
                return Err(Error::Invalid("image_layers"));
            }
        }
        let refs = row
            .get("RepoTags")
            .and_then(Value::as_array)
            .ok_or(Error::Invalid("image_aliases"))?;
        for reference in refs {
            let reference = reference.as_str().ok_or(Error::Invalid("image_aliases"))?;
            if found
                .insert(
                    reference.to_owned(),
                    Found {
                        config_path: config.to_owned(),
                        layers: layers
                            .iter()
                            .map(|name| name.as_str().expect("validated layer path").to_owned())
                            .collect(),
                        config: identity.clone(),
                        export: identity.clone(),
                        architecture: architecture.into(),
                    },
                )
                .is_some()
            {
                return Err(Error::Invalid("image_aliases"));
            }
        }
    }
    if let Some(index) = objects.get("index.json") {
        if index.get("schemaVersion").and_then(Value::as_u64) != Some(2) {
            return Err(Error::Invalid("image_oci_index"));
        }
        let rows = index
            .get("manifests")
            .and_then(Value::as_array)
            .ok_or(Error::Invalid("image_oci_index"))?;
        let mut artifacts = Vec::new();
        for row in rows {
            let annotations = row.get("annotations");
            let reference = annotations
                .and_then(|value| value.get("io.containerd.image.name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let reference = reference.strip_prefix("docker.io/").unwrap_or(reference);
            if reference.is_empty() {
                artifacts.push(annotations);
                continue;
            }
            let item = found
                .get_mut(reference)
                .ok_or(Error::Invalid("image_oci_alias"))?;
            let identity = row
                .get("digest")
                .and_then(Value::as_str)
                .filter(|value| digest(value))
                .ok_or(Error::Invalid("image_oci_digest"))?;
            let graph = objects
                .get(&format!("blobs/sha256/{}", &identity[7..]))
                .ok_or(Error::Invalid("image_oci_graph"))?;
            if graph
                .get("config")
                .and_then(|value| value.get("digest"))
                .and_then(Value::as_str)
                != Some(&item.config)
            {
                return Err(Error::Invalid("image_oci_binding"));
            }
            item.export = identity.into();
        }
        let exported: BTreeSet<_> = found.values().map(|item| item.export.as_str()).collect();
        for annotations in artifacts {
            let subject = annotations
                .and_then(|value| value.get("io.containerd.manifest.subject"))
                .and_then(Value::as_str);
            if subject.is_none_or(|value| !exported.contains(value))
                || annotations
                    .and_then(|value| value.get("org.opencontainers.image.ref.name"))
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty())
            {
                return Err(Error::Invalid("image_oci_artifact"));
            }
        }
    }
    Ok(found)
}
pub(super) fn inspect(
    input: impl Read,
    manifest: &mut Manifest,
) -> Result<Option<super::normalize::Layout>> {
    let Inventory { objects, files } = objects(input)?;
    let actual = bindings(&objects, &files)?;
    for image in manifest.images.values() {
        let row = actual
            .get(&image.reference)
            .ok_or(Error::Invalid("image_inventory"))?;
        if row.config != image.config_digest
            || row.export != image.export_digest
            || row.architecture != manifest.architecture
        {
            return Err(Error::Invalid("image_inventory_identity"));
        }
    }
    if actual.len() != manifest.images.len() {
        return Err(Error::Invalid("image_inventory"));
    }
    if files.contains_key("oci-layout") {
        if objects
            .get("oci-layout")
            .and_then(|value| value.get("imageLayoutVersion"))
            .and_then(Value::as_str)
            != Some("1.0.0")
        {
            return Err(Error::Invalid("image_oci_layout"));
        }
        Ok(None)
    } else {
        super::normalize::prepare(&files, &actual, manifest).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_budget_is_inclusive_and_fails_before_an_extra_byte_is_imported() {
        let mut input = Bounded {
            input: &b"ab"[..],
            total: MAX_RAW - 1,
        };
        let mut byte = [0; 1];
        assert_eq!(input.read(&mut byte).unwrap(), 1);
        assert!(input.read(&mut byte).is_err());
    }
    #[test]
    fn archive_paths_reject_the_same_noncanonical_and_foreign_forms() {
        for value in ["", "/absolute", "a/../b", "a//b", "a\\b", "a\0", "a/./b"] {
            assert!(path(value).is_err(), "{value}");
        }
        for value in [".", "manifest.json", "blobs/sha256/abc"] {
            assert!(path(value).is_ok());
        }
    }
    #[test]
    fn aggregate_json_and_directory_budgets_bound_admission_memory_and_work() {
        let mut builder = tar::Builder::new(Vec::new());
        for name in ["a", "b", "c"] {
            let value = serde_json::to_vec(&"a".repeat((MAX_METADATA / 2) as usize)).unwrap();
            let mut header = tar::Header::new_gnu();
            header.set_size(value.len() as u64);
            header.set_cksum();
            builder
                .append_data(&mut header, name, value.as_slice())
                .unwrap();
        }
        assert!(matches!(
            objects(builder.into_inner().unwrap().as_slice()),
            Err(Error::Invalid("image_metadata_budget"))
        ));
        let mut builder = tar::Builder::new(Vec::new());
        for _ in 0..20001 {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_cksum();
            builder
                .append_data(&mut header, "folder", io::empty())
                .unwrap();
        }
        assert!(matches!(
            objects(builder.into_inner().unwrap().as_slice()),
            Err(Error::Invalid("image_entry_budget"))
        ));
    }
}
