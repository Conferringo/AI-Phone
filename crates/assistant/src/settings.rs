use crate::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap, fs, io::Write, path::Path, sync::Mutex as SyncMutex, time::Instant,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub verified_at_ms: u64,
}
fn default_provider() -> String {
    "deepseek".into()
}
fn default_protocol() -> String {
    "responses".into()
}
impl ModelConfig {
    pub(crate) fn endpoint(&self) -> Result<String, String> {
        let suffix = match self.protocol.as_str() {
            "responses" => "/responses",
            "chat-completions" => "/chat/completions",
            _ => return Err("请选择 Responses 或 Chat Completions 协议".into()),
        };
        let base = self.base_url.trim_end_matches('/');
        let url = reqwest::Url::parse(base).map_err(|_| "请输入完整的 http(s) 地址")?;
        let path = url.path().trim_end_matches('/');
        if path.ends_with(suffix) {
            return Ok(base.into());
        }
        if path.ends_with("/responses") || path.ends_with("/chat/completions") {
            return Err("API 地址的末尾与所选协议不一致".into());
        }
        Ok(format!("{base}{suffix}"))
    }
    pub fn client(&self) -> Result<reqwest::Client, String> {
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(180));
        if !self.proxy_url.is_empty() {
            builder =
                builder.proxy(reqwest::Proxy::all(&self.proxy_url).map_err(|_| "代理地址无效")?);
        }
        builder.build().map_err(|_| "无法创建模型连接".into())
    }
    pub(crate) fn profile(&self, revision: u64) -> wire::ModelProfile {
        wire::ModelProfile {
            revision,
            provider: self.provider.clone(),
            protocol: self.protocol.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            proxy_url: self.proxy_url.clone(),
            has_api_key: !self.api_key.is_empty(),
            verified_at_ms: self.verified_at_ms,
        }
    }
    fn fingerprint(&self) -> String {
        let mut normalized = self.clone();
        normalized.verified_at_ms = 0;
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&normalized).expect("config serializes"))
        )
    }
}
struct Verification {
    scope: String,
    fingerprint: String,
    revision: u64,
    at: Instant,
}
struct State {
    current: u64,
    models: BTreeMap<u64, ModelConfig>,
    verified: BTreeMap<String, Verification>,
    testing: bool,
}
pub struct SettingsStore {
    dir: PathBuf,
    state: SyncMutex<State>,
}
fn atomic_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let temp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        fs::File::open(path.parent().expect("private directory"))?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map_err(|_| "配置无法保存，请检查存储空间与权限".into())
}
impl SettingsStore {
    pub fn open(root: &Path) -> Result<Self, String> {
        use std::os::unix::fs::PermissionsExt;
        let dir = root.join("data/model/revisions");
        fs::create_dir_all(&dir).map_err(|_| "无法创建配置目录")?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
            .map_err(|_| "无法保护配置目录")?;
        let mut models = BTreeMap::new();
        for entry in fs::read_dir(&dir).map_err(|_| "无法读取配置")? {
            let entry = entry.map_err(|_| "无法读取配置")?;
            if let Some(id) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(".json"))
                .and_then(|n| n.parse::<u64>().ok())
            {
                let config = serde_json::from_slice(
                    &fs::read(entry.path()).map_err(|_| "无法读取配置版本")?,
                )
                .map_err(|_| "配置版本损坏")?;
                fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o600))
                    .map_err(|_| "无法保护配置版本")?;
                models.insert(id, config);
            }
        }
        let current = match fs::read_to_string(dir.join("current")) {
            Ok(s) => s.trim().parse::<u64>().map_err(|_| "配置指针损坏")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(_) => return Err("无法读取当前配置".into()),
        };
        if current != 0 && !models.contains_key(&current) {
            return Err("当前配置版本丢失".into());
        }
        let store = Self {
            dir,
            state: SyncMutex::new(State {
                current,
                models,
                verified: BTreeMap::new(),
                testing: false,
            }),
        };
        if current == 0 {
            let legacy = root.join("data/model/credentials.json");
            match fs::read(legacy) {
                Ok(bytes) => {
                    let mut config: ModelConfig =
                        serde_json::from_slice(&bytes).map_err(|_| "旧模型配置无效")?;
                    config.proxy_url = std::env::var("AI_PHONE_HOST_PROXY").unwrap_or_default();
                    store.commit(0, config)?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("无法读取旧模型配置".into()),
            }
        }
        Ok(store)
    }
    #[cfg(test)]
    pub fn test_configured(root: &Path) -> Self {
        let store = Self::open(root).unwrap();
        store
            .commit(
                0,
                ModelConfig {
                    api_key: "unused".into(),
                    base_url: "http://127.0.0.1:1".into(),
                    model: "unused".into(),
                    provider: "deepseek".into(),
                    protocol: "responses".into(),
                    proxy_url: String::new(),
                    verified_at_ms: 0,
                },
            )
            .unwrap();
        store
    }
    pub fn current(&self) -> u64 {
        self.state.lock().expect("settings lock").current
    }
    pub fn get(&self, revision: u64) -> Result<ModelConfig, String> {
        self.state
            .lock()
            .expect("settings lock")
            .models
            .get(&revision)
            .cloned()
            .ok_or("请先在设置中连接模型".into())
    }
    pub fn public(&self) -> wire::SettingsState {
        let state = self.state.lock().expect("settings lock");
        wire::SettingsState {
            configured: state.current > 0,
            revision: state.current,
            model: state
                .models
                .get(&state.current)
                .map(|m| m.profile(state.current)),
            previous: state
                .models
                .iter()
                .rev()
                .filter(|(r, _)| **r != state.current)
                .take(5)
                .map(|(r, m)| m.profile(*r))
                .collect(),
        }
    }
    pub fn credential(&self, expected: u64) -> Result<wire::ModelCredential, String> {
        let state = self.state.lock().expect("settings lock");
        if expected != state.current {
            return Err("配置已更新，请重新载入".into());
        }
        Ok(wire::ModelCredential {
            revision: state.current,
            api_key: state
                .models
                .get(&state.current)
                .map(|m| m.api_key.clone())
                .unwrap_or_default(),
        })
    }
    pub fn resolve(&self, draft: wire::ModelDraft) -> Result<ModelConfig, String> {
        self.resolve_at(draft, self.current())
    }
    pub fn resolve_at(
        &self,
        draft: wire::ModelDraft,
        revision: u64,
    ) -> Result<ModelConfig, String> {
        let base_url = normalize_url(&draft.base_url, false)?;
        let proxy_url = if draft.proxy_url.trim().is_empty() {
            String::new()
        } else {
            normalize_url(&draft.proxy_url, true)?
        };
        if !["deepseek", "compatible"].contains(&draft.provider.as_str())
            || !["responses", "chat-completions"].contains(&draft.protocol.as_str())
        {
            return Err("请选择 Responses 或 Chat Completions 协议".into());
        }
        let model = draft.model.trim().to_string();
        if model.is_empty() || model.len() > 200 || model.chars().any(char::is_control) {
            return Err("请输入有效的模型 ID".into());
        }
        let mut api_key = draft.api_key.trim().to_string();
        if draft.use_saved_key {
            if !api_key.is_empty() {
                return Err("保留密钥与填写新密钥不能同时选择".into());
            }
            let old = self.get(revision)?;
            if old.base_url != base_url
                || old.provider != draft.provider
                || old.protocol != draft.protocol
            {
                return Err("服务地址改变后，请重新填写密钥".into());
            }
            api_key = old.api_key;
        }
        if api_key.len() > 4096 || api_key.chars().any(char::is_control) {
            return Err("API Key 格式无效".into());
        }
        let config = ModelConfig {
            api_key,
            base_url,
            model,
            provider: draft.provider,
            protocol: draft.protocol,
            proxy_url,
            verified_at_ms: 0,
        };
        config.endpoint()?;
        Ok(config)
    }
    fn commit(&self, expected: u64, config: ModelConfig) -> Result<u64, String> {
        let mut state = self.state.lock().expect("settings lock");
        if state.current != expected {
            return Err("配置已在另一处修改，请重新载入后再试".into());
        }
        let revision = state.models.keys().next_back().copied().unwrap_or(0) + 1;
        atomic_private(
            &self.dir.join(format!("{revision}.json")),
            &serde_json::to_vec(&config).map_err(|_| "配置编码失败")?,
        )?;
        // An orphan revision after a crash is safe; only the atomic pointer activates it.
        state.models.insert(revision, config);
        atomic_private(&self.dir.join("current"), revision.to_string().as_bytes())?;
        state.current = revision;
        state.verified.clear();
        Ok(revision)
    }
    fn verified(&self, config: &ModelConfig, revision: u64) -> String {
        self.verified_for(config, revision, "")
    }
    fn verified_for(&self, config: &ModelConfig, revision: u64, scope: &str) -> String {
        let mut state = self.state.lock().expect("settings lock");
        state
            .verified
            .retain(|_, v| v.at.elapsed() < Duration::from_secs(600));
        if state.verified.len() >= 16 {
            state.verified.clear();
        }
        let token = Uuid::new_v4().to_string();
        state.verified.insert(
            token.clone(),
            Verification {
                scope: scope.into(),
                fingerprint: config.fingerprint(),
                revision,
                at: Instant::now(),
            },
        );
        token
    }
    pub fn save(&self, expected: u64, token: &str, mut config: ModelConfig) -> Result<u64, String> {
        {
            let state = self.state.lock().expect("settings lock");
            let valid = state.verified.get(token).is_some_and(|v| {
                v.scope.is_empty()
                    && v.revision == expected
                    && v.at.elapsed() < Duration::from_secs(600)
                    && v.fingerprint == config.fingerprint()
            });
            if !valid {
                return Err("请先测试当前配置；修改后需要重新测试".into());
            }
        }
        config.verified_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.commit(expected, config)
    }
    // Registry revisions never move the legacy current pointer. A failed database
    // transaction may leave an unreferenced immutable file, never an active config.
    pub fn save_connection(
        &self,
        scope: &str,
        expected: u64,
        token: &str,
        mut config: ModelConfig,
    ) -> Result<u64, String> {
        let mut state = self.state.lock().expect("settings lock");
        let valid = state.verified.get(token).is_some_and(|v| {
            v.scope == scope
                && v.revision == expected
                && v.at.elapsed() < Duration::from_secs(600)
                && v.fingerprint == config.fingerprint()
        });
        if !valid {
            return Err("请先测试当前配置；修改后需要重新测试".into());
        }
        config.verified_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let revision = state.models.keys().next_back().copied().unwrap_or(0) + 1;
        atomic_private(
            &self.dir.join(format!("{revision}.json")),
            &serde_json::to_vec(&config).map_err(|_| "配置编码失败")?,
        )?;
        state.models.insert(revision, config);
        state.verified.remove(token);
        Ok(revision)
    }
    pub async fn test_connection(
        &self,
        scope: &str,
        expected: u64,
        config: &ModelConfig,
    ) -> Result<wire::ModelTestResult, String> {
        {
            let mut state = self.state.lock().expect("settings lock");
            if state.testing {
                return Err("已有配置测试正在进行，请稍候".into());
            }
            state.testing = true;
        }
        struct Guard<'a>(&'a SettingsStore);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.state.lock().expect("settings lock").testing = false;
            }
        }
        let _guard = Guard(self);
        let checks = crate::model_adapter::probe(config).await;
        let ready = checks.iter().all(|c| c.passed);
        let verification_id = if ready {
            self.verified_for(config, expected, scope)
        } else {
            String::new()
        };
        Ok(wire::ModelTestResult {
            ready,
            checks,
            verification_id,
        })
    }
    pub fn redact(&self, text: &str) -> String {
        let state = self.state.lock().expect("settings lock");
        state
            .models
            .values()
            .filter(|m| !m.api_key.is_empty())
            .fold(text.to_string(), |text, m| {
                text.replace(&m.api_key, "[redacted]")
            })
    }
}
fn normalize_url(raw: &str, proxy: bool) -> Result<String, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| "请输入完整的 http(s) 地址")?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || (proxy && url.path() != "/")
    {
        return Err("地址格式无效；不支持在地址中携带账号或密钥".into());
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}
pub async fn for_run(app: &App, run: Uuid) -> Result<ModelConfig, String> {
    let current = app.settings.current() as i64;
    let revision: i64=sqlx::query_scalar("UPDATE runs SET model_revision=CASE WHEN model_revision=0 THEN $2 ELSE model_revision END WHERE id=$1 RETURNING model_revision")
        .bind(run).bind(current).fetch_one(&app.db).await.map_err(|_|"无法读取任务的模型配置")?;
    app.settings.get(revision as u64)
}
pub async fn route(
    axum::extract::State(app): axum::extract::State<Shared>,
    axum::extract::Path(operation): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Response {
    let result: Result<Response, String> = async {
        match operation.as_str() {
            "runtime" => {
                if let Err(e) = decode::<wire::SettingsRequest>(&app, &headers, body) {
                    return Ok(e.into());
                }
                let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
                    .fetch_one(&app.db)
                    .await
                    .is_ok();
                let mut checks = vec![wire::CapabilityCheck {
                    name: "数据库".into(),
                    passed: db_ok,
                    detail: if db_ok {
                        "连接正常"
                    } else {
                        "连接失败"
                    }
                    .into(),
                    duration_ms: 0,
                }];
                let media = async {
                    let response = app
                        .local
                        .get("http://127.0.0.1:17865/status")
                        .bearer_auth(&app.phone_token)
                        .timeout(Duration::from_secs(4))
                        .send()
                        .await
                        .ok()?;
                    if !response.status().is_success() {
                        return None;
                    }
                    response.json::<serde_json::Value>().await.ok()
                }
                .await;
                let fresh = media
                    .as_ref()
                    .and_then(|v| v["media"]["verifiedAgeMs"].as_u64())
                    .is_some_and(|age| age < 10000);
                checks.push(wire::CapabilityCheck {
                    name: "手机画面".into(),
                    passed: fresh,
                    detail: if fresh {
                        "正在接收实时画面"
                    } else if media.is_some() {
                        "尚无最近画面，请打开手机预览后重新检查"
                    } else {
                        "画面服务未连接"
                    }
                    .into(),
                    duration_ms: 0,
                });
                Ok(pb(StatusCode::OK, wire::RuntimeStatus { checks }))
            }
            "state" => {
                if let Err(e) = decode::<wire::SettingsRequest>(&app, &headers, body) {
                    return Ok(e.into());
                }
                Ok(pb(StatusCode::OK, app.settings.public()))
            }
            "credential" => {
                let request = match decode::<wire::ModelCredentialRequest>(&app, &headers, body) {
                    Ok(v) => v,
                    Err(e) => return Ok(e.into()),
                };
                Ok(pb(
                    StatusCode::OK,
                    app.settings.credential(request.expected_revision)?,
                ))
            }
            "test" => {
                let request = match decode::<wire::ModelTestRequest>(&app, &headers, body) {
                    Ok(v) => v,
                    Err(e) => return Ok(e.into()),
                };
                let config = app.settings.resolve(request.draft.ok_or("缺少模型配置")?)?;
                {
                    let mut state = app.settings.state.lock().expect("settings lock");
                    if state.testing {
                        return Err("已有配置测试正在进行，请稍候".into());
                    }
                    state.testing = true;
                }
                struct Guard<'a>(&'a SettingsStore);
                impl Drop for Guard<'_> {
                    fn drop(&mut self) {
                        self.0.state.lock().expect("settings lock").testing = false;
                    }
                }
                let _guard = Guard(&app.settings);
                let revision = app.settings.current();
                let checks = crate::model_adapter::probe(&config).await;
                let ready = checks.iter().all(|c| c.passed);
                let verification_id = if ready {
                    app.settings.verified(&config, revision)
                } else {
                    String::new()
                };
                Ok(pb(
                    StatusCode::OK,
                    wire::ModelTestResult {
                        ready,
                        checks,
                        verification_id,
                    },
                ))
            }
            "save" => {
                let request = match decode::<wire::SettingsSaveRequest>(&app, &headers, body) {
                    Ok(v) => v,
                    Err(e) => return Ok(e.into()),
                };
                let config = app.settings.resolve(request.draft.ok_or("缺少模型配置")?)?;
                app.settings
                    .save(request.expected_revision, &request.verification_id, config)?;
                crate::registry::initialize(&app.db, &app.settings)
                    .await
                    .map_err(|_| "模型已保存，注册表需要重新载入")?;
                Ok(pb(StatusCode::OK, app.settings.public()))
            }
            "rollback" => {
                let request = match decode::<wire::SettingsRollbackRequest>(&app, &headers, body) {
                    Ok(v) => v,
                    Err(e) => return Ok(e.into()),
                };
                let config = app.settings.get(request.revision)?;
                app.settings.commit(request.expected_revision, config)?;
                crate::registry::initialize(&app.db, &app.settings)
                    .await
                    .map_err(|_| "模型已保存，注册表需要重新载入")?;
                Ok(pb(StatusCode::OK, app.settings.public()))
            }
            _ => Ok(error(StatusCode::NOT_FOUND, "not_found", "操作不存在")),
        }
    }
    .await;
    result.unwrap_or_else(|message| error(StatusCode::CONFLICT, "settings_error", &message))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> (PathBuf, SettingsStore) {
        let root = std::env::temp_dir().join(format!("aiphone-settings-{}", Uuid::new_v4()));
        let store = SettingsStore::open(&root).unwrap();
        (root, store)
    }
    fn draft() -> wire::ModelDraft {
        wire::ModelDraft {
            provider: "deepseek".into(),
            protocol: "responses".into(),
            base_url: "https://api.deepseek.com/".into(),
            model: "test".into(),
            api_key: "test-secret".into(),
            ..Default::default()
        }
    }
    #[test]
    fn endpoint_validation_keeps_saved_addresses_and_rejects_protocol_mismatches() {
        let (root, store) = store();
        for (base, protocol, expected) in [
            (
                "https://gateway.example/v1",
                "responses",
                "https://gateway.example/v1/responses",
            ),
            (
                "http://responses",
                "responses",
                "http://responses/responses",
            ),
            (
                "https://gateway.example/v1/responses/",
                "responses",
                "https://gateway.example/v1/responses",
            ),
            (
                "http://localhost:8080/api/chat/completions",
                "chat-completions",
                "http://localhost:8080/api/chat/completions",
            ),
        ] {
            let config = store
                .resolve(wire::ModelDraft {
                    base_url: base.into(),
                    protocol: protocol.into(),
                    ..draft()
                })
                .unwrap();
            let saved = config.base_url.clone();
            assert_eq!(config.endpoint().unwrap(), expected);
            assert_eq!(config.base_url, saved);
        }
        assert!(
            store
                .resolve(wire::ModelDraft {
                    base_url: "https://gateway.example/v1/responses".into(),
                    protocol: "chat-completions".into(),
                    ..draft()
                })
                .is_err()
        );
        assert_eq!(store.current(), 0);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn revisions_require_verified_exact_config_and_pin_secrets() {
        let (root, store) = store();
        assert!(!store.public().configured);
        let config = store.resolve(draft()).unwrap();
        assert!(store.save(0, "", config.clone()).is_err());
        let token = store.verified(&config, 0);
        let mut changed = config.clone();
        changed.model = "changed".into();
        assert!(store.save(0, &token, changed).is_err());
        assert_eq!(store.save(0, &token, config.clone()).unwrap(), 1);
        let mut next = config.clone();
        next.api_key = "new-secret".into();
        let token = store.verified(&next, 1);
        assert_eq!(store.save(1, &token, next).unwrap(), 2);
        assert_eq!(store.get(1).unwrap().api_key, "test-secret");
        assert!(store.commit(1, config).is_err());
        assert!(!format!("{:?}", store.public()).contains("secret"));
        assert_eq!(store.credential(2).unwrap().api_key, "new-secret");
        assert!(store.credential(1).is_err());
        assert_eq!(SettingsStore::open(&root).unwrap().current(), 2);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join("data/model/revisions/2.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn saved_key_cannot_follow_endpoint_changes() {
        let (root, store) = store();
        let config = store.resolve(draft()).unwrap();
        store.commit(0, config).unwrap();
        let mut d = draft();
        d.api_key.clear();
        d.use_saved_key = true;
        d.base_url = "https://example.com".into();
        assert!(store.resolve(d).is_err());
        assert!(normalize_url("https://user:secret@example.com", false).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    #[ignore = "requires AI_PHONE_TEST_DATABASE_URL; creates and removes its own isolated database"]
    async fn first_owner_and_task_configuration_survive_changes() {
        use futures_util::FutureExt;
        use sqlx::{Connection, postgres::PgConnectOptions};
        use std::{panic::AssertUnwindSafe, str::FromStr};
        let url = std::env::var("AI_PHONE_TEST_DATABASE_URL").expect("test database URL");
        let name = format!("aiphone_settings_test_{}", Uuid::new_v4().simple());
        let mut admin = sqlx::PgConnection::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut admin)
            .await
            .unwrap();
        let db = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(PgConnectOptions::from_str(&url).unwrap().database(&name))
            .await
            .unwrap();
        let root = std::env::temp_dir().join(&name);
        let store = SettingsStore::open(&root).unwrap();
        let app = Arc::new(App {
            db: db.clone(),
            root: root.clone(),
            token: "test-service-token".into(),
            phone_token: "unused".into(),
            settings: store,
            local: reqwest::Client::new(),
            remote: reqwest::Client::new(),
            active: Mutex::new(None),
            login_attempts: Mutex::new(Vec::new()),
        });
        let result = AssertUnwindSafe(async {
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            crate::setup::initialize(&db, &root).await.unwrap();
            let mut headers = HeaderMap::new();
            headers.insert(
                "authorization",
                "Bearer test-service-token".parse().unwrap(),
            );
            headers.insert("content-type", MEDIA.parse().unwrap());
            let code = fs::read_to_string(root.join("data/assistant/setup-code")).unwrap();
            let owner = wire::SetupOwnerRequest {
                setup_code: code,
                password: "test-password-123".into(),
            };
            let created = crate::setup::route(
                axum::extract::State(app.clone()),
                axum::extract::Path("owner".into()),
                headers.clone(),
                Ok(Bytes::from(owner.encode_to_vec())),
            )
            .await;
            assert_eq!(created.status(), StatusCode::CREATED);
            let hash: String = sqlx::query_scalar("SELECT password_hash FROM admin_account")
                .fetch_one(&db)
                .await
                .unwrap();
            let again = crate::setup::route(
                axum::extract::State(app.clone()),
                axum::extract::Path("owner".into()),
                headers.clone(),
                Ok(Bytes::from(owner.encode_to_vec())),
            )
            .await;
            assert_eq!(again.status(), StatusCode::CONFLICT);
            assert_eq!(
                hash,
                sqlx::query_scalar::<_, String>("SELECT password_hash FROM admin_account")
                    .fetch_one(&db)
                    .await
                    .unwrap()
            );
            assert!(!root.join("data/assistant/setup-code").exists());
            let request = wire::ChatSendRequest {
                text: "你好".into(),
                request_id: Uuid::new_v4().to_string(),
                ..Default::default()
            };
            let no_model = crate::http::send(
                axum::extract::State(app.clone()),
                headers.clone(),
                Ok(Bytes::from(request.encode_to_vec())),
            )
            .await;
            assert_eq!(no_model.status(), StatusCode::CONFLICT);
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM messages")
                    .fetch_one(&db)
                    .await
                    .unwrap(),
                0
            );
            let config = app.settings.resolve(draft()).unwrap();
            app.settings.commit(0, config.clone()).unwrap();
            let accepted = crate::http::send(
                axum::extract::State(app.clone()),
                headers.clone(),
                Ok(Bytes::from(request.encode_to_vec())),
            )
            .await;
            assert_eq!(accepted.status(), StatusCode::ACCEPTED);
            let bytes = axum::body::to_bytes(accepted.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let chat = wire::ChatState::decode(bytes).unwrap();
            let run = Uuid::parse_str(&chat.run_id).unwrap();
            let mut next = config.clone();
            next.model = "second-model".into();
            app.settings.commit(1, next).unwrap();
            assert_eq!(for_run(&app, run).await.unwrap().model, "test");
            sqlx::query("UPDATE runs SET status='succeeded' WHERE id=$1")
                .bind(run)
                .execute(&db)
                .await
                .unwrap();
            let request = wire::ChatSendRequest {
                text: "下一轮".into(),
                request_id: Uuid::new_v4().to_string(),
                ..Default::default()
            };
            let accepted = crate::http::send(
                axum::extract::State(app.clone()),
                headers,
                Ok(Bytes::from(request.encode_to_vec())),
            )
            .await;
            assert_eq!(accepted.status(), StatusCode::ACCEPTED);
            let bytes = axum::body::to_bytes(accepted.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let chat = wire::ChatState::decode(bytes).unwrap();
            assert_eq!(
                for_run(&app, Uuid::parse_str(&chat.run_id).unwrap())
                    .await
                    .unwrap()
                    .model,
                "second-model"
            );
            let rollback = app.settings.get(1).unwrap();
            assert_eq!(app.settings.commit(2, rollback).unwrap(), 3);
            assert_eq!(
                for_run(&app, Uuid::parse_str(&chat.run_id).unwrap())
                    .await
                    .unwrap()
                    .model,
                "second-model"
            );
        })
        .catch_unwind()
        .await;
        drop(app);
        db.close().await;
        sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .execute(&mut admin)
            .await
            .unwrap();
        fs::remove_dir_all(root).unwrap();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
