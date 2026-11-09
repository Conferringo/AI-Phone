use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::Client;
use ring::signature::{ED25519, UnparsedPublicKey};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{error::Error, ffi::OsString, path::PathBuf, sync::Mutex, time::Duration};
use tauri::{
    AppHandle, Manager, Wry,
    menu::MenuItem,
    window::{ProgressBarState, ProgressBarStatus},
};
use tokio::io::AsyncWriteExt;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

pub const ID: &str = "client-updates";
pub const CHECK: &str = "Check for Updates…";
const REPOSITORY: &str = "MrMilkDragon/AI-Phone";
const MAX_MANIFEST: usize = 16 * 1024;
const MAX_INSTALLER: u64 = 2 * 1024 * 1024 * 1024;
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Copy)]
pub enum Platform {
    Mac,
    Windows,
}

impl Platform {
    fn installer(self, version: &Version) -> String {
        let suffix = match self {
            Self::Mac => "macos-arm64.dmg",
            Self::Windows => "windows-x64-setup.exe",
        };
        format!("AI-Phone-{version}-{suffix}")
    }

    fn artifact(self, manifest: Manifest) -> Artifact {
        match self {
            Self::Mac => manifest.macos_arm64,
            Self::Windows => manifest.windows_x64,
        }
    }
}

pub fn platform() -> Option<Platform> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some(Platform::Mac),
        ("windows", "x86_64") => Some(Platform::Windows),
        _ => None,
    }
}

pub struct Menu {
    item: MenuItem<Wry>,
    offered: Mutex<Option<Offer>>,
}

impl Menu {
    pub fn new(item: MenuItem<Wry>) -> Self {
        Self {
            item,
            offered: Mutex::new(None),
        }
    }
}

pub fn cleanup_current(app: &AppHandle) {
    if let (Some(platform), Ok(cache)) = (platform(), app.path().app_cache_dir()) {
        let name = platform.installer(&app.package_info().version);
        let _ = std::fs::remove_file(cache.join("client-updates").join(name));
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    state: String,
}

#[derive(Deserialize)]
struct SignedManifest {
    manifest: String,
    signature: String,
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    macos_arm64: Artifact,
    windows_x64: Artifact,
}

#[derive(Deserialize)]
struct Artifact {
    size: u64,
    sha256: String,
}

struct Offer {
    version: Version,
    artifact: Artifact,
}

fn manifest_name(version: &Version) -> String {
    format!("AI-Phone-{version}-update.json")
}

fn asset_url(version: &Version, name: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/download/v{version}/{name}")
}

fn available(releases: Vec<Release>, current: &Version, platform: Platform) -> Option<Version> {
    releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| {
            let version = Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
            if !version.pre.is_empty() || !version.build.is_empty() || &version <= current {
                return None;
            }
            let uploaded = |name: &str| {
                release
                    .assets
                    .iter()
                    .any(|asset| asset.name == name && asset.state == "uploaded")
            };
            (uploaded(&platform.installer(&version)) && uploaded(&manifest_name(&version)))
                .then_some(version)
        })
        .max()
}

fn verify_manifest(
    bytes: &[u8],
    key: &[u8],
    version: Version,
    platform: Platform,
) -> Result<Offer> {
    let signed: SignedManifest = serde_json::from_slice(bytes)?;
    let signature = STANDARD.decode(signed.signature)?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(signed.manifest.as_bytes(), &signature)
        .map_err(|_| "Update signature did not verify")?;
    let manifest: Manifest = serde_json::from_str(&signed.manifest)?;
    if manifest.version != version.to_string() {
        return Err("Update version did not match its signature".into());
    }
    let artifact = platform.artifact(manifest);
    if !(1..MAX_INSTALLER).contains(&artifact.size)
        || artifact.sha256.len() != 64
        || !artifact
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("Invalid installer metadata".into());
    }
    Ok(Offer { version, artifact })
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!("AI-Phone/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(12))
        .read_timeout(Duration::from_secs(60))
        .build()?)
}

async fn check(current: &Version, platform: Platform) -> Result<Option<Offer>> {
    let client = client()?;
    let response = client
        .get(format!(
            "https://api.github.com/repos/{REPOSITORY}/releases?per_page=20"
        ))
        .header("accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?;
    let releases = serde_json::from_slice(&response.bytes().await?)?;
    let Some(version) = available(releases, current, platform) else {
        return Ok(None);
    };
    let mut response = client
        .get(asset_url(&version, &manifest_name(&version)))
        .timeout(Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > MAX_MANIFEST {
            return Err("Update manifest exceeded its size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    #[derive(Deserialize)]
    struct Policy {
        public_key: String,
    }
    let policy: Policy =
        serde_json::from_str(include_str!("../../../../config/client-update.json"))?;
    verify_manifest(
        &bytes,
        &STANDARD.decode(policy.public_key)?,
        version,
        platform,
    )
    .map(Some)
}

async fn download(app: &AppHandle, offer: &Offer, platform: Platform) -> Result<PathBuf> {
    let folder = app.path().app_cache_dir()?.join("client-updates");
    std::fs::create_dir_all(&folder)?;
    let name = platform.installer(&offer.version);
    let mut response = client()?
        .get(asset_url(&offer.version, &name))
        .send()
        .await?
        .error_for_status()?;
    let temporary = tempfile::NamedTempFile::new_in(&folder)?;
    let mut file = tokio::fs::File::from_std(temporary.reopen()?);
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut shown = 0;
    while let Some(chunk) = response.chunk().await? {
        size += chunk.len() as u64;
        if size > offer.artifact.size {
            return Err("Installer exceeded the signed size".into());
        }
        hash.update(&chunk);
        file.write_all(&chunk).await?;
        let percent = size * 100 / offer.artifact.size;
        if percent != shown {
            shown = percent;
            progress(app, ProgressBarStatus::Normal, Some(percent));
        }
    }
    if size != offer.artifact.size || format!("{:x}", hash.finalize()) != offer.artifact.sha256 {
        return Err("Installer did not match the signed release".into());
    }
    file.sync_all().await?;
    drop(file);
    let path = folder.join(name);
    temporary.persist(&path)?;
    Ok(path)
}

fn progress(app: &AppHandle, status: ProgressBarStatus, value: Option<u64>) {
    if let Some(window) = app
        .get_webview_window("workbench")
        .or_else(|| app.get_webview_window("connection"))
    {
        let _ = window.set_progress_bar(ProgressBarState {
            status: Some(status),
            progress: value,
        });
    }
}

pub fn select(app: &AppHandle) {
    let Some(platform) = platform() else { return };
    let menu = app.state::<Menu>();
    let _ = menu.item.set_enabled(false);
    let offer = menu.offered.lock().expect("update menu").take();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let menu = app.state::<Menu>();
        let label = if let Some(offer) = offer {
            let _ = menu
                .item
                .set_text(format!("Downloading Update {}…", offer.version));
            match install(&app, &offer, platform).await {
                Ok(()) => {
                    app.exit(0);
                    return;
                }
                Err(error) => {
                    progress(&app, ProgressBarStatus::None, None);
                    eprintln!("Client update: {error}");
                    *menu.offered.lock().expect("update menu") = Some(offer);
                    "Update Interrupted · Retry".to_owned()
                }
            }
        } else {
            let _ = menu.item.set_text("Checking for Updates…");
            match check(&app.package_info().version, platform).await {
                Ok(Some(offer)) => {
                    let label = format!("Install Update {}…", offer.version);
                    *menu.offered.lock().expect("update menu") = Some(offer);
                    label
                }
                Ok(None) => "Up to Date · Check Again".into(),
                Err(error) => {
                    eprintln!("Client update: {error}");
                    "Could Not Check for Updates · Retry".into()
                }
            }
        };
        let _ = menu.item.set_text(label);
        let _ = menu.item.set_enabled(true);
    });
}

async fn install(app: &AppHandle, offer: &Offer, platform: Platform) -> Result<()> {
    if crate::bootstrap::is_running(app) {
        return Err("Wait for device setup to finish".into());
    }
    let downloaded = download(app, offer, platform).await?;
    if crate::bootstrap::is_running(app) {
        return Err("Wait for device setup to finish".into());
    }
    progress(app, ProgressBarStatus::Indeterminate, None);
    let _ = app
        .state::<Menu>()
        .item
        .set_text(format!("Installing Update {}…", offer.version));
    #[cfg(target_os = "macos")]
    macos::launch(app, &downloaded, &offer.version)?;
    #[cfg(windows)]
    windows::launch(app, &downloaded, &offer.version)?;
    Ok(())
}

pub(crate) fn run(args: &[OsString]) -> Option<i32> {
    #[cfg(target_os = "macos")]
    return macos::run(args);
    #[cfg(windows)]
    return windows::run(args);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = args;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::{
        rand::SystemRandom,
        signature::{Ed25519KeyPair, KeyPair},
    };

    #[test]
    fn accepts_only_a_newer_platform_installer_bound_to_a_signed_manifest() {
        let current = Version::new(0, 1, 3);
        let releases = r#"[
            {"tag_name":"v0.1.5","draft":false,"prerelease":false,"assets":[{"name":"AI-Phone-0.1.5-windows-x64-setup.exe","state":"uploaded"}]},
            {"tag_name":"v0.1.4","draft":false,"prerelease":false,"assets":[{"name":"AI-Phone-0.1.4-windows-x64-setup.exe","state":"uploaded"},{"name":"AI-Phone-0.1.4-update.json","state":"uploaded"}]}
        ]"#;
        let version = available(
            serde_json::from_str(releases).unwrap(),
            &current,
            Platform::Windows,
        )
        .unwrap();
        assert_eq!(version, Version::new(0, 1, 4));
        assert!(
            available(
                serde_json::from_str(releases).unwrap(),
                &version,
                Platform::Windows
            )
            .is_none()
        );
        assert!(
            available(
                serde_json::from_str(releases).unwrap(),
                &current,
                Platform::Mac
            )
            .is_none()
        );

        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();
        let mac = serde_json::json!({"size":17,"sha256":"a".repeat(64)});
        let windows = serde_json::json!({"size":29,"sha256":"b".repeat(64)});
        let manifest = serde_json::json!({"version":version.to_string(),"macos_arm64":mac,"windows_x64":windows}).to_string();
        let signature = STANDARD.encode(key.sign(manifest.as_bytes()).as_ref());
        let envelope = |manifest| {
            serde_json::to_vec(&serde_json::json!({"manifest":manifest,"signature":signature}))
                .unwrap()
        };
        let offer = verify_manifest(
            &envelope(manifest.clone()),
            key.public_key().as_ref(),
            version.clone(),
            Platform::Windows,
        )
        .unwrap();
        assert_eq!(offer.artifact.size, 29);
        assert!(
            verify_manifest(
                &envelope(manifest.replace("29", "30")),
                key.public_key().as_ref(),
                version,
                Platform::Windows
            )
            .is_err()
        );
        assert!(
            verify_manifest(
                &envelope(manifest),
                key.public_key().as_ref(),
                Version::new(0, 1, 5),
                Platform::Windows
            )
            .is_err()
        );
    }
}
