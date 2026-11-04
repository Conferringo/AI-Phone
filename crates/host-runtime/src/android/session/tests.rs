use super::{
    plan::{Plan, Ports},
    process::{self, Started},
    readiness,
};
use crate::{Error, Result, control::Control};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use tokio::sync::{Mutex, watch};

static SDK_TEST: Mutex<()> = Mutex::const_new(());
fn fixture_platform() -> super::super::manifest::Platform {
    use super::super::manifest::Platform;
    Platform::current().unwrap_or(Platform::LinuxX64)
}
fn ports() -> Ports {
    let console = (5554..=5682)
        .step_by(2)
        .find(|port| {
            std::net::TcpListener::bind(("127.0.0.1", *port)).is_ok()
                && std::net::TcpListener::bind(("127.0.0.1", port + 1)).is_ok()
        })
        .unwrap();
    let random = || {
        std::net::TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    };
    let adb = random();
    let mut grpc = random();
    while grpc == adb {
        grpc = random();
    }
    Ports { adb, console, grpc }
}
fn plan(root: &Path, ports: Ports, mode: &str) -> Plan {
    let canonical = root.canonicalize().unwrap();
    let root = canonical.as_path();
    let sdk = root.join("sdk");
    let avd = root.join("installation/user/data/android/avd");
    fs::create_dir_all(&avd).unwrap();
    let platform = fixture_platform();
    let image = sdk.join(format!(
        "system-images/android-35/default/{}",
        platform.spec().abi
    ));
    fs::create_dir_all(&image).unwrap();
    fs::write(image.join("system.img"), b"public factory fixture").unwrap();
    let program = root.join(if cfg!(windows) {
        "fixture.exe"
    } else {
        "fixture"
    });
    let built = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("--edition=2024")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native_sdk.rs"))
        .arg("-o")
        .arg(&program)
        .output()
        .unwrap();
    assert!(built.status.success(), "fixture compiler: {built:?}");
    for (role, name) in [("emulator", "emulator"), ("platform-tools", "adb")] {
        let folder = sdk.join(role);
        fs::create_dir_all(&folder).unwrap();
        fs::copy(
            &program,
            folder.join(format!("{name}{}", platform.spec().suffix)),
        )
        .unwrap();
    }
    let resources = super::super::Resources {
        cpus: 1,
        memory_mb: 1536,
        storage_gb: 16,
    };
    super::super::device::create(
        &root.join("installation"),
        &sdk,
        platform.spec().abi,
        resources,
        &Control {
            available: &|_| u64::MAX,
            cancelled: &|| false,
        },
    )
    .unwrap();
    // The fake SDK never allocates a real 16GiB image. Model its existing disk;
    // factory allocation and low-space admission have separate creation tests.
    fs::write(
        avd.join("phone-local-01.avd/userdata-qemu.img"),
        b"fixture disk",
    )
    .unwrap();
    // This is a portable native-process fixture, not a host-matched Android boot.
    let prepared = super::super::Prepared {
        sdk_root: sdk,
        avd_home: avd,
        reused_sdk: false,
        reused_device: false,
    };
    let plan = Plan::from_prepared_for(prepared, ports, platform).unwrap();
    fs::write(plan.home.join("mode"), mode).unwrap();
    plan
}
async fn start(plan: Plan, stop: &mut watch::Receiver<bool>) -> Result<Started> {
    let log = plan.home.join("sdk.log");
    match tokio::time::timeout(Duration::from_secs(15), process::start(plan, stop)).await {
        Ok(result) => result,
        Err(error) => panic!(
            "fixture startup must settle: {error}; SDK log: {}",
            fs::read_to_string(log).unwrap_or_default()
        ),
    }
}
fn assert_free(ports: Ports) {
    let reserved = ports.reserve().expect("all owned listeners must be gone");
    drop(reserved);
}

#[tokio::test]
async fn native_children_use_applied_configuration_and_shutdown_releases_every_port() {
    let _serial = SDK_TEST.lock().await;
    let temp = tempfile::tempdir().unwrap();
    let ports = ports();
    let plan = plan(temp.path(), ports, "");
    let home = plan.home.clone();
    let (end, mut stop) = watch::channel(false);
    let Started::Owned(mut phone) = start(plan, &mut stop).await.unwrap() else {
        panic!("first start must own its processes")
    };
    let receipt = phone.receipt.as_ref().unwrap();
    assert_eq!(receipt.version, 1);
    assert_eq!(receipt.serial, format!("emulator-{}", ports.console));
    assert!(
        receipt.root_verified && receipt.width > 0 && receipt.height > 0 && !receipt.reused_session
    );
    let args = fs::read_to_string(home.join("emulator.args")).unwrap();
    assert!(args.contains("-memory\n1536\n-cores\n1\n"));
    assert!(!args.contains("-wipe-data"));
    assert!(
        !home.join("root.requests").exists(),
        "already-root phone must not restart adbd"
    );
    assert_eq!(
        fs::read_to_string(home.join("env-ANDROID_AVD_HOME")).unwrap(),
        super::super::device::text(
            &temp
                .path()
                .join("installation/user/data/android/avd")
                .canonicalize()
                .unwrap()
        )
        .unwrap()
    );
    end.send(true).unwrap();
    phone.wait(&mut stop).await.unwrap();
    phone.shutdown().await.unwrap();
    drop(phone);
    assert_free(ports);
    assert!(!home.join("session.pb").exists());
}

#[tokio::test]
async fn unknown_root_is_observed_without_replaying_and_running_session_is_reused() {
    let _serial = SDK_TEST.lock().await;
    let temp = tempfile::tempdir().unwrap();
    let ports = ports();
    let plan = plan(temp.path(), ports, "unknown-root");
    let home = plan.home.clone();
    let (_end, mut stop) = watch::channel(false);
    let Started::Owned(mut phone) = start(plan, &mut stop).await.unwrap() else {
        panic!("owned")
    };
    assert_eq!(
        fs::read_to_string(home.join("root.requests")).unwrap(),
        "root\n"
    );
    let phone_config = super::super::Prepared {
        sdk_root: temp.path().join("sdk"),
        avd_home: temp.path().join("installation/user/data/android/avd"),
        reused_sdk: true,
        reused_device: true,
    };
    let same = Plan::from_prepared_for(phone_config, ports, fixture_platform()).unwrap();
    let Started::Existing(receipt) = start(same, &mut stop).await.unwrap() else {
        panic!("must reuse existing processes")
    };
    assert!(receipt.reused_session);
    assert_eq!(
        fs::read_to_string(home.join("root.requests")).unwrap(),
        "root\n"
    );
    phone.shutdown().await.unwrap();
    drop(phone);
    assert_free(ports);
}

#[tokio::test]
async fn cancellation_during_boot_and_root_refusal_clean_up_owned_processes() {
    let _serial = SDK_TEST.lock().await;
    for mode in ["never-boot", "denied-root"] {
        let temp = tempfile::tempdir().unwrap();
        let ports = ports();
        let plan = plan(temp.path(), ports, mode);
        let home = plan.home.clone();
        let (end, mut stop) = watch::channel(false);
        let _keep = end.clone();
        let cancel = tokio::spawn(async move {
            if mode == "never-boot" {
                while !home.join("emulator.live").exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                end.send(true).unwrap();
            }
        });
        let error = start(plan, &mut stop)
            .await
            .err()
            .expect("must fail without a readiness receipt");
        if mode == "never-boot" {
            assert!(matches!(error, Error::Cancelled));
        } else {
            assert_eq!(error.code(), "android_root_unavailable");
        }
        cancel.await.unwrap();
        assert_free(ports);
    }
}

#[tokio::test]
async fn server_exit_is_reported_and_shutdown_still_stops_the_emulator() {
    let _serial = SDK_TEST.lock().await;
    let temp = tempfile::tempdir().unwrap();
    let ports = ports();
    let plan = plan(temp.path(), ports, "");
    let home = plan.home.clone();
    let (_end, mut stop) = watch::channel(false);
    let Started::Owned(mut phone) = start(plan, &mut stop).await.unwrap() else {
        panic!("owned")
    };
    fs::write(home.join("adb.exit"), b"exit").unwrap();
    let error = tokio::time::timeout(Duration::from_secs(3), phone.wait(&mut stop))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code(), "android_adb_server_exited");
    phone.shutdown().await.unwrap();
    phone.shutdown().await.unwrap();
    drop(phone);
    assert_free(ports);
    assert!(!home.join("session.pb").exists());
}

#[tokio::test]
async fn occupied_ports_do_not_start_processes_or_overwrite_a_session() {
    let _serial = SDK_TEST.lock().await;
    let temp = tempfile::tempdir().unwrap();
    let ports = ports();
    let plan = plan(temp.path(), ports, "");
    let home = plan.home.clone();
    let listener = std::net::TcpListener::bind(("127.0.0.1", ports.grpc)).unwrap();
    let (_end, mut stop) = watch::channel(false);
    let error = start(plan, &mut stop)
        .await
        .err()
        .expect("must reject the busy port");
    assert_eq!(error.code(), "android_session_port_busy");
    assert!(!home.join("session.pb").exists());
    assert!(!home.join("adb.pid").exists());
    assert!(!home.join("emulator.live").exists());
    drop(listener);
    assert_free(ports);
}

#[tokio::test]
async fn shutdown_waits_for_the_owned_console_before_sending_one_stop() {
    let _serial = SDK_TEST.lock().await;
    let temp = tempfile::tempdir().unwrap();
    let ports = ports();
    let plan = plan(temp.path(), ports, "slow-console");
    let (_end, mut stop) = watch::channel(false);
    let Started::Owned(mut phone) = start(plan, &mut stop).await.unwrap() else {
        panic!("owned")
    };
    phone.shutdown().await.unwrap();
    drop(phone);
    assert_free(ports);
}

#[test]
fn ports_and_png_evidence_reject_ambiguous_or_invalid_inputs() {
    use ai_phone_api::wire::NativeAndroidStart;
    for (adb, console, grpc) in [
        (80, 5564, 8558),
        (15037, 5565, 8558),
        (15037, 65536, 8558),
        (5564, 5564, 8558),
        (15037, 5564, 5565),
    ] {
        assert!(
            Ports::parse(&NativeAndroidStart {
                adb_port: adb,
                console_port: console,
                grpc_port: grpc,
                phone: None
            })
            .is_err()
        );
    }
    assert_eq!(
        Ports::parse(&NativeAndroidStart::default()).unwrap(),
        Ports {
            adb: 15037,
            console: 5564,
            grpc: 8558
        }
    );
    for data in [vec![], b"not an image".to_vec(), vec![0; 33]] {
        assert!(readiness::frame(&data).is_err());
    }
    let png = include_bytes!("../../../../phone-runner/fixtures/ocr-probe.png");
    assert!(readiness::frame(png).is_ok());
    for (offset, value) in [(16, 0_u32), (20, 8193)] {
        let mut invalid = png.to_vec();
        invalid[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        assert!(readiness::frame(&invalid).is_err());
    }
}
