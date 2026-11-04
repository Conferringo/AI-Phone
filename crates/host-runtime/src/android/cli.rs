use super::Resources;
use crate::{Error, Installation, Result};
use ai_phone_api::wire::{NativeAndroidPrepare, NativeAndroidPrepared};
use prost::Message;
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::Path,
};

pub const CONTRACT: &str = "ai-phone-native-android-v1";
fn read_request() -> Result<NativeAndroidPrepare> {
    let mut bytes = Vec::new();
    std::io::stdin().take(8193).read_to_end(&mut bytes)?;
    if bytes.len() > 8192 {
        return Err(Error::Invalid("android_prepare_input"));
    }
    let request = NativeAndroidPrepare::decode(bytes.as_slice())
        .map_err(|_| Error::Invalid("android_prepare_input"))?;
    if request.version != 1 {
        return Err(Error::Invalid("android_prepare_input"));
    }
    Ok(request)
}
fn receipt(prepared: super::Prepared) -> Result<NativeAndroidPrepared> {
    Ok(NativeAndroidPrepared {
        version: 1,
        sdk_root: prepared
            .sdk_root
            .to_str()
            .ok_or(Error::Invalid("android_device_path"))?
            .into(),
        avd_home: prepared
            .avd_home
            .to_str()
            .ok_or(Error::Invalid("android_device_path"))?
            .into(),
        avd_name: super::AVD_NAME.into(),
        reused_sdk: prepared.reused_sdk,
        reused_device: prepared.reused_device,
    })
}
pub fn run(arguments: &[OsString]) -> Result<()> {
    let [home] = arguments else {
        return Err(Error::Invalid("android_prepare_arguments"));
    };
    let request = read_request()?;
    let resources = Resources {
        cpus: request.cpus,
        memory_mb: request.memory_mb,
        storage_gb: request.storage_gb,
    };
    resources.validate()?;
    let prepared = Installation::lock(Path::new(home))?.prepare_android(
        Path::new(&request.payload_path),
        &request.payload_sha256,
        resources,
        &|path| fs2::available_space(path).unwrap_or(0),
        &|| false,
    )?;
    let receipt = receipt(prepared)?;
    std::io::stdout().write_all(&receipt.encode_to_vec())?;
    Ok(())
}
