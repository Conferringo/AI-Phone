//! Device-setup decisions without transport, filesystem, or runtime dependencies.
pub const CONTRACT: &str = ai_phone_host_runtime::wsl::DEVICE_SETUP_CONTRACT;

pub fn resources(cpus: u32, memory_mb: u32) -> Result<(), &'static str> {
    if [1, 2, 4].contains(&cpus) && [2048, 2560, 3072, 4096].contains(&memory_mb) {
        Ok(())
    } else {
        Err("device_setup_resources")
    }
}

pub fn phone_frame(
    id: &str,
    root: &str,
    width: u32,
    height: u32,
    bytes: &[u8],
) -> Result<(), &'static str> {
    if id == "phone-local-01"
        && root == "0"
        && width > 0
        && height > 0
        && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
    {
        Ok(())
    } else {
        Err("device_setup_phone_not_verified")
    }
}

pub fn desktop_frame(width: u32, height: u32, bytes: &[u8]) -> Result<(), &'static str> {
    if width > 0 && height > 0 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok(())
    } else {
        Err("device_setup_desktop_not_verified")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum JobState {
    Waiting,
    Completed,
}
pub fn job_state(value: &str) -> Result<JobState, &'static str> {
    match value {
        "queued" | "running" => Ok(JobState::Waiting),
        "completed" => Ok(JobState::Completed),
        "failed" => Err("device_setup_provision_failed"),
        _ => Err("device_setup_provision_unknown"),
    }
}

pub fn provision_diagnostic(stage: &str, error: &str) -> (&'static str, Vec<&'static str>) {
    let stage = match stage {
        "检查资源" => "check_resources",
        "准备基础镜像" => "prepare_base_image",
        "安装Linux桌面和办公软件" => "install_desktop",
        "启动桌面" => "start_desktop",
        "验证画面与控制" => "verify_frame_control",
        _ => "unknown",
    };
    let error: String = error.chars().take(4096).collect::<String>().to_lowercase();
    let categories: &[(&str, &[&str])] = &[
        (
            "permission",
            &[
                "permission denied",
                "operation not permitted",
                "access is denied",
                "eacces",
                "eperm",
            ],
        ),
        (
            "bind_mount",
            &[
                "invalid mount config",
                "error while mounting",
                "error mounting",
                "mount source",
                "bind source",
                "bind mount",
                "mounts denied",
            ],
        ),
        (
            "disk_budget",
            &[
                "no space left on device",
                "not enough free space",
                "insufficient disk",
                "disk quota exceeded",
                "6gib可用空间",
            ],
        ),
        (
            "image_missing",
            &[
                "bundled desktop image is missing",
                "no such image",
                "image not found",
            ],
        ),
        (
            "runtime",
            &[
                "oci runtime",
                "container runtime",
                "cannot connect to the docker daemon",
                "error during connect",
                "failed to create shim task",
                "unable to start container process",
            ],
        ),
        (
            "frame_missing",
            &["桌面没有返回可验证画面", "desktop did not return a frame"],
        ),
    ];
    let mut flags: Vec<_> = categories
        .iter()
        .filter(|(_, words)| words.iter().any(|word| error.contains(word)))
        .map(|(flag, _)| *flag)
        .collect();
    if flags.is_empty() {
        flags.push("unknown");
    }
    (stage, flags)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_are_fixed_labels_and_never_raw_error_text() {
        let cases = [
            (
                "准备基础镜像",
                "invalid mount config for type bind",
                "prepare_base_image",
                "bind_mount",
            ),
            (
                "检查资源",
                "安装至少需要6GiB可用空间",
                "check_resources",
                "disk_budget",
            ),
            (
                "检查资源",
                "Bundled desktop image is missing",
                "check_resources",
                "image_missing",
            ),
            (
                "启动桌面",
                "OCI runtime create failed",
                "start_desktop",
                "runtime",
            ),
            (
                "验证画面与控制",
                "桌面没有返回可验证画面",
                "verify_frame_control",
                "frame_missing",
            ),
            (
                "__proto__",
                "unrecognized upstream result",
                "unknown",
                "unknown",
            ),
        ];
        for (stage, error, want_stage, want_flag) in cases {
            let (stage, flags) = provision_diagnostic(
                stage,
                &format!("{error} fixture-secret /private/home --private-argv"),
            );
            assert_eq!(stage, want_stage);
            assert_eq!(flags, [want_flag]);
        }
    }
    #[test]
    fn exact_phone_and_root_are_required() {
        let png = b"\x89PNG\r\n\x1a\nfixture";
        assert!(phone_frame("phone-local-01", "0", 1, 1, png).is_ok());
        for (id, root) in [("other", "0"), ("phone-local-01", "2000")] {
            assert!(phone_frame(id, root, 1, 1, png).is_err());
        }
        assert!(phone_frame("phone-local-01", "0", 0, 1, png).is_err());
        assert!(phone_frame("phone-local-01", "0", 1, 1, b"not png").is_err());
    }
    #[test]
    fn desktop_requires_dimensions_and_png() {
        assert!(desktop_frame(1, 1, b"\x89PNG\r\n\x1a\nfixture").is_ok());
        assert!(desktop_frame(0, 1, b"\x89PNG\r\n\x1a\n").is_err());
        assert!(desktop_frame(1, 1, b"anything").is_err());
    }
    #[test]
    fn unknown_never_means_completed() {
        assert_eq!(job_state("completed"), Ok(JobState::Completed));
        assert_eq!(job_state("running"), Ok(JobState::Waiting));
        assert_eq!(job_state("queued"), Ok(JobState::Waiting));
        for value in ["", "unknown", "cancelled", "failed"] {
            assert!(job_state(value).is_err());
        }
    }
    #[test]
    fn resource_presets_match_current_installer() {
        for cpu in [1, 2, 4] {
            for memory in [2048, 2560, 3072, 4096] {
                assert!(resources(cpu, memory).is_ok());
            }
        }
        assert!(resources(8, 2048).is_err());
        assert!(resources(2, 1024).is_err());
    }
}
