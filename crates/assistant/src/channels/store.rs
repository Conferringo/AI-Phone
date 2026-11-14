use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub(super) type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub(super) const PROVIDERS: [&str; 5] = ["wecom", "feishu", "qq", "telegram", "whatsapp"];

pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
pub(super) fn event_id(provider: &str, app: &str, id: &str) -> String {
    let bytes = serde_json::to_vec(&[provider, app, id]).unwrap();
    event_uuid(&format!("{:x}", Sha512::digest(bytes)), '8')
}

pub(super) fn legacy_event_id(provider: &str, app: &str, id: &str) -> String {
    let bytes = serde_json::to_vec(&[provider, app, id]).unwrap();
    event_uuid(&format!("{:x}", Sha256::digest(bytes)), '5')
}

fn event_uuid(hash: &str, version: char) -> String {
    format!(
        "{}-{}-{version}{}-a{}-{}",
        &hash[..8],
        &hash[8..12],
        &hash[13..16],
        &hash[17..20],
        &hash[20..32]
    )
}
pub(super) fn scope(agent: &str, provider: &str) -> String {
    format!("{agent}/{provider}")
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Pairing {
    pub code: String,
    pub expires: u64,
    pub attempts: u8,
}
impl Pairing {
    pub fn new() -> Self {
        Self {
            code: (100_000 + Uuid::new_v4().as_u128() % 900_000).to_string(),
            expires: now() + 600_000,
            attempts: 0,
        }
    }
    pub fn matches(&mut self, text: &str, direct: bool) -> bool {
        let mut parts = text.split_whitespace();
        let Some(command) = parts.next() else {
            return false;
        };
        let Some(code) = parts.next() else {
            return false;
        };
        if !direct
            || !matches!(command, "配对" | "/pair")
            || parts.next().is_some()
            || self.expires < now()
            || self.attempts >= 10
        {
            return false;
        }
        self.attempts += 1;
        self.code == code
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Config {
    pub app_id: String,
    pub secret: String,
    pub allowed: Vec<String>,
    pub version: String,
    pub pairing: Option<Pairing>,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub proxy_url: String,
    pub identity: Option<String>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Stage {
    Queued,
    Running,
    Sending,
    Delivered,
    Unknown,
    Canceled,
    Failed,
}
impl Stage {
    pub fn pending(&self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Sending => "sending",
            Self::Delivered => "delivered",
            Self::Unknown => "unknown",
            Self::Canceled => "canceled",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Job {
    #[serde(default)]
    pub agent_id: String,
    pub device_id: Option<String>,
    pub provider: String,
    pub version: String,
    pub id: String,
    pub text: String,
    pub target: Value,
    pub run_id: Option<String>,
    pub stage: Stage,
    pub at: u64,
    #[serde(default)]
    pub outbound: bool,
    pub push_id: Option<String>,
    pub png: Option<String>,
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub text_sent: bool,
    #[serde(default)]
    pub image_sent: bool,
    #[serde(default)]
    pub screenshot: bool,
    #[serde(default)]
    pub screenshot_only: bool,
    #[serde(default)]
    pub error: String,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Vault {
    pub configs: BTreeMap<String, Config>,
    pub jobs: Vec<Job>,
    #[serde(default)]
    pub disconnect_warnings: BTreeMap<String, String>,
}
pub(super) struct Store {
    pub vault: Vault,
    key: aead::LessSafeKey,
    file: PathBuf,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        let directory = root.join("data/channels");
        fs::create_dir_all(&directory)?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        let key_path = directory.join("vault.key");
        if !key_path.exists() {
            let mut key = [0; 32];
            SystemRandom::new()
                .fill(&mut key)
                .map_err(|_| "channel_key_random")?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&key_path)?;
            file.write_all(&key)?;
            file.sync_all()?;
        }
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_256_GCM, &fs::read(key_path)?)
                .map_err(|_| "channel_key_invalid")?,
        );
        let file = directory.join("vault.bin");
        let mut vault: Vault = if file.exists() {
            let content = fs::read(&file)?;
            if content.len() < 28 {
                return Err("channel_vault_invalid".into());
            }
            let mut encrypted = content[28..].to_vec();
            encrypted.extend_from_slice(&content[12..28]);
            let nonce = aead::Nonce::try_assume_unique_for_key(&content[..12])
                .map_err(|_| "channel_nonce_invalid")?;
            let plain = key
                .open_in_place(nonce, aead::Aad::empty(), &mut encrypted)
                .map_err(|_| "channel_vault_invalid")?;
            serde_json::from_slice(plain)?
        } else {
            Vault::default()
        };
        for job in &mut vault.jobs {
            if job.stage == Stage::Sending {
                job.stage = Stage::Unknown;
            }
        }
        let store = Self { vault, key, file };
        store.save()?;
        Ok(store)
    }
    pub fn save(&self) -> Result<()> {
        let mut nonce = [0; 12];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| "channel_nonce_random")?;
        let mut content = serde_json::to_vec(&self.vault)?;
        self.key
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::empty(),
                &mut content,
            )
            .map_err(|_| "channel_vault_encrypt")?;
        let tag = content.split_off(content.len() - aead::AES_256_GCM.tag_len());
        let mut sealed = Vec::with_capacity(28 + content.len());
        sealed.extend(nonce);
        sealed.extend(tag);
        sealed.extend(content);
        crate::private_file::atomic_private(&self.file, &sealed).map_err(Into::into)
    }
}
