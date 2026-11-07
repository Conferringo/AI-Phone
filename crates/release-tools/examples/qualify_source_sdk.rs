//! Explicit maintainer qualification; run as root on a disposable Linux builder.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rustix::process::{Pid, Signal, kill_process_group};
    use std::{
        fs,
        os::unix::{
            fs::{MetadataExt, PermissionsExt, chown, symlink},
            process::CommandExt,
        },
        path::Path,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let tool = std::env::args_os()
        .nth(1)
        .ok_or("Pass the release-tools binary")?;
    let fixture = tempfile::Builder::new().prefix("source-sdk-").tempdir()?;
    fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o755))?;
    let source = fixture.path().join("source");
    let sdk = source.join("linux");
    let generic = source.join("generic/system-images");
    fs::create_dir_all(&sdk)?;
    fs::create_dir_all(&generic)?;
    for path in [&source, &sdk, &source.join("generic"), &generic] {
        chown(path, Some(65534), Some(65534))?;
    }
    fs::write(sdk.join(".knownPackages"), b"pinned")?;
    fs::write(
        generic.join("source.properties"),
        b"original image metadata",
    )?;
    let image = generic.join("system.img");
    fs::write(&image, vec![73; 16 * 1024 * 1024])?;
    let zero_lengths = [0, 1, 1024 * 1024 - 1, 1024 * 1024, 1024 * 1024 + 1];
    for length in zero_lengths {
        fs::write(generic.join(format!("zero-{length}")), vec![0; length])?;
    }
    fs::set_permissions(&image, fs::Permissions::from_mode(0o640))?;
    for path in [
        &sdk.join(".knownPackages"),
        &generic.join("source.properties"),
        &image,
    ] {
        chown(path, Some(65534), Some(65534))?;
    }
    symlink("../generic/system-images", sdk.join("system-images"))?;
    let inode = source.metadata()?.ino();
    let image_metadata = image.metadata()?;
    let launch = |work: &Path, script: &str| {
        let mut command = Command::new("unshare");
        command
            .args(["--mount", "--propagation", "private", "--"])
            .arg(&tool)
            .arg("source-sdk-build")
            .arg(&source)
            .arg(work)
            .args(["65534", "65534", "/bin/sh", "-eu", "-c", script, "fixture"])
            .arg(&source);
        command
    };
    let assert_original = || -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(source.metadata()?.ino(), inode);
        assert_eq!(fs::read(sdk.join(".knownPackages"))?, b"pinned");
        assert_eq!(
            fs::read(generic.join("source.properties"))?,
            b"original image metadata"
        );
        assert_eq!(fs::read(&image)?, vec![73; 16 * 1024 * 1024]);
        assert_eq!(
            image.metadata()?.permissions().mode(),
            image_metadata.permissions().mode()
        );
        assert_eq!(image.metadata()?.modified()?, image_metadata.modified()?);
        assert_eq!(
            fs::read_link(sdk.join("system-images"))?,
            Path::new("../generic/system-images")
        );
        assert!(!sdk.join("package.xml").exists());
        Ok(())
    };
    let work = fixture.path().join("success");
    fs::create_dir(&work)?;
    let output = launch(
        &work,
        r#"
        test "$(id -u)" = 65534
        test "$(wc -c < "$1/linux/system-images/system.img")" = 16777216
        for path in "$1/linux/system-images"/zero-*; do
            test "$(wc -c < "$path")" = "${path##*-}"
        done
        test "$(cat "$1/linux/.knownPackages")" = pinned
        printf generated > "$1/linux/.knownPackages"
        printf generated > "$1/linux/system-images/source.properties"
        printf package > "$1/linux/package.xml"
    "#,
    )
    .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_original()?;
    assert_eq!(
        fs::read(work.join("sdk-upper/linux/.knownPackages"))?,
        b"generated"
    );
    assert_eq!(
        fs::read(work.join("sdk-upper/generic/system-images/source.properties"))?,
        b"generated"
    );
    assert!(
        !work
            .join("sdk-upper/generic/system-images/system.img")
            .exists()
    );
    for length in zero_lengths {
        assert_eq!(
            fs::read(generic.join(format!("zero-{length}")))?,
            vec![0; length]
        );
        assert!(
            !work
                .join(format!("sdk-upper/generic/system-images/zero-{length}"))
                .exists()
        );
    }
    let report: serde_json::Value = serde_json::from_slice(&fs::read(work.join("sdk-copy.json"))?)?;
    assert_eq!(
        report,
        serde_json::json!({"backend":"overlay","files":0,"logical_bytes":0,"allocated_bytes":0})
    );

    let failed = fixture.path().join("failed");
    fs::create_dir(&failed)?;
    let output = launch(
        &failed,
        r#"
        chmod 600 "$1/linux/system-images/system.img"
        printf X | dd of="$1/linux/system-images/system.img" bs=1 count=1 conv=notrunc status=none
        exit 23
    "#,
    )
    .output()?;
    assert_eq!(
        output.status.code(),
        Some(23),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_original()?;
    assert_eq!(
        fs::read(failed.join("sdk-upper/generic/system-images/system.img"))?[0],
        b'X'
    );

    let cancelled = fixture.path().join("cancelled");
    fs::create_dir(&cancelled)?;
    let mut child = launch(
        &cancelled,
        r#"
        printf generated > "$1/linux/.knownPackages"
        printf ready > "$1/linux/ready"
        sleep 60 & wait
    "#,
    )
    .process_group(0)
    .stdout(Stdio::null())
    .stderr(Stdio::inherit())
    .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cancelled.join("sdk-upper/linux/ready").exists() {
        if child.try_wait()?.is_some() {
            return Err("Overlay child exited before readiness".into());
        }
        if Instant::now() >= deadline {
            kill_process_group(Pid::from_raw(child.id() as i32).unwrap(), Signal::KILL)?;
            child.wait()?;
            return Err("Overlay readiness timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_original()?;
    kill_process_group(Pid::from_raw(child.id() as i32).unwrap(), Signal::KILL)?;
    assert!(!child.wait()?.success());
    assert_original()?;
    println!(
        "SDK overlay: normal exit, failed exit, whole-group cancellation, source metadata and image bytes preserved; no initial image copy"
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("Source SDK overlay qualification requires Linux".into())
}
