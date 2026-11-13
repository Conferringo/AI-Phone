use std::{
    collections::BTreeMap,
    ffi::OsStr,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn command(program: PathBuf) -> Command {
    let mut value = Command::new(program);
    value
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    value
}

fn execute(name: &str, command: &mut Command) -> Result<bool, String> {
    let code = command
        .status()
        .map_err(|error| format!("{name}: {error}"))?
        .code();
    match code {
        Some(0) => Ok(false),
        Some(3010) => Ok(true),
        _ => Err(format!("{name}: exit {code:?}")),
    }
}

fn resource(
    folder: &Path,
    integrity: &BTreeMap<String, String>,
    name: &str,
) -> Result<PathBuf, String> {
    let file = folder.join(name);
    let expected = integrity
        .get(&format!("windows/{name}"))
        .ok_or("windows_runtime_missing")?;
    if super::resources::hash(&file)? != *expected {
        return Err("windows_runtime_changed".into());
    }
    Ok(file)
}

pub(crate) fn prepare() -> Result<i32, String> {
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    let folder = program
        .parent()
        .ok_or("windows_resource_path")?
        .join("host");
    let integrity = super::INTEGRITY
        .as_ref()
        .map_err(|error| error.to_string())?;
    let system = PathBuf::from(std::env::var_os("SystemRoot").ok_or("windows_system_path")?)
        .join("System32");
    if integrity.contains_key("qemu/manifest.json") {
        let manifest = folder.join("qemu/manifest.json");
        if super::resources::hash(&manifest)? != integrity["qemu/manifest.json"] {
            return Err("windows_runtime_changed".into());
        }
        let reboot = execute(
            "Windows Hypervisor Platform",
            command(system.join("dism.exe")).args([
                "/Online",
                "/Enable-Feature",
                "/FeatureName:HypervisorPlatform",
                "/All",
                "/NoRestart",
                "/Quiet",
            ]),
        )?;
        return Ok(if reboot { 3010 } else { 0 });
    }
    let mut reboot = execute(
        "Windows virtualization",
        command(system.join("dism.exe")).args([
            "/Online",
            "/Enable-Feature",
            "/FeatureName:VirtualMachinePlatform",
            "/FeatureName:HypervisorPlatform",
            "/All",
            "/NoRestart",
            "/Quiet",
        ]),
    )?;
    let mut version = command(system.join("wsl.exe"));
    version.arg("--version");
    let installed = match version.status() {
        Ok(status) => status.success(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(format!("WSL: {error}")),
    };
    if !installed {
        let resource = resource(&folder.join("windows"), integrity, "wsl.msi")?;
        reboot |= execute(
            "WSL installation",
            command(system.join("msiexec.exe"))
                .args([OsStr::new("/i"), resource.as_os_str()])
                .args(["/qn", "/norestart"]),
        )?;
    }
    Ok(if reboot { 3010 } else { 0 })
}
