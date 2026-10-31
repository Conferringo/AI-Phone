use crate::*;
use sqlx::Row;
pub async fn state(app: &App, device: &str) -> Result<wire::DesktopExploreState, String> {
    let kind: Option<String> =
        sqlx::query_scalar("SELECT kind FROM devices WHERE id=$1 AND runtime_state='managed'")
            .bind(device)
            .fetch_optional(&app.db)
            .await
            .map_err(|e| e.to_string())?;
    match kind.as_deref() {
        Some("desktop") => desktop::available(app, device).await?,
        Some("phone") if device != registry::PHONE => {
            crate::phone_system::target(app, device).await?;
        }
        _ => return Err("设备尚未接入此探索调度器".into()),
    }
    sqlx::query("INSERT INTO desktop_exploration(device_id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(device)
        .execute(&app.db)
        .await
        .map_err(|e| e.to_string())?;
    let r=sqlx::query("SELECT *, (extract(epoch FROM next_cycle_at)*1000)::bigint wake FROM desktop_exploration WHERE device_id=$1").bind(device).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
    let run:Option<Uuid>=sqlx::query_scalar("SELECT id FROM runs WHERE device_id=$1 AND origin='desktop-exploration' AND status IN ('queued','running','paused') ORDER BY created_at DESC LIMIT 1").bind(device).fetch_optional(&app.db).await.map_err(|e|e.to_string())?;
    let last_error = crate::run_notice::background_error(&app.db, device, r.get("last_error"))
        .await
        .map_err(|e| e.to_string())?;
    Ok(wire::DesktopExploreState {
        device_id: device.into(),
        enabled: r.get("enabled"),
        interval_seconds: r.get::<i32, _>("interval_seconds") as u32,
        last_error,
        next_wake_at_ms: r.get::<i64, _>("wake") as u64,
        run_id: run.map(|v| v.to_string()).unwrap_or_default(),
    })
}
pub async fn update(
    app: &App,
    r: wire::DesktopExploreUpdate,
) -> Result<wire::DesktopExploreState, String> {
    state(app, &r.device_id).await?;
    if !(30..=3600).contains(&r.interval_seconds) {
        return Err("间隔应为30–3600秒".into());
    }
    change_mode(
        app,
        &r.device_id,
        r.enabled,
        Some(r.interval_seconds as i32),
        "",
    )
    .await?;
    state(app, &r.device_id).await
}
pub async fn pause(app: &App, device: &str, reason: &str) -> Result<(), String> {
    change_mode(app, device, false, None, reason).await
}
// Every scheduler/configuration mutation takes conversation, mode, then run locks.
async fn lock_device(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    device: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query("INSERT INTO conversations(id,owner_id,device_id) VALUES($1,'local-user',$2) ON CONFLICT(owner_id,device_id) DO NOTHING").bind(Uuid::now_v7()).bind(device).execute(&mut **tx).await?;
    let conversation: Uuid = sqlx::query_scalar(
        "SELECT id FROM conversations WHERE device_id=$1 AND owner_id='local-user' FOR UPDATE",
    )
    .bind(device)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query("INSERT INTO desktop_exploration(device_id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(device)
        .execute(&mut **tx)
        .await?;
    sqlx::query("SELECT device_id FROM desktop_exploration WHERE device_id=$1 FOR UPDATE")
        .bind(device)
        .fetch_one(&mut **tx)
        .await?;
    Ok(conversation)
}
async fn change_mode(
    app: &App,
    device: &str,
    enabled: bool,
    interval: Option<i32>,
    reason: &str,
) -> Result<(), String> {
    let mut tx = app.db.begin().await.map_err(|e| e.to_string())?;
    lock_device(&mut tx, device)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("UPDATE desktop_exploration SET enabled=$2,interval_seconds=COALESCE($3,interval_seconds),next_cycle_at=CASE WHEN $2 THEN now() ELSE next_cycle_at END,last_error='' WHERE device_id=$1").bind(device).bind(enabled).bind(interval).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    let detail = if enabled {
        "重新开启自主探索，旧暂停轮保留记录"
    } else {
        reason
    };
    let ids:Vec<Uuid>=sqlx::query_scalar("UPDATE runs SET status='canceled',error=$2,background_finalized=true,finished_at=now(),next_wake_at=NULL WHERE device_id=$1 AND origin='desktop-exploration' AND (status='paused' OR (NOT $3 AND status IN ('queued','running'))) RETURNING id").bind(device).bind(detail).bind(enabled).fetch_all(&mut *tx).await.map_err(|e|e.to_string())?;
    sqlx::query("UPDATE harness_events SET status='canceled',finished_at=now() WHERE run_id=ANY($1) AND kind='task' AND status IN ('running','paused','waiting')").bind(&ids).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    if let Some((id, token)) = &*app.active.lock().await
        && ids.contains(id)
    {
        token.cancel();
    }
    Ok(())
}
pub async fn guard(app: &App, run: Uuid) -> Result<(), String> {
    let enabled: bool = sqlx::query_scalar("SELECT e.enabled FROM runs r JOIN desktop_exploration e ON e.device_id=r.device_id WHERE r.id=$1").bind(run).fetch_one(&app.db).await.map_err(|e|e.to_string())?;
    if !enabled {
        return Err("canceled".into());
    }
    // Binding, cable and delivery authorization stay with their adapters.
    Ok(())
}

pub(crate) async fn tick(app: &App) -> Result<(), sqlx::Error> {
    // Discover newly provisioned devices without requiring a browser/settings visit.
    // Unconfigured or archived devices never enqueue model work.
    sqlx::query("INSERT INTO desktop_exploration(device_id) SELECT r.device_id FROM device_runtimes r JOIN devices d ON d.id=r.device_id WHERE d.runtime_state='managed' AND d.id<>'phone-local-01' ON CONFLICT DO NOTHING").execute(&app.db).await?;
    let devices: Vec<String> =
        sqlx::query_scalar("SELECT e.device_id FROM desktop_exploration e JOIN devices d ON d.id=e.device_id JOIN agents a ON a.id=d.default_agent_id JOIN agent_device_bindings b ON b.agent_id=a.id AND b.device_id=d.id JOIN model_connections m ON m.id=a.model_connection_id WHERE d.runtime_state='managed' AND d.owner_id='local-user' AND a.owner_id=d.owner_id AND m.owner_id=d.owner_id AND m.model_revision>0 ORDER BY e.device_id")
            .fetch_all(&app.db)
            .await?;
    for device in devices {
        tick_device(app, &device).await?;
    }
    Ok(())
}
async fn tick_device(app: &App, device: &str) -> Result<(), sqlx::Error> {
    let mut tx = app.db.begin().await?;
    let conversation = lock_device(&mut tx, device).await?;
    reconcile_cycles(&mut tx, device).await?;
    let due: bool = sqlx::query_scalar(
        "SELECT enabled AND next_cycle_at<=now() FROM desktop_exploration WHERE device_id=$1",
    )
    .bind(device)
    .fetch_one(&mut *tx)
    .await?;
    let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE device_id=$1 AND (status IN ('running','queued') OR (status='paused' AND origin IN ('exploration','desktop-exploration'))))").bind(device).fetch_one(&mut *tx).await?;
    if due && !busy {
        queue_cycle(&mut tx, &app.settings, device, conversation).await?;
    }
    tx.commit().await
}
async fn reconcile_cycles(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    device: &str,
) -> Result<(), sqlx::Error> {
    let done=sqlx::query("SELECT id,device_id,status,error FROM runs WHERE device_id=$1 AND origin='desktop-exploration' AND NOT background_finalized AND status IN ('succeeded','failed','canceled','paused') FOR UPDATE").bind(device).fetch_all(&mut **tx).await?;
    for r in done {
        let bad = ["failed", "paused"].contains(&r.get::<String, _>("status").as_str());
        sqlx::query("UPDATE desktop_exploration SET enabled=CASE WHEN $2 THEN false ELSE enabled END,last_error=$3,next_cycle_at=now()+make_interval(secs=>interval_seconds) WHERE device_id=$1").bind(r.get::<String,_>("device_id")).bind(bad).bind(if bad{r.get::<String,_>("error")}else{String::new()}).execute(&mut **tx).await?;
        sqlx::query("UPDATE runs SET background_finalized=true WHERE id=$1")
            .bind(r.get::<Uuid, _>("id"))
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
async fn queue_cycle(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    settings: &settings::SettingsStore,
    device: &str,
    conversation: Uuid,
) -> Result<(), sqlx::Error> {
    let run = Uuid::now_v7();
    let message = Uuid::now_v7();
    sqlx::query("INSERT INTO messages(id,conversation_id,role,text) VALUES($1,$2,'assistant','')")
        .bind(message)
        .bind(conversation)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,origin,device_id) VALUES($1,$2,$3,$4,'desktop-exploration:'||$1::text,'queued','desktop-exploration',$5)").bind(run).bind(conversation).bind(message).bind(Uuid::now_v7()).bind(device).execute(&mut **tx).await?;
    registry::pin_run(tx, settings, run).await?;
    sqlx::query("UPDATE desktop_exploration SET next_cycle_at=now()+make_interval(secs=>interval_seconds) WHERE device_id=$1").bind(device).execute(&mut **tx).await?;
    Ok(())
}

pub async fn work(app: Shared) {
    loop {
        if let Err(e) = tick(&app).await {
            eprintln!("desktop exploration scheduling failed: {e}");
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
