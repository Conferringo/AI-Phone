use super::Engine;
use crate::{
    Error, Result, archive,
    manifest::{Record, canonical, identity},
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub const NAME: &str = "ANDROID-MANIFEST.json";
pub const LIMITS: archive::Limits = archive::Limits {
    manifest: NAME,
    file: 4 * 1024_u64.pow(3),
    total: 8 * 1024_u64.pow(3) + 8 * 1024 * 1024,
    metadata: 8 * 1024 * 1024,
};

#[derive(Clone, Copy)]
pub(super) enum Platform {
    MacArm64,
    LinuxX64,
    WindowsX64,
}
impl Platform {
    pub fn current() -> Result<Self> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => Ok(Self::MacArm64),
            ("linux", "x86_64") => Ok(Self::LinuxX64),
            ("windows", "x86_64") => Ok(Self::WindowsX64),
            _ => Err(Error::Invalid("android_host_unsupported")),
        }
    }
    pub fn spec(self) -> Specification {
        match self {
            Self::MacArm64 => Specification {
                host: "macos-arm64",
                abi: "arm64-v8a",
                suffix: "",
            },
            Self::LinuxX64 => Specification {
                host: "linux-x64",
                abi: "x86_64",
                suffix: "",
            },
            Self::WindowsX64 => Specification {
                host: "windows-x64",
                abi: "x86_64",
                suffix: ".exe",
            },
        }
    }
}

pub(super) struct Specification {
    pub host: &'static str,
    pub abi: &'static str,
    pub suffix: &'static str,
}

#[derive(Deserialize)]
pub(super) struct Manifest {
    format: u32,
    host: String,
    api: u32,
    abi: String,
    #[serde(default)]
    engine: Engine,
    pub files: Vec<Record>,
}

fn windows_name(part: &str) -> Result<()> {
    let stem = part.split('.').next().unwrap_or("").to_lowercase();
    let reserved = ["con", "prn", "aux", "nul"].contains(&stem.as_str())
        || ["com", "lpt"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|v| {
                ["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"].contains(&v)
            })
        });
    if reserved || part.ends_with([' ', '.']) || part.contains(['<', '>', ':', '"', '|', '?', '*'])
    {
        return Err(Error::Invalid("android_resource_path"));
    }
    Ok(())
}
fn name(value: &str, platform: Platform, engine: Engine) -> Result<()> {
    canonical(value)?;
    let prefix = format!(
        "sdk/system-images/android-35/default/{}/",
        platform.spec().abi
    );
    let engine_path = match engine {
        Engine::Goldfish => value.starts_with("sdk/emulator/") || value.starts_with(&prefix),
        Engine::Avo => value.starts_with("sdk/avo/"),
        Engine::Whpx => value.starts_with("sdk/cuttlefish/"),
    };
    if !value.starts_with("sdk/platform-tools/") && !engine_path {
        return Err(Error::Invalid("android_resource_path"));
    }
    for part in value.split('/') {
        private_name(part)?;
        if matches!(platform, Platform::WindowsX64) {
            windows_name(part)?;
        }
    }
    Ok(())
}
fn private_name(part: &str) -> Result<()> {
    if [
        ".env",
        "snapshots",
        "userdata-qemu.img",
        "userdata-qemu.img.qcow2",
    ]
    .contains(&part)
        || part.starts_with("._")
    {
        return Err(Error::Invalid("android_resource_private_path"));
    }
    Ok(())
}
fn record(row: &Record, platform: Platform, engine: Engine) -> Result<String> {
    name(&row.path, platform, engine)?;
    if !identity(&row.sha256) || row.size > LIMITS.file || ![0o644, 0o755].contains(&row.mode) {
        return Err(Error::Invalid("android_resource_inventory"));
    }
    Ok(if matches!(platform, Platform::WindowsX64) {
        row.path.to_lowercase()
    } else {
        row.path.clone()
    })
}
fn records(value: &Manifest, platform: Platform) -> Result<()> {
    if value.files.is_empty() || value.files.len() >= crate::manifest::MAX_FILES {
        return Err(Error::Invalid("android_resource_inventory"));
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for row in &value.files {
        if !names.insert(record(row, platform, value.engine)?) {
            return Err(Error::Invalid("android_resource_inventory"));
        }
        total = total
            .checked_add(row.size)
            .ok_or(Error::Invalid("android_resource_size"))?;
    }
    if total > 8 * 1024_u64.pow(3) {
        return Err(Error::Invalid("android_resource_size"));
    }
    programs(value, platform)
}
fn programs(value: &Manifest, platform: Platform) -> Result<()> {
    let index: BTreeMap<_, _> = value
        .files
        .iter()
        .map(|row| (row.path.as_str(), row))
        .collect();
    let executable = match value.engine {
        Engine::Goldfish => format!("sdk/emulator/emulator{}", platform.spec().suffix),
        Engine::Avo => "sdk/avo/avo-vmm".into(),
        Engine::Whpx => "sdk/cuttlefish/qemu-system-x86_64.exe".into(),
    };
    for path in [
        executable,
        format!("sdk/platform-tools/adb{}", platform.spec().suffix),
    ] {
        if index.get(path.as_str()).is_none_or(|r| r.mode != 0o755) {
            return Err(Error::Invalid("android_resource_programs"));
        }
    }
    let images = value.engine.sdk_images(platform.spec().abi);
    let required: &[&str] = match value.engine {
        Engine::Goldfish => &[
            "system.img",
            "vendor.img",
            "kernel-ranchu",
            "ramdisk.img",
            "NOTICE.txt",
        ],
        Engine::Avo => &[
            "boot.img",
            "init_boot.img",
            "vendor_boot.img",
            "super.img",
            "vbmeta.img",
            "userdata.img",
        ],
        Engine::Whpx => &[
            "aimanity-audio.jar",
            "bzImage",
            "initrd.img",
            "disk.qcow2",
            "userdata-16.qcow2",
            "userdata-24.qcow2",
            "userdata-32.qcow2",
            "qemu-img.exe",
            "share/bios-256k.bin",
            "share/linuxboot_dma.bin",
        ],
    };
    for part in required {
        let path = format!("sdk/{}/{part}", images.to_string_lossy().replace('\\', "/"));
        if !index.contains_key(path.as_str()) {
            return Err(Error::Invalid("android_resource_image"));
        }
    }
    Ok(())
}
pub(super) fn parse(raw: &[u8], platform: Platform) -> Result<Manifest> {
    let value: Manifest =
        serde_json::from_slice(raw).map_err(|_| Error::Invalid("android_resource_manifest"))?;
    if value.format != 1
        || value.host != platform.spec().host
        || value.api != value.engine.api()
        || value.abi != platform.spec().abi
        || !matches!(
            (value.engine, platform),
            (Engine::Goldfish, _)
                | (Engine::Avo, Platform::MacArm64)
                | (Engine::Whpx, Platform::WindowsX64)
        )
    {
        return Err(Error::Invalid("android_resource_manifest"));
    }
    records(&value, platform)?;
    Ok(value)
}
