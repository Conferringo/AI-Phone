//! Windows ships the x86_64 headless engine used by the embedded device surface.
pub fn select(files: Vec<String>) -> Vec<String> {
    files
        .into_iter()
        .filter(|name| {
            !name.starts_with("sdk/emulator/lib64/qt/")
                && (!name.starts_with("sdk/emulator/qemu/windows-x86_64/qemu-system-")
                    || name == "sdk/emulator/qemu/windows-x86_64/qemu-system-x86_64-headless.exe")
        })
        .collect()
}
