//! Exercise the real streaming CLI, including its stdout envelope and failure status.
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    process::{Command, Stdio},
};

fn encode(body: &[u8], architecture: &str, preset: &str) -> std::process::Output {
    let mut input = tempfile::tempfile().unwrap();
    input.write_all(body).unwrap();
    input.seek(SeekFrom::Start(0)).unwrap();
    Command::new(env!("CARGO_BIN_EXE_release-tools"))
        .args(["encode-xz", architecture, preset])
        .arg(std::env::temp_dir())
        .stdin(Stdio::from(input))
        .output()
        .unwrap()
}

#[test]
fn native_encoder_is_deterministic_and_readable_with_the_existing_memory_budget() {
    let body = fs::read(env!("CARGO_BIN_EXE_release-tools")).unwrap();
    for architecture in ["amd64", "arm64"] {
        let first = encode(&body, architecture, "9");
        assert!(
            first.status.success(),
            "{}",
            String::from_utf8_lossy(&first.stderr)
        );
        assert!(first.stderr.is_empty());
        assert_eq!(first.stdout, encode(&body, architecture, "9").stdout);
        let stream = xz2::stream::Stream::new_stream_decoder(128 * 1024 * 1024, 0).unwrap();
        let mut decoded = Vec::new();
        xz2::read::XzDecoder::new_stream(first.stdout.as_slice(), stream)
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, body);
        assert_eq!(&first.stdout[first.stdout.len() - 2..], b"YZ");
    }
}

#[test]
fn unsupported_architecture_and_extra_arguments_emit_no_partial_capsule() {
    let output = encode(b"preserved input", "unknown", "9");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let output = Command::new(env!("CARGO_BIN_EXE_release-tools"))
        .args(["encode-xz", "amd64", "9", "/tmp", "extra"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn existing_android_presets_decode_without_changing_the_envelope() {
    let body: Vec<u8> = (0..251).cycle().take(50_000).collect();
    for architecture in ["amd64", "arm64"] {
        for preset in ["3", "6", "9"] {
            let output = encode(&body, architecture, preset);
            assert!(output.status.success());
            let stream = xz2::stream::Stream::new_stream_decoder(128 * 1024 * 1024, 0).unwrap();
            let mut decoded = Vec::new();
            xz2::read::XzDecoder::new_stream(output.stdout.as_slice(), stream)
                .read_to_end(&mut decoded)
                .unwrap();
            assert_eq!(decoded, body);
        }
    }
}

#[test]
fn windows_sdk_selection_preserves_the_actual_headless_device_and_runtime_libraries() {
    let retained = [
        "sdk/emulator/emulator.exe",
        "sdk/emulator/qemu/windows-x86_64/qemu-system-x86_64-headless.exe",
        "sdk/emulator/lib64/libandroid-emu-agents.dll",
        "sdk/emulator/lib64/gles_swiftshader/libEGL.dll",
        "sdk/emulator/qemu-img.exe",
        "sdk/emulator/NOTICE.txt",
        "sdk/platform-tools/adb.exe",
    ];
    let mut files = retained.to_vec();
    files.extend([
        "sdk/emulator/qemu/windows-x86_64/qemu-system-x86_64.exe",
        "sdk/emulator/qemu/windows-x86_64/qemu-system-armel-headless.exe",
        "sdk/emulator/lib64/qt/lib/Qt6WebEngineCoreAndroidEmu.dll",
    ]);
    let mut input = tempfile::tempfile().unwrap();
    serde_json::to_writer(&input, &files).unwrap();
    input.seek(SeekFrom::Start(0)).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_release-tools"))
        .arg("windows-sdk-files")
        .stdin(Stdio::from(input))
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        serde_json::from_slice::<Vec<String>>(&result.stdout).unwrap(),
        retained
    );
}
