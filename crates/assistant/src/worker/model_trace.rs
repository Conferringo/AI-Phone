//! One model attempt owns one durable timing record, including steering/cancellation.
use crate::*;
use serde_json::Value;

pub async fn start(app: &App, run: Uuid, message: Uuid) -> Result<Uuid, String> {
    let id = Uuid::now_v7();
    sqlx::query("WITH stale AS (UPDATE harness_events SET status='canceled',finished_at=now() WHERE run_id=$2 AND kind='model' AND status='running') INSERT INTO harness_events(id,run_id,message_id,kind,title) VALUES($1,$2,$3,'model','模型响应')")
        .bind(id).bind(run).bind(message).execute(&app.db).await.map_err(|e| e.to_string())?;
    Ok(id)
}

pub async fn finish(app: &App, id: Uuid, status: &str, timing: Value) -> Result<(), String> {
    sqlx::query("UPDATE harness_events SET status=$2,result=$3,finished_at=now() WHERE id=$1 AND kind='model'")
        .bind(id).bind(status).bind(timing).execute(&app.db).await.map_err(|e| e.to_string())?;
    Ok(())
}
