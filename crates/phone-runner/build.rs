fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = "../../packages/protocol/vendor/android-emulator";
    let proto = format!("{root}/emulator_controller.proto");
    println!("cargo:rerun-if-changed={proto}");
    let mut config = prost_build::Config::new();
    config.disable_comments(["."]);
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    let includes = [
        std::path::PathBuf::from(root),
        protoc_bin_vendored::include_path()?,
    ];
    let comments = std::fs::read_to_string(&proto)?
        .lines()
        .filter_map(|line| line.trim().strip_prefix("rpc "))
        .filter_map(|line| line.split_once('('))
        .map(|(name, _)| {
            format!(
                "android.emulation.control.EmulatorController.{}",
                name.trim()
            )
        })
        .collect::<Vec<_>>();
    tonic_prost_build::configure()
        .build_server(false)
        .disable_comments(comments)
        .compile_with_config(config, &[std::path::PathBuf::from(proto)], &includes)?;
    Ok(())
}
