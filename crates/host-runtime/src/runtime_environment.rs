//! One read-only environment policy for source hosts, installed hosts and WSL.
use crate::{Error, Result, images, managed_engine};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub(crate) const CONTRACT: &str = "ai-phone-runtime-environment-v1";
type Environment = BTreeMap<String, String>;
fn invalid() -> Error {
    Error::Invalid("runtime_environment_profile")
}
fn text(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).ok_or_else(invalid)
}
fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn string(value: &Value, key: &str, default: &str) -> Result<String> {
    match value.get(key) {
        None => Ok(default.into()),
        Some(value) => value.as_str().map(str::to_owned).ok_or_else(invalid),
    }
}
fn number(value: &Value, key: &str, default: u64) -> Result<String> {
    match value.get(key) {
        None => Ok(default.to_string()),
        Some(value) => value.as_u64().map(|n| n.to_string()).ok_or_else(invalid),
    }
}
// These paths may not exist before first device creation. Resolve the existing
// data alias without creating or adopting any guest directory.
pub(crate) fn resolved(path: &Path) -> Result<PathBuf> {
    let mut prefix = path.to_path_buf();
    let mut remaining = vec![];
    loop {
        match prefix.canonicalize() {
            Ok(mut path) => {
                for name in remaining.iter().rev() {
                    path.push(name);
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                remaining.push(prefix.file_name().ok_or_else(invalid)?.to_os_string());
                if !prefix.pop() {
                    return Err(error.into());
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}
struct Profile {
    value: Value,
    saved: bool,
    legacy: BTreeMap<String, String>,
}
impl Profile {
    fn load(root: &Path) -> Result<Self> {
        let bytes = read(&root.join("data/deployment/profile.json"))?;
        let value = match &bytes {
            Some(bytes) => serde_json::from_slice::<Value>(bytes).map_err(|_| invalid())?,
            None => serde_json::json!({}),
        };
        if !value.is_object() {
            return Err(invalid());
        }
        let mut legacy = BTreeMap::new();
        if let Some(bytes) = read(&root.join(".env"))? {
            let body = String::from_utf8(bytes).map_err(|_| invalid())?;
            for line in body.lines().filter(|line| !line.starts_with('#')) {
                if let Some((key, value)) = line.split_once('=')
                    && matches!(
                        key,
                        "HOST_HTTP_PROXY" | "CONTAINER_HTTP_PROXY" | "AI_PHONE_LOCAL_ENTRY"
                    )
                {
                    legacy.insert(key.into(), value.trim().into());
                }
            }
        }
        Ok(Self {
            value,
            saved: bytes.is_some(),
            legacy,
        })
    }
    fn legacy(&self, key: &str) -> &str {
        self.legacy.get(key).map(String::as_str).unwrap_or("")
    }
    fn base_path(&self, root: &Path) -> Result<String> {
        let path = if self.value.get("launch_path").is_some() {
            crate::profile::checked_launch_path(self.value["launch_path"].as_str())?.to_owned()
        } else {
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin:/usr/sbin:/sbin".into())
        };
        let prefix = text(&root.join(".tools/bin"))?;
        let mut seen = BTreeSet::new();
        Ok([prefix.as_str(), "/opt/homebrew/bin", "/usr/local/bin"]
            .into_iter()
            .chain(path.split(':'))
            .filter(|value| seen.insert(*value))
            .collect::<Vec<_>>()
            .join(":"))
    }
    fn environment(&self, root: &Path, user: &Path) -> Result<Environment> {
        let empty = serde_json::json!({});
        let phone = self.value.get("phone").unwrap_or(&empty);
        let desktop = self.value.get("desktop").unwrap_or(&empty);
        if !phone.is_object() || !desktop.is_object() {
            return Err(invalid());
        }
        let template = string(phone, "template", "")?;
        let x64 = template == "android35-x86_64"
            || (template.is_empty()
                && std::env::consts::OS == "linux"
                && std::env::consts::ARCH == "x86_64");
        let abi = if x64 { "x86_64" } else { "arm64-v8a" };
        let sdk = selected_path(
            string(&self.value, "sdk_root", "")?,
            ["AI_PHONE_SDK_ROOT", "ANDROID_HOME"],
        );
        let sdk = if sdk.is_empty() {
            let candidates = [
                user.join("Library/Android/sdk"),
                PathBuf::from("/opt/homebrew/share/android-commandlinetools"),
            ];
            text(
                candidates
                    .iter()
                    .find(|p| p.join("platform-tools/adb").exists())
                    .unwrap_or(&candidates[0]),
            )?
        } else {
            sdk
        };
        let java = selected_path(
            string(&self.value, "java_home", "")?,
            ["AI_PHONE_JAVA_HOME", "JAVA_HOME"],
        );
        let homebrew_java = "/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home";
        let java = if java.is_empty() && Path::new(homebrew_java).exists() {
            homebrew_java.into()
        } else {
            java
        };
        let host = string(&self.value, "host_proxy", self.legacy("HOST_HTTP_PROXY"))?;
        let mut env = Environment::from([
            ("AI_PHONE_PROJECT_ROOT".into(), text(root)?),
            ("AI_PHONE_REDIS_PORT".into(), "56382".into()),
            (
                "AI_PHONE_DATA_ROOT".into(),
                text(&resolved(&root.join("data"))?)?,
            ),
            (
                "AI_PHONE_DEFAULT_LOCALE".into(),
                string(&self.value, "locale", "")?,
            ),
            (
                "AI_PHONE_LOCAL_ENTRY".into(),
                match self.value.get("local_entry") {
                    Some(value) => {
                        if value.as_bool().ok_or_else(invalid)? {
                            "1"
                        } else {
                            "0"
                        }
                    }
                    None => {
                        if self.legacy("AI_PHONE_LOCAL_ENTRY") == "1" {
                            "1"
                        } else {
                            "0"
                        }
                    }
                }
                .into(),
            ),
            ("AI_PHONE_SDK_ROOT".into(), sdk),
            ("AI_PHONE_ANDROID_ABI".into(), abi.into()),
            (
                "AI_PHONE_CLASH_DESKTOP_DIR".into(),
                if x64 { "linux-amd64" } else { "linux" }.into(),
            ),
            ("JAVA_HOME".into(), java.clone()),
            ("AI_PHONE_JAVA_HOME".into(), java),
            ("AI_PHONE_HOST_PROXY".into(), host.clone()),
            (
                "AI_PHONE_CONTAINER_PROXY".into(),
                string(
                    &self.value,
                    "container_proxy",
                    self.legacy("CONTAINER_HTTP_PROXY"),
                )?,
            ),
            (
                "AI_PHONE_EMULATOR_PROXY".into(),
                string(
                    &self.value,
                    "emulator_proxy",
                    if self.saved { "" } else { &host },
                )?,
            ),
        ]);
        for (key, folder) in [
            ("ANDROID_USER_HOME", "user"),
            ("ANDROID_EMULATOR_HOME", "user"),
            ("ANDROID_AVD_HOME", "avd"),
        ] {
            env.insert(
                key.into(),
                text(&resolved(&root.join("data/android").join(folder))?)?,
            );
        }
        for (section, key, field, default, suffix) in [
            (phone, "AI_PHONE_PHONE_CPUS", "cpus", 4, ""),
            (phone, "AI_PHONE_PHONE_MEMORY", "memory_mb", 3072, ""),
            (phone, "AI_PHONE_PHONE_STORAGE", "storage_gb", 16, ""),
            (desktop, "AI_PHONE_DESKTOP_CPUS", "cpus", 2, ""),
            (desktop, "AI_PHONE_DESKTOP_MEMORY", "memory_mb", 2560, "m"),
        ] {
            env.insert(key.into(), number(section, field, default)? + suffix);
        }
        env.extend(super::guest_apps::environment(root, abi)?);
        Ok(env)
    }
}
fn selected_path<const N: usize>(configured: String, keys: [&str; N]) -> String {
    if !configured.is_empty() {
        return configured;
    }
    keys.into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}
pub(crate) fn profile_environment(root: &Path, user: &Path) -> Result<Environment> {
    Profile::load(root)?.environment(root, user)
}
pub(crate) fn resolve(root: &Path, user: &Path) -> Result<Environment> {
    resolve_with_engine(root, user, managed_engine::environment(root, user)?)
}

pub(crate) fn resolve_with_engine(
    root: &Path,
    user: &Path,
    managed: Environment,
) -> Result<Environment> {
    let profile = Profile::load(root)?;
    let base = profile.base_path(root)?;
    let mut env = Environment::new();
    let prefix = managed
        .get("AI_PHONE_MANAGED_BIN")
        .cloned()
        .or_else(|| std::env::var("AI_PHONE_MANAGED_BIN").ok())
        .unwrap_or_default();
    let path = env.get("PATH").map(String::as_str).unwrap_or(&base);
    let path = if prefix.is_empty() {
        path.into()
    } else {
        format!("{prefix}:{path}")
    };
    env.extend(managed);
    env.extend(images::environment(root)?);
    env.extend(profile.environment(root, user)?);
    env.insert("PATH".into(), path);
    env.insert(
        "AI_PHONE_NATIVE_HOST_PROGRAM".into(),
        text(&std::env::current_exe()?)?,
    );
    Ok(env)
}
pub(crate) fn run(arguments: &[OsString]) -> Result<()> {
    let (root, user, shell) = match arguments {
        [root, user] => (root, user, false),
        [root, user, format] if format == "shell" => (root, user, true),
        _ => return Err(Error::Invalid("runtime_environment_arguments")),
    };
    let env = resolve(Path::new(root), Path::new(user))?;
    if shell {
        for (key, value) in env {
            println!("export {key}='{}'", value.replace('\'', "'\\''"));
        }
    } else {
        println!("{}", serde_json::to_string(&env).map_err(|_| invalid())?);
    }
    Ok(())
}
