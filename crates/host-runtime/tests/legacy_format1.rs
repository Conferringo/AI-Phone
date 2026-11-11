//! Legacy format-1 metadata is not an opt-in to format-2 runtime components.
use ai_phone_host_runtime::Installation;
use flate2::{Compression, write::GzEncoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Write};

fn stage(components: Value) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let body = b"legacy source";
    let manifest = json!({"format":1,"components":components,"files":[{
        "path":"main.rs","size":body.len(),"sha256":format!("{:x}",Sha256::digest(body)),"mode":420
    }]});
    let mut target = tar::Builder::new(Vec::new());
    for (name, body) in [
        ("main.rs", body.to_vec()),
        ("HOST-MANIFEST.json", serde_json::to_vec(&manifest).unwrap()),
    ] {
        let mut header = tar::Header::new_ustar();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        target
            .append_data(&mut header, name, body.as_slice())
            .unwrap();
    }
    let mut compressed = GzEncoder::new(Vec::new(), Compression::default());
    compressed.write_all(&target.into_inner().unwrap()).unwrap();
    let bytes = compressed.finish().unwrap();
    let identity = format!("{:x}", Sha256::digest(&bytes));
    let payload = root.join("source.payload");
    fs::write(&payload, bytes).unwrap();
    let mut installation = Installation::lock(&root.join("installation")).unwrap();
    let staged = installation
        .stage_source(&payload, &identity, &|_| 64 * 1024 * 1024 * 1024, &|| false)
        .unwrap();
    assert!(staged.components.is_empty());
    assert_eq!(fs::read(staged.directory.join("main.rs")).unwrap(), body);
    let saved: Value =
        serde_json::from_slice(&fs::read(staged.directory.join("HOST-MANIFEST.json")).unwrap())
            .unwrap();
    assert_eq!(saved, manifest);
    assert!(
        installation
            .stage_source(&payload, &identity, &|_| 64 * 1024 * 1024 * 1024, &|| false)
            .unwrap()
            .reused
    );
}

#[test]
fn legacy_component_array_does_not_enable_web_runtime() {
    stage(json!(["web"]));
}

#[test]
fn legacy_component_object_is_preserved_as_metadata() {
    stage(json!({"legacy":"ignored"}));
}

#[test]
fn legacy_component_boolean_is_preserved_as_metadata() {
    stage(json!(true));
}
