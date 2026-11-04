//! Exercise the same native zoom entrypoint used by computer-use, without X11.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn zoom(region: Value) -> std::process::Output {
    let mut request = region;
    request["png"] =
        json!(STANDARD.encode(include_bytes!("../../phone-runner/fixtures/ocr-probe.png")));
    let mut child = Command::new(env!("CARGO_BIN_EXE_desktop-bridge"))
        .arg("zoom")
        .env_remove("DISPLAY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn valid_small_text_and_edge_pixels_can_be_inspected() {
    for (region, expected) in [
        (json!({"x":10,"y":5,"width":60,"height":18}), (120, 36)),
        (json!({"x":319,"y":111,"width":1,"height":1}), (2, 2)),
    ] {
        let output = zoom(region);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["success"], true);
        let bytes = STANDARD.decode(value["png"].as_str().unwrap()).unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!((image.width(), image.height()), expected);
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn empty_negative_overflow_and_outside_regions_never_produce_an_image() {
    for region in [
        json!({"x":0,"y":0,"width":0,"height":18}),
        json!({"x":0,"y":0,"width":60,"height":0}),
        json!({"x":0,"y":0,"width":60,"height":-1}),
        json!({"x":-1,"y":0,"width":60,"height":18}),
        json!({"x":319,"y":111,"width":2,"height":1}),
        json!({"x":i64::MAX,"y":0,"width":2,"height":1}),
    ] {
        let invalid_extent =
            region["width"].as_i64().unwrap() <= 0 || region["height"].as_i64().unwrap() <= 0;
        let output = zoom(region);
        assert!(!output.status.success());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["success"], false);
        assert!(value.get("png").is_none());
        if invalid_extent {
            assert!(
                value["error"]
                    .as_str()
                    .unwrap()
                    .contains("宽高必须为正整数像素"),
                "invalid dimensions need an actionable error, not an encoder failure: {value}"
            );
        }
    }
}
