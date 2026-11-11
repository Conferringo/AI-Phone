use super::*;
use std::cell::Cell;

fn free(_: &Path) -> u64 {
    u64::MAX
}
fn digest(body: &[u8]) -> String {
    format!("{:x}", Sha256::digest(body))
}

fn fixture(root: &Path) -> Sealed {
    let input = root.join("AI-Phone-linux.deb");
    fs::write(
        &input,
        b"complete installer with all Android and Linux resources",
    )
    .unwrap();
    split(
        &input,
        &root.join("volumes"),
        17,
        &digest(&fs::read(&input).unwrap()),
        &free,
        &|| false,
    )
    .unwrap()
}

#[test]
fn restores_exact_installer_and_preserves_existing_destination() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let sealed = fixture(root);
    let destination = root.join("downloads");
    fs::create_dir(&destination).unwrap();
    assert_eq!(sealed.manifest.parts.len(), 4);
    assert!(sealed.manifest.parts.iter().all(|p| p.size <= 17));
    let output = restore(&sealed.path, &sealed.sha256, &destination, &free, &|| false).unwrap();
    assert_eq!(
        fs::read(&output).unwrap(),
        fs::read(root.join("AI-Phone-linux.deb")).unwrap()
    );
    fs::write(&output, b"existing user installer").unwrap();
    assert!(restore(&sealed.path, &sealed.sha256, &destination, &free, &|| false).is_err());
    assert_eq!(fs::read(output).unwrap(), b"existing user installer");
    assert_eq!(fs::read_dir(destination).unwrap().count(), 1);
}

#[test]
fn rejects_corruption_truncation_extra_bytes_and_missing_volumes_without_output() {
    for damage in 0..4 {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let sealed = fixture(root);
        let part = sealed
            .path
            .parent()
            .unwrap()
            .join(&sealed.manifest.parts[0].name);
        match damage {
            0 => fs::write(part, vec![0; 17]).unwrap(),
            1 => fs::write(part, b"short").unwrap(),
            2 => fs::write(part, vec![0; 18]).unwrap(),
            _ => fs::remove_file(part).unwrap(),
        }
        let destination = root.join("downloads");
        fs::create_dir(&destination).unwrap();
        assert!(restore(&sealed.path, &sealed.sha256, &destination, &free, &|| false).is_err());
        assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
    }
}

#[test]
fn rejects_manifest_tampering_and_wrong_installer_identity() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let sealed = fixture(root);
    assert!(verify(&sealed.path, &"0".repeat(64), &|| false).is_err());
    let mut manifest = sealed.manifest.clone();
    manifest.sha256 = "0".repeat(64);
    let body = serde_json::to_vec(&manifest).unwrap();
    fs::write(&sealed.path, &body).unwrap();
    assert!(verify(&sealed.path, &digest(&body), &|| false).is_err());
}

#[test]
fn validates_coherent_manifest_before_reading_any_volume() {
    let temporary = tempfile::tempdir().unwrap();
    let sealed = fixture(temporary.path());
    for change in 0..9 {
        let mut value = serde_json::to_value(&sealed.manifest).unwrap();
        match change {
            0 => value["file_name"] = "../outside".into(),
            1 => value["parts"][0]["name"] = "../outside".into(),
            2 => value["parts"][0]["size"] = 0.into(),
            3 => value["parts"][0]["sha256"] = "invalid".into(),
            4 => value["parts"][1] = value["parts"][0].clone(),
            5 => value["size"] = u64::MAX.into(),
            6 => value["part_size"] = 0.into(),
            7 => value["format"] = 2.into(),
            _ => value["extra"] = "unexpected".into(),
        }
        let body = serde_json::to_vec(&value).unwrap();
        fs::write(&sealed.path, &body).unwrap();
        assert!(
            verify(&sealed.path, &digest(&body), &|| false).is_err(),
            "accepted mutation {change}"
        );
    }
}

#[test]
fn failed_split_cleans_only_its_owned_output_and_keeps_input() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let input = root.join("installer.deb");
    fs::write(&input, b"retained complete installer").unwrap();
    let destination = root.join("volumes");
    assert!(split(&input, &destination, 8, &"0".repeat(64), &free, &|| false).is_err());
    assert!(!destination.exists());
    assert_eq!(fs::read(&input).unwrap(), b"retained complete installer");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), b"old package").unwrap();
    assert!(
        split(
            &input,
            &destination,
            8,
            &digest(b"retained complete installer"),
            &free,
            &|| false
        )
        .is_err()
    );
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"old package");
}

#[test]
fn cancellation_and_low_space_leave_no_partial_installer() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let sealed = fixture(root);
    let destination = root.join("downloads");
    fs::create_dir(&destination).unwrap();
    let calls = Cell::new(0);
    assert!(matches!(
        restore(&sealed.path, &sealed.sha256, &destination, &free, &|| {
            calls.set(calls.get() + 1);
            calls.get() > 3
        }),
        Err(Error::Cancelled)
    ));
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    assert!(matches!(
        restore(&sealed.path, &sealed.sha256, &destination, &|_| 0, &|| {
            false
        }),
        Err(Error::LowSpace)
    ));
    assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
}

#[test]
fn split_cancellation_keeps_the_original_error_and_removes_written_volumes() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let input = root.join("installer.deb");
    let body = b"complete installer remains available";
    fs::write(&input, body).unwrap();
    let folder = root.join("volumes");
    let calls = Cell::new(0);
    let result = split(&input, &folder, 8, &digest(body), &free, &|| {
        calls.set(calls.get() + 1);
        calls.get() > 3
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!folder.exists());
    assert_eq!(fs::read(input).unwrap(), body);
}

#[test]
fn source_digest_pin_and_part_boundaries_apply_to_empty_and_exact_multiple_files() {
    for size in [0, 1, 16, 17, 18, 34] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let input = root.join("installer.deb");
        let body = vec![size as u8; size];
        fs::write(&input, &body).unwrap();
        let result = split(
            &input,
            &root.join("volumes"),
            17,
            &digest(&body),
            &free,
            &|| false,
        );
        if size == 0 {
            assert!(result.is_err());
            continue;
        }
        let sealed = result.unwrap();
        assert_eq!(sealed.manifest.parts.len(), size.div_ceil(17));
        assert_eq!(
            verify(&sealed.path, &sealed.sha256, &|| false)
                .unwrap()
                .size,
            size as u64
        );
    }
}

#[cfg(unix)]
#[test]
fn refuses_symlinked_volume_and_source_without_touching_target() {
    use std::os::unix::fs::symlink;
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let sealed = fixture(root);
    let part = root.join("volumes").join(&sealed.manifest.parts[0].name);
    let outside = root.join("outside");
    fs::rename(&part, &outside).unwrap();
    symlink(&outside, &part).unwrap();
    assert!(verify(&sealed.path, &sealed.sha256, &|| false).is_err());
    let link = root.join("source.deb");
    symlink(root.join("AI-Phone-linux.deb"), &link).unwrap();
    assert!(
        split(
            &link,
            &root.join("other"),
            17,
            &sealed.manifest.sha256,
            &free,
            &|| false
        )
        .is_err()
    );
    assert!(outside.is_file());
    assert!(!root.join("other").exists());
}
