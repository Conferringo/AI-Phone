use crate::*;
use axum::extract::{Multipart, Path, State};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashMap;

async fn resolve(app: &App, id: &str, device: &str) -> Result<Uuid, Response> {
    let device = if device.is_empty() {
        crate::registry::PHONE
    } else {
        device
    };
    if device != crate::registry::PHONE {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM devices WHERE id=$1 AND owner_id='local-user')",
        )
        .bind(device)
        .fetch_one(&app.db)
        .await
        .map_err(db_error)?;
        if !exists {
            return Err(error(
                StatusCode::NOT_FOUND,
                "device_not_found",
                "设备不存在",
            ));
        }
    }
    let canonical: Uuid = match sqlx::query_scalar(
        "SELECT id FROM conversations WHERE owner_id='local-user' AND device_id=$1",
    )
    .bind(device)
    .fetch_optional(&app.db)
    .await
    .map_err(db_error)?
    {
        Some(id) => id,
        None => {
            sqlx::query("INSERT INTO conversations(id,device_id) VALUES($1,$2) ON CONFLICT(owner_id,device_id) DO NOTHING").bind(Uuid::now_v7()).bind(device).execute(&app.db).await.map_err(db_error)?;
            sqlx::query_scalar(
                "SELECT id FROM conversations WHERE owner_id='local-user' AND device_id=$1",
            )
            .bind(device)
            .fetch_one(&app.db)
            .await
            .map_err(db_error)?
        }
    };
    if !id.is_empty() && id != canonical.to_string() {
        return Err(error(
            StatusCode::CONFLICT,
            "conversation_scope",
            "对话不属于此设备，请重新载入",
        ));
    }
    Ok(canonical)
}
pub async fn state(app: &App, id: Uuid) -> Result<wire::ChatState, sqlx::Error> {
    let mut tx = app.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let rows=sqlx::query("SELECT m.id,m.role,m.text,m.attachment_ids,m.ordinal,m.request_id,v.session_id,v.utterance_id,v.revision,v.literal,v.corrects_request_id,COALESCE(m.run_id,r.id) run_id,COALESCE(source.status,'') source_status,COALESCE(source.error,'') source_error,COALESCE(source.loop_state,'{}'::jsonb) source_state,EXISTS(SELECT 1 FROM harness_events e WHERE e.message_id=m.id AND e.content=m.text AND e.status='failed' AND (e.kind='system_error' OR (e.kind='summary' AND e.title='连接重试'))) recorded_retry,COALESCE(r.status IN ('queued','running','paused'),false) active,EXISTS(SELECT 1 FROM harness_events e WHERE e.message_id=m.id AND e.kind NOT IN ('task','system_error') AND NOT(e.kind='summary' AND e.title='连接重试' AND e.status='failed')) has_activity FROM messages m LEFT JOIN chat_voice_inputs v ON v.message_id=m.id LEFT JOIN runs r ON r.assistant_message_id=m.id LEFT JOIN runs source ON source.id=COALESCE(m.run_id,r.id) WHERE m.conversation_id=$1 ORDER BY m.ordinal").bind(id).fetch_all(&mut *tx).await?;
    let ids: Vec<Uuid> = rows
        .iter()
        .flat_map(|row| row.get::<Vec<Uuid>, _>("attachment_ids"))
        .collect();
    let files=sqlx::query("SELECT id,name,media_type,size_bytes FROM attachments WHERE id=ANY($1) AND owner_id='local-user'").bind(&ids).fetch_all(&mut *tx).await?;
    let files: HashMap<Uuid, wire::ChatAttachment> = files
        .iter()
        .map(|r| {
            let id: Uuid = r.get("id");
            (
                id,
                wire::ChatAttachment {
                    id: id.to_string(),
                    name: r.get("name"),
                    media_type: r.get("media_type"),
                    size_bytes: r.get::<i64, _>("size_bytes") as u64,
                },
            )
        })
        .collect();
    let messages = rows
        .iter()
        .map(|r| {
            let notice = crate::run_notice::kind(
                r.get("source_status"),
                r.get("source_error"),
                &r.get::<Value, _>("source_state"),
            );
            let (text, system_notice) = crate::run_notice::message(
                r.get("role"),
                r.get("text"),
                r.get("source_error"),
                notice,
                r.get::<bool, _>("recorded_retry")
                    || crate::run_notice::legacy_pause(
                        r.get("text"),
                        &r.get::<Value, _>("source_state"),
                    ),
            );
            wire::ChatMessage {
                id: r.get::<Uuid, _>("id").to_string(),
                run_id: r
                    .get::<Option<Uuid>, _>("run_id")
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                request_id: r
                    .get::<Option<Uuid>, _>("request_id")
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                ordinal: r.get::<i64, _>("ordinal") as u64,
                active: r.get("active"),
                has_activity: r.get("has_activity"),
                voice: crate::voice_input::from_row(r),
                role: r.get("role"),
                text,
                system_notice,
                attachments: r
                    .get::<Vec<Uuid>, _>("attachment_ids")
                    .iter()
                    .filter_map(|id| files.get(id).cloned())
                    .collect(),
            }
        })
        .collect();
    let run=sqlx::query("SELECT id,status,error,loop_state,agent_id,agent_name,device_id,COALESCE((extract(epoch FROM next_wake_at)*1000)::bigint,0) next_wake_at_ms FROM runs WHERE conversation_id=$1 ORDER BY (status IN ('queued','running','paused')) DESC,created_at DESC,id DESC LIMIT 1").bind(id).fetch_optional(&mut *tx).await?;
    let value = wire::ChatState {
        revision: rows
            .last()
            .map(|r| r.get::<i64, _>("ordinal") as u64)
            .unwrap_or(0),
        conversation_id: id.to_string(),
        device_id: sqlx::query_scalar("SELECT device_id FROM conversations WHERE id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?,
        agent_id: run
            .as_ref()
            .and_then(|r| r.get::<Option<Uuid>, _>("agent_id"))
            .map(|v| v.to_string())
            .unwrap_or_default(),
        agent_name: run
            .as_ref()
            .map(|r| r.get("agent_name"))
            .unwrap_or_default(),
        messages,
        run_id: run
            .as_ref()
            .map(|r| r.get::<Uuid, _>("id").to_string())
            .unwrap_or_default(),
        run_status: run.as_ref().map(|r| r.get("status")).unwrap_or_default(),
        error: run.as_ref().map(|r| r.get("error")).unwrap_or_default(),
        notice_kind: run
            .as_ref()
            .map(|r| {
                crate::run_notice::kind(
                    r.get("status"),
                    r.get("error"),
                    &r.get::<Value, _>("loop_state"),
                ) as i32
            })
            .unwrap_or_default(),
        next_wake_at_ms: run.as_ref().map(|r| r.get("next_wake_at_ms")).unwrap_or(0),
    };
    tx.commit().await?;
    Ok(value)
}
pub async fn history(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let request: wire::ChatHistoryRequest = match decode(&app, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let id = match resolve(&app, &request.conversation_id, &request.device_id).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state(&app, id).await {
        Ok(v) => pb(StatusCode::OK, v),
        Err(e) => db_error(e),
    }
}
pub async fn send(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let request: wire::ChatSendRequest = match decode(&app, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    if request.text.len() > 48000
        || request.attachment_ids.len() > 8
        || (request.text.trim().is_empty() && request.attachment_ids.is_empty())
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_message",
            "消息为空或内容过大",
        );
    }
    let voice = match request
        .voice
        .as_ref()
        .map(crate::voice_input::VoiceInput::parse)
        .transpose()
    {
        Ok(value) => value,
        Err(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_voice",
                "语音片段或修订信息无效",
            );
        }
    };
    if voice.is_some() && (request.text.trim().is_empty() || !request.attachment_ids.is_empty()) {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_voice",
            "语音输入需要文字且不能混入附件",
        );
    }
    let quiet_ms: i16 = if voice.is_some() { 0 } else { 700 };
    let id = match resolve(&app, &request.conversation_id, &request.device_id).await {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !request.device_id.is_empty()
        && request.device_id != crate::registry::PHONE
        && crate::desktop::device_kind(&app, &request.device_id)
            .await
            .is_err()
    {
        return error(
            StatusCode::CONFLICT,
            "device_unavailable",
            "设备尚未安装，暂时不能执行任务",
        );
    }
    let request_id = match Uuid::parse_str(&request.request_id) {
        Ok(v) => v,
        Err(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_request_id",
                "请求标识无效",
            );
        }
    };
    let fingerprint = format!("{:x}", Sha256::digest(request.encode_to_vec()));
    let result: Result<Uuid, sqlx::Error> = async {
        let mut tx = app.db.begin().await?;
        // A receipt is unique across both initial runs and subsequent inputs, including devices.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(request_id.to_string()).execute(&mut *tx).await?;
        sqlx::query("SELECT id FROM conversations WHERE id=$1 FOR UPDATE")
            .bind(id).fetch_one(&mut *tx).await?;
        if let Some(old) = sqlx::query("SELECT id,conversation_id,request_fingerprint FROM runs WHERE request_id=$1")
            .bind(request_id).fetch_optional(&mut *tx).await? {
            if old.get::<Uuid,_>("conversation_id") != id || old.get::<String,_>("request_fingerprint") != fingerprint {
                return Err(sqlx::Error::Protocol("request_conflict".into()));
            }
            let run = old.get::<Uuid,_>("id");
            tx.commit().await?;
            return Ok(run);
        }
        if let Some(old) = sqlx::query("SELECT i.run_id,i.fingerprint,r.conversation_id FROM run_inputs i JOIN runs r ON r.id=i.run_id WHERE i.request_id=$1")
            .bind(request_id).fetch_optional(&mut *tx).await? {
            if old.get::<Uuid,_>("conversation_id") != id || old.get::<String,_>("fingerprint") != fingerprint {
                return Err(sqlx::Error::Protocol("request_conflict".into()));
            }
            let run = old.get::<Uuid,_>("run_id");
            tx.commit().await?;
            return Ok(run);
        }
        let files: Vec<Uuid> = request.attachment_ids.iter().map(|s|Uuid::parse_str(s))
            .collect::<Result<_,_>>().map_err(|_|sqlx::Error::Protocol("invalid_attachments".into()))?;
        let attachments = sqlx::query("SELECT id,name,media_type,size_bytes,storage_key FROM attachments WHERE id=ANY($1) AND owner_id='local-user'")
            .bind(&files).fetch_all(&mut *tx).await?;
        let total: i64 = attachments.iter().map(|r|r.get::<i64,_>("size_bytes")).sum();
        if attachments.len() != files.len() || total > 12*1024*1024 || attachments.iter().any(|r|r.get::<i64,_>("size_bytes") < 0) {
            return Err(sqlx::Error::Protocol("invalid_attachments".into()));
        }
        sqlx::query("UPDATE runs SET status='canceled',error='用户新指令优先',finished_at=now(),next_wake_at=NULL WHERE conversation_id=$1 AND origin<>'user' AND status IN ('queued','running','paused')").bind(id).execute(&mut *tx).await?;
        sqlx::query("UPDATE phone_notifications SET status='pending-summary' WHERE status='assigned' AND run_id IN(SELECT id FROM runs WHERE conversation_id=$1 AND origin='notification' AND status='canceled' AND NOT background_finalized)").bind(id).execute(&mut *tx).await?;
        let active = sqlx::query("SELECT id,status,agent_id FROM runs WHERE conversation_id=$1 AND status IN ('queued','running','paused') FOR UPDATE")
            .bind(id).fetch_optional(&mut *tx).await?;
        if !request.agent_id.is_empty() && active.as_ref().is_some_and(|r| r.get::<Option<Uuid>,_>("agent_id").map(|v|v.to_string()).as_deref()!=Some(request.agent_id.as_str())) {
            return Err(sqlx::Error::Protocol("agent_busy".into()));
        }
        let user = Uuid::now_v7();
        sqlx::query("INSERT INTO messages(id,conversation_id,role,text,attachment_ids,request_id) VALUES($1,$2,'user',$3,$4,$5)")
            .bind(user).bind(id).bind(&request.text).bind(&files).bind(request_id).execute(&mut *tx).await?;
        if let Some(voice) = &voice {voice.persist(&mut tx, id, user, request_id).await?;}

        let run = if let Some(active) = active {
            let run = active.get::<Uuid,_>("id");
            let input = steering_input(&app, &request, &attachments).await?;
            let updated = sqlx::query("UPDATE runs SET steering=steering||jsonb_build_array(jsonb_build_object('id',$2::text,'input',$3::jsonb,'created_at',clock_timestamp())), input_at=clock_timestamp(), input_quiet_ms=$4, next_wake_at=NULL, control_epoch=CASE WHEN status='paused' THEN 0 ELSE control_epoch END, status=CASE WHEN status='paused' THEN 'queued' ELSE status END, error='' WHERE id=$1 AND jsonb_array_length(steering)<32 AND octet_length(steering::text)+octet_length($3::jsonb::text)<67108864")
                .bind(run).bind(request_id.to_string()).bind(input).bind(quiet_ms).execute(&mut *tx).await?;
            if updated.rows_affected()!=1 { return Err(sqlx::Error::Protocol("steering_full".into())); }
            sqlx::query("INSERT INTO run_inputs(request_id,run_id,message_id,fingerprint) VALUES($1,$2,$3,$4)")
                .bind(request_id).bind(run).bind(user).bind(&fingerprint).execute(&mut *tx).await?;
            run
        } else {
            let revision=app.settings.current();
            let assistant = Uuid::now_v7();
            let run = Uuid::now_v7();
            sqlx::query("INSERT INTO messages(id,conversation_id,role) VALUES($1,$2,'assistant')")
                .bind(assistant).bind(id).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,model_revision,input_at,input_quiet_ms) VALUES($1,$2,$3,$4,$5,'queued',$6,clock_timestamp(),$7)")
                .bind(run).bind(id).bind(assistant).bind(request_id).bind(&fingerprint).bind(revision as i64).bind(quiet_ms).execute(&mut *tx).await?;
            sqlx::query("UPDATE messages SET run_id=$2 WHERE id=$1").bind(assistant).bind(run).execute(&mut *tx).await?;
            crate::registry::pin_run_for(&mut tx, &app.settings, run, &request.agent_id).await?;
            run
        };
        tx.commit().await?;
        Ok(run)
    }.await;
    let run_id = match result {
        Ok(id) => id,
        Err(e) => {
            return match e.to_string().as_str() {
                s if s.contains("agent_busy") => error(
                    StatusCode::CONFLICT,
                    "agent_busy",
                    "此设备正由其他 Agent 执行任务，请稍后重试",
                ),
                s if s.contains("model_missing") => error(
                    StatusCode::CONFLICT,
                    "model_missing",
                    "请先在设置中连接模型",
                ),
                s if s.contains("request_conflict") => error(
                    StatusCode::CONFLICT,
                    "request_conflict",
                    "同一请求标识不能更换消息内容",
                ),
                s if s.contains("invalid_attachments") => error(
                    StatusCode::BAD_REQUEST,
                    "invalid_attachments",
                    "附件无效或总大小超过12MiB",
                ),
                s if s.contains("voice_conflict") => error(
                    StatusCode::CONFLICT,
                    "voice_conflict",
                    "语音更正与已接受的片段不一致，请保留文本并重新发送",
                ),
                s if s.contains("steering_full") => error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "steering_full",
                    "待处理指令过多，请等待助手读取后再发送",
                ),
                _ => db_error(e),
            };
        }
    };
    crate::proactive::cancel_stopped(&app).await;
    match state(&app, id).await {
        Ok(mut value) => {
            // A retry's receipt stays attached to the original run (including channel clients).
            let run = match sqlx::query("SELECT status,error,loop_state,COALESCE((extract(epoch FROM next_wake_at)*1000)::bigint,0) next_wake_at_ms FROM runs WHERE id=$1 AND conversation_id=$2")
                .bind(run_id).bind(id).fetch_one(&app.db).await {
                    Ok(row) => row, Err(e) => return db_error(e),
                };
            value.run_id = run_id.to_string();
            value.run_status = run.get("status");
            value.error = run.get("error");
            value.notice_kind = crate::run_notice::kind(
                run.get("status"),
                run.get("error"),
                &run.get::<Value, _>("loop_state"),
            ) as i32;
            value.next_wake_at_ms = run.get("next_wake_at_ms");
            pb(StatusCode::ACCEPTED, value)
        }
        Err(e) => db_error(e),
    }
}

/// Build the exact model input before accepting a steering request. Files remain owner-scoped.
async fn steering_input(
    app: &App,
    request: &wire::ChatSendRequest,
    attachments: &[sqlx::postgres::PgRow],
) -> Result<Value, sqlx::Error> {
    let invalid = || sqlx::Error::Protocol("invalid_attachments".into());
    let mut content = Vec::new();
    if !request.text.is_empty() {
        content.push(json!({"type":"input_text","text":crate::voice_input::model_text(&request.text, &request.request_id, request.voice.as_ref())}));
    }
    let base = app.root.join("data/attachments");
    let mut actual_total = 0u64;
    for id in &request.attachment_ids {
        let attachment_id = Uuid::parse_str(id).map_err(|_| invalid())?;
        let row = attachments
            .iter()
            .find(|r| r.get::<Uuid, _>("id") == attachment_id)
            .ok_or_else(invalid)?;
        let key: String = row.get("storage_key");
        let relative = std::path::Path::new(&key);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err(invalid());
        }
        let root = tokio::fs::canonicalize(&base)
            .await
            .map_err(|_| invalid())?;
        let file = tokio::fs::canonicalize(base.join(relative))
            .await
            .map_err(|_| invalid())?;
        if !file.starts_with(&root) {
            return Err(invalid());
        }
        let metadata = tokio::fs::metadata(&file).await.map_err(|_| invalid())?;
        actual_total = actual_total
            .checked_add(metadata.len())
            .ok_or_else(invalid)?;
        if !metadata.is_file() || actual_total > 12 * 1024 * 1024 {
            return Err(invalid());
        }
        let bytes = tokio::fs::read(file).await.map_err(|_| invalid())?;
        let media: String = row.get("media_type");
        if media.starts_with("image/") {
            content.push(json!({"type":"input_image","image_url":format!("data:{media};base64,{}",STANDARD.encode(bytes))}));
        } else {
            let text = String::from_utf8_lossy(&bytes)
                .chars()
                .take(24000)
                .collect::<String>();
            content.push(json!({"type":"input_text","text":format!("附件 {}：\n{}",row.get::<String,_>("name"),text)}));
        }
    }
    Ok(json!({"role":"user","content":content}))
}

pub async fn cancel(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    run_command(app, headers, body, false).await
}
pub async fn resume(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    run_command(app, headers, body, true).await
}
async fn run_command(
    app: Shared,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
    resume: bool,
) -> Response {
    let request: wire::ChatRunRequest = match decode(&app, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let id = match Uuid::parse_str(&request.run_id) {
        Ok(v) => v,
        Err(_) => return error(StatusCode::BAD_REQUEST, "invalid_run", "任务不存在"),
    };
    let query = if resume {
        "UPDATE runs SET status='queued',control_epoch=0,error='',next_wake_at=NULL,loop_state=jsonb_set(jsonb_set(loop_state,'{stalls}','0'::jsonb),'{last_issue}','\"\"'::jsonb) WHERE id=$1 AND status='paused' RETURNING conversation_id"
    } else {
        "UPDATE runs SET status='canceled',finished_at=now(),next_wake_at=NULL WHERE id=$1 AND status IN ('queued','running','paused') RETURNING conversation_id"
    };
    let conversation: Option<Uuid> = match sqlx::query_scalar(query)
        .bind(id)
        .fetch_optional(&app.db)
        .await
    {
        Ok(v) => v,
        Err(e) => return db_error(e),
    };
    let Some(conversation) = conversation else {
        return error(
            StatusCode::CONFLICT,
            "run_not_active",
            "任务状态已改变，请刷新",
        );
    };
    if !resume {
        let background =
            sqlx::query_scalar::<_, bool>("SELECT origin<>'user' FROM runs WHERE id=$1")
                .bind(id)
                .fetch_one(&app.db)
                .await
                .unwrap_or(false);
        if background {
            let info = sqlx::query("SELECT origin,device_id FROM runs WHERE id=$1")
                .bind(id)
                .fetch_one(&app.db)
                .await;
            if let Ok(info) = info {
                if info.get::<String, _>("origin") == "desktop-exploration" {
                    let _ = crate::desktop_explore::pause(
                        &app,
                        &info.get::<String, _>("device_id"),
                        "已停止自主探索",
                    )
                    .await;
                } else if ["notification", "exploration"]
                    .contains(&info.get::<String, _>("origin").as_str())
                {
                    let _ = crate::proactive::stop_exploration(&app, "已停止自主探索").await;
                }
            }
        }
        if let Err(e)=sqlx::query("UPDATE messages SET text='已停止。' WHERE id=(SELECT assistant_message_id FROM runs WHERE id=$1) AND text=''")
            .bind(id).execute(&app.db).await { return db_error(e); }
        if let Some((active, token)) = &*app.active.lock().await
            && *active == id
        {
            token.cancel();
        }
    }
    match state(&app, conversation).await {
        Ok(v) => pb(StatusCode::OK, v),
        Err(e) => db_error(e),
    }
}

fn media_type(bytes: &[u8], name: &str) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if ["txt", "md", "csv", "json", "log"]
        .iter()
        .any(|ext| name.to_lowercase().ends_with(&format!(".{ext}")))
        && bytes.len() <= 2 * 1024 * 1024
        && std::str::from_utf8(bytes).is_ok()
        && !bytes.contains(&0)
    {
        Some("text/plain; charset=utf-8")
    } else {
        None
    }
}
pub async fn upload(
    State(app): State<Shared>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    if let Err(e) = auth(&app, &headers) {
        return e.into();
    }
    let field = match multipart.next_field().await {
        Ok(Some(v)) => v,
        _ => return error(StatusCode::BAD_REQUEST, "invalid_upload", "请选择附件"),
    };
    let name = field
        .file_name()
        .unwrap_or("附件")
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
        .take(120)
        .collect::<String>();
    let bytes = match field.bytes().await {
        Ok(v) if !v.is_empty() && v.len() <= 8 * 1024 * 1024 => v,
        _ => {
            return error(
                StatusCode::BAD_REQUEST,
                "file_limit",
                "单个附件须为1字节到8MiB",
            );
        }
    };
    let Some(media) = media_type(&bytes, &name) else {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_file",
            "支持PNG、JPEG、WebP、GIF及UTF-8文本文件",
        );
    };
    let id = Uuid::now_v7();
    let key = id.to_string();
    let folder = app.root.join("data/attachments");
    if tokio::fs::create_dir_all(&folder).await.is_err()
        || tokio::fs::write(folder.join(&key), &bytes).await.is_err()
    {
        return db_error("write");
    }
    if let Err(e) = sqlx::query(
        "INSERT INTO attachments(id,name,media_type,size_bytes,storage_key) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(&name)
    .bind(media)
    .bind(bytes.len() as i64)
    .bind(&key)
    .execute(&app.db)
    .await
    {
        return db_error(e);
    }
    pb(
        StatusCode::OK,
        wire::ChatAttachment {
            id: key,
            name,
            media_type: media.into(),
            size_bytes: bytes.len() as u64,
        },
    )
}
pub async fn attachment(
    State(app): State<Shared>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(e) = auth(&app, &headers) {
        return e.into();
    }
    let id = match Uuid::parse_str(&id) {
        Ok(v) => v,
        Err(_) => return error(StatusCode::NOT_FOUND, "not_found", "附件不存在"),
    };
    let row = match sqlx::query(
        "SELECT media_type,storage_key FROM attachments WHERE id=$1 AND owner_id='local-user'",
    )
    .bind(id)
    .fetch_optional(&app.db)
    .await
    {
        Ok(Some(v)) => v,
        _ => return error(StatusCode::NOT_FOUND, "not_found", "附件不存在"),
    };
    let bytes = match tokio::fs::read(
        app.root
            .join("data/attachments")
            .join(row.get::<String, _>("storage_key")),
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return db_error(e),
    };
    (
        [
            ("content-type", row.get::<String, _>("media_type")),
            ("x-content-type-options", "nosniff".into()),
            ("cross-origin-resource-policy", "same-origin".into()),
            ("cache-control", "private, no-store".into()),
        ],
        bytes,
    )
        .into_response()
}

/// Retrieve one durable run without mixing its result with later continuous-chat turns.
pub async fn run_state(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let request: wire::ChatRunRequest = match decode(&app, &headers, body) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let Ok(id) = Uuid::parse_str(&request.run_id) else {
        return error(StatusCode::BAD_REQUEST, "invalid_run", "任务不存在");
    };
    let row=match sqlx::query("SELECT r.id,r.conversation_id,r.status,r.error,r.loop_state,EXISTS(SELECT 1 FROM harness_events e WHERE e.message_id=m.id AND e.content=m.text AND e.status='failed' AND (e.kind='system_error' OR (e.kind='summary' AND e.title='连接重试'))) recorded_retry,r.agent_id,r.agent_name,r.device_id,COALESCE((extract(epoch FROM r.next_wake_at)*1000)::bigint,0) next_wake_at_ms,m.id AS message_id,m.text FROM runs r JOIN messages m ON m.id=r.assistant_message_id JOIN conversations c ON c.id=r.conversation_id WHERE r.id=$1 AND c.owner_id='local-user'").bind(id).fetch_optional(&app.db).await { Ok(Some(r))=>r, Ok(None)=>return error(StatusCode::NOT_FOUND,"not_found","任务不存在"), Err(e)=>return db_error(e) };
    let notice_kind = crate::run_notice::kind(
        row.get("status"),
        row.get("error"),
        &row.get::<Value, _>("loop_state"),
    );
    let (text, system_notice) = crate::run_notice::message(
        "assistant",
        row.get("text"),
        row.get("error"),
        notice_kind,
        row.get::<bool, _>("recorded_retry")
            || crate::run_notice::legacy_pause(row.get("text"), &row.get::<Value, _>("loop_state")),
    );
    pb(
        StatusCode::OK,
        wire::ChatState {
            notice_kind: notice_kind as i32,
            conversation_id: row.get::<Uuid, _>("conversation_id").to_string(),
            run_id: id.to_string(),
            device_id: row.get("device_id"),
            agent_id: row
                .get::<Option<Uuid>, _>("agent_id")
                .map(|v| v.to_string())
                .unwrap_or_default(),
            agent_name: row.get("agent_name"),
            run_status: row.get("status"),
            error: row.get("error"),
            next_wake_at_ms: row.get("next_wake_at_ms"),
            messages: vec![wire::ChatMessage {
                id: row.get::<Uuid, _>("message_id").to_string(),
                run_id: id.to_string(),
                role: "assistant".into(),
                text,
                system_notice,
                attachments: vec![],
                ..Default::default()
            }],
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod steering_tests {
    use super::*;
    use futures_util::FutureExt;
    use sqlx::Connection;
    use std::{panic::AssertUnwindSafe, str::FromStr};

    async fn submit(app: &Shared, request: &wire::ChatSendRequest) -> (StatusCode, Vec<u8>) {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer test-private-token".parse().unwrap(),
        );
        headers.insert("content-type", MEDIA.parse().unwrap());
        let response = send(
            State(app.clone()),
            headers,
            Ok(Bytes::from(request.encode_to_vec())),
        )
        .await;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        (status, bytes.to_vec())
    }

    #[tokio::test]
    #[ignore = "requires AI_PHONE_TEST_DATABASE_URL; creates and removes its own isolated database"]
    async fn running_inputs_are_durable_scoped_and_idempotent() {
        let url =
            std::env::var("AI_PHONE_TEST_DATABASE_URL").expect("test database URL is required");
        let name = format!("aiphone_steering_test_{}", Uuid::new_v4().simple());
        let mut admin = sqlx::PgConnection::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut admin)
            .await
            .unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().database(&name);
        let db = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .unwrap();
        let root = std::env::temp_dir().join(&name);
        let attachments = root.join("data/attachments");
        tokio::fs::create_dir_all(&attachments).await.unwrap();
        let test_db = db.clone();
        let app = Arc::new(App {
            db,
            root: root.clone(),
            token: "test-private-token".into(),
            phone_token: "test-phone-token".into(),
            settings: settings::SettingsStore::test_configured(&root),
            local: reqwest::Client::new(),
            remote: reqwest::Client::new(),
            active: Mutex::new(None),
            login_attempts: Mutex::new(Vec::new()),
        });
        let outcome = AssertUnwindSafe(async {
            sqlx::migrate!("./migrations").run(&app.db).await.unwrap();
            let original = wire::ChatSendRequest { text: "开始处理工作".into(), request_id: Uuid::new_v4().to_string(), ..Default::default() };
            let (status, bytes) = submit(&app, &original).await;
            assert_eq!(status, StatusCode::ACCEPTED);
            let original_state = wire::ChatState::decode(bytes.as_slice()).unwrap();
            let run = Uuid::parse_str(&original_state.run_id).unwrap();
            assert_eq!(original_state.messages.len(), 2);
            sqlx::query("UPDATE runs SET status='running',control_epoch=8,next_wake_at=now()+interval '1 hour' WHERE id=$1").bind(run).execute(&app.db).await.unwrap();
            assert!(state(&app, Uuid::parse_str(&original_state.conversation_id).unwrap()).await.unwrap().next_wake_at_ms > 0);

            let text_id = Uuid::new_v4();
            let image_id = Uuid::new_v4();
            let text_key = text_id.to_string();
            let image_key = image_id.to_string();
            let image = STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j6u8AAAAASUVORK5CYII=").unwrap();
            tokio::fs::write(attachments.join(&text_key), "中文附件 ✅").await.unwrap();
            tokio::fs::write(attachments.join(&image_key), &image).await.unwrap();
            for (id, key, media, size) in [(text_id, &text_key, "text/plain", "中文附件 ✅".len()), (image_id, &image_key, "image/png", image.len())] {
                sqlx::query("INSERT INTO attachments(id,name,media_type,size_bytes,storage_key) VALUES($1,$2,$3,$4,$5)").bind(id).bind(key).bind(media).bind(size as i64).bind(key).execute(&app.db).await.unwrap();
            }
            let steer = wire::ChatSendRequest { text: "改成新的处理顺序".into(), attachment_ids: vec![text_id.to_string(), image_id.to_string().to_uppercase()], request_id: Uuid::new_v4().to_string(), ..Default::default() };
            let (status, bytes) = submit(&app, &steer).await;
            assert_eq!(status, StatusCode::ACCEPTED);
            let value = wire::ChatState::decode(bytes.as_slice()).unwrap();
            assert_eq!(value.run_id, original_state.run_id);
            assert_eq!(value.messages.len(), 3);
            assert_eq!(value.run_status, "running");
            assert_eq!(value.next_wake_at_ms, 0);
            let queue: Value = sqlx::query_scalar("SELECT steering FROM runs WHERE id=$1").bind(run).fetch_one(&app.db).await.unwrap();
            assert_eq!(queue.as_array().unwrap().len(), 1);
            assert_eq!(queue[0]["id"], steer.request_id);
            assert_eq!(queue[0]["input"]["role"], "user");
            assert!(queue[0]["created_at"].as_str().unwrap().contains('T'));
            let content = queue[0]["input"]["content"].as_array().unwrap();
            assert_eq!(content.len(), 3);
            assert!(content[1]["text"].as_str().unwrap().contains("中文附件 ✅"));
            assert_eq!(content[2]["image_url"], format!("data:image/png;base64,{}", STANDARD.encode(&image)));
            let (status, bytes) = submit(&app, &steer).await;
            assert_eq!(status, StatusCode::ACCEPTED);
            assert_eq!(wire::ChatState::decode(bytes.as_slice()).unwrap().messages.len(), 3);
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM run_inputs").fetch_one(&app.db).await.unwrap();
            assert_eq!(count, 1);
            let mut conflict = steer.clone(); conflict.text = "不能换参数重放".into();
            assert_eq!(submit(&app, &conflict).await.0, StatusCode::CONFLICT);

            let other = Uuid::new_v4();
            sqlx::query("INSERT INTO attachments(id,owner_id,name,media_type,size_bytes,storage_key) VALUES($1,'someone-else','other','text/plain',1,'other')").bind(other).execute(&app.db).await.unwrap();
            let mut invalid = wire::ChatSendRequest { text: "不应接受未授权附件".into(), attachment_ids: vec![other.to_string()], request_id: Uuid::new_v4().to_string(), ..Default::default() };
            assert_eq!(submit(&app, &invalid).await.0, StatusCode::BAD_REQUEST);
            invalid.attachment_ids = vec![text_id.to_string()]; invalid.request_id = Uuid::new_v4().to_string();
            sqlx::query("UPDATE attachments SET storage_key='../escape.txt' WHERE id=$1").bind(text_id).execute(&app.db).await.unwrap();
            assert_eq!(submit(&app, &invalid).await.0, StatusCode::BAD_REQUEST);
            sqlx::query("UPDATE attachments SET storage_key=$2 WHERE id=$1").bind(text_id).bind(&text_key).execute(&app.db).await.unwrap();

            sqlx::query("UPDATE runs SET status='paused',next_wake_at=now()+interval '1 hour' WHERE id=$1").bind(run).execute(&app.db).await.unwrap();
            let resume = wire::ChatSendRequest { text: "按这条指令继续".into(), request_id: Uuid::new_v4().to_string(), ..Default::default() };
            let (status, bytes) = submit(&app, &resume).await;
            assert_eq!(status, StatusCode::ACCEPTED);
            let value = wire::ChatState::decode(bytes.as_slice()).unwrap();
            assert_eq!(value.run_status, "queued"); assert_eq!(value.next_wake_at_ms, 0);
            let epoch: i64 = sqlx::query_scalar("SELECT control_epoch FROM runs WHERE id=$1").bind(run).fetch_one(&app.db).await.unwrap();
            assert_eq!(epoch, 0);

            let before: i64 = sqlx::query_scalar("SELECT count(*) FROM messages").fetch_one(&app.db).await.unwrap();
            sqlx::query("UPDATE runs SET steering=(SELECT jsonb_agg(jsonb_build_object('id',n)) FROM generate_series(1,32)n) WHERE id=$1").bind(run).execute(&app.db).await.unwrap();
            let full = wire::ChatSendRequest { text: "超过队列上限".into(), request_id: Uuid::new_v4().to_string(), ..Default::default() };
            assert_eq!(submit(&app, &full).await.0, StatusCode::TOO_MANY_REQUESTS);
            let after: i64 = sqlx::query_scalar("SELECT count(*) FROM messages").fetch_one(&app.db).await.unwrap();
            assert_eq!(before, after);
            sqlx::query("UPDATE runs SET status='succeeded',next_wake_at=NULL WHERE id=$1").bind(run).execute(&app.db).await.unwrap();
            let later = wire::ChatSendRequest { text: "另一个任务".into(), request_id: Uuid::new_v4().to_string(), ..Default::default() };
            let (_, bytes) = submit(&app, &later).await;
            assert_ne!(wire::ChatState::decode(bytes.as_slice()).unwrap().run_id, original_state.run_id);
            for request in [&original, &steer] {
                let (status, bytes) = submit(&app, request).await;
                assert_eq!(status, StatusCode::ACCEPTED);
                let result = wire::ChatState::decode(bytes.as_slice()).unwrap();
                assert_eq!(result.run_id, original_state.run_id);
                assert_eq!(result.run_status, "succeeded");
            }
            let assistants: i64 = sqlx::query_scalar("SELECT count(*) FROM messages WHERE role='assistant'").fetch_one(&app.db).await.unwrap();
            assert_eq!(assistants, 2);
        }).catch_unwind().await;
        drop(app);
        test_db.close().await;
        sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .execute(&mut admin)
            .await
            .unwrap();
        tokio::fs::remove_dir_all(&root).await.unwrap();
        if let Err(error) = outcome {
            std::panic::resume_unwind(error);
        }
    }
}
