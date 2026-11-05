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
        schedule(&app).await.unwrap();
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
