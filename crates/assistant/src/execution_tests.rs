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
        let busy = crate::run_queue::reserve(&app.db, phone.0, "fixture-desktop")
            .await
            .unwrap_err();
        assert!(!fatal_tool_error(&busy));
        let result = tool_value(&recoverable_tool_failure(busy)).unwrap();
        assert_eq!(result["code"], "device_busy");
        assert_eq!(result["not_executed"], true);
        assert_eq!(result["next_tool"], "task_wait");
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
                    ..Default::default()
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

#[tokio::test]
#[ignore = "real disposable PostgreSQL; rejected commands never dispatch to a guest"]
async fn ordinary_search_rejects_stored_command_calls_before_dispatch() {
    database_case(|app| async move {
        let (run, message) = queued(&app, "fixture-desktop", "user").await;
        for request in [
            "找园艺相关的企业，记到电脑的表格里",
            "阿里巴巴最近发展怎么样",
            "在电脑浏览器搜索root教程",
        ] {
            let context = json!([
                {"role":"system","content":[{"type":"input_text","text":"[当前设备类型:desktop]"}]},
                {"role":"user","content":[{"type":"input_text","text":request}]}
            ]);
            sqlx::query("UPDATE runs SET context=$2 WHERE id=$1")
                .bind(run)
                .bind(context)
                .execute(&app.db)
                .await
                .unwrap();
            for name in [
                "computer_run",
                "phone_shell",
                "workspace_run",
                "workspace_fetch",
            ] {
                // Invalid fixture arguments cannot launch a guest or fetch a URL,
                // even if the command policy regresses.
                let call = json!({"name":name,"arguments":json!({
                    "device_id":"unregistered-fixture","script":"x".repeat(16001),
                    "url":"invalid-fixture-url"
                }).to_string()});
                let output = tool(&app, run, message, &call, &mut 0, &CancellationToken::new())
                    .await
                    .unwrap();
                let result = tool_value(&output).unwrap();
                assert_eq!(
                    result["code"], "visible_device_required",
                    "{request}: {name}"
                );
                assert_eq!(result["not_executed"], true);
                assert_eq!(result["success"], false);
                assert_eq!(result["next_tool"], "computer_observe");
            }
        }
        assert!(!app.root.join("data/workspaces").exists());
    })
    .await;
}

async fn events(app: &Shared, request: wire::HarnessEventRequest) -> wire::HarnessEventList {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "authorization",
        "Bearer steering-fixture-token".parse().unwrap(),
    );
    headers.insert("content-type", MEDIA.parse().unwrap());
    let response = crate::workbench::route(
        axum::extract::State(app.clone()),
        axum::extract::Path("events".into()),
        headers,
        Ok(axum::body::Bytes::from(request.encode_to_vec())),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    wire::HarnessEventList::decode(bytes).unwrap()
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; protobuf timing and message scope"]
async fn timing_projection_is_message_scoped_and_legacy_reads_remain_complete() {
    database_case(|app| async move {
        let (run,first)=queued(&app,"fixture-desktop","user").await;
        let second=Uuid::now_v7();
        sqlx::query("INSERT INTO messages(id,conversation_id,role) SELECT $2,conversation_id,'assistant' FROM runs WHERE id=$1").bind(run).bind(second).execute(&app.db).await.unwrap();
        let old=super::model_trace::start(&app,run,first).await.unwrap();
        let current=super::model_trace::start(&app,run,second).await.unwrap();
        let canceled:String=sqlx::query_scalar("SELECT status FROM harness_events WHERE id=$1").bind(old).fetch_one(&app.db).await.unwrap();
        assert_eq!(canceled,"canceled","new attempt closes stale running metadata");
        let timing=json!({"preparation_ms":2,"headers_ms":5,"first_byte_ms":9,"model_ms":20,"elapsed_ms":22,"request_bytes":100,"unpublished":"fixture-private"});
        super::model_trace::finish(&app,current,"completed",timing).await.unwrap();
        let current_events=events(&app,wire::HarnessEventRequest{run_id:run.to_string(),message_id:second.to_string(),..Default::default()}).await;
        assert_eq!(current_events.events.len(),1);
        let event=&current_events.events[0];
        assert_eq!(event.id,current.to_string());
        let measured=event.model_timing.as_ref().unwrap();
        assert_eq!(measured.preparation_ms,2);
        assert_eq!(measured.first_byte_ms,Some(9));
        assert_eq!(measured.request_bytes,100);
        assert!(!format!("{current_events:?}").contains("fixture-private"));
        let legacy=events(&app,wire::HarnessEventRequest{run_id:run.to_string(),..Default::default()}).await;
        assert_eq!(legacy.events.len(),2,"empty new fields preserve old history queries");
        let cutoff=events(&app,wire::HarnessEventRequest{run_id:run.to_string(),after_ms:u32::MAX as u64 * 1000,..Default::default()}).await;
        assert!(cutoff.events.is_empty());
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; operational persistence errors must not retry a model"]
async fn model_trace_database_failure_is_an_execution_failure() {
    database_case(|app| async move {
        app.db.close().await;
        let run = Uuid::now_v7();
        let start = super::model_trace::start(&app, run, Uuid::now_v7())
            .await
            .unwrap_err();
        let finish = super::model_trace::finish(&app, Uuid::now_v7(), "failed", json!({}))
            .await
            .unwrap_err();
        assert!(
            fatal_tool_error(&start),
            "a database failure must not enter model retry/prompt handling: {start}"
        );
        assert!(
            fatal_tool_error(&finish),
            "timing persistence must stop further execution: {finish}"
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; phone dispatch preserves skill requests"]
async fn phone_dispatch_preserves_skill_request_without_guest_side_effects() {
    database_case(|app| async move {
        crate::registry::initialize(&app.db, &app.settings)
            .await
            .unwrap();
        let (run, message) = queued(&app, crate::registry::PHONE, "user").await;
        sqlx::query("UPDATE runs SET agent_id=$2 WHERE id=$1")
            .bind(run)
            .bind(Uuid::from_u128(2))
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO run_device_bindings(run_id,device_id) VALUES($1,$2)")
            .bind(run)
            .bind(crate::registry::PHONE)
            .execute(&app.db)
            .await
            .unwrap();
        let call = json!({"name":"phone_tap","arguments":"{\"x\":100,\"y\":100}"});
        let mut epoch = 0;
        sqlx::query("SELECT 'dispatch-benchmark-start'")
            .execute(&app.db)
            .await
            .unwrap();
        let start = std::time::Instant::now();
        for _ in 0..100 {
            let result = tool(
                &app,
                run,
                message,
                &call,
                &mut epoch,
                &CancellationToken::new(),
            )
            .await
            .unwrap();
            let value = tool_value(&result).unwrap();
            assert_eq!(value["not_executed"], true);
            assert_eq!(value["next_tool"], "skill_read");
            assert_eq!(epoch, 0);
        }
        println!(
            "dispatch benchmark: 100 calls in {} microseconds",
            start.elapsed().as_micros()
        );
        sqlx::query("SELECT 'dispatch-benchmark-end'")
            .execute(&app.db)
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; phone target selection and current bindings"]
async fn phone_target_keeps_primary_order_and_current_binding_decisions() {
    database_case(|app| async move {
        crate::registry::initialize(&app.db, &app.settings).await.unwrap();
        let agent = Uuid::from_u128(2);
        let (run, _) = queued(&app, crate::registry::PHONE, "user").await;
        sqlx::query("UPDATE runs SET agent_id=$2 WHERE id=$1")
            .bind(run).bind(agent).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO devices(id,name,kind,system,runtime_state,created_at) VALUES
            ('route-a','Older phone','phone','fixture','managed',now()-interval '1 day'),
            ('route-b','Equal age phone','phone','fixture','managed',now()-interval '1 day'),
            ('route-registered','Registered phone','phone','fixture','registered',now()-interval '2 days'),
            ('route-desktop','Computer','desktop','fixture','managed',now()-interval '2 days')")
            .execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO run_device_bindings(run_id,device_id) SELECT $1,id FROM devices")
            .bind(run).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO agent_device_bindings(agent_id,device_id) SELECT $1,id FROM devices WHERE id<>$2")
            .bind(agent).bind(crate::registry::PHONE).execute(&app.db).await.unwrap();
        assert_eq!(crate::phone_system::for_run(&app, run).await.unwrap(), crate::registry::PHONE);

        sqlx::query("SELECT 'routing-benchmark-start'").execute(&app.db).await.unwrap();
        let start = std::time::Instant::now();
        for _ in 0..100 {
            assert_eq!(crate::phone_system::for_run(&app, run).await.unwrap(), crate::registry::PHONE);
        }
        println!("routing benchmark: 100 resolutions in {} microseconds", start.elapsed().as_micros());
        sqlx::query("SELECT 'routing-benchmark-end'").execute(&app.db).await.unwrap();

        sqlx::query("UPDATE runs SET device_id='route-desktop' WHERE id=$1")
            .bind(run).execute(&app.db).await.unwrap();
        assert_eq!(crate::phone_system::for_run(&app, run).await.unwrap(), "route-a");
        sqlx::query("DELETE FROM agent_device_bindings WHERE agent_id=$1 AND device_id='route-a'")
            .bind(agent).execute(&app.db).await.unwrap();
        assert_eq!(crate::phone_system::for_run(&app, run).await.unwrap_err(), "该任务没有此设备的授权");
        sqlx::query("DELETE FROM run_device_bindings WHERE run_id=$1 AND device_id='route-a'")
            .bind(run).execute(&app.db).await.unwrap();
        assert_eq!(crate::phone_system::for_run(&app, run).await.unwrap(), "route-b");
        sqlx::query("UPDATE devices SET runtime_state='registered' WHERE id='route-b'")
            .execute(&app.db).await.unwrap();
        assert_eq!(crate::phone_system::for_run(&app, run).await.unwrap(), crate::registry::PHONE);
        sqlx::query("DELETE FROM run_device_bindings WHERE run_id=$1 AND device_id=$2")
            .bind(run).bind(crate::registry::PHONE).execute(&app.db).await.unwrap();
        assert_eq!(crate::phone_system::for_run(&app, run).await.unwrap_err(), "此Agent未授权手机");
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL and loopback protobuf guest; readiness and observation"]
async fn phone_dispatch_reads_status_before_control_and_keeps_the_real_frame() {
    use axum::{Router, body::Bytes, routing::post};
    database_case(|mut app| async move {
        crate::registry::initialize(&app.db, &app.settings).await.unwrap();
        let (run, message) = queued(&app, crate::registry::PHONE, "user").await;
        let context = json!([
            {"type":"function_call","name":"skill_read","call_id":"skill","arguments":"{\"name\":\"phone-use\"}"},
            {"type":"function_call_output","call_id":"skill","output":[{"type":"input_text","text":"Skill id=phone-use，name=phone-use，enabled=true\nBody"}]}
        ]);
        sqlx::query("UPDATE runs SET agent_id=$2,status='running',context=$3 WHERE id=$1")
            .bind(run).bind(Uuid::from_u128(2)).bind(context).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO run_device_bindings(run_id,device_id) VALUES($1,$2)")
            .bind(run).bind(crate::registry::PHONE).execute(&app.db).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, mut requests) = tokio::sync::mpsc::channel(4);
        let controls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let control_calls = controls.clone();
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let root_ready = ready.clone();
        let frame = b"fixture-current-frame";
        let router = Router::new()
            .route("/v1/observe", post(move |body: Bytes| {
                let sender = sender.clone();
                let root_ready = root_ready.clone();
                async move {
                    let request = wire::DeviceRequest::decode(body).unwrap();
                    assert_eq!(request.phone_id, crate::registry::PHONE);
                    assert_eq!(request.owner_id, "local-user");
                    sender.send(request.clone()).await.unwrap();
                    pb(StatusCode::OK, wire::DeviceObservation {
                        phone_id: request.phone_id, width: 720, height: 1440,
                        root_identity: if root_ready.load(std::sync::atomic::Ordering::Relaxed) { "0".into() } else { String::new() }, control_epoch: 7, controller: "agent".into(),
                        png: if request.metadata_only { vec![] } else { frame.to_vec() },
                        ..Default::default()
                    })
                }
            }))
            .route("/v1/control", post(move |body: Bytes| {
                control_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                async move {
                    let request = wire::TakeControlRequest::decode(body).unwrap();
                    assert_eq!(request.controller, "agent");
                    pb(StatusCode::OK, wire::ControlState {
                        phone_id: request.phone_id, control_epoch: 7, controller: "agent".into(),
                    })
                }
            }));
        let server = FixtureTask(tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); }));
        Arc::get_mut(&mut app).unwrap().local = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{address}")).unwrap())
            .timeout(Duration::from_secs(4)).build().unwrap();
        let call = json!({"name":"phone_observe","arguments":"{}"});
        let mut epoch = 0;
        let result = tool(&app, run, message, &call, &mut epoch, &CancellationToken::new()).await.unwrap();
        assert_eq!(epoch, 7);
        assert!(requests.recv().await.unwrap().metadata_only, "readiness must not capture a discarded screenshot");
        let observed = requests.recv().await.unwrap();
        assert!(!observed.metadata_only);
        assert!(observed.include_elements);
        let image = format!("data:image/png;base64,{}", STANDARD.encode(frame));
        assert!(result.as_array().unwrap().iter().any(|part| part["image_url"] == image));

        sqlx::query("UPDATE runs SET origin='notification',background_read_only=true WHERE id=$1")
            .bind(run).execute(&app.db).await.unwrap();
        epoch = 0;
        let result = tool(&app, run, message, &call, &mut epoch, &CancellationToken::new()).await.unwrap();
        assert!(!requests.recv().await.unwrap().metadata_only);
        assert_eq!(epoch, 0, "read-only notification must not take control");
        assert_eq!(controls.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert!(result.as_array().unwrap().iter().any(|part| part["image_url"] == image));
        sqlx::query("UPDATE runs SET origin='user',background_read_only=false WHERE id=$1")
            .bind(run).execute(&app.db).await.unwrap();
        ready.store(false, std::sync::atomic::Ordering::Relaxed);
        let result = tokio::time::timeout(Duration::from_millis(700),
            tool(&app, run, message, &call, &mut epoch, &CancellationToken::new())).await
            .expect("an unready phone must return its status without a hidden retry loop");
        assert_eq!(result.unwrap_err(), "手机尚未就绪，请稍后继续。");
        assert!(requests.recv().await.unwrap().metadata_only);
        assert!(requests.try_recv().is_err());
        assert_eq!(controls.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(epoch, 0);
        drop(server);
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; connected files do not grant GUI control"]
async fn file_authorization_tracks_bindings_and_both_link_directions() {
    database_case(|app| async move {
        crate::registry::initialize(&app.db, &app.settings).await.unwrap();
        let (run, _) = queued(&app, crate::registry::PHONE, "user").await;
        let agent = Uuid::from_u128(2);
        sqlx::query("UPDATE runs SET agent_id=$2 WHERE id=$1")
            .bind(run).bind(agent).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO run_device_bindings(run_id,device_id) VALUES($1,$2)")
            .bind(run).bind(crate::registry::PHONE).execute(&app.db).await.unwrap();
        let peer = "transfer-peer";
        sqlx::query("INSERT INTO devices(id,name,kind,system,runtime_state) VALUES($1,'Peer','desktop','fixture','managed')")
            .bind(peer).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO device_links(id,source_device_id,target_device_id) VALUES($1,$2,$3)")
            .bind(Uuid::now_v7()).bind(crate::registry::PHONE).bind(peer).execute(&app.db).await.unwrap();
        crate::desktop::file_authorized(&app, run, crate::registry::PHONE).await.unwrap();
        assert!(crate::desktop::authorized(&app, run, peer).await.is_err());
        let peer_root = json!({"device_id":peer,"path":"/"});
        let workspace = json!({"device_id":"","path":"/"});
        for (tool, args) in [
            ("device_file_list", peer_root.clone()),
            ("device_file_manage", json!({"operation":"copy","sources":[peer_root],"target":workspace})),
            ("device_file_manage", json!({"operation":"copy","sources":[workspace],"target":peer_root})),
        ] {
            assert_eq!(
                crate::files::tool(&app, run, tool, &args).await.unwrap_err(),
                "该任务没有此设备的授权",
                "a cable must not grant {tool} on peer guest roots"
            );
        }
        sqlx::query("SELECT 'files-benchmark-start'").execute(&app.db).await.unwrap();
        let started = std::time::Instant::now();
        for _ in 0..100 {
            crate::desktop::file_authorized(&app, run, peer).await.unwrap();
        }
        println!("files benchmark: 100 calls in {} microseconds", started.elapsed().as_micros());
        sqlx::query("SELECT 'files-benchmark-end'").execute(&app.db).await.unwrap();
        sqlx::query("UPDATE device_links SET connected=false").execute(&app.db).await.unwrap();
        assert!(crate::desktop::file_authorized(&app, run, peer).await.is_err());
        crate::desktop::file_authorized(&app, run, crate::registry::PHONE).await.unwrap();
        sqlx::query("UPDATE device_links SET connected=true").execute(&app.db).await.unwrap();
        sqlx::query("DELETE FROM run_device_bindings WHERE run_id=$1").bind(run).execute(&app.db).await.unwrap();
        assert!(crate::desktop::file_authorized(&app, run, peer).await.is_err());
        sqlx::query("INSERT INTO agent_device_bindings(agent_id,device_id) VALUES($1,$2)")
            .bind(agent).bind(peer).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO run_device_bindings(run_id,device_id) VALUES($1,$2)")
            .bind(run).bind(peer).execute(&app.db).await.unwrap();
        crate::desktop::file_authorized(&app, run, crate::registry::PHONE).await.unwrap();
        sqlx::query("DELETE FROM agent_device_bindings WHERE agent_id=$1 AND device_id=$2")
            .bind(agent).bind(peer).execute(&app.db).await.unwrap();
        assert!(crate::desktop::file_authorized(&app, run, crate::registry::PHONE).await.is_err());
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; desktop dispatch rejects input without guest calls"]
async fn desktop_dispatch_checks_authority_once_at_the_execution_boundary() {
    database_case(|app| async move {
        crate::registry::initialize(&app.db, &app.settings).await.unwrap();
        let (run, _) = queued(&app, crate::registry::PHONE, "user").await;
        let agent = Uuid::from_u128(2);
        sqlx::query("UPDATE runs SET agent_id=$2,status='running' WHERE id=$1")
            .bind(run).bind(agent).execute(&app.db).await.unwrap();
        let device = "dispatch-desktop";
        sqlx::query("INSERT INTO devices(id,name,kind,system,runtime_state) VALUES($1,'Computer','desktop','fixture','managed')")
            .bind(device).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO device_runtimes(device_id,provider,instance_ref) VALUES($1,'docker-desktop','ai-phone-desktop')")
            .bind(device).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO agent_device_bindings(agent_id,device_id) VALUES($1,$2)")
            .bind(agent).bind(device).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO run_device_bindings(run_id,device_id) VALUES($1,$2)")
            .bind(run).bind(device).execute(&app.db).await.unwrap();
        let observation = Uuid::now_v7();
        sqlx::query("INSERT INTO desktop_observations(id,device_id,epoch,title,png_path,sha256) VALUES($1,$2,7,'Fixture','unused','unused')")
            .bind(observation).bind(device).execute(&app.db).await.unwrap();
        let call = json!({"name":"computer_click","local_action_id":Uuid::now_v7().to_string()});
        let args = json!({"device_id":device,"observation_id":observation.to_string(),"x":-1,"y":100});
        sqlx::query("SELECT 'desktop-benchmark-start'").execute(&app.db).await.unwrap();
        let started = std::time::Instant::now();
        for _ in 0..100 {
            assert_eq!(crate::desktop::tool(&app, run, &call, &args).await.unwrap_err(), "坐标超出屏幕");
        }
        println!("desktop benchmark: 100 calls in {} microseconds", started.elapsed().as_micros());
        sqlx::query("SELECT 'desktop-benchmark-end'").execute(&app.db).await.unwrap();
        sqlx::query("DELETE FROM agent_device_bindings WHERE agent_id=$1 AND device_id=$2")
            .bind(agent).bind(device).execute(&app.db).await.unwrap();
        assert_eq!(crate::desktop::tool(&app, run, &call, &args).await.unwrap_err(), "该任务没有此设备的授权");
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM desktop_actions")
            .fetch_one(&app.db).await.unwrap(), 0);
    }).await;
}
