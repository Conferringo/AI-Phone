//! Agent-owned speech configuration and transport. Chat model verification is independent.
pub(crate) mod playback;
mod provider;
pub(crate) mod realtime;
use crate::*;
use sqlx::Row;

type Result<T> = std::result::Result<T, String>;
enum Failure {
    Transport(Rejection),
    Message(String),
}
impl From<Rejection> for Failure {
    fn from(value: Rejection) -> Self {
        Self::Transport(value)
    }
}
impl From<String> for Failure {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}
const CONFIGURATION: &str = "SELECT a.id,v.revision,v.configuration FROM agents a LEFT JOIN agent_voice_profiles v ON v.agent_id=a.id WHERE a.id=$1 AND a.owner_id='local-user'";

fn agent(id: &str) -> Result<Uuid> {
    Uuid::parse_str(id).map_err(|_| "Agent 标识无效".into())
}
fn field(value: &str, label: &str, limit: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        return Err(format!("{label}无效"));
    }
    Ok(value.into())
}
fn normalize(mut draft: wire::VoiceDraft) -> Result<wire::VoiceDraft> {
    if draft == wire::VoiceDraft::default() {
        return Ok(draft);
    }
    draft.base_url = settings::normalize_url(&draft.base_url, false)?;
    draft.proxy_url = if draft.proxy_url.trim().is_empty() {
        String::new()
    } else {
        settings::normalize_url(&draft.proxy_url, true)?
    };
    draft.recognition_model = field(&draft.recognition_model, "语音识别模型", 200)?;
    draft.speech_model = field(&draft.speech_model, "语音合成模型", 200)?;
    draft.voice = field(&draft.voice, "音色", 200)?;
    if !matches!(draft.input_protocol, 1..=3) {
        return Err("请选择语音输入协议".into());
    }
    draft.api_key = draft.api_key.trim().into();
    if draft.api_key.len() > 4096 || draft.api_key.chars().any(char::is_control) {
        return Err("API Key 格式无效".into());
    }
    Ok(draft)
}
fn public(id: &str, revision: u64, config: &wire::VoiceDraft) -> wire::VoiceSettings {
    wire::VoiceSettings {
        agent_id: id.into(),
        revision,
        configured: !config.recognition_model.is_empty(),
        base_url: config.base_url.clone(),
        recognition_model: config.recognition_model.clone(),
        speech_model: config.speech_model.clone(),
        voice: config.voice.clone(),
        proxy_url: config.proxy_url.clone(),
        input_protocol: config.input_protocol,
        has_api_key: !config.api_key.is_empty(),
    }
}
async fn load(app: &App, id: &str) -> Result<(u64, wire::VoiceDraft)> {
    let row = sqlx::query(CONFIGURATION)
        .bind(agent(id)?)
        .fetch_optional(&app.db)
        .await
        .map_err(|_| "语音配置读取失败")?
        .ok_or("Agent 不存在")?;
    let revision = row.get::<Option<i64>, _>("revision").unwrap_or(0) as u64;
    let draft = row
        .get::<Option<Vec<u8>>, _>("configuration")
        .map(|v| wire::VoiceDraft::decode(v.as_slice()))
        .transpose()
        .map_err(|_| "语音配置损坏")?
        .unwrap_or_default();
    Ok((revision, draft))
}
async fn save(app: &App, request: wire::VoiceSettingsSaveRequest) -> Result<wire::VoiceSettings> {
    let draft = normalize(request.draft.ok_or("缺少语音配置")?)?;
    let id = agent(&request.agent_id)?;
    let bytes = draft.encode_to_vec();
    let revision:Option<i64>=if request.expected_revision==0 {
        sqlx::query_scalar("INSERT INTO agent_voice_profiles(agent_id,revision,configuration) SELECT id,1,$2 FROM agents WHERE id=$1 AND owner_id='local-user' ON CONFLICT(agent_id) DO NOTHING RETURNING revision").bind(id).bind(bytes).fetch_optional(&app.db).await
    }else{
        sqlx::query_scalar("UPDATE agent_voice_profiles v SET revision=v.revision+1,configuration=$2 FROM agents a WHERE v.agent_id=$1 AND a.id=v.agent_id AND a.owner_id='local-user' AND v.revision=$3 RETURNING v.revision").bind(id).bind(bytes).bind(i64::try_from(request.expected_revision).map_err(|_|"语音配置版本无效")?).fetch_optional(&app.db).await
    }.map_err(|_|"语音配置保存失败")?;
    let revision = revision.ok_or("语音配置已更新，请重新载入")? as u64;
    Ok(public(&request.agent_id, revision, &draft))
}
async fn configured(app: &App, id: &str, device: &str, revision: u64) -> Result<wire::VoiceDraft> {
    let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_device_bindings b JOIN devices d ON d.id=b.device_id WHERE b.agent_id=$1 AND b.device_id=$2 AND d.owner_id='local-user')").bind(agent(id)?).bind(device).fetch_one(&app.db).await.map_err(|_|"设备配置读取失败")?;
    if !allowed {
        return Err("Agent 未绑定此设备".into());
    }
    let (actual, draft) = load(app, id).await?;
    if actual != revision {
        return Err("语音配置已更新，请重新开启 Copilot".into());
    }
    if draft.recognition_model.is_empty() {
        return Err("请先配置 Agent 的语音服务".into());
    }
    Ok(draft)
}

fn origin(app: &App, headers: &HeaderMap) -> bool {
    let value = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if matches!(value, "http://127.0.0.1:17860" | "http://localhost:17860") {
        return true;
    }
    let deployment = std::fs::read(app.root.join("data/deployment.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
    deployment
        .and_then(|d| d["origin"].as_str().map(str::to_owned))
        .is_some_and(|allowed| allowed == value)
}
fn cookie(headers: &HeaderMap) -> &str {
    headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .map(str::trim)
        .find_map(|v| v.strip_prefix("ai_phone_session="))
        .unwrap_or("")
}
async fn websocket_authorization(
    app: &App,
    headers: &HeaderMap,
) -> std::result::Result<(), Response> {
    if !origin(app, headers) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "wrong_origin",
            "请从工作台操作",
        ));
    }
    match auth::valid_session(&app.db, cookie(headers)).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "请先进入工作台",
        )),
        Err(e) => Err(db_error(e)),
    }
}

pub async fn route(
    axum::extract::State(app): axum::extract::State<Shared>,
    axum::extract::Path(operation): axum::extract::Path<String>,
    headers: HeaderMap,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Response {
    let result: std::result::Result<Response, Failure> = async {
        match operation.as_str() {
            "settings" => {
                let request = decode::<wire::VoiceSettingsRequest>(&app, &headers, body)?;
                let (revision, draft) = load(&app, &request.agent_id).await?;
                Ok(pb(
                    StatusCode::OK,
                    public(&request.agent_id, revision, &draft),
                ))
            }
            "credential" => {
                let request = decode::<wire::VoiceCredentialRequest>(&app, &headers, body)?;
                let (revision, draft) = load(&app, &request.agent_id).await?;
                if request.expected_revision != revision {
                    return Err(Failure::Message("语音配置已更新，请重新载入".into()));
                }
                Ok(pb(
                    StatusCode::OK,
                    wire::VoiceCredential {
                        api_key: draft.api_key,
                    },
                ))
            }
            "save" => {
                let request = decode::<wire::VoiceSettingsSaveRequest>(&app, &headers, body)?;
                Ok(pb(StatusCode::OK, save(&app, request).await?))
            }
            "transcribe" => {
                let request = decode::<wire::VoiceTranscribeRequest>(&app, &headers, body)?;
                let draft = configured(
                    &app,
                    &request.agent_id,
                    &request.device_id,
                    request.configuration_revision,
                )
                .await?;
                let client = app.settings.http_client(&draft.proxy_url)?;
                Ok(pb(
                    StatusCode::OK,
                    provider::transcribe(&client, &draft, request).await?,
                ))
            }
            _ => Ok(error(StatusCode::NOT_FOUND, "not_found", "操作不存在")),
        }
    }
    .await;
    match result {
        Ok(value) => value,
        Err(Failure::Transport(rejection)) => rejection.into(),
        Err(Failure::Message(message)) => error(StatusCode::BAD_REQUEST, "voice_error", &message),
    }
}

#[cfg(test)]
mod tests;
