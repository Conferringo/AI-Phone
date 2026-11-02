//! Real disposable PostgreSQL; guest control and external providers are never used.
use super::*;
use crate::worker::steering_regressions::database_case;

async fn queued(app: &App, device: &str, origin: &str) -> (Uuid, Uuid) {
    let conversation: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations(id,device_id) VALUES($1,$2)
         ON CONFLICT(owner_id,device_id) DO UPDATE SET device_id=excluded.device_id RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(device)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let message = Uuid::now_v7();
    let run = Uuid::now_v7();
    sqlx::query("INSERT INTO messages(id,conversation_id,role) VALUES($1,$2,'assistant')")
        .bind(message)
        .bind(conversation)
        .execute(&app.db)
        .await
        .unwrap();
    let context = json!([{"role":"user","content":[{"type":"input_text","text":device}]}]);
    sqlx::query("INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,device_id,origin,context) VALUES($1,$2,$3,$1,'execution-fixture','queued',$4,$5,$6)")
        .bind(run).bind(conversation).bind(message).bind(device).bind(origin).bind(context)
        .execute(&app.db).await.unwrap();
    (run, message)
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; device ownership and result finalization"]
async fn same_device_waits_until_the_attempt_result_is_committed() {
    database_case(|app| async move {
        let user = queued(&app, "fixture-phone", "user").await;
        let notification = queued(&app, "fixture-phone", "notification").await;
        assert_eq!(claim_due_run(&app).await.unwrap(), Some(user));
        assert!(claim_due_run(&app).await.unwrap().is_none());
        // A checkpoint may already say queued while finalization is still busy.
        sqlx::query(
            "UPDATE runs SET status='queued',next_wake_at=now()+interval '1 hour' WHERE id=$1",
        )
        .bind(user.0)
        .execute(&app.db)
        .await
        .unwrap();
        assert!(claim_due_run(&app).await.unwrap().is_none());
        finish_attempt(&app, user.0, user.1, &Ok(RunDisposition::Queued), false)
            .await
            .unwrap();
        assert_eq!(claim_due_run(&app).await.unwrap(), Some(notification));
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; simultaneous claim and borrowed device exclusion"]
async fn distinct_devices_run_together_and_borrowed_devices_cannot_be_stolen() {
    database_case(|app| async move {
        let phone = queued(&app, "fixture-phone", "user").await;
        let desktop = queued(&app, "fixture-desktop", "user").await;
        let (a, b) = tokio::join!(claim_due_run(&app), claim_due_run(&app));
        let mut claimed = [a.unwrap().unwrap().0, b.unwrap().unwrap().0];
        claimed.sort();
        let mut expected = [phone.0, desktop.0];
        expected.sort();
        assert_eq!(claimed, expected);
        assert!(
            crate::run_queue::reserve(&app.db, phone.0, "fixture-desktop")
                .await
                .is_err()
        );
        sqlx::query("UPDATE runs SET status='succeeded' WHERE id=$1")
            .bind(desktop.0)
            .execute(&app.db)
            .await
            .unwrap();
        // A terminal status alone cannot release a still-finalizing attempt.
        assert!(
            crate::run_queue::reserve(&app.db, phone.0, "fixture-desktop")
                .await
                .is_err()
        );
        finish_attempt(
            &app,
            desktop.0,
            desktop.1,
            &Ok(RunDisposition::Completed),
            false,
        )
        .await
        .unwrap();
        crate::run_queue::reserve(&app.db, phone.0, "fixture-desktop")
            .await
            .unwrap();
        crate::run_queue::reserve(&app.db, phone.0, "fixture-desktop")
            .await
            .unwrap();
        let next = queued(&app, "fixture-desktop", "user").await;
        assert!(claim_due_run(&app).await.unwrap().is_none());
        sqlx::query("UPDATE runs SET status='succeeded' WHERE id=$1")
            .bind(phone.0)
            .execute(&app.db)
            .await
            .unwrap();
        finish_attempt(
            &app,
            phone.0,
            phone.1,
            &Ok(RunDisposition::Completed),
            false,
        )
        .await
        .unwrap();
        assert_eq!(claim_due_run(&app).await.unwrap(), Some(next));
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; crash recovery preserves unknown tools and deadlines"]
async fn recovery_releases_attempt_ownership_without_replaying_or_losing_checkpoints() {
    database_case(|app| async move {
        let run = queued(&app,"fixture-phone","user").await;
        assert_eq!(claim_due_run(&app).await.unwrap(),Some(run));
        crate::run_queue::reserve(&app.db,run.0,"fixture-desktop").await.unwrap();
        let pending=json!([{"local_action_id":Uuid::new_v4().to_string(),"name":"phone_tap"}]);
        sqlx::query("UPDATE runs SET pending_tools=$2,step=17,next_wake_at=now()+interval '1 hour' WHERE id=$1")
            .bind(run.0).bind(&pending).execute(&app.db).await.unwrap();
        let before: (Value,i32,Value) = sqlx::query_as("SELECT context,step,to_jsonb(next_wake_at) FROM runs WHERE id=$1")
            .bind(run.0).fetch_one(&app.db).await.unwrap();
        recover_interrupted_runs(&app.db).await.unwrap();
        recover_interrupted_runs(&app.db).await.unwrap();
        let after: (Value,i32,Value) = sqlx::query_as("SELECT context,step,to_jsonb(next_wake_at) FROM runs WHERE id=$1")
            .bind(run.0).fetch_one(&app.db).await.unwrap();
        assert_eq!(before,after);
        assert_eq!(sqlx::query_scalar::<_,Value>("SELECT pending_tools FROM runs WHERE id=$1")
            .bind(run.0).fetch_one(&app.db).await.unwrap(),pending);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM device_execution_leases")
            .fetch_one(&app.db).await.unwrap(),0);
        assert!(claim_due_run(&app).await.unwrap().is_none(),"wait deadline survives restart");
    }).await;
}

struct FixtureTask(tokio::task::JoinHandle<()>);
impl Drop for FixtureTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
#[ignore = "real PostgreSQL and loopback stalled model; two real worker attempts, no devices"]
async fn stalled_phone_model_does_not_block_desktop_and_cancel_is_per_run() {
    use axum::{Json, Router, body::Body, extract::State, routing::post};
    database_case(|app| async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, mut received) = tokio::sync::mpsc::channel::<Value>(4);
        let router = Router::new().route(
            "/responses",
            post(move |Json(body): Json<Value>| {
                let sender = sender.clone();
                async move {
                    sender.send(body).await.unwrap();
                    let pending = futures_util::stream::pending::<Result<Bytes, std::io::Error>>();
                    (
                        [("content-type", "text/event-stream")],
                        Body::from_stream(pending),
                    )
                }
            }),
        );
        let _server = FixtureTask(tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }));
        app.settings.test_revision(settings::ModelConfig {
            api_key: "fixture".into(),
            base_url: format!("http://{address}"),
            model: "parallel-fixture".into(),
            provider: "compatible".into(),
            protocol: "responses".into(),
            proxy_url: String::new(),
            verified_at_ms: 1,
        });
        let phone = queued(&app, "fixture-phone", "user").await;
        let desktop = queued(&app, "fixture-desktop", "user").await;
        let worker = FixtureTask(tokio::spawn(work(app.clone())));
        let (first, second) = tokio::time::timeout(Duration::from_secs(10), async {
            (
                received.recv().await.unwrap(),
                received.recv().await.unwrap(),
            )
        })
        .await
        .expect("both devices must reach the model while the first response is stalled");
        let bodies = format!("{first}{second}");
        assert!(bodies.contains("fixture-phone") && bodies.contains("fixture-desktop"));
        let desktop_cancel = app.active.lock().await.get(&desktop.0).unwrap().clone();
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer steering-fixture-token".parse().unwrap(),
        );
        headers.insert("content-type", MEDIA.parse().unwrap());
        let response = crate::http::cancel(
            State(app.clone()),
            headers,
            Ok(Bytes::from(
                wire::ChatRunRequest {
                    run_id: phone.0.to_string(),
                }
                .encode_to_vec(),
            )),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        tokio::time::timeout(Duration::from_secs(5), async {
            while app.active.lock().await.contains_key(&phone.0) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("canceled attempt must persist its outcome and release ownership");
        assert!(!desktop_cancel.is_cancelled());
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id=$1")
                .bind(desktop.0)
                .fetch_one(&app.db)
                .await
                .unwrap(),
            "running"
        );
        assert_eq!(
            sqlx::query_scalar::<_, Vec<Uuid>>(
                "SELECT ARRAY(SELECT run_id FROM device_execution_leases)"
            )
            .fetch_one(&app.db)
            .await
            .unwrap(),
            vec![desktop.0]
        );
        sqlx::query("UPDATE runs SET status='canceled' WHERE id=$1")
            .bind(desktop.0)
            .execute(&app.db)
            .await
            .unwrap();
        crate::proactive::cancel_stopped(&app).await;
        assert!(desktop_cancel.is_cancelled());
        tokio::time::timeout(Duration::from_secs(5), async {
            while !app.active.lock().await.is_empty() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        worker.0.abort();
    })
    .await;
}
