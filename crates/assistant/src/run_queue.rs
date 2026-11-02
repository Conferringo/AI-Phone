//! Durable device ownership for task attempts, including cross-device control.
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) async fn recover(db: &PgPool) -> Result<(), sqlx::Error> {
    // Startup holds the installation's listening socket. Preserve checkpoints,
    // unknown actions and wait deadlines; only the previous process's leases end.
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE runs SET status='queued' WHERE status='running'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM device_execution_leases")
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

pub(crate) async fn claim(
    db: &PgPool,
    active: &[Uuid],
) -> Result<Option<(Uuid, Uuid)>, sqlx::Error> {
    let mut tx = db.begin().await?;
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT r.id,r.assistant_message_id,r.device_id FROM runs r
         WHERE r.status='queued' AND NOT(r.id=ANY($1))
         AND (r.next_wake_at IS NULL OR r.next_wake_at<=now())
         AND NOT EXISTS(SELECT 1 FROM device_execution_leases l WHERE l.device_id=r.device_id)
         AND NOT EXISTS(SELECT 1 FROM runs busy WHERE busy.device_id=r.device_id AND busy.status='running')
         ORDER BY CASE WHEN r.origin='user' AND (r.step=0 OR jsonb_array_length(r.steering)>0) THEN 0 WHEN r.origin='notification' THEN 1 WHEN r.origin='user' THEN 2 WHEN r.origin='device-handoff' THEN 3 ELSE 4 END,COALESCE(r.heartbeat_at,r.created_at),r.id
         FOR UPDATE OF r SKIP LOCKED LIMIT 1",
    )
    .bind(active)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, message, device)) = row else {
        return Ok(None);
    };
    let acquired = sqlx::query(
        "INSERT INTO device_execution_leases(device_id,run_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(device)
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if acquired == 0 {
        return Ok(None);
    }
    sqlx::query("UPDATE runs SET status='running',heartbeat_at=now(),loop_state=CASE WHEN next_wake_at IS NOT NULL THEN jsonb_set(loop_state,'{wait_completed_at_ms}',to_jsonb(floor(extract(epoch FROM next_wake_at)*1000)::bigint)) ELSE loop_state END,next_wake_at=NULL,error='' WHERE id=$1")
        .bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some((id, message)))
}

pub(crate) async fn reserve(db: &PgPool, run: Uuid, device: &str) -> Result<(), String> {
    // Authorization belongs to the device adapter. This atomic claim must finish
    // before contacting the guest; it never holds a transaction during I/O.
    let acquired = sqlx::query(
        "INSERT INTO device_execution_leases(device_id,run_id)
         SELECT $2,id FROM runs WHERE id=$1 AND status='running'
         AND NOT EXISTS(SELECT 1 FROM runs busy WHERE busy.device_id=$2 AND busy.status='running' AND busy.id<>$1)
         ON CONFLICT(device_id) DO UPDATE SET run_id=excluded.run_id
         WHERE device_execution_leases.run_id=excluded.run_id",
    )
    .bind(run)
    .bind(device)
    .execute(db)
    .await
    .map_err(|_| "无法确认设备任务占用，未开始操作".to_string())?
    .rows_affected();
    if acquired == 0 {
        return Err(
            "设备正由其他任务使用或当前任务已停止；请等待后重新观察，或用device_message交接".into(),
        );
    }
    Ok(())
}
