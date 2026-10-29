//! Real PostgreSQL contracts for accepted input and published conversation fragments.
//! Each case creates its own database; no model, host device or production service runs.
use super::*;
use axum::{body::Bytes, extract::State, http::HeaderMap};
use sqlx::{Connection, postgres::PgConnectOptions, postgres::PgPoolOptions};
use std::{future::Future, panic::AssertUnwindSafe, str::FromStr};

async fn database_case<F, Fut>(test: F)
where
    F: FnOnce(Shared) -> Fut,
    Fut: Future<Output = ()>,
{
    let url = std::env::var("AI_PHONE_TEST_PERSISTENCE_DATABASE_URL")
        .expect("explicit disposable database URL is required");
    let options = PgConnectOptions::from_str(&url).unwrap();
    assert_eq!(options.get_username(), "aiphone_test");
    assert_eq!(options.get_host(), "127.0.0.1");
    assert_ne!(options.get_port(), 55482, "refuse production PostgreSQL");
    let name = format!("aiphone_steering_regression_{}", Uuid::new_v4().simple());
    let mut admin = sqlx::PgConnection::connect_with(&options).await.unwrap();
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&mut admin)
        .await
        .unwrap();
    let db = PgPoolOptions::new()
        .max_connections(12)
        .connect_with(options.database(&name))
        .await
        .unwrap();
    let root = std::env::temp_dir().join(&name);
    tokio::fs::create_dir_all(root.join("data/attachments"))
        .await
        .unwrap();
    let app = Arc::new(App {
        db: db.clone(),
        root: root.clone(),
        token: "steering-fixture-token".into(),
        phone_token: "unused".into(),
        settings: settings::SettingsStore::test_configured(&root),
        local: reqwest::Client::new(),
        remote: reqwest::Client::new(),
        active: Mutex::new(None),
        login_attempts: Mutex::new(vec![]),
    });
    let outcome = AssertUnwindSafe(async {
        sqlx::migrate!().run(&db).await.unwrap();
        test(app).await;
    })
    .catch_unwind()
    .await;
    db.close().await;
    sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
        .execute(&mut admin)
        .await
        .unwrap();
    tokio::fs::remove_dir_all(root).await.unwrap();
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

fn request(text: &str) -> wire::ChatSendRequest {
    wire::ChatSendRequest {
        text: text.into(),
        request_id: Uuid::new_v4().to_string(),
        ..Default::default()
    }
}

fn input(text: &str) -> Value {
    json!({"role":"user","content":[{"type":"input_text","text":text}]})
}

async fn submit(app: &Shared, request: &wire::ChatSendRequest) -> (StatusCode, Vec<u8>) {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        "Bearer steering-fixture-token".parse().unwrap(),
    );
    headers.insert("content-type", MEDIA.parse().unwrap());
    let response = crate::http::send(
        State(app.clone()),
        headers,
        Ok(Bytes::from(request.encode_to_vec())),
    )
    .await;
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (status, body.to_vec())
}

async fn accept(app: &Shared, request: &wire::ChatSendRequest) -> wire::ChatState {
    let (status, body) = submit(app, request).await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "request was not durably accepted"
    );
    wire::ChatState::decode(body.as_slice()).unwrap()
}

struct FixtureRun {
    id: Uuid,
    message: Uuid,
    conversation: Uuid,
    context: Vec<Value>,
    pending: Vec<Value>,
    answer: String,
    state: crate::agent_loop::LoopState,
    epoch: u64,
}

impl FixtureRun {
    async fn start(app: &Shared, text: &str) -> Self {
        let admitted = accept(app, &request(text)).await;
        let id = Uuid::parse_str(&admitted.run_id).unwrap();
        let message = sqlx::query_scalar(
            "UPDATE runs SET status='running' WHERE id=$1 RETURNING assistant_message_id",
        )
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap();
        Self {
            id,
            message,
            conversation: Uuid::parse_str(&admitted.conversation_id).unwrap(),
            context: vec![input(text)],
            pending: vec![],
            answer: String::new(),
            state: crate::agent_loop::LoopState::default(),
            epoch: 7,
        }
    }

    async fn consume(&mut self, app: &App) -> Result<bool, String> {
        consume_steering(
            app,
            self.id,
            &mut self.message,
            SteeringInput {
                context: &mut self.context,
                pending: &mut self.pending,
                answer: &mut self.answer,
                step: 2,
                state: &mut self.state,
                epoch: &mut self.epoch,
            },
        )
        .await
    }

    async fn finish(&mut self, app: &App, answer: &str, disposition: RunDisposition) -> bool {
        persist_disposition(
            app,
            self.id,
            self.message,
            Checkpoint {
                context: &mut self.context,
                pending: &self.pending,
                answer,
                step: 2,
                state: &self.state,
            },
            disposition,
            None,
        )
        .await
        .unwrap()
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no production data, model or device calls"]
async fn steering_preserves_published_text_events_and_rotates_the_assistant_turn() {
    database_case(|app| async move {
        let mut run = FixtureRun::start(&app, "处理表格，保留原始文件").await;
        assert!(run.finish(&app, "已完成读取，等待列名确认。", RunDisposition::Paused).await);
        let old_message = run.message;
        let event = Uuid::now_v7();
        sqlx::query(r#"INSERT INTO harness_events(id,run_id,message_id,kind,title,content,status,result,finished_at) VALUES($1,$2,$3,'tool','已读取文件','第一阶段真实记录','completed','{"success":true}'::jsonb,now())"#)
            .bind(event).bind(run.id).bind(old_message).execute(&app.db).await.unwrap();
        let before: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM harness_events e WHERE id=$1")
            .bind(event).fetch_one(&app.db).await.unwrap();
        let supplement = request("使用中文列名，继续保留原始文件");
        assert_eq!(accept(&app, &supplement).await.run_id, run.id.to_string());
        sqlx::query("UPDATE runs SET status='running' WHERE id=$1").bind(run.id).execute(&app.db).await.unwrap();
        assert!(run.consume(&app).await.unwrap());

        let old_text: String = sqlx::query_scalar("SELECT text FROM messages WHERE id=$1")
            .bind(old_message).fetch_one(&app.db).await.unwrap();
        assert_eq!(old_text, "已完成读取，等待列名确认。", "a supplement must not erase already published assistant text");
        let after: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM harness_events e WHERE id=$1")
            .bind(event).fetch_one(&app.db).await.unwrap();
        assert_eq!(before, after, "an earlier event keeps its original message and contents");
        let current: Uuid = sqlx::query_scalar("SELECT assistant_message_id FROM runs WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_ne!(current, old_message, "new input starts a new assistant fragment");
        assert_eq!(run.message, current, "the running worker must receive the rotated pointer");
        let active = crate::http::state(&app, run.conversation).await.unwrap();
        let previous = active.messages.iter().find(|message| message.id == old_message.to_string()).unwrap();
        assert_eq!(previous.run_id, run.id.to_string());
        assert!(!previous.active);
        assert!(previous.has_activity, "completed fragments retain their expandable activity");
        let current = active.messages.iter().find(|message| message.id == run.message.to_string()).unwrap();
        assert!(current.active);
        assert!(run.finish(&app, "中文列名处理完成，原文件保留。", RunDisposition::Completed).await);
        let displayed = crate::http::state(&app, run.conversation).await.unwrap();
        assert_eq!(displayed.messages.iter().filter(|m| m.text == old_text).count(), 1);
        assert_eq!(displayed.messages.iter().filter(|m| m.text == "中文列名处理完成，原文件保留。").count(), 1);
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; finalization uses the durable current fragment"]
async fn failed_attempt_after_steering_does_not_rewrite_the_prior_fragment() {
    database_case(|app| async move {
        let mut run = FixtureRun::start(&app, "处理工作文件").await;
        assert!(
            run.finish(&app, "第一阶段已记录。", RunDisposition::Paused)
                .await
        );
        let claimed_message = run.message;
        accept(&app, &request("下一阶段使用中文文件名")).await;
        sqlx::query("UPDATE runs SET status='running' WHERE id=$1")
            .bind(run.id)
            .execute(&app.db)
            .await
            .unwrap();
        assert!(run.consume(&app).await.unwrap());
        assert_ne!(run.message, claimed_message);
        finish_attempt(
            &app,
            run.id,
            claimed_message,
            &Err("fixture model disconnected".into()),
            false,
        )
        .await
        .unwrap();
        let state = crate::http::state(&app, run.conversation).await.unwrap();
        let previous = state
            .messages
            .iter()
            .find(|message| message.id == claimed_message.to_string())
            .unwrap();
        let current = state
            .messages
            .iter()
            .find(|message| message.id == run.message.to_string())
            .unwrap();
        assert_eq!(previous.text, "第一阶段已记录。");
        assert_eq!(current.text, "fixture model disconnected");
        assert_eq!(state.run_status, "failed");
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no production data, model or device calls"]
async fn concurrent_supplements_and_retries_keep_the_accepted_order() {
    database_case(|app| async move {
        let mut run = FixtureRun::start(&app, "原任务：不要覆盖已有文件").await;
        let initial = crate::http::state(&app, run.conversation).await.unwrap();
        let requests: Vec<_> = (0..8).map(|i| request(&format!("追加要求 {i}"))).collect();
        let responses = futures_util::future::join_all(requests.iter().map(|r| accept(&app, r))).await;
        assert!(responses.iter().all(|r| r.run_id == run.id.to_string()));
        let queue: Value = sqlx::query_scalar("SELECT steering FROM runs WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        let accepted_ids: Vec<_> = queue.as_array().unwrap().iter()
            .map(|entry| entry["id"].as_str().unwrap().to_owned()).collect();
        assert_eq!(accepted_ids.len(), requests.len());

        let retried = futures_util::future::join_all(requests.iter().rev().map(|r| accept(&app, r))).await;
        assert!(retried.iter().all(|r| r.run_id == run.id.to_string()));
        let after_retry: Value = sqlx::query_scalar("SELECT steering FROM runs WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(queue, after_retry, "retry cannot append or reorder accepted input");
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM run_inputs WHERE run_id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(receipts, requests.len() as i64);
        let mut changed = requests[0].clone();
        changed.text = "同一请求不能变更内容".into();
        assert_eq!(submit(&app, &changed).await.0, StatusCode::CONFLICT);

        let ordered = crate::http::state(&app, run.conversation).await.unwrap();
        let shown_ids: Vec<_> = ordered.messages.iter().filter(|m| accepted_ids.contains(&m.request_id))
            .map(|m| m.request_id.clone()).collect();
        assert_eq!(shown_ids, accepted_ids, "display and durable steering share the accepted order");
        assert!(ordered.messages.windows(2).all(|pair| pair[0].ordinal < pair[1].ordinal));
        assert!(ordered.revision > initial.revision);
        assert_eq!(ordered.revision, ordered.messages.last().unwrap().ordinal);
        sqlx::query("UPDATE messages SET created_at='2001-01-01'::timestamptz + (100000-ordinal)*interval '1 second' WHERE conversation_id=$1")
            .bind(run.conversation).execute(&app.db).await.unwrap();
        let clock_changed = crate::http::state(&app, run.conversation).await.unwrap();
        assert_eq!(clock_changed.messages.iter().map(|m| &m.id).collect::<Vec<_>>(), ordered.messages.iter().map(|m| &m.id).collect::<Vec<_>>(), "wall clock changes cannot reorder accepted messages");
        assert!(run.consume(&app).await.unwrap());
        assert!(!run.consume(&app).await.unwrap(), "the same batch is not consumed twice");
        for r in &requests {
            assert_eq!(run.context.iter().filter(|item| **item == input(&r.text)).count(), 1);
        }
        let persisted: Value = sqlx::query_scalar("SELECT context FROM runs WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(persisted, json!(run.context), "restart reads the same incorporated input");
        let final_state = crate::http::state(&app, run.conversation).await.unwrap();
        assert!(final_state.revision > ordered.revision, "rotated assistant fragment advances the snapshot revision");
    }).await;
}

async fn wait_for_blocked_query(app: &App, pattern: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND state='active' AND wait_event_type='Lock' AND query LIKE $1)")
                .bind(pattern).fetch_one(&app.db).await.unwrap();
            if waiting { return; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("expected database lock contention was not observed");
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; deterministic row-lock barriers"]
async fn completion_and_new_input_respect_both_serialization_orders() {
    for send_first in [true, false] {
        database_case(|app| async move {
            let mut run = FixtureRun::start(&app, "原任务").await;
            let run_id = run.id;
            let conversation = run.conversation;
            let mut blocker = app.db.begin().await.unwrap();
            sqlx::query("SELECT id FROM runs WHERE id=$1 FOR UPDATE")
                .bind(run_id)
                .fetch_one(&mut *blocker)
                .await
                .unwrap();
            let next = request("另一个必须保留的要求");
            let sender_wait = "SELECT id,status,agent_id FROM runs%FOR UPDATE%";
            let finisher_wait = "SELECT steering,status%FROM runs%FOR UPDATE%";
            let (admitted, completed) = tokio::time::timeout(Duration::from_secs(10), async {
                if send_first {
                    let (admitted, completed, ()) = tokio::join!(
                        accept(&app, &next),
                        async {
                            wait_for_blocked_query(&app, sender_wait).await;
                            run.finish(&app, "旧任务完成", RunDisposition::Completed)
                                .await
                        },
                        async {
                            wait_for_blocked_query(&app, finisher_wait).await;
                            blocker.commit().await.unwrap();
                        }
                    );
                    (admitted, completed)
                } else {
                    let (completed, admitted, ()) = tokio::join!(
                        run.finish(&app, "旧任务完成", RunDisposition::Completed),
                        async {
                            wait_for_blocked_query(&app, finisher_wait).await;
                            accept(&app, &next).await
                        },
                        async {
                            wait_for_blocked_query(&app, sender_wait).await;
                            blocker.commit().await.unwrap();
                        }
                    );
                    (admitted, completed)
                }
            })
            .await
            .expect("row-lock scenario did not settle");
            if send_first {
                assert!(!completed, "accepted pending input vetoes stale completion");
                assert_eq!(admitted.run_id, run_id.to_string());
                assert!(run.consume(&app).await.unwrap());
                assert!(
                    run.finish(&app, "已结合新要求完成", RunDisposition::Completed)
                        .await
                );
            } else {
                assert!(completed);
                assert_ne!(
                    admitted.run_id,
                    run_id.to_string(),
                    "input after committed completion starts its own run"
                );
            }
            let final_state = crate::http::state(&app, conversation).await.unwrap();
            assert_eq!(
                final_state
                    .messages
                    .iter()
                    .filter(|m| m.request_id == next.request_id)
                    .count(),
                1
            );
            assert_eq!(
                accept(&app, &next).await.run_id,
                admitted.run_id,
                "retry stays with its accepted run across the completion race"
            );
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; unknown side effects are never executed"]
async fn unknown_side_effect_keeps_pending_input_and_published_output_untouched() {
    database_case(|app| async move {
        let mut run = FixtureRun::start(&app, "手机处理文件").await;
        assert!(run.finish(&app, "文件已读取，下一步待核对。", RunDisposition::Paused).await);
        sqlx::query("UPDATE runs SET status='running' WHERE id=$1").bind(run.id).execute(&app.db).await.unwrap();
        let action = Uuid::now_v7();
        let call = json!({"type":"function_call","call_id":"unknown-tap","local_action_id":action.to_string(),"name":"phone_tap","arguments":"{\"x\":10,\"y\":20}"});
        run.context.push(call.clone());
        run.pending.push(call);
        checkpoint(&app, run.id, &mut run.context, &run.pending, &run.answer, 2, &run.state).await.unwrap();
        sqlx::query("INSERT INTO harness_events(id,run_id,message_id,kind,title,content,status) VALUES($1,$2,$3,'tool','点击手机','执行已派发，回执未知','running')")
            .bind(action).bind(run.id).bind(run.message).execute(&app.db).await.unwrap();
        let next = request("先不要移动原始文件");
        accept(&app, &next).await;
        let before: Value = sqlx::query_scalar("SELECT jsonb_build_object('steering',steering,'context',context,'pending',pending_tools,'message',assistant_message_id) FROM runs WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        let error = run.consume(&app).await.unwrap_err();
        assert!(error.starts_with("paused:unknown_outcome:"));
        let after: Value = sqlx::query_scalar("SELECT jsonb_build_object('steering',steering,'context',context,'pending',pending_tools,'message',assistant_message_id) FROM runs WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(before, after, "uncertain execution cannot consume, rewrite or replay pending work");
        let events: i64 = sqlx::query_scalar("SELECT count(*) FROM harness_events WHERE run_id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(events, 1, "no replacement tool action is created");
        let event: (String, Option<Value>) = sqlx::query_as("SELECT status,result FROM harness_events WHERE id=$1")
            .bind(action).fetch_one(&app.db).await.unwrap();
        assert_eq!(event, ("running".into(), None));
        let text: String = sqlx::query_scalar("SELECT text FROM messages WHERE id=$1")
            .bind(run.message).fetch_one(&app.db).await.unwrap();
        assert_eq!(text, "文件已读取，下一步待核对。");
        assert_eq!(accept(&app, &next).await.run_id, run.id.to_string());
        let queue: Value = sqlx::query_scalar("SELECT steering FROM runs WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(queue.as_array().unwrap().len(), 1);
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; context projection makes no model calls"]
async fn compacted_model_context_retains_all_user_constraints_and_raw_history() {
    database_case(|app| async move {
        let mut run = FixtureRun::start(&app, "原任务：整理表格，不覆盖、不发送原始文件").await;
        let mut constraints = run.context.clone();
        let system = json!({"role":"system","content":[{"type":"input_text","text":"任何文件移动须核对实际结果"}]});
        run.context.push(system.clone());
        for i in 0..28 {
            let call_id = format!("context-{i}");
            run.context.push(json!({"type":"function_call","call_id":call_id,"name":"workspace_list","arguments":"{}"}));
            run.context.push(json!({"type":"function_call_output","call_id":call_id,"output":[{"type":"input_text","text":"fixture-file-detail ".repeat(400)}]}));
            if i % 4 == 0 {
                let extra = input(&format!("补充 {i}：保留第 {i} 列，不发送给他人"));
                constraints.push(extra.clone());
                run.context.push(extra);
            }
        }
        checkpoint(&app, run.id, &mut run.context, &run.pending, &run.answer, 28, &run.state).await.unwrap();
        let original = run.context.clone();
        for projected in [crate::model_context::for_api(&run.context), crate::model_context::for_chat_api(&run.context)] {
            assert!(serde_json::to_vec(&projected).unwrap().len() < serde_json::to_vec(&original).unwrap().len(), "fixture must actually cross the compression boundary");
            for constraint in &constraints {
                assert_eq!(projected.iter().filter(|item| *item == constraint).count(), 1, "each original user instruction survives exactly once");
            }
            assert_eq!(projected.iter().filter(|item| **item == system).count(), 1);
        }
        assert_eq!(run.context, original);
        let stored: Value = sqlx::query_scalar("SELECT context FROM runs WHERE id=$1")
            .bind(run.id).fetch_one(&app.db).await.unwrap();
        assert_eq!(stored, json!(original), "projection must not replace the durable raw context");
    }).await;
}

struct FixtureServer(tokio::task::JoinHandle<()>);
impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL and loopback slow HTTP model only"]
async fn successive_inputs_interrupt_a_slow_model_without_canceling_the_run() {
    use axum::{Json, Router, body::Body, routing::post};
    database_case(|mut app| async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, mut received) = tokio::sync::mpsc::channel::<Value>(4);
        let router = Router::new().route(
            "/v1/responses",
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
        let _server = FixtureServer(tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }));
        let config = settings::ModelConfig {
            api_key: String::new(),
            base_url: format!("http://{address}/v1"),
            model: "slow-steering-fixture".into(),
            provider: "compatible".into(),
            protocol: "responses".into(),
            proxy_url: String::new(),
            verified_at_ms: 0,
        };
        tokio::fs::write(
            app.root.join("data/model/revisions/1.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .await
        .unwrap();
        let configured = settings::SettingsStore::open(&app.root).unwrap();
        Arc::get_mut(&mut app)
            .expect("fixture app must have one owner before requests")
            .settings = configured;
        let original = "原任务：只整理副本，不发送原文件";
        let mut run = FixtureRun::start(&app, original).await;
        let cancel = CancellationToken::new();
        let mut expected = vec![input(original)];
        for text in [
            "第一条补充：所有金额保留两位小数",
            "第二条补充：再加一张汇总表",
        ] {
            let next = request(text);
            let (model, admitted_at) = tokio::time::timeout(Duration::from_secs(4), async {
                tokio::join!(
                    respond_to_current_input(&app, run.id, &run.context, &cancel, false),
                    async {
                        let body = received
                            .recv()
                            .await
                            .expect("slow local model received a request");
                        assert_eq!(body["model"], "slow-steering-fixture");
                        let projected = body["input"].as_array().unwrap();
                        for constraint in &expected {
                            assert_eq!(
                                projected.iter().filter(|item| *item == constraint).count(),
                                1
                            );
                        }
                        assert_eq!(accept(&app, &next).await.run_id, run.id.to_string());
                        std::time::Instant::now()
                    }
                )
            })
            .await
            .expect("new input must release the waiting model without a provider response");
            assert!(
                model.unwrap().is_none(),
                "stale model output is discarded before tools are planned"
            );
            assert!(
                admitted_at.elapsed() < Duration::from_secs(1),
                "durable input should be noticed promptly"
            );
            assert!(
                !cancel.is_cancelled(),
                "interrupting model inference must not cancel the run or device actions"
            );
            let queue: Value = sqlx::query_scalar("SELECT steering FROM runs WHERE id=$1")
                .bind(run.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
            assert_eq!(queue.as_array().unwrap().len(), 1);
            assert_eq!(queue[0]["id"], next.request_id);
            assert!(run.consume(&app).await.unwrap());
            expected.push(input(text));
            for constraint in &expected {
                assert_eq!(
                    run.context
                        .iter()
                        .filter(|item| *item == constraint)
                        .count(),
                    1
                );
            }
            let stored: Value = sqlx::query_scalar("SELECT context FROM runs WHERE id=$1")
                .bind(run.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
            assert_eq!(stored, json!(run.context));
        }
        assert!(
            received.try_recv().is_err(),
            "only two local model requests were made"
        );
        let state = crate::http::state(&app, run.conversation).await.unwrap();
        assert_eq!(
            state
                .messages
                .iter()
                .filter(|message| message.role == "user")
                .count(),
            3
        );
        assert_eq!(state.run_status, "running");
    })
    .await;
}
