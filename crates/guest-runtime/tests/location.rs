//! CLI receipts retain the existing protobuf contract without starting a browser.
use ai_phone_api::wire::{LocationDevice, LocationFix};
use prost::Message;
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn invalid_location_input_returns_the_protobuf_receipt_before_connecting() {
    for bytes in [
        LocationFix {
            enabled: true,
            ..Default::default()
        }
        .encode_to_vec(),
        vec![0; 4097],
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_desktop-bridge"))
            .arg("location")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&bytes).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "CLI failure: {:?}", output.stdout);
        let receipt = LocationDevice::decode(output.stdout.as_slice()).unwrap();
        assert_eq!(receipt.kind, "desktop");
        assert_eq!(receipt.status, "failed");
        assert_eq!(receipt.error, "位置坐标无效");
    }
}
