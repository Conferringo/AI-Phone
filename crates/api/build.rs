fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = "../../packages/protocol/proto";
    let files = [
        format!("{root}/ai_phone/v1/system.proto"),
        format!("{root}/ai_phone/v1/location.proto"),
        format!("{root}/ai_phone/v1/workspace.proto"),
        format!("{root}/ai_phone/v1/phone_runtime.proto"),
        format!("{root}/ai_phone/v1/assistant.proto"),
        format!("{root}/ai_phone/v1/auth.proto"),
        format!("{root}/ai_phone/v1/channels.proto"),
        format!("{root}/ai_phone/v1/workbench.proto"),
        format!("{root}/ai_phone/v1/settings.proto"),
        format!("{root}/ai_phone/v1/registry.proto"),
        format!("{root}/ai_phone/v1/desktop.proto"),
        format!("{root}/ai_phone/v1/proactive.proto"),
        format!("{root}/ai_phone/v1/device_network.proto"),
    ];
    for file in &files {
        println!("cargo:rerun-if-changed={file}");
    }
    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    config.compile_protos(&files, &[root])?;
    Ok(())
}
