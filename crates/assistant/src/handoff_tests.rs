//! Isolated PostgreSQL only: never start a worker, Docker, model, or device helper.
use super::*;
use crate::worker::steering_regressions::{database_case, wait_for_blocked_query};

const COMPUTER: &str = "handoff-fixture-desktop";

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no model or device calls"]
async fn completed_handoff_wakes_its_waiting_source_without_resetting_the_task() {
    database_case(|app| async move {
        let fixture = Fixture::new(app).await;
        let sent = fixture.enqueue().await;
        tick(&fixture.app).await.unwrap();
        let (_, target) = fixture.message(&sent.id).await;
        let original = json!([{"role":"user","content":[{"type":"input_text","text":"Finish the requested workbook and return it to its original conversation"}]}]);
        sqlx::query("UPDATE runs SET status='queued',next_wake_at=now()+interval '5 minutes',context=$2 WHERE id=$1")
            .bind(fixture.source_run).bind(&original).execute(&fixture.app.db).await.unwrap();
        sqlx::query("UPDATE runs SET status='succeeded' WHERE id=$1")
            .bind(target.unwrap()).execute(&fixture.app.db).await.unwrap();
        tick(&fixture.app).await.unwrap();
        let (due,context):(bool,Value)=sqlx::query_as("SELECT next_wake_at<=now(),context FROM runs WHERE id=$1")
            .bind(fixture.source_run).fetch_one(&fixture.app.db).await.unwrap();
        assert!(due,"completed device work must wake its original waiting task, without a model polling loop");
        assert_eq!(context,original,"waking must preserve task context until the worker consumes the receipt");
    }).await;
}

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
            sqlx::query_scalar("SELECT id FROM conversations WHERE owner_id='local-user'")
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
        let conversation = phone_conversation;
        let exploration = Uuid::now_v7();
        let message = Uuid::now_v7();
        sqlx::query("INSERT INTO messages(id,conversation_id,role) VALUES($1,$2,'assistant')")
            .bind(message)
            .bind(conversation)
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,origin,device_id) VALUES($1,$2,$3,$1,'fixture','queued','exploration',$4)")
            .bind(exploration).bind(conversation).bind(message).bind(COMPUTER).execute(&app.db).await.unwrap();
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
        crate::proactive::stop_exploration(&fixture.app, "manual takeover fixture")
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
        let owner = fixture.source_run;
        let (scheduled, ()) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(tick(&fixture.app), async {
                wait_for_blocked_query(&fixture.app, "SELECT id FROM conversations WHERE owner_id='local-user' FOR%").await;
                sqlx::query("UPDATE runs SET device_id=$2 WHERE id=$1")
                    .bind(owner).bind(COMPUTER).execute(&mut *foreground).await.unwrap();
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

async fn finish_receiving(fixture: &Fixture, sent: &wire::DeviceMessage, status: &str) -> Uuid {
    tick(&fixture.app).await.unwrap();
    let (_, target) = fixture.message(&sent.id).await;
    let target = target.unwrap();
    sqlx::query("UPDATE messages SET text='Fixture workbook saved; inspect the real file before delivery' WHERE id=(SELECT assistant_message_id FROM runs WHERE id=$1)")
        .bind(target).execute(&fixture.app.db).await.unwrap();
    sqlx::query("UPDATE runs SET status=$2,error=CASE WHEN $2='succeeded' THEN '' ELSE 'fixture interrupted' END WHERE id=$1")
        .bind(target).bind(status).execute(&fixture.app.db).await.unwrap();
    tick(&fixture.app).await.unwrap();
    target
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; receipt ledger fixtures, not actual device files"]
async fn return_receipt_keeps_the_original_task_and_transfer_evidence_exactly_once() {
    database_case(|app|async move {
        let f=Fixture::new(app).await;let sent=f.enqueue().await;
        assert!(returns::pending(&f.app,f.source_run).await.unwrap());
        let target=finish_receiving(&f,&sent,"succeeded").await;
        let transfer=Uuid::new_v4();
        sqlx::query("INSERT INTO device_transfers(id,link_id,link_revision,run_id,source_device_id,target_device_id,source_path,target_path,name,bytes,sha256,status) SELECT $1,link_id,link_revision,$2,target_device_id,source_device_id,'output/report.xlsx','Download/report.xlsx','report.xlsx',42,repeat('a',64),'completed' FROM device_messages WHERE id=$3")
            .bind(transfer).bind(target).bind(Uuid::parse_str(&sent.id).unwrap()).execute(&f.app.db).await.unwrap();
        let mut context=vec![json!({"role":"user","content":[{"type":"input_text","text":"Keep the original social conversation and requested edits"}]})];
        let original=context.clone();
        assert!(returns::receive(&f.app,f.source_run,&mut context).await.unwrap());
        assert_eq!(&context[..original.len()],original.as_slice());
        let event:Value=sqlx::query_scalar("SELECT result FROM harness_events WHERE run_id=$1 AND title='设备协作'")
            .bind(f.source_run).fetch_one(&f.app.db).await.unwrap();
        assert_eq!(event[0]["handoff_id"],sent.id);assert_eq!(event[0]["target_run_id"],target.to_string());
        assert_eq!(event[0]["status"],"completed");assert_eq!(event[0]["transfers"][0]["id"],transfer.to_string());
        assert_eq!(event[0]["transfers"][0]["sha256"],"a".repeat(64));assert_eq!(event[0]["transfers"][0]["target_path"],"Download/report.xlsx");
        let saved:Value=sqlx::query_scalar("SELECT context FROM runs WHERE id=$1").bind(f.source_run).fetch_one(&f.app.db).await.unwrap();assert_eq!(saved,json!(context));
        assert!(!returns::pending(&f.app,f.source_run).await.unwrap());
        assert!(!returns::receive(&f.app,f.source_run,&mut context).await.unwrap());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM harness_events WHERE run_id=$1 AND title='设备协作'").bind(f.source_run).fetch_one(&f.app.db).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM device_messages").fetch_one(&f.app.db).await.unwrap(),1,"return does not create a reverse task");
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no control or provider calls"]
async fn results_preserve_pauses_cancellation_and_provider_backoff() {
    for (status, error) in [
        ("paused", "user took control"),
        ("canceled", "stopped"),
        ("queued", "HTTP 429"),
    ] {
        database_case(|app|async move {
            let f=Fixture::new(app).await;let sent=f.enqueue().await;
            tick(&f.app).await.unwrap();
            sqlx::query("UPDATE runs SET status=$2,error=$3,next_wake_at=now()+interval '5 minutes' WHERE id=$1")
                .bind(f.source_run).bind(status).bind(error).execute(&f.app.db).await.unwrap();
            let before:(String,String,i64)=sqlx::query_as("SELECT status,error,floor(extract(epoch FROM next_wake_at)*1000)::bigint FROM runs WHERE id=$1")
                .bind(f.source_run).fetch_one(&f.app.db).await.unwrap();
            finish_receiving(&f,&sent,"succeeded").await;
            let after:(String,String,i64)=sqlx::query_as("SELECT status,error,floor(extract(epoch FROM next_wake_at)*1000)::bigint FROM runs WHERE id=$1")
                .bind(f.source_run).fetch_one(&f.app.db).await.unwrap();assert_eq!(before,after);
            let mut context=vec![];assert!(!returns::receive(&f.app,f.source_run,&mut context).await.unwrap());assert!(context.is_empty());
            assert!(returns::pending(&f.app,f.source_run).await.unwrap());
        }).await;
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no automatic replay of failed device work"]
async fn failure_and_disconnection_return_facts_without_requeueing_the_receiver() {
    for status in ["failed", "paused", "canceled"] {
        database_case(|app| async move {
            let f = Fixture::new(app).await;
            let sent = f.enqueue().await;
            let target = finish_receiving(&f, &sent, status).await;
            desktop::link(
                &f.app,
                wire::DeviceLinkRequest {
                    source_device_id: registry::PHONE.into(),
                    target_device_id: COMPUTER.into(),
                    connected: false,
                },
            )
            .await
            .unwrap();
            let mut context = vec![];
            assert!(
                returns::receive(&f.app, f.source_run, &mut context)
                    .await
                    .unwrap()
            );
            let result: Value = sqlx::query_scalar(
                "SELECT result FROM harness_events WHERE run_id=$1 AND title='设备协作'",
            )
            .bind(f.source_run)
            .fetch_one(&f.app.db)
            .await
            .unwrap();
            assert_eq!(result[0]["cable_connected"], false);
            assert_eq!(result[0]["error"], "fixture interrupted");
            assert_eq!(
                result[0]["status"],
                if status == "canceled" {
                    "canceled"
                } else {
                    "failed"
                }
            );
            tick(&f.app).await.unwrap();
            assert_eq!(f.message(&sent.id).await.1, Some(target));
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM runs WHERE origin='device-handoff'"
                )
                .fetch_one(&f.app.db)
                .await
                .unwrap(),
                1
            );
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; transaction failure and row-lock injection"]
async fn failed_publication_rolls_back_and_locked_receipts_can_be_consumed_later() {
    database_case(|app|async move {
        let f=Fixture::new(app).await;let sent=f.enqueue().await;finish_receiving(&f,&sent,"succeeded").await;
        let mut context=vec![json!({"role":"user","content":"original task"})];
        sqlx::query("UPDATE runs SET context=$2 WHERE id=$1").bind(f.source_run).bind(json!(context)).execute(&f.app.db).await.unwrap();
        let original=context.clone();
        sqlx::raw_sql("CREATE FUNCTION fixture_fail_return() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected return publication failure'; END $$; CREATE TRIGGER fixture_fail_return BEFORE UPDATE OF return_delivered_at ON device_messages FOR EACH ROW EXECUTE FUNCTION fixture_fail_return();")
            .execute(&f.app.db).await.unwrap();
        assert!(returns::receive(&f.app,f.source_run,&mut context).await.is_err());assert_eq!(context,original);
        assert_eq!(sqlx::query_scalar::<_,Value>("SELECT context FROM runs WHERE id=$1").bind(f.source_run).fetch_one(&f.app.db).await.unwrap(),json!(original));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM harness_events WHERE run_id=$1 AND title='设备协作'").bind(f.source_run).fetch_one(&f.app.db).await.unwrap(),0);
        assert!(returns::pending(&f.app,f.source_run).await.unwrap());
        sqlx::raw_sql("DROP TRIGGER fixture_fail_return ON device_messages; DROP FUNCTION fixture_fail_return();").execute(&f.app.db).await.unwrap();
        let mut blocker=f.app.db.begin().await.unwrap();
        sqlx::query("SELECT id FROM device_messages WHERE id=$1 FOR UPDATE").bind(Uuid::parse_str(&sent.id).unwrap()).fetch_one(&mut *blocker).await.unwrap();
        assert!(!tokio::time::timeout(Duration::from_secs(2),returns::receive(&f.app,f.source_run,&mut context)).await.expect("must skip scheduler-owned row instead of reversing locks").unwrap());
        blocker.rollback().await.unwrap();assert!(returns::receive(&f.app,f.source_run,&mut context).await.unwrap());
        assert!(!returns::receive(&f.app,f.source_run,&mut context).await.unwrap());
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; an opposite queued task would deadlock both Agents"]
async fn receiving_agent_returns_to_its_parent_instead_of_reverse_dispatch() {
    database_case(|app| async move {
        let f = Fixture::new(app).await;
        let sent = f.enqueue().await;
        tick(&f.app).await.unwrap();
        let target = f.message(&sent.id).await.1.unwrap();
        let error = send(
            &f.app,
            wire::DeviceMessageRequest {
                request_id: Uuid::new_v4().to_string(),
                source_device_id: COMPUTER.into(),
                target_device_id: registry::PHONE.into(),
                text: "Return the completed workbook".into(),
            },
            Some(target),
        )
        .await
        .unwrap_err();
        assert!(error.contains("task_finish"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM device_messages")
                .fetch_one(&f.app.db)
                .await
                .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; disconnect before receiving work starts"]
async fn disconnected_queued_assignment_returns_without_inventing_a_receiving_run() {
    database_case(|app| async move {
        let f = Fixture::new(app).await;
        let sent = f.enqueue().await;
        desktop::link(
            &f.app,
            wire::DeviceLinkRequest {
                source_device_id: registry::PHONE.into(),
                target_device_id: COMPUTER.into(),
                connected: false,
            },
        )
        .await
        .unwrap();
        tick(&f.app).await.unwrap();
        assert_eq!(f.message(&sent.id).await, ("canceled".into(), None));
        let mut context = vec![];
        assert!(
            returns::receive(&f.app, f.source_run, &mut context)
                .await
                .unwrap()
        );
        let event: Value = sqlx::query_scalar(
            "SELECT result FROM harness_events WHERE run_id=$1 AND title='设备协作'",
        )
        .bind(f.source_run)
        .fetch_one(&f.app.db)
        .await
        .unwrap();
        assert!(event[0]["target_run_id"].is_null());
        assert_eq!(event[0]["status"], "canceled");
        assert_eq!(event[0]["transfers"], json!([]));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM runs WHERE origin='device-handoff'")
                .fetch_one(&f.app.db)
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; cancellation retains unknown-action receipts"]
async fn canceling_the_source_stops_its_queued_or_running_delegation_without_erasing_evidence() {
    for started in [false, true] {
        database_case(|app|async move {
            let f=Fixture::new(app).await;let sent=f.enqueue().await;
            let action=Uuid::new_v4();
            if started {
                tick(&f.app).await.unwrap();let target=f.message(&sent.id).await.1.unwrap();
                sqlx::query("UPDATE runs SET status='running',pending_tools=$2 WHERE id=$1").bind(target).bind(json!([{"local_action_id":action,"name":"computer_click"}])).execute(&f.app.db).await.unwrap();
                sqlx::query("INSERT INTO harness_events(id,run_id,message_id,kind,title,status) SELECT $1,id,assistant_message_id,'tool','Fixture input','running' FROM runs WHERE id=$2")
                    .bind(action).bind(target).execute(&f.app.db).await.unwrap();
            }
            sqlx::query("UPDATE runs SET status='canceled' WHERE id=$1").bind(f.source_run).execute(&f.app.db).await.unwrap();
            tick(&f.app).await.unwrap();let (status,target)=f.message(&sent.id).await;assert_eq!(status,"canceled");
            if let Some(target)=target {
                assert!(guard(&f.app,target,"computer_click").await.is_err());
                assert_eq!(sqlx::query_scalar::<_,String>("SELECT status FROM runs WHERE id=$1").bind(target).fetch_one(&f.app.db).await.unwrap(),"canceled");
                let (status,unknown):(String,bool)=sqlx::query_as("SELECT status,result IS NULL FROM harness_events WHERE id=$1").bind(action).fetch_one(&f.app.db).await.unwrap();assert_eq!(status,"running");assert!(unknown,"cancellation must not invent a receipt for an input already dispatched");
                assert_eq!(sqlx::query_scalar::<_,Value>("SELECT pending_tools FROM runs WHERE id=$1").bind(target).fetch_one(&f.app.db).await.unwrap()[0]["local_action_id"],action.to_string());
            }else{assert!(!started);}
            let mut context=vec![];assert!(!returns::receive(&f.app,f.source_run,&mut context).await.unwrap());
        }).await;
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; cable authority does not grant another Agent's GUI"]
async fn a_task_can_discover_and_connect_an_owned_computer_without_stealing_its_agent_control() {
    database_case(|app|async move {
        let f=Fixture::new(app).await;
        desktop::link(&f.app,wire::DeviceLinkRequest{source_device_id:registry::PHONE.into(),target_device_id:COMPUTER.into(),connected:false}).await.unwrap();
        let call=json!({"name":"device_list"});
        let result=desktop::tool(&f.app,f.source_run,&call,&json!({})).await.unwrap();
        let state:Value=serde_json::from_str(result[0]["text"].as_str().unwrap()).unwrap();
        let computer=state["devices"].as_array().unwrap().iter().find(|d|d["device_id"]==COMPUTER).expect("notification work must discover the owner's installed receiving computer before the cable exists");
        assert_eq!(computer["can_control"],false);assert_eq!(computer["can_handoff"],true);
        let args=json!({"source_device_id":registry::PHONE,"target_device_id":COMPUTER,"connected":true});
        desktop::tool(&f.app,f.source_run,&json!({"name":"device_link"}),&args).await.unwrap();
        assert!(desktop::file_authorized(&f.app,f.source_run,COMPUTER).await.is_ok());
        assert!(desktop::authorized(&f.app,f.source_run,COMPUTER).await.is_err(),"a data cable must not grant GUI control");
        sqlx::query("INSERT INTO devices(id,name,owner_id,kind,system,runtime_state) VALUES('foreign-fixture','Foreign','another-owner','desktop','fixture','managed')").execute(&f.app.db).await.unwrap();
        let result=desktop::tool(&f.app,f.source_run,&call,&json!({})).await.unwrap();let visible:Value=serde_json::from_str(result[0]["text"].as_str().unwrap()).unwrap();
        assert!(!visible["devices"].as_array().unwrap().iter().any(|d|d["device_id"]=="foreign-fixture"));
        assert!(desktop::tool(&f.app,f.source_run,&json!({"name":"device_link"}),&json!({"source_device_id":registry::PHONE,"target_device_id":"foreign-fixture","connected":true})).await.is_err());
    }).await;
}
