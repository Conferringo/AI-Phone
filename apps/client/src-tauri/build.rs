fn main() {
    println!("cargo:rerun-if-changed=gen/host/integrity.json");
    assert!(
        std::path::Path::new("gen/host/integrity.json").is_file(),
        "Prepare client resources with scripts/bun scripts/prepare-client.ts before building"
    );
    let manifest = tauri_build::AppManifest::new().commands(&[
        "fit_setup",
        "current_server",
        "connect_server",
        "deployment_status",
        "start_deployment",
        "cancel_deployment",
        "accept_deployment_license",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("Unable to build client capability manifest");
}
