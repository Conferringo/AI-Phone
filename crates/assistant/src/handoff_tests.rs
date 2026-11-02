//! Isolated PostgreSQL only: never start a worker, Docker, model, or device helper.
use super::*;
use crate::worker::steering_regressions::{database_case, wait_for_blocked_query};

const COMPUTER: &str = "handoff-fixture-desktop";

struct Fixture {
    app: Shared,
    source_run: Uuid,
    recipient: Uuid,
    exploration: Uuid,
    conversation: Uuid,
}

impl Fixture {
    async fn new(app: Shared) -> Self {
        registry::initialize(&app.db, &app.settings).await.unwrap();
        let recipient = Uuid::new_v4();
        let mut tx = app.db.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO agents(id,name,model_connection_id) VALUES($1,'Receiving Agent',$2)",
        )
        .bind(recipient)
        .bind(Uuid::from_u128(1))
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query("INSERT INTO devices(id,name,kind,system,runtime_state,default_agent_id) VALUES($1,'Computer','desktop','fixture','managed',$2)")
            .bind(COMPUTER).bind(recipient).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO agent_device_bindings(agent_id,device_id) VALUES($1,$2)")
            .bind(recipient)
            .bind(COMPUTER)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO device_runtimes(device_id,provider,instance_ref) VALUES($1,'docker-desktop','ai-phone-desktop')")
            .bind(COMPUTER).execute(&mut *tx).await.unwrap();
        let phone_conversation: Uuid =
            sqlx::query_scalar("SELECT id FROM conversations WHERE device_id=$1")
                .bind(registry::PHONE)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        let message = Uuid::now_v7();
        let source_run = Uuid::now_v7();
        sqlx::query("INSERT INTO messages(id,conversation_id,role) VALUES($1,$2,'assistant')")
            .bind(message)
            .bind(phone_conversation)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status) VALUES($1,$2,$3,$1,'fixture','running')")
            .bind(source_run).bind(phone_conversation).bind(message).execute(&mut *tx).await.unwrap();
        registry::pin_run(&mut tx, &app.settings, source_run)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        desktop::link(
            &app,
            wire::DeviceLinkRequest {
                source_device_id: registry::PHONE.into(),
                target_device_id: COMPUTER.into(),
                connected: true,
            },
        )
        .await
        .unwrap();
        crate::desktop_explore::tick(&app).await.unwrap();
        let (exploration, conversation) = sqlx::query_as("SELECT id,conversation_id FROM runs WHERE device_id=$1 AND origin='desktop-exploration'")
            .bind(COMPUTER).fetch_one(&app.db).await.unwrap();
        Self {
            app,
            source_run,
            recipient,
            exploration,
            conversation,
        }
    }

    async fn enqueue(&self) -> wire::DeviceMessage {
        send(
            &self.app,
            wire::DeviceMessageRequest {
                request_id: Uuid::new_v4().to_string(),
                source_device_id: registry::PHONE.into(),
                target_device_id: COMPUTER.into(),
                text: "Process the owned fixture spreadsheet".into(),
            },
            Some(self.source_run),
        )
        .await
        .unwrap()
    }

    async fn message(&self, id: &str) -> (String, Option<Uuid>) {
        sqlx::query_as("SELECT status,target_run_id FROM device_messages WHERE id=$1")
            .bind(Uuid::parse_str(id).unwrap())
            .fetch_one(&self.app.db)
            .await
            .unwrap()
    }

    async fn checkpoint(&self) -> Value {
        sqlx::query_scalar("SELECT jsonb_build_object('run',to_jsonb(r),'events',COALESCE((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM harness_events e WHERE e.run_id=r.id),'[]'::jsonb)) FROM runs r WHERE id=$1")
            .bind(self.exploration).fetch_one(&self.app.db).await.unwrap()
    }

    async fn pending(&self, status: Option<&str>, result: Option<Value>) {
        let action = Uuid::new_v4();
        let call = json!({"type":"function_call","name":"computer_run","call_id":"fixture-call","local_action_id":action.to_string(),"arguments":"{\"script\":\"fixture only; never execute\"}"});
        sqlx::query("UPDATE runs SET context=jsonb_build_array($2::jsonb),pending_tools=jsonb_build_array($2::jsonb),loop_state='{\"plan\":{\"goal\":\"preserve exploration\"}}',step=8 WHERE id=$1")
            .bind(self.exploration).bind(call).execute(&self.app.db).await.unwrap();
        if let Some(status) = status {
            sqlx::query("INSERT INTO harness_events(id,run_id,message_id,kind,title,status,result) SELECT $1,id,assistant_message_id,'tool','fixture command',$3,$4 FROM runs WHERE id=$2")
                .bind(action).bind(self.exploration).bind(status).bind(result).execute(&self.app.db).await.unwrap();
        }
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no device or model execution"]
async fn queued_exploration_yields_without_losing_its_checkpoint() {
    for sleeping in [false, true] {
        database_case(|app| async move {
            let fixture = Fixture::new(app).await;
            fixture.pending(None, None).await;
            sqlx::query("UPDATE runs SET next_wake_at=CASE WHEN $2 THEN now()+interval '1 hour' ELSE NULL END,heartbeat_at=now()-interval '1 day' WHERE id=$1")
                .bind(fixture.exploration).bind(sleeping).execute(&fixture.app.db).await.unwrap();
            let before = fixture.checkpoint().await;
            let message = fixture.enqueue().await;
            tick(&fixture.app).await.unwrap();
            let (status, target) = fixture.message(&message.id).await;
            assert_eq!(status, "running", "a queued exploration must not monopolize the receiving device");
            let target = target.expect("accepted handoff has one durable run");
            tick(&fixture.app).await.unwrap();
            assert_eq!(fixture.message(&message.id).await, (status, Some(target)));
            assert_eq!(fixture.checkpoint().await, before, "yield does not cancel, reset, or consume exploration");
            let identity: (Uuid, String) = sqlx::query_as("SELECT agent_id,device_id FROM runs WHERE id=$1")
                .bind(target).fetch_one(&fixture.app.db).await.unwrap();
            assert_eq!(identity, (fixture.recipient, COMPUTER.into()));
            assert_eq!(crate::worker::claim_due_run(&fixture.app).await.unwrap().map(|r| r.0), Some(target), "handoff must win over an older due exploration");
        }).await;
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; retain unknown effects without replay"]
async fn uncertain_exploration_actions_block_handoff_but_unstarted_or_known_actions_do_not() {
    for (status, known, admitted) in [
        ("running", false, false),
        ("failed", false, false),
        ("canceled", false, false),
        ("completed", false, false),
        ("queued", false, true),
        ("completed", true, true),
    ] {
        database_case(|app| async move {
            let fixture = Fixture::new(app).await;
            fixture
                .pending(
                    Some(status),
                    known.then(|| json!([{"type":"input_text","text":"verified fixture receipt"}])),
                )
                .await;
            let before = fixture.checkpoint().await;
            let message = fixture.enqueue().await;
            tick(&fixture.app).await.unwrap();
            let (state, target) = fixture.message(&message.id).await;
            assert_eq!(target.is_some(), admitted, "{status}/known={known}");
            assert_eq!(state, if admitted { "running" } else { "queued" });
            assert_eq!(
                fixture.checkpoint().await,
                before,
                "uncertainty and action identity must survive scheduling"
            );
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no currently executing or paused work is displaced"]
async fn running_and_paused_exploration_are_not_reclassified_as_yielded() {
    for status in ["running", "paused"] {
        database_case(|app| async move {
            let fixture = Fixture::new(app).await;
            sqlx::query("UPDATE runs SET status=$2 WHERE id=$1")
                .bind(fixture.exploration)
                .bind(status)
                .execute(&fixture.app.db)
                .await
                .unwrap();
            let before = fixture.checkpoint().await;
            let message = fixture.enqueue().await;
            tick(&fixture.app).await.unwrap();
            assert_eq!(fixture.message(&message.id).await, ("queued".into(), None));
            assert_eq!(fixture.checkpoint().await, before);
            desktop::link(
                &fixture.app,
                wire::DeviceLinkRequest {
                    source_device_id: registry::PHONE.into(),
                    target_device_id: COMPUTER.into(),
                    connected: false,
                },
            )
            .await
            .unwrap();
            tick(&fixture.app).await.unwrap();
            assert_eq!(
                fixture.message(&message.id).await,
                ("canceled".into(), None)
            );
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; exercise the exact pause called by manual desktop takeover"]
async fn manual_desktop_pause_keeps_the_phone_worker_token_live() {
    database_case(|app| async move {
        let fixture = Fixture::new(app).await;
        let phone_token = CancellationToken::new();
        let desktop_token = CancellationToken::new();
        fixture
            .app
            .active
            .lock()
            .await
            .insert(fixture.source_run, phone_token.clone());
        fixture
            .app
            .active
            .lock()
            .await
            .insert(fixture.exploration, desktop_token.clone());
        crate::desktop_explore::pause(&fixture.app, COMPUTER, "manual takeover fixture")
            .await
            .unwrap();
        assert!(
            !phone_token.is_cancelled(),
            "manual computer control cannot cancel the phone run"
        );
        assert!(
            desktop_token.is_cancelled(),
            "desktop pause cancels its own exploration token"
        );
        let status: String = sqlx::query_scalar("SELECT status FROM runs WHERE id=$1")
            .bind(fixture.source_run)
            .fetch_one(&fixture.app.db)
            .await
            .unwrap();
        assert_eq!(status, "running");
        let exploration: String = sqlx::query_scalar("SELECT status FROM runs WHERE id=$1")
            .bind(fixture.exploration)
            .fetch_one(&fixture.app.db)
            .await
            .unwrap();
        assert_eq!(exploration, "canceled");
        assert!(
            fixture
                .app
                .active
                .lock()
                .await
                .contains_key(&fixture.source_run)
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; foreground admission is serialized with handoff"]
async fn handoff_rechecks_after_waiting_for_foreground_admission() {
    database_case(|app| async move {
        let fixture = Fixture::new(app).await;
        sqlx::query("UPDATE runs SET status='succeeded' WHERE id=$1")
            .bind(fixture.exploration).execute(&fixture.app.db).await.unwrap();
        let message = fixture.enqueue().await;
        let mut foreground = fixture.app.db.begin().await.unwrap();
        sqlx::query("SELECT id FROM conversations WHERE id=$1 FOR UPDATE")
            .bind(fixture.conversation).fetch_one(&mut *foreground).await.unwrap();
        let owner = Uuid::now_v7();
        let (scheduled, ()) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(tick(&fixture.app), async {
                wait_for_blocked_query(&fixture.app, "SELECT id FROM conversations WHERE device_id=$1 AND owner_id='local-user' FOR%").await;
                let mid = Uuid::now_v7();
                sqlx::query("INSERT INTO messages(id,conversation_id,role) VALUES($1,$2,'assistant')")
                    .bind(mid).bind(fixture.conversation).execute(&mut *foreground).await.unwrap();
                sqlx::query("INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,origin,device_id) VALUES($1,$2,$3,$1,'fixture','queued','user',$4)")
                    .bind(owner).bind(fixture.conversation).bind(mid).bind(COMPUTER).execute(&mut *foreground).await.unwrap();
                foreground.commit().await.unwrap();
            })
        }).await.expect("handoff and foreground admission must settle without a deadlock");
        scheduled.expect("a concurrent owner run defers handoff instead of violating its unique lane");
        assert_eq!(fixture.message(&message.id).await, ("queued".into(), None));
        let owners: i64 = sqlx::query_scalar("SELECT count(*) FROM runs WHERE conversation_id=$1 AND origin IN ('user','device-handoff') AND status IN ('queued','running','paused')")
            .bind(fixture.conversation).fetch_one(&fixture.app.db).await.unwrap();
        assert_eq!(owners, 1);
        sqlx::query("UPDATE runs SET status='succeeded' WHERE id=$1")
            .bind(owner).execute(&fixture.app.db).await.unwrap();
        tick(&fixture.app).await.unwrap();
        assert_eq!(fixture.message(&message.id).await.0, "running");
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; existing run writer may retain its message foreign key"]
async fn handoff_waiting_on_a_run_does_not_block_its_conversation_foreign_key() {
    database_case(|app| async move {
        let fixture = Fixture::new(app).await;
        let message = fixture.enqueue().await;
        let mut worker = fixture.app.db.begin().await.unwrap();
        sqlx::query("SELECT id FROM runs WHERE id=$1 FOR UPDATE")
            .bind(fixture.exploration)
            .fetch_one(&mut *worker)
            .await
            .unwrap();
        let (scheduled, ()) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(tick(&fixture.app), async {
                wait_for_blocked_query(
                    &fixture.app,
                    "SELECT id FROM runs WHERE device_id=$1%ORDER BY id FOR UPDATE",
                )
                .await;
                // consume_steering holds the run before inserting a message. This
                // FK key-share must remain compatible with handoff's conversation lock.
                sqlx::query(
                    "INSERT INTO messages(id,conversation_id,role) VALUES($1,$2,'assistant')",
                )
                .bind(Uuid::now_v7())
                .bind(fixture.conversation)
                .execute(&mut *worker)
                .await
                .unwrap();
                sqlx::query("UPDATE runs SET status='running' WHERE id=$1")
                    .bind(fixture.exploration)
                    .execute(&mut *worker)
                    .await
                    .unwrap();
                worker.commit().await.unwrap();
            })
        })
        .await
        .expect("run-first message insertion must not deadlock with handoff admission");
        scheduled.unwrap();
        assert_eq!(fixture.message(&message.id).await, ("queued".into(), None));
        let status: String = sqlx::query_scalar("SELECT status FROM runs WHERE id=$1")
            .bind(fixture.exploration)
            .fetch_one(&fixture.app.db)
            .await
            .unwrap();
        assert_eq!(
            status, "running",
            "an already claimed exploration retains ownership"
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; claim cannot enter the handoff admission window"]
async fn worker_skips_exploration_until_handoff_admission_is_committed() {
    database_case(|app| async move {
        let fixture = Fixture::new(app).await;
        fixture.pending(None, None).await;
        let before = fixture.checkpoint().await;
        let message = fixture.enqueue().await;
        let mut registry_save = fixture.app.db.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(704117_i64)
            .execute(&mut *registry_save)
            .await
            .unwrap();
        let (scheduled, ()) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(tick(&fixture.app), async {
                // pin_run shares the registry's real transaction lock. At this
                // barrier admission has checked/locked the existing exploration.
                wait_for_blocked_query(&fixture.app, "SELECT pg_advisory_xact_lock($1)").await;
                assert!(
                    crate::worker::claim_due_run(&fixture.app)
                        .await
                        .unwrap()
                        .is_none(),
                    "claim must skip the locked exploration while the handoff is uncommitted"
                );
                assert_eq!(fixture.message(&message.id).await, ("queued".into(), None));
                registry_save.commit().await.unwrap();
            })
        })
        .await
        .expect("handoff admission must complete when the registry writer releases");
        scheduled.unwrap();
        let target = fixture.message(&message.id).await.1.unwrap();
        assert_eq!(fixture.checkpoint().await, before);
        assert_eq!(
            crate::worker::claim_due_run(&fixture.app)
                .await
                .unwrap()
                .map(|r| r.0),
            Some(target)
        );
    })
    .await;
}
