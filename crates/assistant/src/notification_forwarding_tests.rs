//! Loopback HTTP and isolated PostgreSQL: no real device, model or recipient.
use super::*;
use crate::worker::steering_regressions::database_case;
use futures_util::FutureExt;
use std::{
    panic::AssertUnwindSafe,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::sync::Notify;

#[derive(Default)]
struct Inbox {
    available: AtomicBool,
    block_phone: AtomicBool,
    phone_polling: Notify,
    finish_phone: Notify,
    manual: AtomicBool,
    bad_request: AtomicBool,
    pushes: AtomicUsize,
    pushing: Notify,
    finish_push: Notify,
}

async fn response(inbox: Arc<Inbox>, request: axum::extract::Request) -> Response {
    match request.uri().path() {
        "/v1/push" => {
            let bytes = axum::body::to_bytes(request.into_body(), 16384)
                .await
                .unwrap();
            let input = wire::ChannelPushRequest::decode(bytes).unwrap();
            if input.text != "设备通知\nSynthetic completed notification report" {
                inbox.bad_request.store(true, Ordering::SeqCst);
            }
            inbox.pushes.fetch_add(1, Ordering::SeqCst);
            inbox.pushing.notify_one();
            inbox.finish_push.notified().await;
            pb(StatusCode::OK, wire::ChannelPushResult::default())
        }
        "/v1/notifications" => {
            inbox.phone_polling.notify_one();
            if inbox.block_phone.load(Ordering::SeqCst) {
                inbox.finish_phone.notified().await;
            }
            let notifications = if inbox.available.load(Ordering::SeqCst) {
                vec![wire::PhoneNotification {
                    event_id: "a".repeat(64),
                    notification_key: "loopback-new-message".into(),
                    package_name: "com.tencent.mm".into(),
                    app_name: "WeChat fixture".into(),
                    title: "Fixture sender".into(),
                    text: "Synthetic incoming request; never send to a real recipient".into(),
                    posted_at_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                }]
            } else {
                vec![]
            };
            pb(
                StatusCode::OK,
                wire::PhoneNotificationList {
                    connected: true,
                    attention_connected: true,
                    notifications,
                    ..Default::default()
                },
            )
        }
        "/v1/observe" => {
            let manual = inbox.manual.load(Ordering::SeqCst);
            pb(
                StatusCode::OK,
                wire::DeviceObservation {
                    controller: if manual { "manual" } else { "agent" }.into(),
                    control_epoch: if manual { 2 } else { 1 },
                    ..Default::default()
                },
            )
        }
        _ => {
            inbox.bad_request.store(true, Ordering::SeqCst);
            error(
                StatusCode::NOT_FOUND,
                "fixture_route",
                "No real service is permitted",
            )
        }
    }
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL and loopback blocked push; no devices/models/recipients"]
async fn stalled_forwarding_cannot_delay_notification_wake_or_manual_takeover() {
    database_case(|app| async move {
        let inbox = Arc::new(Inbox::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = axum::Router::new().fallback(axum::routing::any({
            let inbox = inbox.clone();
            move |request| response(inbox.clone(), request)
        }));
        let shutdown = CancellationToken::new();
        let finish_server = shutdown.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).with_graceful_shutdown(finish_server.cancelled_owned()).await.unwrap();
        });
        let mut app = Arc::try_unwrap(app).unwrap_or_else(|_| panic!("unshared fixture required"));
        // Every fixed local service URL goes only to this loopback fixture.
        app.local = reqwest::Client::builder().proxy(reqwest::Proxy::all(format!("http://{address}")).unwrap()).build().unwrap();
        let app = Arc::new(app);
        registry::initialize(&app.db, &app.settings).await.unwrap();
        schedule(&app, &["phone", "desktop"]).await.unwrap();
        sqlx::query("UPDATE messages SET text='Synthetic completed notification report' WHERE id IN (SELECT assistant_message_id FROM runs)").execute(&app.db).await.unwrap();
        sqlx::query("UPDATE runs SET origin='notification',status='succeeded',finished_at=now()").execute(&app.db).await.unwrap();
        reconcile(&app).await.unwrap();
        sqlx::query("UPDATE proactive_state SET exploring=false,wechat_watch=true,notifications_since='epoch'").execute(&app.db).await.unwrap();
        let work = tokio::spawn(work(app.clone()));
        let outcome = AssertUnwindSafe(async {
            tokio::time::timeout(Duration::from_secs(2), inbox.pushing.notified()).await.expect("forwarding must actually be blocked before injecting the incoming event");
            let started = std::time::Instant::now();
            inbox.available.store(true, Ordering::SeqCst);
            let run = tokio::time::timeout(Duration::from_secs(9), async {
                loop {
                    let run: Option<Uuid> = sqlx::query_scalar("SELECT run_id FROM phone_notifications WHERE event_id=$1 AND status='assigned'").bind("a".repeat(64)).fetch_optional(&app.db).await.unwrap().flatten();
                    if let Some(run) = run { break run; }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }).await.expect("a stalled summary push must not prevent new notification work from waking");
            let wake_ms = started.elapsed().as_millis();
            let status: String = sqlx::query_scalar("SELECT status FROM proactive_reports").fetch_one(&app.db).await.unwrap();
            assert_eq!(status, "pending", "the old push must still be blocked at notification admission");
            inbox.manual.store(true, Ordering::SeqCst);
            tokio::time::timeout(Duration::from_secs(4), async {
                loop {
                    if sqlx::query_scalar::<_,bool>("SELECT wechat_quiet_until>now() FROM proactive_state").fetch_one(&app.db).await.unwrap() { break; }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }).await.expect("manual supervision must remain live while report forwarding is blocked");
            inbox.finish_push.notify_one();
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if sqlx::query_scalar::<_,bool>("SELECT status='settled' FROM proactive_reports").fetch_one(&app.db).await.unwrap() { break; }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }).await.expect("the same durable report must settle after the original push returns");
            assert_eq!(inbox.pushes.load(Ordering::SeqCst), 1, "no competing forwarding owner or duplicate send");
            assert!(!inbox.bad_request.load(Ordering::SeqCst));
            assert_eq!(sqlx::query_scalar::<_,String>("SELECT origin FROM runs WHERE id=$1").bind(run).fetch_one(&app.db).await.unwrap(), "notification");
            println!("loopback blocked-push fixture: notification woke in {wake_ms} ms; no real provider or device");
        }).catch_unwind().await;
        work.abort();
        assert!(work.await.unwrap_err().is_cancelled());
        inbox.finish_push.notify_one();
        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(2), server).await.unwrap().unwrap();
        if let Err(panic) = outcome { std::panic::resume_unwind(panic); }
    }).await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL and blocked loopback phone; no devices or models"]
async fn stalled_device_collection_cannot_delay_other_device_admission() {
    for block_phone in [true, false] {
        database_case(|app| async move {
            let inbox = Arc::new(Inbox::default());
            inbox.block_phone.store(block_phone, Ordering::SeqCst);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let router = axum::Router::new().fallback(axum::routing::any({
                let inbox = inbox.clone();
                move |request| response(inbox.clone(), request)
            }));
            let shutdown = CancellationToken::new();
            let finish = shutdown.clone();
            let server = tokio::spawn(async move {
                axum::serve(listener, router).with_graceful_shutdown(finish.cancelled_owned()).await.unwrap();
            });
            let mut app = Arc::try_unwrap(app).unwrap_or_else(|_| panic!("unshared fixture required"));
            app.local = reqwest::Client::builder().proxy(reqwest::Proxy::all(format!("http://{address}")).unwrap()).build().unwrap();
            let app = Arc::new(app);
            registry::initialize(&app.db, &app.settings).await.unwrap();
            sqlx::query("UPDATE proactive_state SET exploring=false,wechat_watch=true,notifications_since='epoch'").execute(&app.db).await.unwrap();
            let agent: Uuid = sqlx::query_scalar("SELECT default_agent_id FROM devices WHERE id='phone-local-01'").fetch_one(&app.db).await.unwrap();
            let desktop = "collector-desktop-fixture";
            sqlx::query("INSERT INTO devices(id,name,kind,system,runtime_state) VALUES($1,'Fixture desktop','desktop','Linux','managed')").bind(desktop).execute(&app.db).await.unwrap();
            sqlx::query("INSERT INTO agent_device_bindings(agent_id,device_id) VALUES($1,$2)").bind(agent).bind(desktop).execute(&app.db).await.unwrap();
            sqlx::query("UPDATE devices SET default_agent_id=$2 WHERE id=$1").bind(desktop).bind(agent).execute(&app.db).await.unwrap();
            // This foreign instance ref makes available() reject before any Docker process.
            sqlx::query("INSERT INTO device_runtimes(device_id,provider,instance_ref) VALUES($1,'docker-desktop','fixture-never-a-real-device')").bind(desktop).execute(&app.db).await.unwrap();
            schedule(&app, &["phone", "desktop"]).await.unwrap();
            for (device, id) in [(registry::PHONE, "b"), (desktop, "c")] {
                ingest_device_feed(&app, device, wire::PhoneNotificationList {
                    connected: true,
                    notifications: vec![wire::PhoneNotification {
                        event_id: id.repeat(64), notification_key: format!("isolation-{device}"),
                        package_name: "fixture.message".into(), app_name: "Fixture".into(),
                        title: "Synthetic sender".into(), text: "No real recipient".into(),
                        posted_at_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
                    }], ..Default::default()
                }).await.unwrap();
            }

            let mut lock = app.db.begin().await.unwrap();
            if !block_phone {
                // An actual blocked desktop collection query; phone routing does not use this table.
                sqlx::query("LOCK TABLE device_runtimes IN ACCESS EXCLUSIVE MODE").execute(&mut *lock).await.unwrap();
            }
            let work = tokio::spawn(work(app.clone()));
            let outcome = AssertUnwindSafe(async {
                if block_phone {
                    tokio::time::timeout(Duration::from_secs(2), inbox.phone_polling.notified()).await.unwrap();
                } else {
                    tokio::time::timeout(Duration::from_secs(2), async {
                        loop {
                            let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks l JOIN pg_stat_activity a USING(pid) WHERE a.datname=current_database() AND l.relation='device_runtimes'::regclass AND NOT l.granted)").fetch_one(&app.db).await.unwrap();
                            if blocked { break; }
                            tokio::time::sleep(Duration::from_millis(25)).await;
                        }
                    }).await.expect("desktop collection must actually be blocked");
                }
                let started = std::time::Instant::now();
                let ready = if block_phone { desktop } else { registry::PHONE };
                tokio::time::timeout(Duration::from_secs(9), async {
                    loop {
                        if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM runs WHERE device_id=$1 AND origin='notification' AND status='queued')").bind(ready).fetch_one(&app.db).await.unwrap() { break; }
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                }).await.expect("another device's blocked collection must not hold up durable notification work");
                let held = if block_phone { registry::PHONE } else { desktop };
                assert!(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM runs WHERE device_id=$1)").bind(held).fetch_one(&app.db).await.unwrap(), "a different collector must not bypass this device's supervision or admission");
                assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM runs").fetch_one(&app.db).await.unwrap(), 1);
                assert!(!inbox.bad_request.load(Ordering::SeqCst));
                println!("blocked {} collector: other durable notification admitted in {}ms; no model or real device", if block_phone { "phone HTTP" } else { "desktop SQL" }, started.elapsed().as_millis());
            }).catch_unwind().await;
            work.abort();
            assert!(work.await.unwrap_err().is_cancelled());
            lock.rollback().await.unwrap();
            inbox.finish_phone.notify_waiters();
            shutdown.cancel();
            tokio::time::timeout(Duration::from_secs(2), server).await.unwrap().unwrap();
            if outcome.is_ok() {
                assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM runs").fetch_one(&app.db).await.unwrap(), 1, "releasing an old blocked operation cannot admit work after the owner was dropped");
            }
            if let Err(panic) = outcome { std::panic::resume_unwind(panic); }
        }).await;
    }
}
