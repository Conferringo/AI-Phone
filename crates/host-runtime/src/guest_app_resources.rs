//! Guest application acquisition; installed application state has a separate owner.
use crate::{Error, Result, filesystem};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const LIMIT: u64 = 300 * 1024 * 1024;
pub(crate) const CONTRACT: &str = "ai-phone-guest-app-resources-v1";
fn invalid(code: &'static str) -> Error {
    Error::Invalid(code)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Android,
    Linux,
}
impl Kind {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "android" => Ok(Self::Android),
            "linux" => Ok(Self::Linux),
            _ => Err(invalid("guest_app_kind")),
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Linux => "linux",
        }
    }
}
#[derive(Deserialize)]
struct Pin {
    file: String,
    size_bytes: u64,
    sha256: String,
    url: String,
    version: String,
}
struct Resources {
    root: PathBuf,
    env: BTreeMap<String, String>,
    android: Pin,
    linux: Pin,
}
impl Resources {
    fn new(root: &Path) -> Result<Self> {
        let env: BTreeMap<_, _> = std::env::vars().collect();
        let value: Value = serde_json::from_slice(&fs::read(root.join("config/clash-apps.json"))?)
            .map_err(|_| invalid("guest_app_metadata"))?;
        let x64 = env
            .get("AI_PHONE_ANDROID_ABI")
            .is_some_and(|v| v == "x86_64");
        let android = if x64 {
            &value["alternatives"]["android-x86_64"]
        } else {
            &value["android"]
        };
        let linux = if x64 {
            &value["alternatives"]["linux-amd64"]
        } else {
            &value["linux"]
        };
        Ok(Self {
            root: std::path::absolute(root)?,
            env,
            android: serde_json::from_value(android.clone())
                .map_err(|_| invalid("guest_app_metadata"))?,
            linux: serde_json::from_value(linux.clone())
                .map_err(|_| invalid("guest_app_metadata"))?,
        })
    }
    fn pin(&self, kind: Kind) -> &Pin {
        match kind {
            Kind::Android => &self.android,
            Kind::Linux => &self.linux,
        }
    }
    fn default_kinds(&self) -> Vec<Kind> {
        let bundled = self
            .env
            .get("AI_PHONE_IMAGE_PULL_POLICY")
            .is_some_and(|v| v == "never")
            && self
                .env
                .get("AI_PHONE_DESKTOP_IMAGE")
                .and_then(|v| v.strip_prefix("ai-phone-bundle/desktop:sha-"))
                .is_some_and(|v| {
                    v.len() == 64
                        && v.bytes()
                            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                });
        if bundled {
            vec![Kind::Android]
        } else {
            vec![Kind::Android, Kind::Linux]
        }
    }
    fn path(&self, kind: Kind) -> Result<(PathBuf, bool)> {
        let pin = self.pin(kind);
        if kind == Kind::Android {
            let folder = self.root.join("runtime/guest-apps");
            let bundled = match fs::symlink_metadata(&folder) {
                Ok(meta) => {
                    if !meta.is_dir() || filesystem::alias(&meta) {
                        return Err(invalid("guest_app_component"));
                    }
                    true
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(e.into()),
            };
            if bundled
                || self
                    .env
                    .get("AI_PHONE_CLASH_ANDROID_APK")
                    .is_some_and(|v| !v.is_empty())
            {
                let abi = self
                    .env
                    .get("AI_PHONE_ANDROID_ABI")
                    .map(String::as_str)
                    .unwrap_or("arm64-v8a");
                if !["arm64-v8a", "x86_64"].contains(&abi) {
                    return Err(invalid("guest_app_abi"));
                }
                let path = folder.join(format!("clash-{abi}.apk"));
                if self.env.get("AI_PHONE_CLASH_ANDROID_APK").map(Path::new) != Some(path.as_path())
                {
                    return Err(invalid("guest_app_selection"));
                }
                let meta = fs::symlink_metadata(&path)?;
                if !meta.is_file() || filesystem::alias(&meta) {
                    return Err(invalid("guest_app_component"));
                }
                return Ok((path, true));
            }
        }
        let folder = match kind {
            Kind::Android => "android",
            Kind::Linux => self
                .env
                .get("AI_PHONE_CLASH_DESKTOP_DIR")
                .filter(|v| !v.is_empty())
                .map(String::as_str)
                .unwrap_or("linux"),
        };
        Ok((
            self.root
                .join(".tools/clash-apps")
                .join(folder)
                .join(&pin.file),
            false,
        ))
    }
    fn verify(&self, kinds: &[Kind]) -> Result<Value> {
        for &kind in kinds {
            let (path, _) = self.path(kind)?;
            if !path.exists() {
                return Err(invalid("guest_app_cache_missing"));
            }
            verify(&path, self.pin(kind))?;
        }
        Ok(json!({"verified":true}))
    }
    async fn download(&self, kinds: &[Kind]) -> Result<Value> {
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(180));
        if let Some(proxy) = self
            .env
            .get("AI_PHONE_BOOTSTRAP_PROXY")
            .filter(|v| !v.is_empty())
        {
            builder =
                builder.proxy(reqwest::Proxy::all(proxy).map_err(|_| invalid("guest_app_proxy"))?);
        }
        let client = builder.build().map_err(|_| invalid("guest_app_download"))?;
        let client = &client;
        let prepare = |kind| async move {
            if kinds.contains(&kind) {
                self.prepare(kind, client).await?;
            }
            Ok::<_, Error>(())
        };
        tokio::try_join!(prepare(Kind::Android), prepare(Kind::Linux))?;
        let paths = kinds
            .iter()
            .map(|&kind| Ok((kind.key(), self.path(kind)?.0)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        Ok(
            json!({"verified":true,"versions":{"android":self.android.version,"linux":self.linux.version},"paths":paths}),
        )
    }
    async fn prepare(&self, kind: Kind, client: &reqwest::Client) -> Result<()> {
        let (path, bundled) = self.path(kind)?;
        let pin = self.pin(kind);
        if path.exists() {
            return verify(&path, pin);
        }
        if bundled {
            return Err(invalid("guest_app_bundled_missing"));
        }
        let mut response = client
            .get(&pin.url)
            .send()
            .await
            .map_err(|_| invalid("guest_app_download"))?;
        if !response.status().is_success() {
            return Err(invalid("guest_app_download"));
        }
        let parent = path.parent().expect("resource parent");
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        let mut size = 0;
        let mut hash = Sha256::new();
        while let Some(bytes) = response
            .chunk()
            .await
            .map_err(|_| invalid("guest_app_download"))?
        {
            size += bytes.len() as u64;
            if size > pin.size_bytes || size > LIMIT {
                return Err(invalid("guest_app_size"));
            }
            hash.update(&bytes);
            temp.write_all(&bytes)?;
        }
        if size != pin.size_bytes || format!("{:x}", hash.finalize()) != pin.sha256 {
            return Err(invalid("guest_app_integrity"));
        }
        filesystem::mode(temp.path(), 0o644)?;
        temp.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}
fn verify(path: &Path, pin: &Pin) -> Result<()> {
    let mut file = fs::File::open(path)?;
    if file.metadata()?.len() != pin.size_bytes {
        return Err(invalid("guest_app_integrity"));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if format!("{:x}", hash.finalize()) != pin.sha256 {
        return Err(invalid("guest_app_integrity"));
    }
    Ok(())
}
pub(crate) async fn run(args: &[OsString]) -> Result<()> {
    #[cfg(unix)]
    let (args, owned) = if args.last().is_some_and(|v| v == "--stdin-lease") {
        (&args[..args.len() - 1], true)
    } else {
        (args, false)
    };
    if args.len() < 2 {
        return Err(invalid("guest_app_arguments"));
    }
    let root = Path::new(&args[0]);
    let operation = args[1]
        .to_str()
        .ok_or_else(|| invalid("guest_app_arguments"))?;
    let resources = Resources::new(root)?;
    let kinds = if args.len() == 2 {
        resources.default_kinds()
    } else {
        args[2..]
            .iter()
            .map(|v| {
                v.to_str()
                    .ok_or_else(|| invalid("guest_app_arguments"))
                    .and_then(Kind::parse)
            })
            .collect::<Result<Vec<_>>>()?
    };
    let work = async {
        match operation {
            "verify" => resources.verify(&kinds),
            "download" => resources.download(&kinds).await,
            _ => Err(invalid("guest_app_operation")),
        }
    };
    #[cfg(unix)]
    let result = if owned {
        crate::native_ipc::while_owned(work).await?
    } else {
        work.await?
    };
    #[cfg(not(unix))]
    let result = work.await?;
    println!("{result}");
    Ok(())
}
