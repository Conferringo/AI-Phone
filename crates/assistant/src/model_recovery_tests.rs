//! Paused recovery uses the original Agent's saved model without rewriting its ledger.
use super::*;

async fn resume(app: &Shared, run: Uuid) -> (StatusCode, Vec<u8>) {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        "Bearer steering-fixture-token".parse().unwrap(),
    );
    headers.insert("content-type", MEDIA.parse().unwrap());
    let response = crate::http::resume(
        State(app.clone()),
        headers,
        Ok(Bytes::from(
            wire::ChatRunRequest {
                run_id: run.to_string(),
            }
            .encode_to_vec(),
        )),
    )
    .await;
    let status = response.status();
    (
        status,
        axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap()
            .to_vec(),
    )
}

async fn replace_model(app: &Shared, run: Uuid, model: &str, key: &str) -> u64 {
    let mut config = settings::for_run(app, run).await.unwrap();
    config.model = model.into();
    config.api_key = key.into();
    config.verified_at_ms = 1;
    let revision = app.settings.test_revision(config);
    sqlx::query("UPDATE model_connections SET model_revision=$2 WHERE id=(SELECT model_connection_id FROM agents WHERE id=(SELECT agent_id FROM runs WHERE id=$1))")
        .bind(run).bind(revision as i64).execute(&app.db).await.unwrap();
    revision
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no production data or device calls"]
async fn explicit_resume_refreshes_saved_key_and_resets_retry_budget() {
    database_case(|app| async move {
        let run = FixtureRun::start(&app, "继续整理表格").await;
        sqlx::query("UPDATE runs SET status='paused',error='HTTP 402',loop_state='{\"model_retries\":9,\"stalls\":3,\"last_issue\":\"HTTP 402\",\"notice_kind\":\"system_failure\"}' WHERE id=$1")
            .bind(run.id).execute(&app.db).await.unwrap();
        let revision = replace_model(&app, run.id, "unused", "repaired-key").await;
        assert_eq!(resume(&app, run.id).await.0, StatusCode::OK);
        let row = sqlx::query("SELECT status,model_revision,loop_state,error FROM runs WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(row.get::<i64,_>("model_revision"), revision as i64, "resume must refresh the saved key before a worker claims the task");
        assert_eq!(settings::for_run(&app, run.id).await.unwrap().api_key, "repaired-key");
        assert_eq!(row.get::<String,_>("status"), "queued");
        assert_eq!(row.get::<String,_>("error"), "");
        let state: Value = row.get("loop_state");
        assert_eq!(state["model_retries"], 0);
        assert_eq!(state["stalls"], 0);
        assert_eq!(state["last_issue"], "");
        assert_eq!(resume(&app, run.id).await.0, StatusCode::CONFLICT, "a queued task cannot be rebound again");
    }).await;
}

async fn pause(app: &Shared, run: Uuid) {
    sqlx::query("UPDATE runs SET status='paused',error='HTTP 402',loop_state=jsonb_set(loop_state,'{notice_kind}','\"system_failure\"'::jsonb) WHERE id=$1")
        .bind(run).execute(&app.db).await.unwrap();
}

async fn snapshot(app: &Shared, run: Uuid) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('run',to_jsonb(r),'messages',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM messages m WHERE m.conversation_id=r.conversation_id),'events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM harness_events e WHERE e.run_id=r.id),'inputs',(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.request_id) FROM run_inputs i WHERE i.run_id=r.id)) FROM runs r WHERE r.id=$1")
        .bind(run).fetch_one(&app.db).await.unwrap()
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; failed recovery must roll back the accepted input"]
async fn invalid_saved_configuration_and_full_queue_leave_paused_run_untouched() {
    database_case(|app| async move {
        let mut run = FixtureRun::start(&app, "保留账户和原始文件").await;
        assert!(run.finish(&app, "已读取文件，等待继续。", RunDisposition::NeedsInput).await);
        let original = settings::for_run(&app, run.id).await.unwrap();
        let mut unverified = original.clone();
        unverified.api_key = "unverified-key".into();
        let unverified_revision = app.settings.test_revision(unverified);
        for (revision, expected_code) in [(999999,"model_missing"), (0,"model_missing"), (unverified_revision,"model_unverified")] {
            sqlx::query("UPDATE model_connections SET model_revision=$2 WHERE id=(SELECT model_connection_id FROM agents WHERE id=(SELECT agent_id FROM runs WHERE id=$1))")
                .bind(run.id).bind(revision as i64).execute(&app.db).await.unwrap();
            let before = snapshot(&app,run.id).await;
            for (status, body) in [resume(&app,run.id).await, submit(&app,&request("新的指令必须保留或整体拒绝")).await] {
                assert_eq!(status,StatusCode::CONFLICT);
                assert_eq!(wire::ApiError::decode(body.as_slice()).unwrap().code,expected_code);
                assert_eq!(snapshot(&app,run.id).await,before,"failed recovery cannot alter status, ledger, input receipts or messages");
            }
        }
        replace_model(&app,run.id,"repaired-model","verified-key").await;
        sqlx::query("UPDATE runs SET steering=(SELECT jsonb_agg(jsonb_build_object('id',i)) FROM generate_series(1,32) i) WHERE id=$1")
            .bind(run.id).execute(&app.db).await.unwrap();
        let before = snapshot(&app,run.id).await;
        let (status,body) = submit(&app,&request("队列满时不能部分切换配置")).await;
        assert_eq!(status,StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(wire::ApiError::decode(body.as_slice()).unwrap().code,"steering_full");
        assert_eq!(snapshot(&app,run.id).await,before,"model refresh and new input roll back together");
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; original Agent and device grants are immutable"]
async fn paused_send_uses_original_agent_and_preserves_snapshots_and_request_receipts() {
    database_case(|app| async move {
        let mut run = FixtureRun::start(&app,"同一助手继续处理").await;
        assert!(run.finish(&app,"已完成第一步。",RunDisposition::NeedsInput).await);
        let frozen: Value = sqlx::query_scalar("SELECT jsonb_build_object('id',agent_id,'name',agent_name,'instructions',agent_instructions,'revision',agent_revision,'device',device_id,'bindings',(SELECT jsonb_agg(device_id ORDER BY device_id) FROM run_device_bindings WHERE run_id=r.id)) FROM runs r WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        let model=Uuid::new_v4(); let replacement=Uuid::new_v4();
        sqlx::query("INSERT INTO model_connections(id,name,model_revision) VALUES($1,'other model',1)").bind(model).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO agents(id,name,model_connection_id) VALUES($1,'new device default',$2)").bind(replacement).bind(model).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO agent_device_bindings(agent_id,device_id) VALUES($1,'phone-local-01')").bind(replacement).execute(&app.db).await.unwrap();
        sqlx::query("UPDATE devices SET default_agent_id=$1 WHERE id='phone-local-01'").bind(replacement).execute(&app.db).await.unwrap();
        // The original identity now points at a different connection. Its latest
        // name and device bindings must not replace the run's frozen grants.
        let original_agent=Uuid::parse_str(frozen["id"].as_str().unwrap()).unwrap();
        let updated_model=Uuid::new_v4();
        sqlx::query("INSERT INTO model_connections(id,name,model_revision) VALUES($1,'original agent replacement',1)").bind(updated_model).execute(&app.db).await.unwrap();
        sqlx::query("UPDATE agents SET name='renamed original',instructions='later instructions',revision=revision+1,model_connection_id=$2 WHERE id=$1").bind(original_agent).bind(updated_model).execute(&app.db).await.unwrap();
        sqlx::query("DELETE FROM agent_device_bindings WHERE agent_id=$1").bind(original_agent).execute(&app.db).await.unwrap();
        let revision=replace_model(&app,run.id,"new-model","new-original-agent-key").await;
        let next=request("按原目标继续，保留第一步");
        assert_eq!(accept(&app,&next).await.run_id,run.id.to_string());
        let after: Value = sqlx::query_scalar("SELECT jsonb_build_object('id',agent_id,'name',agent_name,'instructions',agent_instructions,'revision',agent_revision,'device',device_id,'bindings',(SELECT jsonb_agg(device_id ORDER BY device_id) FROM run_device_bindings WHERE run_id=r.id)) FROM runs r WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(after,frozen);
        let binding:(Uuid,i64,Value)=sqlx::query_as("SELECT model_connection_id,model_revision,loop_state FROM runs WHERE id=$1").bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!((binding.0,binding.1),(updated_model,revision as i64));
        assert_eq!(binding.2["model_replay_boundary"],run.context.len());
        assert_eq!(settings::for_run(&app,run.id).await.unwrap().api_key,"new-original-agent-key");
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT text FROM messages WHERE id=$1").bind(run.message).fetch_one(&app.db).await.unwrap(),"已完成第一步。");
        pause(&app,run.id).await;
        replace_model(&app,run.id,"yet-another-model","later-key").await;
        let before_retry=snapshot(&app,run.id).await;
        assert_eq!(accept(&app,&next).await.run_id,run.id.to_string());
        assert_eq!(snapshot(&app,run.id).await,before_retry,"replaying an accepted request does not implicitly resume or select another model");
    }).await;
}

async fn wait_for_run_waiters(app: &App, count: i64) {
    tokio::time::timeout(Duration::from_secs(5),async {
        loop {
            let waiting:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock' AND query LIKE '%FROM runs WHERE id=$1%status=''paused'' FOR UPDATE%'")
                .fetch_one(&app.db).await.unwrap();
            if waiting >= count {return;}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("resume requests must reach the row-lock barrier");
}

async fn wait_for_conversation_waiters(app: &App, count: i64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock' AND query='SELECT id FROM conversations WHERE id=$1 FOR UPDATE'")
                .fetch_one(&app.db).await.unwrap();
            if waiting>=count {return;}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("requests must reach the shared conversation lock barrier");
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; deterministic concurrent resume and claim"]
async fn concurrent_resumes_have_one_winner_and_worker_cannot_claim_partial_refresh() {
    database_case(|app| async move {
        let run = FixtureRun::start(&app, "恢复一次").await;
        pause(&app, run.id).await;
        let revision = replace_model(&app, run.id, "same-task-new-model", "recovered-key").await;
        let mut blocker = app.db.begin().await.unwrap();
        sqlx::query("SELECT id FROM conversations WHERE id=$1 FOR UPDATE")
            .bind(run.conversation)
            .execute(&mut *blocker)
            .await
            .unwrap();
        let (a, b, ()) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(resume(&app, run.id), resume(&app, run.id), async {
                wait_for_conversation_waiters(&app, 2).await;
                assert!(
                    claim_due_run(&app).await.unwrap().is_none(),
                    "the worker cannot observe a half-rebound run"
                );
                blocker.commit().await.unwrap();
            })
        })
        .await
        .unwrap();
        assert!([a.0, b.0].contains(&StatusCode::OK));
        assert!([a.0, b.0].contains(&StatusCode::CONFLICT));
        assert_eq!(
            claim_due_run(&app).await.unwrap(),
            Some((run.id, run.message))
        );
        assert!(claim_due_run(&app).await.unwrap().is_none());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT model_revision FROM runs WHERE id=$1")
                .bind(run.id)
                .fetch_one(&app.db)
                .await
                .unwrap(),
            revision as i64
        );
        replace_model(&app, run.id, "future-model", "future-key").await;
        assert_eq!(
            accept(&app, &request("active steering stays pinned"))
                .await
                .run_id,
            run.id.to_string()
        );
        assert_eq!(resume(&app, run.id).await.0, StatusCode::CONFLICT);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT model_revision FROM runs WHERE id=$1")
                .bind(run.id)
                .fetch_one(&app.db)
                .await
                .unwrap(),
            revision as i64
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; model updates and recovery use the same registry lock"]
async fn recovery_serializes_with_registry_save_and_cancel_cannot_resurrect_a_run() {
    database_case(|app| async move {
        let run=FixtureRun::start(&app,"模型配置保存完成后恢复").await;
        pause(&app,run.id).await;
        let mut config=settings::for_run(&app,run.id).await.unwrap();config.api_key="saved-during-recovery".into();config.verified_at_ms=1;
        let revision=app.settings.test_revision(config);
        let mut writer=app.db.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(704117)").execute(&mut *writer).await.unwrap();
        let (result,())=tokio::time::timeout(Duration::from_secs(10),async {
            tokio::join!(resume(&app,run.id),async {
                wait_for_blocked_query(&app,"SELECT pg_advisory_xact_lock($1)").await;
                sqlx::query("UPDATE model_connections SET model_revision=$2 WHERE id=(SELECT model_connection_id FROM agents WHERE id=(SELECT agent_id FROM runs WHERE id=$1))")
                    .bind(run.id).bind(revision as i64).execute(&mut *writer).await.unwrap();
                writer.commit().await.unwrap();
            })
        }).await.unwrap();
        assert_eq!(result.0,StatusCode::OK);
        assert_eq!(settings::for_run(&app,run.id).await.unwrap().api_key,"saved-during-recovery");
        pause(&app,run.id).await;
        let mut cancel=app.db.begin().await.unwrap();
        sqlx::query("UPDATE runs SET status='canceled' WHERE id=$1").bind(run.id).execute(&mut *cancel).await.unwrap();
        let (result,())=tokio::join!(resume(&app,run.id),async {
            wait_for_run_waiters(&app,1).await;
            cancel.commit().await.unwrap();
        });
        assert_eq!(result.0,StatusCode::CONFLICT);
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT status FROM runs WHERE id=$1").bind(run.id).fetch_one(&app.db).await.unwrap(),"canceled");
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; recorded unknown actions never call devices or workspaces"]
async fn model_recovery_preserves_unknown_action_and_never_replays_it() {
    database_case(|app| async move {
        let mut run=FixtureRun::start(&app,"处理文件").await;
        let action=Uuid::now_v7();
        let call=json!({"type":"function_call","call_id":"unknown-write","local_action_id":action.to_string(),"name":"workspace_write","arguments":"{\"path\":\"result.txt\",\"content\":\"result\"}","provider_native":true});
        run.context.push(call.clone());run.pending.push(call.clone());
        checkpoint(&app,run.id,&mut run.context,&run.pending,&run.answer,2,&run.state).await.unwrap();
        sqlx::query("INSERT INTO harness_events(id,run_id,message_id,kind,title,status) VALUES($1,$2,$3,'tool','write','running')").bind(action).bind(run.id).bind(run.message).execute(&app.db).await.unwrap();
        let event:Value=sqlx::query_scalar("SELECT to_jsonb(e) FROM harness_events e WHERE id=$1").bind(action).fetch_one(&app.db).await.unwrap();
        pause(&app,run.id).await;replace_model(&app,run.id,"different-model","different-key").await;
        assert_eq!(resume(&app,run.id).await.0,StatusCode::OK);
        let preserved:(Value,Value)=sqlx::query_as("SELECT context,pending_tools FROM runs WHERE id=$1").bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(preserved,(json!(run.context),json!(run.pending)));
        let error=recorded_tool(&app,run.id,run.message,&call,&mut run.epoch,&CancellationToken::new()).await.unwrap_err();
        assert!(error.contains("结果未知"));
        assert_eq!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(e) FROM harness_events e WHERE id=$1").bind(action).fetch_one(&app.db).await.unwrap(),event);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM harness_events WHERE run_id=$1").bind(run.id).fetch_one(&app.db).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,Value>("SELECT pending_tools FROM runs WHERE id=$1").bind(run.id).fetch_one(&app.db).await.unwrap(),json!(run.pending));
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; raw context checkpoints preserve trusted replay boundaries"]
async fn repeated_model_switches_and_image_compaction_preserve_boundary_and_ledger() {
    database_case(|app| async move {
        let mut run=FixtureRun::start(&app,"保留原始工具记录").await;
        for i in 0..6 {
            run.context.extend([
                json!({"type":"reasoning","encrypted_content":format!("OLD_{i}")}),
                json!({"type":"function_call","call_id":format!("observe-{i}"),"name":"phone_observe","arguments":"{}","provider_native":true}),
                json!({"type":"function_call_output","call_id":format!("observe-{i}"),"output":[{"type":"input_text","text":json!({"observation_id":format!("obs-{i}"),"device_id":"phone-local-01"}).to_string()},{"type":"input_image","image_url":"data:image/png;base64,aGVsbG8="}]})
            ]);
        }
        let original_len=run.context.len();
        checkpoint(&app,run.id,&mut run.context,&run.pending,&run.answer,2,&run.state).await.unwrap();
        assert_eq!(run.context.len(),original_len,"image compaction cannot shift the trusted raw-context boundary");
        assert!(serde_json::to_string(&run.context).unwrap().contains(crate::agent_loop::ARCHIVED_OBSERVATION), "fixture must exercise actual image archival");
        assert_eq!(run.context.iter().filter_map(|item|item["output"].as_array()).flatten().filter(|part|part["type"]=="input_image").count(),2);
        let raw=run.context.clone();
        pause(&app,run.id).await;replace_model(&app,run.id,"second-model","second-key").await;
        assert_eq!(resume(&app,run.id).await.0,StatusCode::OK);
        let state:Value=sqlx::query_scalar("SELECT loop_state FROM runs WHERE id=$1").bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(state["model_replay_boundary"],original_len);
        run.state=serde_json::from_value(state).unwrap();
        let fresh=json!({"type":"reasoning","encrypted_content":"SECOND_MODEL_PRIVATE"});
        run.context.extend([fresh.clone(),json!({"type":"function_call","call_id":"new","name":"workspace_list","arguments":"{}","provider_native":true}),json!({"type":"function_call_output","call_id":"new","output":"verified new result"})]);
        claim_due_run(&app).await.unwrap().unwrap();
        checkpoint(&app,run.id,&mut run.context,&run.pending,&run.answer,3,&run.state).await.unwrap();
        assert_eq!(&run.context[..original_len],raw.as_slice());
        let projected=crate::model_context::for_api(&crate::model_context::after_model_switch(&run.context,run.state.model_replay_boundary));
        assert!(projected.contains(&fresh));assert!(!serde_json::to_string(&projected).unwrap().contains("OLD_"));
        pause(&app,run.id).await;replace_model(&app,run.id,"second-model","key-only-repair").await;
        assert_eq!(resume(&app,run.id).await.0,StatusCode::OK);
        let kept:Value=sqlx::query_scalar("SELECT loop_state FROM runs WHERE id=$1").bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(kept["model_replay_boundary"],original_len,"credential repair retains the already established boundary");
        pause(&app,run.id).await;replace_model(&app,run.id,"third-model","third-key").await;
        assert_eq!(resume(&app,run.id).await.0,StatusCode::OK);
        let (stored,state):(Value,Value)=sqlx::query_as("SELECT context,loop_state FROM runs WHERE id=$1").bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(stored,json!(run.context),"multiple switches never rewrite earlier reasoning or tool records");
        assert_eq!(state["model_replay_boundary"],run.context.len());
        let reloaded:crate::agent_loop::LoopState=serde_json::from_value(state).unwrap();
        let projected=crate::model_context::for_api(&crate::model_context::after_model_switch(&run.context,reloaded.model_replay_boundary));
        let serialized=serde_json::to_string(&projected).unwrap();
        assert!(!serialized.contains("SECOND_MODEL_PRIVATE"));assert!(serialized.contains("verified new result"));
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL and loopback HTTP capture; no external model/device calls"]
async fn resumed_provider_request_uses_saved_key_and_strips_previous_protocol_metadata() {
    database_case(|app| async move {
        let captured=Arc::new(std::sync::Mutex::new(Vec::<(HeaderMap,Value)>::new()));
        let capture=captured.clone();
        let router=Router::new().fallback(post(move |headers:HeaderMap,axum::Json(body):axum::Json<Value>| {
            let capture=capture.clone();async move {capture.lock().unwrap().push((headers,body));StatusCode::PAYMENT_REQUIRED}
        }));
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move {axum::serve(listener,router).await.unwrap()});
        let mut run=FixtureRun::start(&app,"继续已有任务").await;
        run.context.extend([
            json!({"type":"reasoning","encrypted_content":"OLD_ENCRYPTED","anthropic_blocks":[{"type":"thinking","thinking":"OLD_THINKING","signature":"OLD_SIGNATURE"}]}),
            json!({"type":"function_call","call_id":"read","name":"workspace_read","arguments":"{}","provider_native":true,"chat_reasoning_content":"OLD_REASONING","chat_extra_content":{"google":{"thought_signature":"OLD_EXTRA"}}}),
            json!({"type":"function_call_output","call_id":"read","output":"verified-original-result"})
        ]);
        checkpoint(&app,run.id,&mut run.context,&run.pending,&run.answer,2,&run.state).await.unwrap();
        for protocol in ["responses","chat-completions","anthropic-messages"] {
            pause(&app,run.id).await;
            let mut config=settings::for_run(&app,run.id).await.unwrap();
            config.protocol=protocol.into();config.provider="compatible".into();config.base_url=format!("http://{address}");config.model=format!("new-{protocol}");config.api_key=format!("saved-{protocol}");config.verified_at_ms=1;
            let revision=app.settings.test_revision(config);
            sqlx::query("UPDATE model_connections SET model_revision=$2 WHERE id=(SELECT model_connection_id FROM runs WHERE id=$1)").bind(run.id).bind(revision as i64).execute(&app.db).await.unwrap();
            assert_eq!(resume(&app,run.id).await.0,StatusCode::OK);
            claim_due_run(&app).await.unwrap().unwrap();
            let state:Value=sqlx::query_scalar("SELECT loop_state FROM runs WHERE id=$1").bind(run.id).fetch_one(&app.db).await.unwrap();
            let error=respond(&app,run.id,&run.context,&CancellationToken::new(),false,state["model_replay_boundary"].as_u64().unwrap() as usize).await.unwrap_err();
            assert!(error.contains("HTTP 402"),"loopback fixture must receive a correctly encoded request");
            {
            let seen=captured.lock().unwrap();let (headers,body)=seen.last().unwrap();
            assert_eq!(body["model"],format!("new-{protocol}"));
            let key=if protocol=="anthropic-messages" {headers.get("x-api-key").unwrap().to_str().unwrap().to_string()} else {headers.get("authorization").unwrap().to_str().unwrap().strip_prefix("Bearer ").unwrap().to_string()};
            assert_eq!(key,format!("saved-{protocol}"));
            let encoded=body.to_string();assert!(!encoded.contains("OLD_"));assert!(encoded.contains("verified-original-result"));
            }
            assert_eq!(sqlx::query_scalar::<_,Value>("SELECT context FROM runs WHERE id=$1").bind(run.id).fetch_one(&app.db).await.unwrap(),json!(run.context));
        }
        assert_eq!(captured.lock().unwrap().len(),3);server.abort();
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; legacy annotation must not form a reverse lock cycle"]
async fn legacy_null_agent_fails_before_registry_lock_and_can_be_initialized_afterward() {
    database_case(|app| async move {
        let run = FixtureRun::start(&app, "legacy fixture").await;
        pause(&app, run.id).await;
        sqlx::query("UPDATE runs SET agent_id=NULL WHERE id=$1")
            .bind(run.id)
            .execute(&app.db)
            .await
            .unwrap();
        let before = snapshot(&app, run.id).await;
        let mut registry = app.db.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(704117)")
            .execute(&mut *registry)
            .await
            .unwrap();
        let (status, body) = tokio::time::timeout(Duration::from_secs(2), resume(&app, run.id))
            .await
            .expect("NULL-agent recovery must not wait for registry initialization");
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(
            wire::ApiError::decode(body.as_slice()).unwrap().code,
            "model_missing"
        );
        assert_eq!(snapshot(&app, run.id).await, before);
        registry.rollback().await.unwrap();
        crate::registry::initialize(&app.db, &app.settings)
            .await
            .unwrap();
        assert_eq!(resume(&app, run.id).await.0, StatusCode::OK);
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; both lock serialization orders of resume versus paused send"]
async fn resume_and_paused_send_accept_input_once_without_switching_an_active_run() {
    for resume_first in [true, false] {
        database_case(|app| async move {
            let mut run = FixtureRun::start(&app, "原目标").await;
            assert!(
                run.finish(&app, "第一阶段真实内容。", RunDisposition::NeedsInput)
                    .await
            );
            let revision = replace_model(&app, run.id, "recovered-model", "recovered-key").await;
            let next = request("新约束必须被接受一次");
            let mut blocker = app.db.begin().await.unwrap();
            sqlx::query("SELECT id FROM conversations WHERE id=$1 FOR UPDATE")
                .bind(run.conversation)
                .execute(&mut *blocker)
                .await
                .unwrap();
            let (resumed, sent) = tokio::time::timeout(Duration::from_secs(10), async {
                if resume_first {
                    let (resumed, sent, ()) = tokio::join!(
                        resume(&app, run.id),
                        async {
                            wait_for_conversation_waiters(&app, 1).await;
                            submit(&app, &next).await
                        },
                        async {
                            wait_for_conversation_waiters(&app, 2).await;
                            blocker.commit().await.unwrap();
                        }
                    );
                    (resumed, sent)
                } else {
                    let (sent, resumed, ()) = tokio::join!(
                        submit(&app, &next),
                        async {
                            wait_for_conversation_waiters(&app, 1).await;
                            resume(&app, run.id).await
                        },
                        async {
                            wait_for_conversation_waiters(&app, 2).await;
                            blocker.commit().await.unwrap();
                        }
                    );
                    (resumed, sent)
                }
            })
            .await
            .expect("resume/send must settle without a lock cycle");
            assert_eq!(
                resumed.0,
                if resume_first {
                    StatusCode::OK
                } else {
                    StatusCode::CONFLICT
                }
            );
            assert_eq!(
                sent.0,
                StatusCode::ACCEPTED,
                "response: {:?}",
                wire::ApiError::decode(sent.1.as_slice())
            );
            assert_eq!(
                wire::ChatState::decode(sent.1.as_slice()).unwrap().run_id,
                run.id.to_string()
            );
            let row = sqlx::query("SELECT status,model_revision,steering FROM runs WHERE id=$1")
                .bind(run.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
            assert_eq!(row.get::<String, _>("status"), "queued");
            assert_eq!(row.get::<i64, _>("model_revision"), revision as i64);
            assert_eq!(row.get::<Value, _>("steering").as_array().unwrap().len(), 1);
            let before = snapshot(&app, run.id).await;
            assert_eq!(accept(&app, &next).await.run_id, run.id.to_string());
            assert_eq!(snapshot(&app, run.id).await, before);
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; historical system notices must stay outside dialogue after recovery"]
async fn recovery_durably_preserves_exact_legacy_notice_classification() {
    for (text, error, state) in [
        (
            "连续三轮没有新增执行进展，任务已暂停。工具失败",
            "连续三轮没有新增执行进展，任务已暂停。工具失败",
            json!({"stalls":3,"last_issue":"工具失败"}),
        ),
        (
            "已重新获取手机画面4次仍未恢复，任务保留并暂停。观察失败",
            "已重新获取手机画面4次仍未恢复，任务保留并暂停。观察失败",
            json!({"observation_recoveries":4,"last_issue":"观察失败"}),
        ),
        (
            "模型服务返回 HTTP 402",
            "需要补充：模型服务返回 HTTP 402",
            json!({"model_retries":1,"last_issue":"模型服务返回 HTTP 402"}),
        ),
    ] {
        database_case(|app| async move {
            let run=FixtureRun::start(&app,"继续原任务").await;
            sqlx::query("UPDATE runs SET status='paused',error=$2,loop_state=$3 WHERE id=$1").bind(run.id).bind(error).bind(state).execute(&app.db).await.unwrap();
            sqlx::query("UPDATE messages SET text=$2 WHERE id=$1").bind(run.message).bind(text).execute(&app.db).await.unwrap();
            let before=crate::http::state(&app,run.conversation).await.unwrap();
            assert!(before.messages.iter().find(|m|m.id==run.message.to_string()).unwrap().text.is_empty());
            assert_eq!(resume(&app,run.id).await.0,StatusCode::OK);
            let after=crate::http::state(&app,run.conversation).await.unwrap();
            let message=after.messages.iter().find(|m|m.id==run.message.to_string()).unwrap();
            assert!(message.text.is_empty(),"clearing retry state must not reintroduce a legacy operational notice as assistant dialogue");
            assert_eq!(message.system_notice,text);
            assert_eq!(sqlx::query_scalar::<_,String>("SELECT text FROM messages WHERE id=$1").bind(run.message).fetch_one(&app.db).await.unwrap(),text,"retain raw historical message exactly");
        }).await;
    }
}
