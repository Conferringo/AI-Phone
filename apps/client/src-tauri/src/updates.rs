use semver::Version;
use serde::Deserialize;
use std::{io, sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager, Wry, menu::MenuItem};

pub const ID: &str = "client-updates";
pub const CHECK: &str = "Check for Updates…";
const REPOSITORY: &str = "MrMilkDragon/AI-Phone";

pub fn platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("macos-arm64.dmg"),
        ("windows", "x86_64") => Some("windows-x64-setup.exe"),
        _ => None,
    }
}

pub struct Menu {
    item: MenuItem<Wry>,
    offered: Mutex<Option<Version>>,
}

impl Menu {
    pub fn new(item: MenuItem<Wry>) -> Self {
        Self {
            item,
            offered: Mutex::new(None),
        }
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    state: String,
}

fn installer(version: &Version, platform: &str) -> String {
    format!("AI-Phone-{version}-{platform}")
}

fn available(releases: Vec<Release>, current: &Version, platform: &str) -> Option<Version> {
    releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| {
            let version = Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
            if !version.pre.is_empty() || !version.build.is_empty() || &version <= current {
                return None;
            }
            let name = installer(&version, platform);
            release
                .assets
                .iter()
                .any(|asset| asset.name == name && asset.state == "uploaded")
                .then_some(version)
        })
        .max()
}

async fn check(current: &Version, platform: &str) -> Result<Option<Version>, String> {
    let response = reqwest::Client::builder()
        .user_agent(concat!("AI-Phone/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|error| error.to_string())?
        .get(format!(
            "https://api.github.com/repos/{REPOSITORY}/releases?per_page=20"
        ))
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.to_string())?;
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    let releases = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    Ok(available(releases, current, platform))
}

pub fn select(app: &AppHandle) {
    let Some(platform) = platform() else {
        return;
    };
    let menu = app.state::<Menu>();
    let offered = menu.offered.lock().expect("update menu").take();
    if let Some(version) = offered {
        let url = format!(
            "https://github.com/{REPOSITORY}/releases/download/v{version}/{}",
            installer(&version, platform)
        );
        let label = if open(&url).is_ok() {
            CHECK
        } else {
            "Could Not Open Download · Retry"
        };
        let _ = menu.item.set_text(label);
        return;
    }
    let _ = menu.item.set_enabled(false);
    let _ = menu.item.set_text("Checking for Updates…");
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = check(&app.package_info().version, platform).await;
        let menu = app.state::<Menu>();
        let label = match result {
            Ok(Some(version)) => {
                let label = format!("Download Update {version}…");
                *menu.offered.lock().expect("update menu") = Some(version);
                label
            }
            Ok(None) => "Up to Date · Check Again".into(),
            Err(_) => "Could Not Check for Updates · Retry".into(),
        };
        let _ = menu.item.set_text(label);
        let _ = menu.item.set_enabled(true);
    });
}

#[cfg(target_os = "macos")]
fn open(url: &str) -> io::Result<()> {
    let mut child = std::process::Command::new("/usr/bin/open")
        .arg(url)
        .spawn()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(windows)]
fn open(url: &str) -> io::Result<()> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let url: Vec<u16> = url.encode_utf16().chain([0]).collect();
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            null(),
            url.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err(io::Error::other("Unable to open the installer download"))
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn open(_: &str) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_lookup_uses_the_newest_downloadable_stable_platform() {
        let releases = r#"[
            {"tag_name":"v0.1.5","draft":false,"prerelease":true,"assets":[{"name":"AI-Phone-0.1.5-windows-x64-setup.exe","state":"uploaded"}]},
            {"tag_name":"v0.1.4","draft":false,"prerelease":false,"assets":[{"name":"AI-Phone-0.1.4-macos-arm64.dmg","state":"uploaded"}]},
            {"tag_name":"v0.1.3","draft":false,"prerelease":false,"assets":[{"name":"AI-Phone-0.1.3-windows-x64-setup.exe","state":"uploaded"}]}
        ]"#;
        let current = Version::new(0, 1, 2);
        assert_eq!(
            available(
                serde_json::from_str(releases).unwrap(),
                &current,
                "windows-x64-setup.exe"
            ),
            Some(Version::new(0, 1, 3))
        );
        assert_eq!(
            available(
                serde_json::from_str(releases).unwrap(),
                &current,
                "macos-arm64.dmg"
            ),
            Some(Version::new(0, 1, 4))
        );
    }
}
