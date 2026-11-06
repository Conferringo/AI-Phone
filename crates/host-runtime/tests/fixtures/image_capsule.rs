use flate2::{Compression, write::GzEncoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const ROLES: [&str; 4] = ["workspace", "desktop", "postgres", "redis"];
pub fn hash(body: &[u8]) -> String {
    format!("{:x}", Sha256::digest(body))
}
pub fn tar(entries: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let mut archive = tar::Builder::new(Vec::new());
    for (name, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, name, bytes.as_slice())
            .unwrap();
    }
    archive.into_inner().unwrap()
}
pub fn write(home: &Path, mut manifest: Value, raw: &[u8], xz: bool) -> (PathBuf, Value) {
    let compressed = if xz {
        let mut out = xz2::write::XzEncoder::new(Vec::new(), 6);
        out.write_all(raw).unwrap();
        out.finish().unwrap()
    } else {
        let mut out = GzEncoder::new(Vec::new(), Compression::default());
        out.write_all(raw).unwrap();
        out.finish().unwrap()
    };
    let name = if xz { "images.tar.xz" } else { "images.tar.gz" };
    manifest["format"] = json!(if xz { 2 } else { 1 });
    manifest["archive"] = json!({"path":name,"codec":if xz {"xz"}else{"gzip"},"size":compressed.len(),"sha256":hash(&compressed)});
    let capsule = home.join("images.payload");
    fs::write(
        &capsule,
        tar(vec![
            (name.into(), compressed),
            (
                "IMAGES-MANIFEST.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            ),
        ]),
    )
    .unwrap();
    fs::write(
        home.join("expected.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    (capsule, manifest)
}
pub fn create(
    home: &Path,
    xz: bool,
    architecture: &str,
    nonce: &str,
    oci: bool,
) -> (PathBuf, Value, Vec<u8>) {
    build(home, xz, architecture, nonce, oci, false)
}
pub fn create_layered(
    home: &Path,
    xz: bool,
    architecture: &str,
    nonce: &str,
    oci: bool,
) -> (PathBuf, Value, Vec<u8>) {
    build(home, xz, architecture, nonce, oci, true)
}
fn build(
    home: &Path,
    xz: bool,
    architecture: &str,
    nonce: &str,
    oci: bool,
    layered: bool,
) -> (PathBuf, Value, Vec<u8>) {
    let mut objects = Vec::new();
    let mut rows = Vec::new();
    let mut images = json!({});
    let mut descriptors = Vec::new();
    for role in ROLES {
        let layer = tar(vec![(
            "qualification.txt".into(),
            format!("{nonce}-{role}").into_bytes(),
        )]);
        let layer_digest = hash(&layer);
        let layer_name = if oci {
            format!("blobs/sha256/{layer_digest}")
        } else {
            format!("{role}/layer.tar")
        };
        let layer_paths = if layered {
            vec![layer_name.clone()]
        } else {
            Vec::new()
        };
        let diff_ids = if layered {
            vec![format!("sha256:{layer_digest}")]
        } else {
            Vec::new()
        };
        let layer_descriptors = if layered {
            vec![
                json!({"mediaType":"application/vnd.oci.image.layer.v1.tar","digest":format!("sha256:{layer_digest}"),"size":layer.len()}),
            ]
        } else {
            Vec::new()
        };
        if layered {
            objects.push((layer_name, layer));
        }
        let data=serde_json::to_vec(&json!({"architecture":architecture,"os":"linux","config":{"Labels":{"dev.aiphone.qa":nonce,"dev.aiphone.qa.role":role}},"rootfs":{"type":"layers","diff_ids":diff_ids},"history":[]})).unwrap();
        let config = hash(&data);
        let name = if oci {
            format!("blobs/sha256/{config}")
        } else {
            format!("{config}.json")
        };
        let size = data.len();
        objects.push((name.clone(), data));
        let export = if oci {
            let graph=serde_json::to_vec(&json!({"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"mediaType":"application/vnd.oci.image.config.v1+json","digest":format!("sha256:{config}"),"size":size},"layers":layer_descriptors})).unwrap();
            let digest = hash(&graph);
            let reference = format!("ai-phone-bundle/{role}:sha-{digest}");
            descriptors.push(json!({"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":format!("sha256:{digest}"),"size":graph.len(),"platform":{"architecture":architecture,"os":"linux"},"annotations":{"io.containerd.image.name":format!("docker.io/{reference}")}}));
            objects.push((format!("blobs/sha256/{digest}"), graph));
            digest
        } else {
            config.clone()
        };
        let reference = format!("ai-phone-bundle/{role}:sha-{export}");
        rows.push(json!({"Config":name,"RepoTags":[reference],"Layers":layer_paths}));
        images[role] = json!({"ref":reference,"source_digest":format!("sha256:{config}"),"export_digest":format!("sha256:{export}"),"config_digest":format!("sha256:{config}"),"size":size});
    }
    objects.push(("manifest.json".into(), serde_json::to_vec(&rows).unwrap()));
    if oci {
        objects.push((
            "index.json".into(),
            serde_json::to_vec(&json!({"schemaVersion":2,"manifests":descriptors})).unwrap(),
        ));
        objects.push((
            "oci-layout".into(),
            br#"{"imageLayoutVersion":"1.0.0"}"#.to_vec(),
        ));
    }
    let raw = tar(objects);
    let (payload, manifest) = write(
        home,
        json!({"os":"linux","architecture":architecture,"redistribution":"review-pending","images":images}),
        &raw,
        xz,
    );
    (payload, manifest, raw)
}
