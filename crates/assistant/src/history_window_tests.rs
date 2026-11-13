//! Disposable PostgreSQL only: no App, model, guest, host files or real credentials.
use super::*;
use futures_util::{FutureExt, future::BoxFuture};
use prost::Message;
use sqlx::{
    Connection,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{panic::AssertUnwindSafe, str::FromStr};

fn request(limit: u32) -> wire::ChatHistoryRequest {
    wire::ChatHistoryRequest {
        limit,
        ..Default::default()
    }
}

fn parsed(value: &wire::ChatHistoryRequest) -> Request {
    Request::parse(value).unwrap().unwrap()
}

#[test]
fn bounds_are_explicit_and_legacy_does_not_accept_hidden_window_selectors() {
    assert!(Request::parse(&request(0)).unwrap().is_none());
    assert!(Request::parse(&request(1)).unwrap().is_some());
    assert!(Request::parse(&request(MAX_PAGE)).unwrap().is_some());
    assert_eq!(
        Request::parse(&request(MAX_PAGE + 1)).unwrap_err(),
        "history_window_limit"
    );
    for field in ["anchor", "watch", "receipt"] {
        let mut value = request(0);
        let id = Uuid::new_v4().to_string();
        match field {
            "anchor" => value.before_message_id = id,
            "watch" => value.watch_message_ids.push(id),
            _ => value.receipt_request_ids.push(id),
        }
        assert_eq!(Request::parse(&value).unwrap_err(), "history_window_limit");
    }
}

#[test]
fn identifiers_are_parsed_once_and_caps_apply_before_deduplication() {
    for (watch, receipt) in [(MAX_WATCH + 1, 0), (0, MAX_RECEIPTS + 1)] {
        let value = wire::ChatHistoryRequest {
            watch_message_ids: vec![Uuid::new_v4().to_string(); watch],
            receipt_request_ids: vec![Uuid::new_v4().to_string(); receipt],
            ..request(40)
        };
        assert_eq!(Request::parse(&value).unwrap_err(), "history_window_limit");
    }
    let id = Uuid::new_v4().to_string();
    let mut value = wire::ChatHistoryRequest {
        watch_message_ids: vec![id.clone(), id],
        ..request(40)
    };
    assert_eq!(parsed(&value).watch.len(), 1);
    value.receipt_request_ids.push("malformed".into());
    assert_eq!(
        Request::parse(&value).unwrap_err(),
        "history_window_identifier"
    );
    value.receipt_request_ids.clear();
    value.before_message_id = "malformed".into();
    assert_eq!(
        Request::parse(&value).unwrap_err(),
        "history_window_identifier"
    );
}

async fn database_case(test: impl for<'a> FnOnce(&'a PgPool) -> BoxFuture<'a, ()>) {
    let url = std::env::var("AI_PHONE_TEST_PERSISTENCE_DATABASE_URL")
        .expect("explicit labelled disposable PostgreSQL required");
    let options = PgConnectOptions::from_str(&url).unwrap();
    assert_eq!(
        options.get_username(),
        "aiphone_test",
        "refuse production database credentials"
    );
    assert_eq!(options.get_host(), "127.0.0.1", "loopback fixture only");
    assert_ne!(
        options.get_port(),
        55482,
        "refuse production PostgreSQL port"
    );
    let name = format!("aiphone_history_window_{}", Uuid::new_v4().simple());
    let mut admin = sqlx::PgConnection::connect_with(&options).await.unwrap();
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&mut admin)
        .await
        .unwrap();
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(options.database(&name))
        .await
        .unwrap();
    let result = AssertUnwindSafe(async {
        sqlx::migrate!().run(&db).await.unwrap();
        test(&db).await;
    })
    .catch_unwind()
    .await;
    db.close().await;
    drop(db);
    sqlx::query(&format!("DROP DATABASE {name}"))
        .execute(&mut admin)
        .await
        .unwrap();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

async fn conversation(db: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM conversations WHERE owner_id='local-user'")
        .fetch_one(db)
        .await
        .unwrap()
}

async fn message(
    db: &PgPool,
    conversation: Uuid,
    role: &str,
    text: &str,
    receipt: Option<Uuid>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO messages(id,conversation_id,role,text,request_id) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(conversation)
    .bind(role)
    .bind(text)
    .bind(receipt)
    .execute(db)
    .await
    .unwrap();
    id
}

async fn run(db: &PgPool, conversation: Uuid, origin: &str, status: &str) -> (Uuid, Uuid) {
    let head = message(db, conversation, "assistant", "outside-tail head", None).await;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,origin) VALUES($1,$2,$3,$1,'history-fixture',$4,$5)")
        .bind(id).bind(conversation).bind(head).bind(status).bind(origin).execute(db).await.unwrap();
    sqlx::query("UPDATE messages SET run_id=$2 WHERE id=$1")
        .bind(head)
        .bind(id)
        .execute(db)
        .await
        .unwrap();
    (id, head)
}

async fn bulk(db: &PgPool, conversation: Uuid, amount: i32) {
    sqlx::query("INSERT INTO messages(id,conversation_id,role,text) SELECT md5('bulk:'||$1::text||':'||n::text)::uuid,$1,'user','fixture '||n FROM generate_series(1,$2) n")
        .bind(conversation).bind(amount).execute(db).await.unwrap();
}

async fn ordinal(db: &PgPool, id: Uuid) -> u64 {
    sqlx::query_scalar::<_, i64>("SELECT ordinal FROM messages WHERE id=$1")
        .bind(id)
        .fetch_one(db)
        .await
        .unwrap() as u64
}

#[tokio::test]
#[ignore = "labelled disposable PostgreSQL; parent schedules execution"]
async fn real_pg_row_bound_receipts_and_lane_heads_do_not_scan_into_the_response() {
    database_case(|db| {
        Box::pin(async move {
            let conversation = conversation(db).await;
            let (user, user_head) = run(db, conversation, "user", "paused").await;
            let (_, exploration_head) = run(db, conversation, "exploration", "queued").await;
            let (_, notification_head) = run(db, conversation, "notification", "running").await;
            let mut paused_head = notification_head;
            for _ in 0..100 {
                (_, paused_head) = run(db, conversation, "notification", "paused").await;
            }
            let (_, handoff_head) = run(db, conversation, "device-handoff", "running").await;
            let receipt = Uuid::new_v4();
            let received = message(
                db,
                conversation,
                "user",
                "exact old accepted input",
                Some(receipt),
            )
            .await;
            let first = Uuid::new_v4();
            let second = Uuid::new_v4();
            let foreign = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO attachments(id,owner_id,name,media_type,size_bytes,storage_key)
                         VALUES($1,'local-user','first','text/plain',1,'one'),
                               ($2,'local-user','second','text/plain',2,'two'),
                               ($3,'another-owner','private','text/plain',3,'three')",
            )
            .bind(first)
            .bind(second)
            .bind(foreign)
            .execute(db)
            .await
            .unwrap();
            sqlx::query("UPDATE messages SET attachment_ids=$2 WHERE id=$1")
                .bind(received)
                .bind(vec![second, foreign, first])
                .execute(db)
                .await
                .unwrap();
            let watched =
                message(db, conversation, "assistant", "watched previous head", None).await;
            bulk(db, conversation, 400).await;
            let value = wire::ChatHistoryRequest {
                watch_message_ids: vec![watched.to_string()],
                receipt_request_ids: vec![receipt.to_string()],
                ..request(40)
            };
            let state = read(db, conversation, parsed(&value)).await.unwrap();
            assert_eq!(
                state
                    .message_window
                    .as_ref()
                    .unwrap()
                    .page_message_ids
                    .len(),
                40
            );
            assert!(state.messages.len() <= 40 + 4 + 1 + 1);
            eprintln!(
                "history-window fixture rows={} protobuf_bytes={}",
                state.messages.len(),
                state.encoded_len()
            );
            for id in [
                user_head,
                exploration_head,
                notification_head,
                handoff_head,
                received,
                watched,
            ] {
                assert!(
                    state
                        .messages
                        .iter()
                        .any(|message| message.id == id.to_string())
                );
            }
            assert!(
                !state
                    .messages
                    .iter()
                    .any(|message| message.id == paused_head.to_string())
            );
            assert!(state.message_window.as_ref().unwrap().has_older);
            assert!(
                state
                    .messages
                    .windows(2)
                    .all(|rows| rows[0].ordinal < rows[1].ordinal)
            );
            let receipt_rows: Vec<_> = state
                .messages
                .iter()
                .filter(|message| message.request_id == receipt.to_string())
                .collect();
            assert_eq!(receipt_rows.len(), 1);
            assert_eq!(receipt_rows[0].role, "user");
            assert_eq!(
                receipt_rows[0]
                    .attachments
                    .iter()
                    .map(|file| (file.name.as_str(), file.size_bytes))
                    .collect::<Vec<_>>(),
                [("second", 2), ("first", 1)]
            );
            assert_eq!(
                state.revision,
                ordinal(
                    db,
                    state
                        .messages
                        .last()
                        .map(|m| Uuid::parse_str(&m.id).unwrap())
                        .unwrap()
                )
                .await
            );
            assert!(
                sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id=$1")
                    .bind(user)
                    .fetch_one(db)
                    .await
                    .unwrap()
                    == "paused"
            );
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "labelled disposable PostgreSQL; parent schedules execution"]
async fn real_pg_older_pages_and_foreign_identifiers_preserve_canonical_scope() {
    database_case(|db| {
        Box::pin(async move {
            let conversation = conversation(db).await;
            bulk(db, conversation, 120).await;
            let latest = read(db, conversation, parsed(&request(20))).await.unwrap();
            let latest_revision = latest.revision;
            let anchor = latest
                .message_window
                .as_ref()
                .unwrap()
                .next_before_message_id
                .clone();
            let older = read(
                db,
                conversation,
                parsed(&wire::ChatHistoryRequest {
                    before_message_id: anchor.clone(),
                    ..request(20)
                }),
            )
            .await
            .unwrap();
            assert_eq!(
                older.message_window.as_ref().unwrap().kind,
                wire::ChatWindowKind::Before as i32
            );
            assert_eq!(older.messages.len(), 20);
            assert_eq!(older.revision, latest_revision);
            assert!(
                older.messages.last().unwrap().ordinal < latest.messages.first().unwrap().ordinal
            );
            let other = Uuid::new_v4();
            sqlx::query("INSERT INTO conversations(id,owner_id) VALUES($1,'other-user')")
                .bind(other)
                .execute(db)
                .await
                .unwrap();
            let foreign_receipt = Uuid::new_v4();
            let foreign = message(
                db,
                other,
                "user",
                "must never cross owner",
                Some(foreign_receipt),
            )
            .await;
            let scoped = read(
                db,
                conversation,
                parsed(&wire::ChatHistoryRequest {
                    watch_message_ids: vec![foreign.to_string()],
                    receipt_request_ids: vec![foreign_receipt.to_string()],
                    ..request(20)
                }),
            )
            .await
            .unwrap();
            assert!(
                scoped
                    .messages
                    .iter()
                    .all(|message| message.id != foreign.to_string())
            );
            let error = read(
                db,
                conversation,
                parsed(&wire::ChatHistoryRequest {
                    before_message_id: foreign.to_string(),
                    ..request(20)
                }),
            )
            .await
            .unwrap_err();
            assert!(matches!(error,sqlx::Error::Protocol(code) if code == "history_anchor_scope"));
            let legacy = crate::http::full_state(db, conversation).await.unwrap();
            assert_eq!(legacy.messages.len(), 120);
            assert!(legacy.message_window.is_none());
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "labelled disposable PostgreSQL; parent schedules execution"]
async fn real_pg_same_ordinal_updates_and_rr_snapshot_are_authoritative() {
    database_case(|db| Box::pin(async move {
        let conversation = conversation(db).await;
        let (run_id,head) = run(db,conversation,"user","running").await;
        bulk(db,conversation,60).await;
        let first = read(db,conversation,parsed(&request(1))).await.unwrap();
        let mut tx = transaction(db).await.unwrap();
        let baseline = snapshot(&mut tx,conversation,parsed(&request(1))).await.unwrap();
        sqlx::query("UPDATE messages SET text='final reply\n第二行' WHERE id=$1").bind(head).execute(db).await.unwrap();
        sqlx::query("UPDATE runs SET status='succeeded',error='',next_wake_at=NULL WHERE id=$1").bind(run_id).execute(db).await.unwrap();
        let prior = snapshot(&mut tx,conversation,parsed(&request(1))).await.unwrap();
        assert_eq!(prior,baseline);
        tx.commit().await.unwrap();
        assert_eq!(prior.messages.iter().find(|m|m.id==head.to_string()).unwrap().text,"outside-tail head");
        let next = read(db,conversation,parsed(&wire::ChatHistoryRequest {watch_message_ids:vec![head.to_string()],..request(1)})).await.unwrap();
        assert_eq!(first.revision,next.revision);assert_eq!(next.run_status,"succeeded");
        let updated = next.messages.iter().find(|m|m.id==head.to_string()).unwrap();
        assert_eq!(updated.text,"final reply\n第二行");assert!(!updated.active);
        sqlx::query("UPDATE runs SET status='paused',error='provider unavailable',loop_state='{\"notice_kind\":\"system_failure\"}'::jsonb WHERE id=$1").bind(run_id).execute(db).await.unwrap();
        let notice = read(db,conversation,parsed(&request(1))).await.unwrap();
        assert_eq!(notice.revision,first.revision);assert!(!notice.error.is_empty());
        sqlx::query("UPDATE runs SET status='succeeded',error='' WHERE id=$1").bind(run_id).execute(db).await.unwrap();
        let (_,next_head) = run(db,conversation,"user","running").await;
        let unwatched = read(db,conversation,parsed(&request(1))).await.unwrap();
        assert!(unwatched.messages.iter().any(|m|m.id==next_head.to_string()));
        assert!(unwatched.messages.iter().all(|m|m.id!=head.to_string()));
        let watched = read(db,conversation,parsed(&wire::ChatHistoryRequest {watch_message_ids:vec![head.to_string()],..request(1)})).await.unwrap();
        assert_eq!(watched.messages.iter().find(|m|m.id==head.to_string()).unwrap().text,"final reply\n第二行");
    })).await;
}

#[tokio::test]
#[ignore = "labelled disposable PostgreSQL; parent schedules execution"]
async fn real_pg_reverse_commit_ordinals_are_locations_not_append_watermarks() {
    database_case(|db| Box::pin(async move {
        let conversation = conversation(db).await;
        let early = Uuid::new_v4();let later = Uuid::new_v4();
        let mut held = db.begin().await.unwrap();
        sqlx::query("INSERT INTO messages(id,conversation_id,role,text) VALUES($1,$2,'user','allocated first, commits last')").bind(early).bind(conversation).execute(&mut *held).await.unwrap();
        message(db,conversation,"user","between",None).await;
        sqlx::query("INSERT INTO messages(id,conversation_id,role,text) VALUES($1,$2,'user','commits before earlier ordinal')").bind(later).bind(conversation).execute(db).await.unwrap();
        let before = read(db,conversation,parsed(&request(1))).await.unwrap();
        assert_eq!(before.messages[0].id,later.to_string());
        held.commit().await.unwrap();
        let after = read(db,conversation,parsed(&wire::ChatHistoryRequest {before_message_id:later.to_string(),..request(10)})).await.unwrap();
        assert_eq!(before.revision,after.revision);
        assert!(after.messages.iter().any(|message|message.id==early.to_string()));
        assert!(ordinal(db,early).await < ordinal(db,later).await);
    })).await;
}
