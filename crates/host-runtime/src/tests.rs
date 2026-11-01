use super::*;
use crate::control::RESERVE;
use flate2::{Compression, write::GzEncoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn free(_path: &Path) -> u64 {
    64 * 1024 * 1024 * 1024
}
fn record(name: &str, body: &[u8], mode: u32) -> Value {
    json!({"path":name,"size":body.len(),"sha256":sha(body),"mode":mode})
}
fn append(target: &mut tar::Builder<Vec<u8>>, name: &str, bytes: &[u8], kind: u8) {
    let mut header = tar::Header::new_ustar();
    header.set_path(name).unwrap();
    header.set_mode(0o644);
    header.set_size(bytes.len() as u64);
    header.set_entry_type(tar::EntryType::new(kind));
    header.set_cksum();
    target.append(&header, bytes).unwrap();
}
fn tar_bytes(files: &[(&str, &[u8])], manifest: &Value, first: bool) -> Vec<u8> {
    let mut target = tar::Builder::new(Vec::new());
    let raw = serde_json::to_vec(manifest).unwrap();
    if first {
        append(&mut target, manifest::HOST_MANIFEST, &raw, b'0');
    }
    for (name, body) in files {
        append(&mut target, name, body, b'0');
    }
    if !first {
        append(&mut target, manifest::HOST_MANIFEST, &raw, b'0');
    }
    target.into_inner().unwrap()
}
fn encoded(raw: &[u8], xz: bool) -> Vec<u8> {
    if xz {
        let mut output = xz2::write::XzEncoder::new(Vec::new(), 6);
        output.write_all(raw).unwrap();
        output.finish().unwrap()
    } else {
        let mut output = GzEncoder::new(Vec::new(), Compression::new(6));
        output.write_all(raw).unwrap();
        output.finish().unwrap()
    }
}
fn payload(root: &Path, raw: &[u8], xz: bool) -> (PathBuf, String) {
    let body = encoded(raw, xz);
    let path = root.join(format!("{}.payload", sha(&body)));
    fs::write(&path, &body).unwrap();
    (path, sha(&body))
}
fn minimal(root: &Path, body: &[u8], xz: bool) -> (PathBuf, String) {
    let manifest = json!({"format":1,"files":[record("scripts/main.rs",body,0o755)]});
    payload(
        root,
        &tar_bytes(&[("scripts/main.rs", body)], &manifest, false),
        xz,
    )
}
fn empty_releases(home: &Path) -> bool {
    fs::read_dir(home.join("releases"))
        .map(|entries| entries.count() == 0)
        .unwrap_or(true)
}

#[test]
fn full_gzip_and_xz_source_chain_preserves_users_and_reuses_exact_release() {
    for xz in [false, true] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let home = root.join("install");
        fs::create_dir_all(home.join("user/data")).unwrap();
        fs::write(home.join("user/data/chat"), b"retained chat").unwrap();
        fs::write(home.join("user/.env"), b"retained credential").unwrap();
        let (path, identity) = minimal(root, b"actual Rust staging", xz);
        let mut install = Installation::lock(&home).unwrap();
        let source = install
            .stage_source(&path, &identity, &free, &|| false)
            .unwrap();
        assert!(!source.reused);
        assert_eq!(
            fs::read(source.directory.join("scripts/main.rs")).unwrap(),
            b"actual Rust staging"
        );
        assert_eq!(
            fs::read(source.directory.join("data/chat")).unwrap(),
            b"retained chat"
        );
        assert_eq!(
            fs::read(source.directory.join(".env")).unwrap(),
            b"retained credential"
        );
        assert!(
            install
                .stage_source(&path, &identity, &free, &|| false)
                .unwrap()
                .reused
        );
        fs::write(source.directory.join("scripts/main.rs"), b"changed").unwrap();
        assert!(
            install
                .stage_source(&path, &identity, &free, &|| false)
                .is_err()
        );
        assert_eq!(
            fs::read(home.join("user/data/chat")).unwrap(),
            b"retained chat"
        );
    }
}

#[test]
fn manifest_may_be_first_or_last_and_preserves_old_serialized_manifest() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let manifest = json!({"format":1,"files":[record("a",b"a",0o644)]});
    for first in [false, true] {
        let (path, identity) = payload(root, &tar_bytes(&[("a", b"a")], &manifest, first), false);
        let home = root.join(if first { "first" } else { "last" });
        let mut install = Installation::lock(&home).unwrap();
        let source = install
            .stage_source(&path, &identity, &free, &|| false)
            .unwrap();
        fs::write(
            source.directory.join(manifest::HOST_MANIFEST),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        assert!(
            install
                .stage_source(&path, &identity, &free, &|| false)
                .unwrap()
                .reused
        );
    }
}

#[test]
fn malformed_digest_crc_truncation_and_compressed_tail_do_not_publish() {
    for xz in [false, true] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let (path, _) = minimal(root, b"content", xz);
        let original = fs::read(path).unwrap();
        let mut corrupt = original.clone();
        let at = corrupt.len() - 5;
        corrupt[at] ^= 1;
        let mut tail = original.clone();
        tail.extend_from_slice(b"trailing bytes");
        let mut concatenated = original.clone();
        concatenated.extend_from_slice(&original);
        for (number, bytes) in [
            corrupt,
            original[..original.len() - 4].to_vec(),
            tail,
            concatenated,
        ]
        .iter()
        .enumerate()
        {
            let path = root.join(format!("bad-{number}.payload"));
            fs::write(&path, bytes).unwrap();
            let home = root.join(format!("bad-{number}"));
            let mut install = Installation::lock(&home).unwrap();
            assert!(
                install
                    .stage_source(&path, &sha(bytes), &free, &|| false)
                    .is_err()
            );
            assert!(empty_releases(&home));
        }
    }
}

#[test]
fn traversal_private_names_duplicates_and_nonregular_tar_entries_are_rejected() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    for (number, name) in [
        "data/chat",
        ".git/config",
        "a\\b",
        ".env",
        "runtime/bin/bun",
        "a/./b",
    ]
    .iter()
    .enumerate()
    {
        let manifest = json!({"format":1,"files":[record(name,b"x",0o644)]});
        // USTAR carries backslashes literally, without platform path conversion.
        let (path, identity) = payload(root, &tar_bytes(&[(name, b"x")], &manifest, false), false);
        let home = root.join(format!("bad-path-{number}"));
        let mut install = Installation::lock(&home).unwrap();
        assert!(
            install
                .stage_source(&path, &identity, &free, &|| false)
                .is_err()
        );
        assert!(empty_releases(&home));
    }
    for kind in [b'1', b'2', b'3', b'4', b'5', b'6', b'S'] {
        let mut target = tar::Builder::new(Vec::new());
        append(&mut target, "a", b"", kind);
        let (path, identity) = payload(root, &target.into_inner().unwrap(), false);
        let mut install = Installation::lock(&root.join(format!("kind-{kind}"))).unwrap();
        assert!(
            install
                .stage_source(&path, &identity, &free, &|| false)
                .is_err()
        );
    }
    let row = record("a", b"a", 0o644);
    for (rows, files) in [
        (
            json!([row.clone(), row.clone()]),
            vec![("a", b"a".as_slice())],
        ),
        (
            json!([row]),
            vec![("a", b"a".as_slice()), ("a", b"a".as_slice())],
        ),
    ] {
        let (path, identity) = payload(
            root,
            &tar_bytes(&files, &json!({"format":1,"files":rows}), false),
            false,
        );
        let mut install =
            Installation::lock(&root.join(format!("duplicate-{}", identity))).unwrap();
        assert!(
            install
                .stage_source(&path, &identity, &free, &|| false)
                .is_err()
        );
    }
}

fn pax_path(path: &str) -> Vec<u8> {
    let body = format!(" path={path}\n");
    let mut length = body.len() + 1;
    loop {
        let actual = body.len() + length.to_string().len();
        if actual == length {
            return format!("{length}{body}").into_bytes();
        }
        length = actual;
    }
}

#[test]
fn real_pax_and_gnu_unicode_long_names_keep_their_original_path() {
    for kind in [b'x', b'L'] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let name = format!("docs/{}/資料.md", "long-path-".repeat(14));
        let mut target = tar::Builder::new(Vec::new());
        let data = if kind == b'x' {
            pax_path(&name)
        } else {
            let mut bytes = name.as_bytes().to_vec();
            bytes.push(0);
            bytes
        };
        append(&mut target, "././@PaxHeader", &data, kind);
        append(&mut target, "placeholder", b"unicode body", b'0');
        let manifest = json!({"format":1,"files":[record(&name,b"unicode body",0o644)]});
        append(
            &mut target,
            manifest::HOST_MANIFEST,
            &serde_json::to_vec(&manifest).unwrap(),
            b'0',
        );
        let (path, identity) = payload(root, &target.into_inner().unwrap(), false);
        let mut install = Installation::lock(&root.join("install")).unwrap();
        let result = install
            .stage_source(&path, &identity, &free, &|| false)
            .unwrap();
        assert_eq!(
            fs::read(result.directory.join(name)).unwrap(),
            b"unicode body"
        );
    }
}

fn runtime_tar(escape: bool) -> Vec<u8> {
    let link = if escape {
        "../../../outside"
    } else {
        ".store/library"
    };
    let metadata = serde_json::to_vec(
        &json!({"format":1,"links":[{"path":"runtime/web/node_modules/library","target":link}]}),
    )
    .unwrap();
    let files = [
        ("runtime/bin/bun", b"binary".as_slice()),
        (
            "runtime/web/node_modules/.store/library/index.js",
            b"module".as_slice(),
        ),
        ("runtime/manifest.json", metadata.as_slice()),
        ("outside", b"outside".as_slice()),
    ];
    let rows: Vec<_> = files
        .iter()
        .map(|(name, data)| record(name, data, 0o644))
        .collect();
    tar_bytes(
        &files,
        &json!({"format":2,"components":["web"],"files":rows}),
        false,
    )
}

#[test]
fn component_links_are_created_verified_and_cannot_escape_or_replace_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (path, identity) = payload(root, &runtime_tar(false), false);
    let mut install = Installation::lock(&root.join("install")).unwrap();
    let source = install
        .stage_source(&path, &identity, &free, &|| false)
        .unwrap();
    assert_eq!(source.components, vec!["web"]);
    let alias = source.directory.join("runtime/web/node_modules/library");
    assert_eq!(fs::read(alias.join("index.js")).unwrap(), b"module");
    assert!(
        install
            .stage_source(&path, &identity, &free, &|| false)
            .unwrap()
            .reused
    );
    fs::remove_file(&alias).unwrap();
    fs::create_dir(&alias).unwrap();
    assert!(
        install
            .stage_source(&path, &identity, &free, &|| false)
            .is_err()
    );
    let (bad, identity) = payload(root, &runtime_tar(true), false);
    let home = root.join("escape");
    let mut other = Installation::lock(&home).unwrap();
    assert!(
        other
            .stage_source(&bad, &identity, &free, &|| false)
            .is_err()
    );
    assert!(empty_releases(&home));
}

#[test]
fn bad_manifest_types_components_and_body_hash_never_activate() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let good = record("a", b"a", 0o644);
    for (number, manifest) in [
        json!({"format":true,"files":[good.clone()]}),
        json!({"format":1,"files":[{"path":"a","size":true,"mode":420,"sha256":sha(b"a")}]}),
        json!({"format":2,"components":["web","web"],"files":[good.clone()]}),
        json!({"format":2,"components":["unknown"],"files":[good.clone()]}),
        json!({"format":1,"files":[record("a",b"b",0o644)]}),
    ]
    .iter()
    .enumerate()
    {
        let (path, identity) = payload(root, &tar_bytes(&[("a", b"a")], manifest, false), false);
        let home = root.join(format!("manifest-{number}"));
        let mut install = Installation::lock(&home).unwrap();
        assert!(
            install
                .stage_source(&path, &identity, &free, &|| false)
                .is_err()
        );
        assert!(empty_releases(&home));
    }
}

#[test]
fn cancellation_after_partial_copy_and_space_loss_clean_only_private_stage() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let home = root.join("install");
    let (path, identity) = minimal(root, &vec![b'x'; 128 * 1024], false);
    let observed = RefCell::new(None::<PathBuf>);
    let available = |path: &Path| {
        if path.ends_with("source") {
            *observed.borrow_mut() = Some(path.to_owned());
        }
        free(path)
    };
    let cancelled = || {
        observed.borrow().as_ref().is_some_and(|path| {
            fs::metadata(path.join("scripts/main.rs")).is_ok_and(|metadata| metadata.len() > 0)
        })
    };
    let mut install = Installation::lock(&home).unwrap();
    assert!(matches!(
        install.stage_source(&path, &identity, &available, &cancelled),
        Err(Error::Cancelled)
    ));
    assert!(empty_releases(&home));
    assert!(
        install
            .stage_source(&path, &identity, &free, &|| false)
            .is_ok()
    );
    let low = root.join("low");
    let mut other = Installation::lock(&low).unwrap();
    assert!(matches!(
        other.stage_source(&path, &identity, &|_| RESERVE - 1, &|| false),
        Err(Error::LowSpace)
    ));
    assert!(empty_releases(&low));
}

#[test]
fn archive_path_replacement_cannot_change_the_opened_capsule_identity() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (path, identity) = minimal(root, b"original", false);
    let (replacement, _) = minimal(root, b"different", false);
    let changed = Cell::new(false);
    let available = |at: &Path| {
        if !changed.replace(true) {
            fs::rename(&replacement, &path).unwrap();
        }
        free(at)
    };
    let mut install = Installation::lock(&root.join("install")).unwrap();
    let result = install
        .stage_source(&path, &identity, &available, &|| false)
        .unwrap();
    assert_eq!(
        fs::read(result.directory.join("scripts/main.rs")).unwrap(),
        b"original"
    );
}

#[test]
fn concurrent_owner_is_refused_and_does_not_change_lock_or_home_permissions() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join("install");
    fs::create_dir(&home).unwrap();
    fs::write(home.join("bootstrap.lock"), b"existing lock").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o750)).unwrap();
        fs::set_permissions(
            home.join("bootstrap.lock"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
    }
    let owner = Installation::lock(&home).unwrap();
    assert!(matches!(Installation::lock(&home), Err(Error::Busy)));
    assert_eq!(
        fs::read(home.join("bootstrap.lock")).unwrap(),
        b"existing lock"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&home).unwrap().permissions().mode() & 0o777,
            0o750
        );
        assert_eq!(
            fs::metadata(home.join("bootstrap.lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
    drop(owner);
    assert!(Installation::lock(&home).is_ok());
}

#[test]
fn subprocess_lock_owner() {
    let Some(home) = std::env::var_os("AI_PHONE_HOST_LOCK_CHILD") else {
        return;
    };
    let home = PathBuf::from(home);
    let _owner = Installation::lock(&home).unwrap();
    fs::write(home.join("child-ready"), b"ready").unwrap();
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes).unwrap();
}

#[test]
fn real_child_crash_releases_the_lock_for_a_new_owner() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join("install");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::subprocess_lock_owner", "--nocapture"])
        .env("AI_PHONE_HOST_LOCK_CHILD", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !home.join("child-ready").exists() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owned child did not lock fixture installation");
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(Installation::lock(&home), Err(Error::Busy)));
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(Installation::lock(&home).is_ok());
}
