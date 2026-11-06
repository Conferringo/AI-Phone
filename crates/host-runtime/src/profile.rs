//! Native first-run configuration. Resuming never rewrites an owned resource profile.
use crate::{Error, Result, filesystem};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{ffi::OsString, fs, io::Read, path::Path};

pub(crate) const CONTRACT: &str = "ai-phone-host-profile-v1";
const LOCALES: [&str; 10] = [
    "zh-CN", "en", "zh-TW", "ja", "ko", "de", "fr", "es", "ru", "ar",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Phone {
    cpus: u32,
    memory_mb: u32,
    storage_gb: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Desktop {
    cpus: u32,
    memory_mb: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    locale: String,
    phone: Phone,
    desktop: Desktop,
}

fn resources(phone: &Phone, desktop: &Desktop) -> bool {
    [phone.cpus, desktop.cpus]
        .iter()
        .all(|cpus| [1, 2, 4].contains(cpus))
        && [1536, 2048, 3072, 4096].contains(&phone.memory_mb)
        && [16, 24, 32].contains(&phone.storage_gb)
        && [2048, 2560, 3072, 4096].contains(&desktop.memory_mb)
}

fn absolute(text: &str) -> bool {
    Path::new(text).is_absolute() && !text.chars().any(|value| value < ' ')
}

fn proxy(text: &str) -> bool {
    text.is_empty()
        || url::Url::parse(text).is_ok_and(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && matches!(url.path(), "" | "/")
                && url.query().is_none()
                && url.fragment().is_none()
        })
}

fn validate(value: &Value, template: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct Profile {
        schema_version: u32,
        locale: String,
        sdk_root: String,
        java_home: String,
        host_proxy: String,
        container_proxy: String,
    }
    #[derive(Deserialize)]
    struct Device {
        template: String,
        cpus: u32,
        memory_mb: u32,
    }
    let header: Profile =
        serde_json::from_value(value.clone()).map_err(|_| Error::Invalid("host_profile"))?;
    let phone: Device = serde_json::from_value(value["phone"].clone())
        .map_err(|_| Error::Invalid("host_profile"))?;
    let desktop: Device = serde_json::from_value(value["desktop"].clone())
        .map_err(|_| Error::Invalid("host_profile"))?;
    let storage = value["phone"]["storage_gb"]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(Error::Invalid("host_profile"))?;
    if header.schema_version != 1
        || !LOCALES.contains(&header.locale.as_str())
        || phone.template != template
        || !matches!(template, "android35-arm64" | "android35-x86_64")
        || desktop.template != "debian13-openbox"
        || !resources(
            &Phone {
                cpus: phone.cpus,
                memory_mb: phone.memory_mb,
                storage_gb: storage,
            },
            &Desktop {
                cpus: desktop.cpus,
                memory_mb: desktop.memory_mb,
            },
        )
        || !absolute(&header.sdk_root)
        || (!header.java_home.is_empty() && !absolute(&header.java_home))
        || !proxy(&header.host_proxy)
        || !proxy(&header.container_proxy)
        || value
            .get("local_entry")
            .is_some_and(|entry| !entry.is_boolean())
    {
        return Err(Error::Invalid("host_profile"));
    }
    if let Some(path) = value.get("launch_path") {
        let valid = path
            .as_str()
            .is_some_and(|path| !path.trim().is_empty() && !path.chars().any(char::is_control));
        if !valid {
            return Err(Error::Invalid("host_profile_launch_path"));
        }
    }
    Ok(())
}

fn credentials(root: &Path) -> Result<()> {
    let path = root.join(".env");
    if path.exists() {
        return Ok(());
    }
    // Source staging deliberately links .env to the installation's persistent user directory.
    let path = match fs::read_link(&path) {
        Ok(target) => root.join(target),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path,
        Err(error) => return Err(error.into()),
    };
    let mut random = [0; 24];
    getrandom::fill(&mut random).map_err(|_| Error::Invalid("host_profile_random"))?;
    let password = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let bytes =
        format!("POSTGRES_DB=ai_phone\nPOSTGRES_USER=ai_phone\nPOSTGRES_PASSWORD={password}\n");
    filesystem::publish_private(
        path.parent().ok_or(Error::Invalid("host_profile"))?,
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or(Error::Invalid("host_profile"))?,
        bytes.as_bytes(),
        true,
    )
}

pub(crate) fn prepare(
    root: &Path,
    sdk: &Path,
    template: &str,
    bytes: &[u8],
    launch_path: &str,
    java: &str,
) -> Result<()> {
    prepare_request(
        root,
        sdk,
        template,
        parse_request(bytes)?,
        launch_path,
        java,
    )
}

pub(crate) fn parse_request(bytes: &[u8]) -> Result<Request> {
    if bytes.len() > 8192 {
        return Err(Error::Invalid("host_profile_request"));
    }
    let request: Request =
        serde_json::from_slice(bytes).map_err(|_| Error::Invalid("host_profile_request"))?;
    if !LOCALES.contains(&request.locale.as_str()) || !resources(&request.phone, &request.desktop) {
        return Err(Error::Invalid("host_profile_request"));
    }
    Ok(request)
}

pub(crate) fn prepare_request(
    root: &Path,
    sdk: &Path,
    template: &str,
    request: Request,
    launch_path: &str,
    java: &str,
) -> Result<()> {
    if !matches!(template, "android35-arm64" | "android35-x86_64") {
        return Err(Error::Invalid("host_profile_request"));
    }
    let destination = root.join("data/deployment/profile.json");
    if destination.exists() {
        return validate(
            &serde_json::from_slice(&fs::read(destination)?)
                .map_err(|_| Error::Invalid("host_profile"))?,
            template,
        );
    }
    if !sdk.is_absolute()
        || sdk
            .as_os_str()
            .to_string_lossy()
            .chars()
            .any(|value| value < ' ')
        || (!java.is_empty() && !absolute(java))
    {
        return Err(Error::Invalid("host_profile"));
    }
    if launch_path.trim().is_empty() || launch_path.chars().any(char::is_control) {
        return Err(Error::Invalid("host_profile_launch_path"));
    }
    let value = json!({"schema_version":1,"local_entry":true,"locale":request.locale,
        "launch_path":launch_path,
        "sdk_root":sdk,"java_home":java,
        "host_proxy":"","container_proxy":"",
        "phone":{"template":template,"cpus":request.phone.cpus,"memory_mb":request.phone.memory_mb,"storage_gb":request.phone.storage_gb},
        "desktop":{"template":"debian13-openbox","cpus":request.desktop.cpus,"memory_mb":request.desktop.memory_mb}});
    persist(root, &value)
}

fn persist(root: &Path, value: &Value) -> Result<()> {
    credentials(root)?;
    let destination = root.join("data/deployment/profile.json");
    let folder = destination.parent().ok_or(Error::Invalid("host_profile"))?;
    filesystem::directory(folder, true)?;
    filesystem::publish_private(
        folder,
        "profile.json",
        &serde_json::to_vec_pretty(value).map_err(|_| Error::Invalid("host_profile"))?,
        false,
    )
}

fn document() -> Result<Value> {
    let value: Value =
        serde_json::from_reader(std::io::stdin()).map_err(|_| Error::Invalid("host_profile"))?;
    let template = value
        .pointer("/phone/template")
        .and_then(Value::as_str)
        .ok_or(Error::Invalid("host_profile"))?;
    validate(&value, template)?;
    Ok(value)
}

pub(crate) fn check(arguments: &[OsString]) -> Result<()> {
    if !arguments.is_empty() {
        return Err(Error::Invalid("host_profile_arguments"));
    }
    document()?;
    println!("{{\"valid\":true}}");
    Ok(())
}

pub(crate) fn write(arguments: &[OsString]) -> Result<()> {
    let [root] = arguments else {
        return Err(Error::Invalid("host_profile_arguments"));
    };
    persist(Path::new(root), &document()?)?;
    println!("{{\"prepared\":true}}");
    Ok(())
}

pub(crate) fn run(arguments: &[OsString]) -> Result<()> {
    let [root, sdk, template] = arguments else {
        return Err(Error::Invalid("host_profile_arguments"));
    };
    let template = template
        .to_str()
        .ok_or(Error::Invalid("host_profile_arguments"))?;
    let mut bytes = Vec::new();
    std::io::stdin().take(8193).read_to_end(&mut bytes)?;
    if bytes.len() > 8192 {
        return Err(Error::Invalid("host_profile_request"));
    }
    prepare(
        Path::new(root),
        Path::new(sdk),
        template,
        &bytes,
        &std::env::var("PATH").unwrap_or_else(|_| "/bin:/usr/bin".into()),
        &std::env::var("JAVA_HOME").unwrap_or_default(),
    )?;
    println!("{{\"prepared\":true}}");
    Ok(())
}
