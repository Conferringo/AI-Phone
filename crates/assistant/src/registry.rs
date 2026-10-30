//! Persistent configuration registry. Runtime creation belongs to provisioning, not registration.
use crate::*;
use sqlx::{Postgres, Row, Transaction};

const OWNER: &str = "local-user";
pub const PHONE: &str = "phone-local-01";
const MODEL: Uuid = Uuid::from_u128(1);
const AGENT: Uuid = Uuid::from_u128(2);
const LOCK: i64 = 704117;

#[derive(Debug)]
struct Failure {
    status: StatusCode,
    code: &'static str,
    message: String,
}
type Result<T> = std::result::Result<T, Failure>;
fn invalid(message: &str) -> Failure {
    Failure {
        status: StatusCode::BAD_REQUEST,
        code: "invalid_configuration",
        message: message.into(),
    }
}
fn conflict(message: &str) -> Failure {
    Failure {
        status: StatusCode::CONFLICT,
        code: "configuration_conflict",
        message: message.into(),
    }
}
fn storage(_: sqlx::Error) -> Failure {
    Failure {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "storage_error",
        message: "配置保存失败，请重试".into(),
    }
}
fn uuid(id: &str) -> Result<Uuid> {
    Uuid::parse_str(id).map_err(|_| invalid("配置标识无效"))
}
fn name(raw: &str) -> Result<String> {
    let value = raw.trim();
    if value.is_empty() || value.chars().count() > 80 || value.chars().any(char::is_control) {
        return Err(invalid("名称需要 1–80 个字符"));
    }
    Ok(value.into())
}
fn expected(actual: Option<i64>, revision: u64) -> Result<()> {
    if actual.map(|v| v as u64).unwrap_or(0) != revision {
        return Err(conflict("配置已在另一处修改，请重新载入后再保存"));
    }
    Ok(())
}

// All short registry mutations and run snapshots take the same transaction lock.
// External model probes happen before this lock, with a revision recheck afterwards.
async fn bootstrap(
    tx: &mut Transaction<'_, Postgres>,
    settings: &settings::SettingsStore,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(LOCK)
        .execute(&mut **tx)
        .await?;
    let legacy: Option<i64> =
        sqlx::query_scalar("SELECT legacy_revision FROM registry_meta WHERE owner_id=$1")
            .bind(OWNER)
            .fetch_optional(&mut **tx)
            .await?;
    if legacy.is_none() {
        sqlx::query(
            "INSERT INTO model_connections(id,name,model_revision) VALUES($1,'默认模型',$2)",
        )
        .bind(MODEL)
        .bind(settings.current() as i64)
        .execute(&mut **tx)
        .await?;
        sqlx::query("INSERT INTO agents(id,name,model_connection_id) VALUES($1,'默认助手',$2)")
            .bind(AGENT)
            .bind(MODEL)
            .execute(&mut **tx)
            .await?;
        sqlx::query("INSERT INTO devices(id,name,kind,system,runtime_state,default_agent_id) VALUES($1,'我的手机','phone','Android 15 · ARM64','managed',$2)").bind(PHONE).bind(AGENT).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO agent_device_bindings(agent_id,device_id) VALUES($1,$2)")
            .bind(AGENT)
            .bind(PHONE)
            .execute(&mut **tx)
            .await?;
        sqlx::query("INSERT INTO registry_meta(owner_id,legacy_revision) VALUES($1,$2)")
            .bind(OWNER)
            .bind(settings.current() as i64)
            .execute(&mut **tx)
            .await?;
    } else if legacy != Some(settings.current() as i64) {
        // Compatibility for setup and old settings clients. Never replace device/Agent bindings.
        sqlx::query("UPDATE model_connections SET model_revision=$2,revision=revision+1,updated_at=now() WHERE id=$1 AND owner_id=$3").bind(MODEL).bind(settings.current() as i64).bind(OWNER).execute(&mut **tx).await?;
        sqlx::query("UPDATE registry_meta SET legacy_revision=$2 WHERE owner_id=$1")
            .bind(OWNER)
            .bind(settings.current() as i64)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
pub async fn initialize(
    db: &PgPool,
    settings: &settings::SettingsStore,
) -> std::result::Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    bootstrap(&mut tx, settings).await?;
    // Only annotate legacy tasks. Their original model revision and context stay intact.
    sqlx::query("UPDATE runs SET agent_id=$1,agent_revision=1,agent_name='默认助手',model_connection_id=$2 WHERE agent_id IS NULL")
        .bind(AGENT).bind(MODEL).execute(&mut *tx).await?;
    sqlx::query("UPDATE memories SET agent_id=$1 WHERE agent_id IS NULL")
        .bind(AGENT)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}
pub async fn configured(app: &App) -> bool {
    sqlx::query_scalar::<_,bool>("SELECT CASE WHEN EXISTS(SELECT 1 FROM registry_meta WHERE owner_id=$2) THEN EXISTS(SELECT 1 FROM devices d JOIN agents a ON a.id=d.default_agent_id JOIN model_connections m ON m.id=a.model_connection_id WHERE d.id=$1 AND d.owner_id=$2 AND m.model_revision>0) ELSE $3::bigint>0 END")
        .bind(PHONE).bind(OWNER).bind(app.settings.current() as i64).fetch_one(&app.db).await.unwrap_or(false)
}
pub async fn pin_run(
    tx: &mut Transaction<'_, Postgres>,
    settings: &settings::SettingsStore,
    run: Uuid,
) -> std::result::Result<(), sqlx::Error> {
    pin_run_for(tx, settings, run, "").await
}
pub async fn pin_run_for(
    tx: &mut Transaction<'_, Postgres>,
    settings: &settings::SettingsStore,
    run: Uuid,
    agent: &str,
) -> std::result::Result<(), sqlx::Error> {
    bootstrap(tx, settings).await?;
    let device:String=sqlx::query_scalar("SELECT c.device_id FROM runs r JOIN conversations c ON c.id=r.conversation_id WHERE r.id=$1").bind(run).fetch_one(&mut **tx).await?;
    if device != PHONE && !sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM device_runtimes r JOIN devices d ON d.id=r.device_id WHERE d.id=$1 AND d.runtime_state='managed')").bind(&device).fetch_one(&mut **tx).await? {
        return Err(sqlx::Error::Protocol("device_unavailable".into()));
    }
    let selected = if agent.is_empty() {
        None
    } else {
        Some(Uuid::parse_str(agent).map_err(|_| sqlx::Error::Protocol("invalid_agent".into()))?)
    };
    let row = sqlx::query("SELECT a.id,a.revision,a.name,a.instructions,a.model_connection_id,m.model_revision FROM devices d JOIN agent_device_bindings b ON b.device_id=d.id AND b.agent_id=COALESCE($3,d.default_agent_id) JOIN agents a ON a.id=b.agent_id JOIN model_connections m ON m.id=a.model_connection_id WHERE d.id=$1 AND d.owner_id=$2 AND a.owner_id=$2 AND m.owner_id=$2")
        .bind(&device).bind(OWNER).bind(selected).fetch_optional(&mut **tx).await?
        .ok_or_else(|| sqlx::Error::Protocol("model_missing".into()))?;
    let revision: i64 = row.get("model_revision");
    if revision == 0 || settings.get(revision as u64).is_err() {
        return Err(sqlx::Error::Protocol("model_missing".into()));
    }
    sqlx::query("UPDATE runs SET agent_id=$2,agent_revision=$3,agent_name=$4,agent_instructions=$5,model_connection_id=$6,model_revision=$7,device_id=$8 WHERE id=$1")
        .bind(run).bind(row.get::<Uuid,_>("id")).bind(row.get::<i64,_>("revision")).bind(row.get::<String,_>("name")).bind(row.get::<String,_>("instructions"))
        .bind(row.get::<Uuid,_>("model_connection_id")).bind(revision).bind(&device).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO run_device_bindings(run_id,device_id) SELECT $1,b.device_id FROM agent_device_bindings b JOIN runs r ON r.agent_id=b.agent_id WHERE r.id=$1 ON CONFLICT DO NOTHING").bind(run).execute(&mut **tx).await?;
    Ok(())
}
fn device_limit(kind: &str) -> u32 {
    let presets: serde_json::Value =
        serde_json::from_str(include_str!("../../../config/device-presets.json"))
            .expect("valid bundled presets");
    presets["limits"][kind].as_u64().unwrap_or(1).clamp(1, 8) as u32
}
async fn state(app: &App) -> Result<wire::RegistryState> {
    let mut tx = app.db.begin().await.map_err(storage)?;
    bootstrap(&mut tx, &app.settings).await.map_err(storage)?;
    let devices = sqlx::query("SELECT id,revision,name,kind,system,runtime_state,default_agent_id FROM devices WHERE owner_id=$1 AND runtime_state<>'archived' ORDER BY created_at,id").bind(OWNER).fetch_all(&mut *tx).await.map_err(storage)?;
    let agents = sqlx::query("SELECT id,revision,name,model_connection_id,instructions FROM agents WHERE owner_id=$1 ORDER BY created_at,id").bind(OWNER).fetch_all(&mut *tx).await.map_err(storage)?;
    let bindings = sqlx::query("SELECT b.agent_id,b.device_id FROM agent_device_bindings b JOIN agents a ON a.id=b.agent_id WHERE a.owner_id=$1 ORDER BY b.device_id").bind(OWNER).fetch_all(&mut *tx).await.map_err(storage)?;
    let rows = sqlx::query("SELECT id,revision,name,model_revision FROM model_connections WHERE owner_id=$1 ORDER BY created_at,id").bind(OWNER).fetch_all(&mut *tx).await.map_err(storage)?;
    let mut models = Vec::new();
    for row in rows {
        let version: i64 = row.get("model_revision");
        let profile = if version == 0 {
            None
        } else {
            Some(
                app.settings
                    .get(version as u64)
                    .map_err(|m| conflict(&m))?
                    .profile(version as u64),
            )
        };
        models.push(wire::RegisteredModel {
            id: row.get::<Uuid, _>("id").to_string(),
            revision: row.get::<i64, _>("revision") as u64,
            name: row.get("name"),
            profile,
        });
    }
    let result = wire::RegistryState {
        active_device_id: PHONE.into(),
        phone_limit: device_limit("phone"),
        desktop_limit: device_limit("desktop"),
        models,
        devices: devices
            .iter()
            .map(|r| wire::RegisteredDevice {
                id: r.get("id"),
                revision: r.get::<i64, _>("revision") as u64,
                name: r.get("name"),
                kind: r.get("kind"),
                system: r.get("system"),
                runtime_state: r.get("runtime_state"),
                default_agent_id: r
                    .get::<Option<Uuid>, _>("default_agent_id")
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            })
            .collect(),
        agents: agents
            .iter()
            .map(|r| {
                let id: Uuid = r.get("id");
                wire::RegisteredAgent {
                    id: id.to_string(),
                    revision: r.get::<i64, _>("revision") as u64,
                    name: r.get("name"),
                    model_connection_id: r.get::<Uuid, _>("model_connection_id").to_string(),
                    instructions: r.get("instructions"),
                    device_ids: bindings
                        .iter()
                        .filter(|b| b.get::<Uuid, _>("agent_id") == id)
                        .map(|b| b.get("device_id"))
                        .collect(),
                }
            })
            .collect(),
    };
    tx.commit().await.map_err(storage)?;
    Ok(result)
}
async fn save_device(app: &App, r: wire::DeviceSaveRequest) -> Result<()> {
    if r.id != PHONE {
        uuid(&r.id)?;
    }
    let label = name(&r.name)?;
    if !["phone", "desktop"].contains(&r.kind.as_str()) {
        return Err(invalid("设备类型无效"));
    }
    let agent = if r.default_agent_id.is_empty() {
        None
    } else {
        Some(uuid(&r.default_agent_id)?)
    };
    let mut tx = app.db.begin().await.map_err(storage)?;
    bootstrap(&mut tx, &app.settings).await.map_err(storage)?;
    let old = sqlx::query("SELECT revision,kind FROM devices WHERE id=$1 AND owner_id=$2 AND runtime_state<>'archived'")
        .bind(&r.id)
        .bind(OWNER)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
    expected(old.as_ref().map(|v| v.get("revision")), r.expected_revision)?;
    if old
        .as_ref()
        .is_some_and(|v| v.get::<String, _>("kind") != r.kind)
    {
        return Err(invalid("已有设备不能变更系统类型"));
    }
    if r.id == PHONE && agent.is_none() {
        return Err(conflict("请为当前手机选择默认 Agent"));
    }
    if let Some(agent) = agent {
        let row=sqlx::query("SELECT m.model_revision FROM agents a JOIN model_connections m ON m.id=a.model_connection_id WHERE a.id=$1 AND a.owner_id=$2 AND m.owner_id=$2").bind(agent).bind(OWNER).fetch_optional(&mut *tx).await.map_err(storage)?.ok_or_else(||invalid("Agent 不存在"))?;
        if r.id == PHONE && row.get::<i64, _>("model_revision") == 0 {
            return Err(conflict("请先为该 Agent 连接并验证模型"));
        }
    }
    if old.is_none() {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM devices WHERE owner_id=$1 AND kind=$2 AND runtime_state<>'archived'")
                .bind(OWNER)
                .bind(&r.kind)
                .fetch_one(&mut *tx)
                .await
                .map_err(storage)?;
        if count >= device_limit(&r.kind) as i64 {
            return Err(conflict("已达到此类型设备的数量上限"));
        }
        sqlx::query("INSERT INTO devices(id,name,kind,system,runtime_state,default_agent_id) VALUES($1,$2,$3,$4,'registered',$5)")
            .bind(&r.id).bind(label).bind(&r.kind).bind(if r.kind=="phone"{"Android · 待安装"}else{"Linux · 待安装"}).bind(agent).execute(&mut *tx).await.map_err(storage)?;
    } else {
        sqlx::query("UPDATE devices SET name=$2,default_agent_id=$3,revision=revision+1,updated_at=now() WHERE id=$1").bind(&r.id).bind(label).bind(agent).execute(&mut *tx).await.map_err(storage)?;
    }
    if let Some(agent) = agent {
        let added=sqlx::query("INSERT INTO agent_device_bindings(agent_id,device_id) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(agent).bind(&r.id).execute(&mut *tx).await.map_err(storage)?.rows_affected();
        if added > 0 {
            sqlx::query("UPDATE agents SET revision=revision+1,updated_at=now() WHERE id=$1")
                .bind(agent)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
        }
    }
    tx.commit().await.map_err(storage)
}
async fn save_agent(app: &App, r: wire::AgentSaveRequest) -> Result<()> {
    let mut tx = app.db.begin().await.map_err(storage)?;
    bootstrap(&mut tx, &app.settings).await.map_err(storage)?;
    save_agent_in(app, &mut tx, r).await?;
    tx.commit().await.map_err(storage)
}
async fn save_agent_in(
    _app: &App,
    tx: &mut Transaction<'_, Postgres>,
    r: wire::AgentSaveRequest,
) -> Result<()> {
    let id = uuid(&r.id)?;
    let model = uuid(&r.model_connection_id)?;
    let label = name(&r.name)?;
    if r.instructions.len() > 8000 || r.instructions.contains('\0') || r.device_ids.len() > 64 {
        return Err(invalid("Agent 配置过大"));
    }
    let mut wanted = r.device_ids;
    wanted.sort();
    wanted.dedup();
    let old: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM agents WHERE id=$1 AND owner_id=$2")
            .bind(id)
            .bind(OWNER)
            .fetch_optional(&mut **tx)
            .await
            .map_err(storage)?;
    expected(old, r.expected_revision)?;
    let model_revision: Option<i64> = sqlx::query_scalar(
        "SELECT model_revision FROM model_connections WHERE id=$1 AND owner_id=$2",
    )
    .bind(model)
    .bind(OWNER)
    .fetch_optional(&mut **tx)
    .await
    .map_err(storage)?;
    let model_revision = model_revision.ok_or_else(|| invalid("模型连接不存在"))?;
    let devices: Vec<String> =
        sqlx::query_scalar("SELECT id FROM devices WHERE id=ANY($1) AND owner_id=$2")
            .bind(&wanted)
            .bind(OWNER)
            .fetch_all(&mut **tx)
            .await
            .map_err(storage)?;
    if devices.len() != wanted.len() {
        return Err(invalid("设备不存在"));
    }
    let current_default: Option<Uuid> =
        sqlx::query_scalar("SELECT default_agent_id FROM devices WHERE id=$1")
            .bind(PHONE)
            .fetch_one(&mut **tx)
            .await
            .map_err(storage)?;
    if current_default == Some(id) && (!wanted.iter().any(|d| d == PHONE) || model_revision == 0) {
        return Err(conflict("请先为当前手机选择其他默认 Agent，并保留可用模型"));
    }
    if old.is_none() {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM agents WHERE owner_id=$1")
            .bind(OWNER)
            .fetch_one(&mut **tx)
            .await
            .map_err(storage)?;
        if count >= 64 {
            return Err(conflict("Agent 配置已达 64 个"));
        }
        sqlx::query(
            "INSERT INTO agents(id,name,model_connection_id,instructions) VALUES($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(label)
        .bind(model)
        .bind(&r.instructions)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    } else {
        sqlx::query("UPDATE agents SET name=$2,model_connection_id=$3,instructions=$4,revision=revision+1,updated_at=now() WHERE id=$1").bind(id).bind(label).bind(model).bind(&r.instructions).execute(&mut **tx).await.map_err(storage)?;
    }
    sqlx::query("UPDATE devices SET default_agent_id=NULL,revision=revision+1,updated_at=now() WHERE default_agent_id=$1 AND NOT(id=ANY($2))").bind(id).bind(&wanted).execute(&mut **tx).await.map_err(storage)?;
    sqlx::query("DELETE FROM agent_device_bindings WHERE agent_id=$1 AND NOT(device_id=ANY($2))")
        .bind(id)
        .bind(&wanted)
        .execute(&mut **tx)
        .await
        .map_err(storage)?;
    sqlx::query("INSERT INTO agent_device_bindings(agent_id,device_id) SELECT $1,unnest($2::text[]) ON CONFLICT DO NOTHING").bind(id).bind(&wanted).execute(&mut **tx).await.map_err(storage)?;
    Ok(())
}
async fn model_version(app: &App, id: Uuid, revision: u64) -> Result<u64> {
    let row =
        sqlx::query("SELECT revision,model_revision,owner_id FROM model_connections WHERE id=$1")
            .bind(id)
            .fetch_optional(&app.db)
            .await
            .map_err(storage)?;
    if row
        .as_ref()
        .is_some_and(|r| r.get::<String, _>("owner_id") != OWNER)
    {
        return Err(invalid("模型连接不存在"));
    }
    expected(row.as_ref().map(|r| r.get("revision")), revision)?;
    Ok(row
        .map(|r| r.get::<i64, _>("model_revision") as u64)
        .unwrap_or(0))
}
async fn save_model(app: &App, r: wire::ConnectionSaveRequest) -> Result<()> {
    let mut tx = app.db.begin().await.map_err(storage)?;
    bootstrap(&mut tx, &app.settings).await.map_err(storage)?;
    save_model_in(app, &mut tx, r).await?;
    tx.commit().await.map_err(storage)
}
async fn save_model_in(
    app: &App,
    tx: &mut Transaction<'_, Postgres>,
    r: wire::ConnectionSaveRequest,
) -> Result<()> {
    let id = uuid(&r.id)?;
    let label = name(&r.name)?;
    let row =
        sqlx::query("SELECT revision,model_revision,owner_id FROM model_connections WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(storage)?;
    if row
        .as_ref()
        .is_some_and(|r| r.get::<String, _>("owner_id") != OWNER)
    {
        return Err(invalid("模型连接不存在"));
    }
    expected(row.as_ref().map(|r| r.get("revision")), r.expected_revision)?;
    let previous = row
        .as_ref()
        .map(|r| r.get::<i64, _>("model_revision") as u64)
        .unwrap_or(0);
    if row.is_none() {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM model_connections WHERE owner_id=$1")
                .bind(OWNER)
                .fetch_one(&mut **tx)
                .await
                .map_err(storage)?;
        if count >= 64 {
            return Err(conflict("模型连接已达 64 个"));
        }
    }
    let version = if let Some(draft) = r.draft {
        let config = app
            .settings
            .resolve_at(draft, previous)
            .map_err(|m| invalid(&m))?;
        app.settings
            .save_connection(&r.id, r.expected_revision, &r.verification_id, config)
            .map_err(|m| conflict(&m))?
    } else {
        if row.is_none() {
            return Err(invalid("缺少模型配置"));
        }
        previous
    };
    if row.is_none() {
        sqlx::query("INSERT INTO model_connections(id,name,model_revision) VALUES($1,$2,$3)")
            .bind(id)
            .bind(label)
            .bind(version as i64)
            .execute(&mut **tx)
            .await
            .map_err(storage)?;
    } else {
        sqlx::query("UPDATE model_connections SET name=$2,model_revision=$3,revision=revision+1,updated_at=now() WHERE id=$1 AND owner_id=$4")
            .bind(id).bind(label).bind(version as i64).bind(OWNER).execute(&mut **tx).await.map_err(storage)?;
    }
    Ok(())
}

async fn save_configuration(app: &App, input: wire::AgentConfigurationSaveRequest) -> Result<()> {
    let agent = input.agent.ok_or_else(|| invalid("缺少 Agent 配置"))?;
    let mut tx = app.db.begin().await.map_err(storage)?;
    bootstrap(&mut tx, &app.settings).await.map_err(storage)?;
    if let Some(connection) = input.connection {
        if connection.id != agent.model_connection_id {
            return Err(invalid("模型归属不匹配"));
        }
        let others: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM agents WHERE model_connection_id=$1 AND id<>$2 AND owner_id=$3",
        )
        .bind(uuid(&connection.id)?)
        .bind(uuid(&agent.id)?)
        .bind(OWNER)
        .fetch_one(&mut *tx)
        .await
        .map_err(storage)?;
        if others > 0 {
            return Err(conflict("该连接正在被其他 Agent 使用，请复制连接后编辑"));
        }
        save_model_in(app, &mut tx, connection).await?;
    }
    save_agent_in(app, &mut tx, agent).await?;
    tx.commit().await.map_err(storage)
}
// Empty scope is only for older single-Agent clients.
pub async fn memory_agent(app: &App, id: &str) -> std::result::Result<Uuid, String> {
    let id = if id.is_empty() {
        sqlx::query_scalar::<_, Option<Uuid>>(
            "SELECT default_agent_id FROM devices WHERE id=$1 AND owner_id=$2",
        )
        .bind(PHONE)
        .bind(OWNER)
        .fetch_optional(&app.db)
        .await
        .map_err(|e| e.to_string())?
        .flatten()
        .unwrap_or(AGENT)
    } else {
        Uuid::parse_str(id).map_err(|_| "Agent 不存在")?
    };
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agents WHERE id=$1 AND owner_id=$2)")
            .bind(id)
            .bind(OWNER)
            .fetch_one(&app.db)
            .await
            .map_err(|e| e.to_string())?;
    if !exists {
        return Err("Agent 不存在".into());
    }
    Ok(id)
}

pub async fn route(
    axum::extract::State(app): axum::extract::State<Shared>,
    axum::extract::Path(operation): axum::extract::Path<String>,
    headers: HeaderMap,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Response {
    // Authenticate before parsing any operation or disclosing registry existence.
    if let Err(e) = auth(&app, &headers) {
        return e.into();
    }
    macro_rules! read {
        ($t:ty) => {
            match decode::<$t>(&app, &headers, body) {
                Ok(r) => r,
                Err(e) => return Ok(e.into()),
            }
        };
    }
    let result: Result<Response> = async {
        match operation.as_str() {
            "state" => {
                read!(wire::RegistryRequest);
            }
            "device-save" => save_device(&app, read!(wire::DeviceSaveRequest)).await?,
            "agent-config-save" => {
                save_configuration(&app, read!(wire::AgentConfigurationSaveRequest)).await?
            }
            "agent-save" => save_agent(&app, read!(wire::AgentSaveRequest)).await?,
            "model-save" => save_model(&app, read!(wire::ConnectionSaveRequest)).await?,
            "model-test" => {
                let r = read!(wire::ConnectionTestRequest);
                let id = uuid(&r.id)?;
                let version = model_version(&app, id, r.expected_revision).await?;
                let config = app
                    .settings
                    .resolve_at(r.draft.ok_or_else(|| invalid("缺少模型配置"))?, version)
                    .map_err(|m| invalid(&m))?;
                let result = app
                    .settings
                    .test_connection(&r.id, r.expected_revision, &config)
                    .await
                    .map_err(|m| conflict(&m))?;
                model_version(&app, id, r.expected_revision).await?;
                return Ok(pb(StatusCode::OK, result));
            }
            "credential" => {
                let r = read!(wire::ConnectionCredentialRequest);
                let version = model_version(&app, uuid(&r.id)?, r.expected_revision).await?;
                if version == 0 {
                    return Err(invalid("尚未保存模型连接"));
                }
                let config = app.settings.get(version).map_err(|m| conflict(&m))?;
                return Ok(pb(
                    StatusCode::OK,
                    wire::ModelCredential {
                        revision: r.expected_revision,
                        api_key: config.api_key,
                    },
                ));
            }
            _ => return Ok(error(StatusCode::NOT_FOUND, "not_found", "操作不存在")),
        }
        Ok(pb(StatusCode::OK, state(&app).await?))
    }
    .await;
    result.unwrap_or_else(|e| error(e.status, e.code, &e.message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use serde_json::{Value, json};
    use std::{panic::AssertUnwindSafe, str::FromStr, sync::Mutex as StdMutex};

    #[test]
    fn names_and_versions_reject_invalid_or_stale_writes() {
        assert!(name(" \n ").is_err());
        assert!(name(&"界".repeat(81)).is_err());
        assert_eq!(name(" 工作手机 ").unwrap(), "工作手机");
        assert!(expected(Some(2), 1).is_err());
        assert!(expected(None, 1).is_err());
        assert!(expected(None, 0).is_ok());
    }
    async fn call<T: Message>(app: &Shared, operation: &str, input: T) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer registry-test".parse().unwrap());
        headers.insert("content-type", MEDIA.parse().unwrap());
        route(
            axum::extract::State(app.clone()),
            axum::extract::Path(operation.into()),
            headers,
            Ok(Bytes::from(input.encode_to_vec())),
        )
        .await
    }
    async fn response<T: Message + Default>(r: Response) -> T {
        assert!(
            r.status().is_success(),
            "unexpected response status {}",
            r.status()
        );
        T::decode(
            axum::body::to_bytes(r.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap()
    }
    async fn check_model(
        app: &Shared,
        id: Uuid,
        expected: u64,
        draft: wire::ModelDraft,
    ) -> wire::ModelTestResult {
        let result: wire::ModelTestResult = response(
            call(
                app,
                "model-test",
                wire::ConnectionTestRequest {
                    id: id.to_string(),
                    expected_revision: expected,
                    draft: Some(draft),
                },
            )
            .await,
        )
        .await;
        assert!(result.ready, "mock provider capability checks must pass");
        result
    }
    async fn send(app: &Shared, text: &str) -> Uuid {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer registry-test".parse().unwrap());
        headers.insert("content-type", MEDIA.parse().unwrap());
        let input = wire::ChatSendRequest {
            text: text.into(),
            request_id: Uuid::new_v4().to_string(),
            ..Default::default()
        };
        let chat: wire::ChatState = response(
            crate::http::send(
                axum::extract::State(app.clone()),
                headers,
                Ok(Bytes::from(input.encode_to_vec())),
            )
            .await,
        )
        .await;
        Uuid::parse_str(&chat.run_id).unwrap()
    }
    #[tokio::test]
    #[ignore = "requires AI_PHONE_TEST_DATABASE_URL; isolated database and loopback mock provider, no production writes"]
    async fn registry_migration_binding_versions_and_runtime_pinning() {
        use sqlx::Connection;
        let url = std::env::var("AI_PHONE_TEST_DATABASE_URL").expect("test database URL");
        let name = format!("aiphone_registry_test_{}", Uuid::new_v4().simple());
        let mut admin = sqlx::PgConnection::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut admin)
            .await
            .unwrap();
        let db = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(PgConnectOptions::from_str(&url).unwrap().database(&name))
            .await
            .unwrap();
        let root = std::env::temp_dir().join(&name);
        let app = Arc::new(App {
            db: db.clone(),
            root: root.clone(),
            token: "registry-test".into(),
            phone_token: "unused".into(),
            settings: settings::SettingsStore::test_configured(&root),
            local: reqwest::Client::new(),
            remote: reqwest::Client::new(),
            active: Mutex::new(None),
            login_attempts: Mutex::new(vec![]),
        });
        let seen = Arc::new(StdMutex::new(Vec::<String>::new()));
        let seen_clone = seen.clone();
        let mock=Router::new().route("/responses",post(move |axum::Json(body):axum::Json<Value>|{let seen=seen_clone.clone();async move {
            seen.lock().unwrap().push(body["model"].as_str().unwrap_or("").into());
            let input=body["input"].as_array().unwrap();
            let output=if let Some(result)=input.iter().find(|v|v["type"]=="function_call_output"){
                let result:Value=serde_json::from_str(result["output"].as_str().unwrap()).unwrap();json!([{"type":"message","content":[{"type":"output_text","text":result["receipt"]}]}])
            }else if body.get("tools").is_some(){
                let text=input[0]["content"][0]["text"].as_str().unwrap();let nonce=text.split("code ").nth(1).unwrap().split('.').next().unwrap();
                json!([{"type":"function_call","name":"diagnostic_echo","call_id":"mock-call","arguments":json!({"code":nonce}).to_string()}])
            }else{
                let content=input[0]["content"].as_array().unwrap();let text=if content.iter().any(|v|v["type"]=="input_image"){"7319"}else{content[0]["text"].as_str().unwrap().split(": ").last().unwrap()};
                json!([{"type":"message","content":[{"type":"output_text","text":text}]}])
            };
            ([("content-type","text/event-stream")],format!("data: {}\n\n",json!({"type":"response.completed","response":{"output":output}})))
        }}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
        let outcome=AssertUnwindSafe(async{
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            let conversation:Uuid=sqlx::query_scalar("SELECT id FROM conversations WHERE owner_id='local-user'").fetch_one(&db).await.unwrap();
            let legacy_message=Uuid::now_v7();let legacy_run=Uuid::now_v7();
            sqlx::query("INSERT INTO messages(id,conversation_id,role,text) VALUES($1,$2,'assistant','existing conversation stays')").bind(legacy_message).bind(conversation).execute(&db).await.unwrap();
            sqlx::query("INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,model_revision,context) VALUES($1,$2,$3,$1,'legacy','succeeded',1,'[{\"legacy\":true}]')").bind(legacy_run).bind(conversation).bind(legacy_message).execute(&db).await.unwrap();
            initialize(&db,&app.settings).await.unwrap();initialize(&db,&app.settings).await.unwrap();
            let initial=state(&app).await.unwrap();assert_eq!(initial.devices.len(),1);assert_eq!(initial.agents.len(),1);assert_eq!(initial.models.len(),1);assert_eq!(initial.models[0].revision,1);
            assert_eq!(initial.devices[0].id,PHONE);
            assert_eq!(sqlx::query_scalar::<_,String>("SELECT text FROM messages WHERE id=$1").bind(legacy_message).fetch_one(&db).await.unwrap(),"existing conversation stays");
            assert_eq!(sqlx::query_scalar::<_,Value>("SELECT context FROM runs WHERE id=$1").bind(legacy_run).fetch_one(&db).await.unwrap(),json!([{"legacy":true}]));
            let unauthorized=route(axum::extract::State(app.clone()),axum::extract::Path("credential".into()),HeaderMap::new(),Ok(Bytes::new())).await;assert_eq!(unauthorized.status(),StatusCode::UNAUTHORIZED);
            let model=Uuid::new_v4();let other=Uuid::new_v4();
            let draft=wire::ModelDraft{provider:"compatible".into(),protocol:"responses".into(),base_url:format!("http://{address}"),model:"alpha".into(),api_key:"key-alpha".into(),..Default::default()};
            let request=wire::ConnectionSaveRequest{id:model.to_string(),expected_revision:0,name:"办公模型".into(),draft:Some(draft.clone()),verification_id:String::new()};
            assert_eq!(call(&app,"model-save",request.clone()).await.status(),StatusCode::CONFLICT);
            let foreign = Uuid::new_v4();
            sqlx::query("INSERT INTO model_connections(id,owner_id,name,model_revision) VALUES($1,'other-owner','foreign',1)").bind(foreign).execute(&db).await.unwrap();
            assert!(model_version(&app,foreign,0).await.is_err(),"foreign connection cannot be treated as creation");
            assert!(save_model(&app,wire::ConnectionSaveRequest{id:foreign.to_string(),name:"attempt".into(),..Default::default()}).await.is_err());
            let checked=check_model(&app,model,0,draft.clone()).await;
            let mut wrong=request.clone();wrong.id=other.to_string();wrong.verification_id=checked.verification_id.clone();assert_eq!(call(&app,"model-save",wrong).await.status(),StatusCode::CONFLICT,"test token is connection-scoped");
            let mut wrong=request.clone();wrong.draft.as_mut().unwrap().api_key="different".into();wrong.verification_id=checked.verification_id.clone();assert_eq!(call(&app,"model-save",wrong).await.status(),StatusCode::CONFLICT,"changed draft invalidates test");
            let mut request=request;request.verification_id=checked.verification_id;
            let listed:wire::RegistryState=response(call(&app,"model-save",request).await).await;
            assert!(!format!("{listed:?}").contains("key-alpha"),"list never contains secrets");
            let version=listed.models.iter().find(|m|m.id==model.to_string()).unwrap().profile.as_ref().unwrap().revision;
            let mut keep=draft.clone();keep.api_key.clear();keep.use_saved_key=true;assert_eq!(app.settings.resolve_at(keep.clone(),version).unwrap().api_key,"key-alpha");keep.base_url="https://example.invalid".into();assert!(app.settings.resolve_at(keep,version).is_err());
            let credential:wire::ModelCredential=response(call(&app,"credential",wire::ConnectionCredentialRequest{id:model.to_string(),expected_revision:1}).await).await;assert_eq!(credential.api_key,"key-alpha");
            assert_eq!(call(&app,"credential",wire::ConnectionCredentialRequest{id:model.to_string(),expected_revision:0}).await.status(),StatusCode::CONFLICT);
            let desktop=Uuid::new_v4().to_string();save_device(&app,wire::DeviceSaveRequest{id:desktop.clone(),name:"办公电脑".into(),kind:"desktop".into(),..Default::default()}).await.unwrap();
            let agent=Uuid::new_v4();let agent_request=wire::AgentSaveRequest{id:agent.to_string(),name:"办公助手".into(),model_connection_id:model.to_string(),device_ids:vec![PHONE.into(),desktop.clone()],instructions:"报告进度".into(),..Default::default()};
            save_agent(&app,agent_request.clone()).await.unwrap();
            save_device(&app,wire::DeviceSaveRequest{id:PHONE.into(),expected_revision:1,name:"我的手机".into(),kind:"phone".into(),default_agent_id:agent.to_string()}).await.unwrap();
            let before=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM messages").fetch_one(&db).await.unwrap();
            let run=send(&app,"first task").await;
            let pinned=sqlx::query("SELECT agent_id,agent_name,agent_instructions,device_id,model_revision FROM runs WHERE id=$1").bind(run).fetch_one(&db).await.unwrap();
            assert_eq!(pinned.get::<Uuid,_>("agent_id"),agent);assert_eq!(pinned.get::<String,_>("device_id"),PHONE);assert_eq!(pinned.get::<i64,_>("model_revision"),version as i64);
            let mut beta=draft.clone();beta.model="beta".into();beta.api_key="key-beta".into();let checked=check_model(&app,model,1,beta.clone()).await;
            let _:wire::RegistryState=response(call(&app,"model-save",wire::ConnectionSaveRequest{id:model.to_string(),expected_revision:1,name:"办公模型".into(),draft:Some(beta),verification_id:checked.verification_id}).await).await;
            let mut changed_agent=agent_request;changed_agent.expected_revision=1;changed_agent.name="新版助手".into();changed_agent.instructions="新的工作偏好".into();save_agent(&app,changed_agent.clone()).await.unwrap();
            assert_eq!(settings::for_run(&app,run).await.unwrap().model,"alpha");
            assert_eq!(sqlx::query_scalar::<_,String>("SELECT agent_name FROM runs WHERE id=$1").bind(run).fetch_one(&db).await.unwrap(),"办公助手");
            let steered=send(&app,"additional instruction").await;assert_eq!(steered,run);assert_eq!(settings::for_run(&app,run).await.unwrap().model,"alpha","steering cannot switch model midway");
            sqlx::query("UPDATE runs SET status='succeeded' WHERE id=$1").bind(run).execute(&db).await.unwrap();
            let next=send(&app,"second task").await;let config=settings::for_run(&app,next).await.unwrap();assert_eq!(config.model,"beta");assert_eq!(config.api_key,"key-beta");
            crate::model_adapter::request(&config,json!({"input":[{"role":"user","content":[{"type":"input_text","text":"Reply exactly with this code: runtime-proof"}]}]}),&CancellationToken::new()).await.unwrap();
            assert_eq!(seen.lock().unwrap().last().unwrap(),"beta");
            changed_agent.expected_revision=2;changed_agent.device_ids=vec![desktop.clone()];assert!(save_agent(&app,changed_agent).await.is_err(),"cannot remove active phone default binding");
            let edit=wire::DeviceSaveRequest{id:desktop.clone(),expected_revision:1,name:"重命名电脑".into(),kind:"desktop".into(),..Default::default()};
            let(a,b)=tokio::join!(save_device(&app,edit.clone()),save_device(&app,edit));assert_ne!(a.is_ok(),b.is_ok(),"one concurrent writer wins");
            let reloaded=settings::SettingsStore::open(&root).unwrap();initialize(&db,&reloaded).await.unwrap();assert_eq!(reloaded.get(version).unwrap().model,"alpha");
            assert_eq!(state(&app).await.unwrap().models.iter().find(|m|m.id==model.to_string()).unwrap().profile.as_ref().unwrap().model,"beta","legacy startup cannot overwrite new registry config");
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM conversations").fetch_one(&db).await.unwrap(),1);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM messages").fetch_one(&db).await.unwrap(),before+5);
            assert!(state(&app).await.unwrap().devices.iter().any(|d|d.id==desktop&&d.runtime_state=="registered"));

            // Inline Agent/model save is atomic even when Agent validation fails last.
            let inline_agent=Uuid::new_v4();let inline_model=Uuid::new_v4();
            let checked=check_model(&app,inline_model,0,draft.clone()).await;
            let mut inline=wire::AgentConfigurationSaveRequest{agent:Some(wire::AgentSaveRequest{id:inline_agent.to_string(),name:"".into(),model_connection_id:inline_model.to_string(),device_ids:vec![PHONE.into()],..Default::default()}),connection:Some(wire::ConnectionSaveRequest{id:inline_model.to_string(),name:"独立模型".into(),draft:Some(draft.clone()),verification_id:checked.verification_id,..Default::default()})};
            assert_eq!(call(&app,"agent-config-save",inline.clone()).await.status(),StatusCode::BAD_REQUEST);
            let absent:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM model_connections WHERE id=$1)").bind(inline_model).fetch_one(&db).await.unwrap();assert!(absent,"failed Agent save cannot leave a model row");
            inline.agent.as_mut().unwrap().name="独立助手".into();inline.connection.as_mut().unwrap().verification_id=check_model(&app,inline_model,0,draft.clone()).await.verification_id;
            let _:wire::RegistryState=response(call(&app,"agent-config-save",inline).await).await;
            let mut headers=HeaderMap::new();headers.insert("authorization","Bearer registry-test".parse().unwrap());headers.insert("content-type",MEDIA.parse().unwrap());
            let scoped=wire::ChatHistoryRequest{device_id:desktop.clone(),..Default::default()};
            let desktop_chat:wire::ChatState=response(crate::http::history(axum::extract::State(app.clone()),headers.clone(),Ok(Bytes::from(scoped.encode_to_vec()))).await).await;
            assert_ne!(desktop_chat.conversation_id,conversation.to_string());assert!(desktop_chat.messages.is_empty());
            let wrong=wire::ChatHistoryRequest{device_id:desktop.clone(),conversation_id:conversation.to_string()};
            assert_eq!(crate::http::history(axum::extract::State(app.clone()),headers.clone(),Ok(Bytes::from(wrong.encode_to_vec()))).await.status(),StatusCode::CONFLICT);
            let pending=wire::ChatSendRequest{device_id:desktop.clone(),text:"must not touch phone".into(),request_id:Uuid::new_v4().to_string(),..Default::default()};
            assert_eq!(crate::http::send(axum::extract::State(app.clone()),headers.clone(),Ok(Bytes::from(pending.encode_to_vec()))).await.status(),StatusCode::CONFLICT);
            assert_eq!(crate::http::state(&app,conversation).await.unwrap().messages.len() as i64,before+5);
            // Memories are scoped by identity, not by a reused model endpoint.
            let mem=wire::MemorySaveRequest{agent_id:inline_agent.to_string(),content:"private A memory".into(),..Default::default()};
            let created:wire::ModelMemory=response(crate::workbench::route(axum::extract::State(app.clone()),axum::extract::Path("memory-save".into()),headers.clone(),Ok(Bytes::from(mem.encode_to_vec()))).await).await;
            for (who,expected_count) in [(inline_agent,1),(agent,0)]{
                let request=wire::WorkbenchRequest{agent_id:who.to_string(),..Default::default()};
                let list:wire::MemoryList=response(crate::workbench::route(axum::extract::State(app.clone()),axum::extract::Path("memories".into()),headers.clone(),Ok(Bytes::from(request.encode_to_vec()))).await).await;
                assert_eq!(list.memories.len(),expected_count);
            }
            let wrong_memory=wire::MemorySaveRequest{id:created.id.clone(),expected_revision:1,content:"wrong owner".into(),agent_id:agent.to_string()};
            assert!(!crate::workbench::route(axum::extract::State(app.clone()),axum::extract::Path("memory-save".into()),headers.clone(),Ok(Bytes::from(wrong_memory.encode_to_vec()))).await.status().is_success());
            let wrong_delete=wire::MemoryDeleteRequest{id:created.id,expected_revision:1,agent_id:agent.to_string()};
            assert!(!crate::workbench::route(axum::extract::State(app.clone()),axum::extract::Path("memory-delete".into()),headers.clone(),Ok(Bytes::from(wrong_delete.encode_to_vec()))).await.status().is_success());
            let routed=wire::ChatSendRequest{device_id:PHONE.into(),agent_id:inline_agent.to_string(),text:"remote scoped request".into(),request_id:Uuid::new_v4().to_string(),..Default::default()};
            assert_eq!(crate::http::send(axum::extract::State(app.clone()),headers.clone(),Ok(Bytes::from(routed.encode_to_vec()))).await.status(),StatusCode::CONFLICT,"cannot steer another Agent's active run");
            sqlx::query("UPDATE runs SET status='succeeded' WHERE id=$1").bind(next).execute(&db).await.unwrap();
            let routed_chat:wire::ChatState=response(crate::http::send(axum::extract::State(app.clone()),headers.clone(),Ok(Bytes::from(routed.encode_to_vec()))).await).await;
            let pinned:Uuid=sqlx::query_scalar("SELECT agent_id FROM runs WHERE id=$1").bind(Uuid::parse_str(&routed_chat.run_id).unwrap()).fetch_one(&db).await.unwrap();assert_eq!(pinned,inline_agent);
            // A fresh install configured solely through the registry has no legacy pointer.
            // Its setup status and task execution must still resolve the registered model.
            sqlx::query("TRUNCATE runs,messages,agents,model_connections,devices,agent_device_bindings,registry_meta CASCADE").execute(&db).await.unwrap();
            let fresh_root=root.join("fresh-instance");
            let fresh=Arc::new(App{db:db.clone(),root:fresh_root.clone(),token:"registry-test".into(),phone_token:"unused".into(),settings:settings::SettingsStore::open(&fresh_root).unwrap(),local:reqwest::Client::new(),remote:reqwest::Client::new(),active:Mutex::new(None),login_attempts:Mutex::new(vec![])});
            initialize(&db,&fresh.settings).await.unwrap();assert!(!configured(&fresh).await);
            let checked=check_model(&fresh,MODEL,1,draft.clone()).await;
            let _:wire::RegistryState=response(call(&fresh,"model-save",wire::ConnectionSaveRequest{id:MODEL.to_string(),expected_revision:1,name:"新实例模型".into(),draft:Some(draft),verification_id:checked.verification_id}).await).await;
            assert_eq!(fresh.settings.current(),0);assert!(configured(&fresh).await);
            let fresh_run=send(&fresh,"new install task").await;assert_eq!(settings::for_run(&fresh,fresh_run).await.unwrap().model,"alpha");
            let mut headers=HeaderMap::new();headers.insert("authorization","Bearer registry-test".parse().unwrap());headers.insert("content-type",MEDIA.parse().unwrap());
            let setup:wire::SetupState=response(crate::setup::route(axum::extract::State(fresh.clone()),axum::extract::Path("status".into()),headers,Ok(Bytes::new())).await).await;assert!(setup.model_configured);
            let reopened=settings::SettingsStore::open(&fresh_root).unwrap();initialize(&db,&reopened).await.unwrap();assert!(configured(&fresh).await);

        }).catch_unwind().await;
        server.abort();
        drop(app);
        db.close().await;
        sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .execute(&mut admin)
            .await
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        if let Err(error) = outcome {
            std::panic::resume_unwind(error);
        }
    }
}
