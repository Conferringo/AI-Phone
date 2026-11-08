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
        .join("host/windows");
    let integrity: BTreeMap<String, String> =
        serde_json::from_str(super::INTEGRITY).map_err(|error| error.to_string())?;
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
        let resource = resource(&folder, &integrity, "wsl.msi")?;
        reboot |= execute(
            "WSL installation",
            command(system.join("msiexec.exe"))
                .args([OsStr::new("/i"), resource.as_os_str()])
                .args(["/qn", "/norestart"]),
        )?;
    }
    Ok(if reboot { 3010 } else { 0 })
}

#[derive(serde::Deserialize)]
pub(super) struct VmManifest {
    format: u8,
    runtime: String,
    files: Vec<VmFile>,
}
#[derive(serde::Deserialize)]
struct VmFile {
    path: String,
    size: u64,
    sha256: String,
}

impl VmManifest {
    pub(super) fn base_identity(folder: &Path) -> Result<String, String> {
        let manifest: Self = serde_json::from_reader(
            std::fs::File::open(folder.join("manifest.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|_| "vm_bundle_invalid")?;
        if manifest.format != 1 || manifest.runtime != "qemu-whpx-backend" {
            return Err("vm_bundle_invalid".into());
        }
        let base = manifest
            .files
            .iter()
            .find(|file| file.path == "guest/guest.qcow2")
            .ok_or("vm_bundle_invalid")?;
        if std::fs::metadata(folder.join(&base.path))
            .map_err(|e| e.to_string())?
            .len()
            != base.size
        {
            return Err("vm_bundle_changed".into());
        }
        Ok(base.sha256.clone())
    }
}
