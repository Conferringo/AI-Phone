//! Host capacity probes. Windows uses caller-quota-aware native APIs.
use std::path::Path;

pub(super) fn available_bytes(path: &Path) -> u64 {
    let mut directory = path;
    while !directory.is_dir() {
        let Some(parent) = directory.parent() else {
            return 0;
        };
        directory = parent;
    }
    disk_bytes(directory)
}

#[cfg(not(target_os = "windows"))]
fn disk_bytes(directory: &Path) -> u64 {
    std::process::Command::new("/bin/df")
        .args(["-kP"])
        .arg(directory)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| {
            text.lines()
                .nth(1)
                .and_then(|line| line.split_whitespace().nth(3))
                .and_then(|value| value.parse::<u64>().ok())
        })
        .and_then(|value| value.checked_mul(1024))
        .unwrap_or(0)
}

#[cfg(target_os = "windows")]
fn disk_bytes(directory: &Path) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut name: Vec<u16> = directory.as_os_str().encode_wide().collect();
    if name.contains(&0) {
        return 0;
    }
    // A trailing separator is required for a UNC share root and valid elsewhere.
    if !name.ends_with(&[b'\\' as u16]) && !name.ends_with(&[b'/' as u16]) {
        name.push(b'\\' as u16);
    }
    name.push(0);
    let mut available = 0u64;
    // SAFETY: name is NUL-terminated UTF-16 and available is writable for the call.
    // Other outputs are optional; use caller-available bytes, respecting disk quotas.
    let result = unsafe {
        GetDiskFreeSpaceExW(
            name.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result == 0 { 0 } else { available }
}

#[cfg(target_os = "windows")]
pub(super) fn total_memory_bytes() -> u64 {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut state = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: the initialized structure has the length required by the Windows API.
    if unsafe { GlobalMemoryStatusEx(&mut state) } == 0 {
        0
    } else {
        state.ullTotalPhys
    }
}

#[cfg(target_os = "macos")]
pub(super) fn total_memory_bytes() -> u64 {
    std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
pub(super) fn total_memory_bytes() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("MemTotal:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse::<u64>().ok())
        })
        .and_then(|value| value.checked_mul(1024))
        .unwrap_or(0)
}

#[cfg(all(
    test,
    any(target_os = "macos", target_os = "linux", target_os = "windows")
))]
mod tests {
    use super::*;

    #[test]
    fn a_future_installation_uses_its_existing_volume() {
        let temporary = std::env::temp_dir();
        let future = temporary
            .join(format!("ai-phone-capacity-{}", std::process::id()))
            .join("新建 installation")
            .join("user");
        assert!(!future.exists());
        assert!(available_bytes(&temporary) > 0);
        assert!(available_bytes(&future) > 0);
        assert!(
            !future.exists(),
            "capacity probing must not create user state"
        );
    }

    #[test]
    fn physical_memory_is_reported_by_the_running_host() {
        assert!(total_memory_bytes() > 0);
    }

    #[test]
    fn failed_queries_do_not_admit_capacity() {
        assert_eq!(available_bytes(Path::new("")), 0);
        assert_eq!(disk_bytes(Path::new("\0")), 0);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn native_capacity_works_for_unicode_paths_without_posix_tools() {
        let directory = std::env::temp_dir().join(format!(
            "ai-phone-capacity-中文 العربية-{}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let available = available_bytes(&directory);
        std::fs::remove_dir(&directory).unwrap();
        assert!(available > 0, "Windows capacity must not depend on /bin/df");
        assert!(total_memory_bytes() > 0);
    }
}
