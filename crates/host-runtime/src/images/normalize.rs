//! Give legacy Docker archives an explicit OCI target instead of guessing a daemon-generated ID.
use super::{
    inventory::{Blob, Found},
    manifest::Manifest,
};
use crate::{Error, Result};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
};

pub(super) struct Layout {
    files: BTreeMap<String, String>,
    metadata: Vec<(String, Vec<u8>)>,
}
pub(super) fn prepare(
    files: &BTreeMap<String, Blob>,
    found: &BTreeMap<String, Found>,
    manifest: &mut Manifest,
) -> Result<Layout> {
    let mut plan = BTreeMap::new();
    let mut metadata = Vec::new();
    let mut descriptors = Vec::new();
    let mut rows = Vec::new();
    for (role, image) in &mut manifest.images {
        let source = &found[&image.reference];
        let config = &files[&source.config_path];
        let config_path = format!("blobs/sha256/{}", config.digest);
        plan.insert(source.config_path.clone(), config_path.clone());
        let mut layers = Vec::new();
        let mut paths = Vec::new();
        for name in &source.layers {
            let blob = &files[name];
            let path = format!("blobs/sha256/{}", blob.digest);
            plan.insert(name.clone(), path.clone());
            paths.push(path);
            layers.push(json!({"mediaType":blob.media_type,"digest":format!("sha256:{}",blob.digest),"size":blob.size}));
        }
        let graph=serde_json::to_vec(&json!({"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"mediaType":"application/vnd.oci.image.config.v1+json","digest":image.config_digest,"size":config.size},"layers":layers})).map_err(|_|Error::Invalid("image_normalization"))?;
        let identity = format!("{:x}", Sha256::digest(&graph));
        image.export_digest = format!("sha256:{identity}");
        image.reference = format!("ai-phone-bundle/{role}:sha-{identity}");
        descriptors.push(json!({"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":image.export_digest,"size":graph.len(),"platform":{"os":"linux","architecture":manifest.architecture},"annotations":{"io.containerd.image.name":format!("docker.io/{}",image.reference)}}));
        metadata.push((format!("blobs/sha256/{identity}"), graph));
        rows.push(json!({"Config":config_path,"RepoTags":[image.reference],"Layers":paths}));
    }
    manifest
        .extra
        .insert("import_layout".into(), json!("oci-v1"));
    for (name, value) in [
        (
            "index.json",
            json!({"schemaVersion":2,"manifests":descriptors}),
        ),
        ("oci-layout", json!({"imageLayoutVersion":"1.0.0"})),
        ("manifest.json", json!(rows)),
    ] {
        metadata.push((
            name.into(),
            serde_json::to_vec(&value).map_err(|_| Error::Invalid("image_normalization"))?,
        ));
    }
    Ok(Layout {
        files: plan,
        metadata,
    })
}
impl Layout {
    pub fn write(self, input: impl Read, output: impl Write) -> Result<()> {
        let mut source = tar::Archive::new(input);
        let mut target = tar::Builder::new(output);
        let mut emitted = BTreeSet::new();
        for entry in source.entries()? {
            let mut entry = entry?;
            let name = String::from_utf8(entry.path_bytes().into_owned())
                .map_err(|_| Error::Invalid("image_archive_path"))?;
            if let Some(path) = self.files.get(&name)
                && emitted.insert(path.clone())
            {
                let mut header = tar::Header::new_gnu();
                header.set_mode(0o644);
                header.set_size(entry.size());
                header.set_cksum();
                target.append_data(&mut header, path, &mut entry)?;
            }
        }
        for (name, bytes) in self.metadata {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            header.set_size(bytes.len() as u64);
            header.set_cksum();
            target.append_data(&mut header, name, bytes.as_slice())?;
        }
        target.finish()?;
        Ok(())
    }
}
