fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&["current_server", "connect_server"]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("Unable to build client capability manifest");
}
