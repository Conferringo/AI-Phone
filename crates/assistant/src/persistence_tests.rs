//! Isolated PostgreSQL and temporary guest transport regressions; never a real device.
use super::*;
use futures_util::{FutureExt, future::BoxFuture};
use sqlx::{
    Connection,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{os::unix::fs::PermissionsExt, str::FromStr};

#[path = "skill_persistence_tests.rs"]
mod skill_persistence_tests;

async fn database_case(test: impl for<'a> FnOnce(&'a App) -> BoxFuture<'a, ()>) {
    let url = std::env::var("AI_PHONE_TEST_PERSISTENCE_DATABASE_URL")
        .expect("explicit persistence fixture URL; run scripts/ci_integration.py");
    let options = PgConnectOptions::from_str(&url).unwrap();
    assert_eq!(
        options.get_username(),
        "aiphone_test",
        "refuse production database credentials"
    );
    assert_eq!(
        options.get_host(),
        "127.0.0.1",
        "tests require a loopback disposable database"
    );
    assert_ne!(
        options.get_port(),
        55482,
        "refuse the production PostgreSQL port"
    );
    let name = format!("aiphone_persistence_{}", Uuid::new_v4().simple());
    let mut admin = sqlx::PgConnection::connect_with(&options).await.unwrap();
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&mut admin)
        .await
        .unwrap();
    let db = PgPoolOptions::new()
        .max_connections(5)
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET idle_in_transaction_session_timeout='100ms'")
                    .execute(c)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options.database(&name))
        .await
        .unwrap();
    sqlx::migrate!().run(&db).await.unwrap();
    let root = std::env::temp_dir().join(&name);
    let app = App {
        db: db.clone(),
        root: root.clone(),
        token: "fixture".into(),
        phone_token: "unused".into(),
        settings: settings::SettingsStore::test_configured(&root),
        local: reqwest::Client::new(),
        remote: reqwest::Client::new(),
        active: Mutex::new(None),
        login_attempts: Mutex::new(vec![]),
    };
    let result = std::panic::AssertUnwindSafe(test(&app))
        .catch_unwind()
        .await;
    drop(app);
    db.close().await;
    sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
        .execute(&mut admin)
        .await
        .unwrap();
    let _ = std::fs::remove_dir_all(root);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

async fn document(app: &App, title: &str) -> wire::KnowledgeDocument {
    knowledge_save(
        app,
        wire::KnowledgeSaveRequest {
            title: title.into(),
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires a labelled disposable PostgreSQL; no production writes"]
async fn knowledge_save_preserves_bidirectional_relationships() {
    database_case(|app| {
        Box::pin(async move {
            let a = document(app, "A").await;
            let b = document(app, "B").await;
            let saved = knowledge_save(
                app,
                wire::KnowledgeSaveRequest {
                    id: a.id.clone(),
                    title: "A edited".into(),
                    expected_revision: a.revision,
                    related_ids: vec![b.id.clone(), b.id.clone()],
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            let neighbor = knowledge_read(app, &b.id).await.unwrap();
            assert_eq!(
                neighbor.related_ids,
                vec![a.id.clone()],
                "saving a relationship must update its reverse edge"
            );
            assert_eq!(
                saved.related_ids,
                vec![b.id.clone()],
                "relationships are a set"
            );
            assert!(
                neighbor.revision > b.revision,
                "reverse changes invalidate stale editors"
            );
            let cleared = knowledge_save(
                app,
                wire::KnowledgeSaveRequest {
                    id: a.id.clone(),
                    title: "A".into(),
                    expected_revision: saved.revision,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            assert!(
                knowledge_read(app, &b.id)
                    .await
                    .unwrap()
                    .related_ids
                    .is_empty()
            );
            assert!(cleared.related_ids.is_empty());
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires a labelled disposable PostgreSQL; no production writes"]
async fn knowledge_rejects_invalid_edges_without_partial_writes() {
    database_case(|app| {
        Box::pin(async move {
            let a = document(app, "A").await;
            for target in [a.id.clone(), Uuid::new_v4().to_string()] {
                let result = knowledge_save(
                    app,
                    wire::KnowledgeSaveRequest {
                        id: a.id.clone(),
                        title: "must not change".into(),
                        expected_revision: a.revision,
                        related_ids: vec![target],
                        ..Default::default()
                    },
                )
                .await;
                assert!(
                    result.is_err(),
                    "self references and nonexistent targets must be rejected"
                );
                assert_eq!(knowledge_read(app, &a.id).await.unwrap(), a);
            }
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires a labelled disposable PostgreSQL; no production writes"]
async fn knowledge_relationship_limits_conflicts_and_trash_are_atomic() {
    database_case(|app| Box::pin(async move {
        let a = document(app, "A").await;
        let b = document(app, "B").await;
        let linked = knowledge_link(app, wire::KnowledgeLinkRequest { id: a.id.clone(), target_id: b.id.clone(),
            present: true, expected_revision: a.revision }).await.unwrap();
        let stale = knowledge_save(app, wire::KnowledgeSaveRequest { id: b.id.clone(), title: "stale".into(),
            expected_revision: b.revision, ..Default::default() }).await;
        assert!(stale.is_err(), "a reverse-link change must invalidate the other editor");
        sqlx::query("UPDATE knowledge_documents SET deleted_at=now(),revision=revision+1 WHERE id=$1")
            .bind(Uuid::parse_str(&b.id).unwrap()).execute(&app.db).await.unwrap();
        let retained = knowledge_save(app, wire::KnowledgeSaveRequest { id: a.id.clone(), title: "content edit".into(),
            expected_revision: linked.revision, related_ids: linked.related_ids.clone(), ..Default::default() }).await.unwrap();
        assert_eq!(retained.related_ids, linked.related_ids, "trash/restore must preserve existing edges");
        let removed = knowledge_save(app, wire::KnowledgeSaveRequest { id: a.id.clone(), title: "content edit".into(),
            expected_revision: retained.revision, ..Default::default() }).await.unwrap();
        assert!(knowledge_read(app, &b.id).await.unwrap().related_ids.is_empty());
        assert!(knowledge_save(app, wire::KnowledgeSaveRequest { id: a.id.clone(), title: "do not add trash".into(),
            expected_revision: removed.revision, related_ids: vec![b.id.clone()], ..Default::default() }).await.is_err());
        assert_eq!(knowledge_read(app, &a.id).await.unwrap(), removed);

        let hub = document(app, "Full hub").await;
        let hub_id = Uuid::parse_str(&hub.id).unwrap();
        let neighbors: Vec<Uuid> = (0..50).map(|_| Uuid::new_v4()).collect();
        for neighbor in &neighbors {
            sqlx::query("INSERT INTO knowledge_documents(id,title,related_ids) VALUES($1,'existing',ARRAY[$2]::uuid[])")
                .bind(neighbor).bind(hub_id).execute(&app.db).await.unwrap();
        }
        sqlx::query("UPDATE knowledge_documents SET related_ids=$2 WHERE id=$1")
            .bind(hub_id).bind(&neighbors).execute(&app.db).await.unwrap();
        assert!(knowledge_save(app, wire::KnowledgeSaveRequest { id: a.id.clone(), title: "must roll back".into(),
            expected_revision: removed.revision, related_ids: vec![hub.id.clone()], ..Default::default() }).await.is_err());
        assert_eq!(knowledge_read(app, &a.id).await.unwrap(), removed);
        assert_eq!(knowledge_read(app, &hub.id).await.unwrap().related_ids.len(), 50);
    })).await;
}

#[tokio::test]
#[ignore = "requires a labelled disposable PostgreSQL; no production writes"]
async fn concurrent_knowledge_editors_cannot_create_half_edges() {
    database_case(|app| Box::pin(async move {
        let a = document(app, "A").await;
        let b = document(app, "B").await;
        let c = document(app, "C").await;
        let save = knowledge_save(app, wire::KnowledgeSaveRequest { id: a.id.clone(), title: "A".into(),
            expected_revision: a.revision, related_ids: vec![b.id.clone()], ..Default::default() });
        let link = knowledge_link(app, wire::KnowledgeLinkRequest { id: b.id.clone(), target_id: c.id.clone(),
            present: true, expected_revision: b.revision });
        let (saved, linked) = tokio::join!(save, link);
        assert!(saved.is_ok());
        if let Err(error) = linked { assert!(error.contains("更新")); }
        let broken: i64 = sqlx::query_scalar("SELECT count(*) FROM knowledge_documents d CROSS JOIN LATERAL unnest(d.related_ids) r(id) LEFT JOIN knowledge_documents t ON t.id=r.id WHERE t.id IS NULL OR NOT(d.id=ANY(t.related_ids))")
            .fetch_one(&app.db).await.unwrap();
        assert_eq!(broken, 0);
    })).await;
}

async fn transfer_devices(app: &App) {
    sqlx::query("INSERT INTO devices(id,name,kind,system,runtime_state) VALUES('phone-local-01','fixture phone','phone','test','managed'),('desktop-fixture','fixture desktop','desktop','test','managed')")
        .execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO device_runtimes(device_id,provider,instance_ref) VALUES('desktop-fixture','docker-desktop','ai-phone-desktop')")
        .execute(&app.db).await.unwrap();
    crate::desktop::link(
        app,
        wire::DeviceLinkRequest {
            source_device_id: "phone-local-01".into(),
            target_device_id: "desktop-fixture".into(),
            connected: true,
        },
    )
    .await
    .unwrap();
    std::fs::create_dir_all(app.root.join("data/desktop/home/Files")).unwrap();
    std::fs::create_dir_all(app.root.join("scripts")).unwrap();
    let adb = app.root.join("scripts/android-adb");
    std::fs::write(&adb, r#"#!/usr/bin/env python3
import hashlib, pathlib, shlex, shutil, sys, time
root=pathlib.Path(__file__).resolve().parent.parent
args=sys.argv[1:]
def guest(path): return root/'guest'/path.lstrip('/')
if args[0]=='push':
    path=guest(args[2]);path.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(args[1],path)
elif args[0]=='exec-out':
    (root/'copy-started').write_text('started');time.sleep(0.35);sys.stdout.buffer.write(b'fixture document')
elif args[0]=='shell':
    cmd=shlex.split(args[1])
    if cmd[0]=='readlink': print(cmd[-1])
    elif cmd[0]=='stat': print(len(b'fixture document'))
    elif cmd[0]=='mkdir': guest(cmd[-1]).mkdir(parents=True,exist_ok=True)
    elif cmd[0]=='sha256sum': print(hashlib.sha256(guest(cmd[-1]).read_bytes()).hexdigest())
    elif cmd[0]=='test':
        source,target=guest(cmd[-2]),guest(cmd[-1])
        if target.exists():sys.exit(1)
        shutil.move(source,target)
        with (root/'published').open('a') as f:f.write('publish\n')
        if (root/'slow-publication').exists():time.sleep(0.35)
        if (root/'fail-after-publish').exists():sys.exit(3)
    elif cmd[0]=='am': pass
    else: raise RuntimeError('Unexpected fixture shell request')
else: raise RuntimeError('Unexpected fixture transport request')
"#).unwrap();
    std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; uses temporary ADB fixture, never real devices"]
async fn slow_transfer_survives_database_idle_timeout() {
    database_case(|app| {
        Box::pin(async move {
            transfer_devices(app).await;
            let result = crate::desktop::transfer(
                app,
                wire::TransferRequest {
                    request_id: Uuid::new_v4().to_string(),
                    source_device_id: "phone-local-01".into(),
                    target_device_id: "desktop-fixture".into(),
                    source_path: "/sdcard/Download/fixture.txt".into(),
                    target_name: "fixture.txt".into(),
                },
                None,
            )
            .await
            .unwrap();
            assert_eq!(
                result.status, "completed",
                "guest I/O must not keep an idle PostgreSQL transaction open: {}",
                result.error
            );
            assert_eq!(
                std::fs::read(
                    app.root
                        .join("data/desktop/home/Files")
                        .join(result.target_path)
                )
                .unwrap(),
                b"fixture document"
            );
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; uses temporary ADB fixture, never real devices"]
async fn lost_transfer_reply_is_unknown_and_never_replayed() {
    database_case(|app| {
        Box::pin(async move {
            transfer_devices(app).await;
            std::fs::write(
                app.root.join("data/desktop/home/Files/fixture.txt"),
                b"fixture document",
            )
            .unwrap();
            std::fs::write(app.root.join("fail-after-publish"), b"").unwrap();
            let request = wire::TransferRequest {
                request_id: Uuid::new_v4().to_string(),
                source_device_id: "desktop-fixture".into(),
                target_device_id: "phone-local-01".into(),
                source_path: "fixture.txt".into(),
                target_name: "fixture.txt".into(),
            };
            let result = crate::desktop::transfer(app, request.clone(), None)
                .await
                .unwrap();
            assert_eq!(
                result.status, "unknown",
                "a lost response after guest publication cannot be called failed"
            );
            assert!(
                app.root
                    .join("guest")
                    .join(result.target_path.trim_start_matches('/'))
                    .is_file()
            );
            let replay = crate::desktop::transfer(app, request, None).await.unwrap();
            assert_eq!(replay.status, "unknown");
            assert_eq!(
                std::fs::read_to_string(app.root.join("published")).unwrap(),
                "publish\n"
            );
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; uses temporary ADB fixture, never real devices"]
async fn disconnect_during_copy_prevents_publication_without_waiting_for_copy() {
    database_case(|app| {
        Box::pin(async move {
            transfer_devices(app).await;
            let request = wire::TransferRequest {
                request_id: Uuid::new_v4().to_string(),
                source_device_id: "phone-local-01".into(),
                target_device_id: "desktop-fixture".into(),
                source_path: "/sdcard/Download/fixture.txt".into(),
                target_name: "fixture.txt".into(),
            };
            let disconnect = async {
                tokio::time::timeout(Duration::from_secs(3), async {
                    while !app.root.join("copy-started").exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                tokio::time::timeout(
                    Duration::from_millis(100),
                    crate::desktop::link(
                        app,
                        wire::DeviceLinkRequest {
                            source_device_id: "phone-local-01".into(),
                            target_device_id: "desktop-fixture".into(),
                            connected: false,
                        },
                    ),
                )
                .await
                .expect("disconnect should not wait for guest I/O")
                .unwrap();
            };
            let (result, ()) =
                tokio::join!(crate::desktop::transfer(app, request, None), disconnect);
            let result = result.unwrap();
            assert_eq!(result.status, "failed");
            assert!(result.error.contains("数据线"));
            assert!(
                !app.root
                    .join("data/desktop/home/Files")
                    .join(result.target_path)
                    .exists()
            );
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; uses temporary ADB fixture, never real devices"]
async fn database_loss_after_publication_keeps_unknown_receipt() {
    database_case(|app| Box::pin(async move {
        transfer_devices(app).await;
        std::fs::write(app.root.join("data/desktop/home/Files/fixture.txt"), b"fixture document").unwrap();
        std::fs::write(app.root.join("slow-publication"), b"").unwrap();
        let result = crate::desktop::transfer(app, wire::TransferRequest { request_id: Uuid::new_v4().to_string(),
            source_device_id: "desktop-fixture".into(), target_device_id: "phone-local-01".into(),
            source_path: "fixture.txt".into(), target_name: "fixture.txt".into() }, None).await.unwrap();
        assert_eq!(result.status, "unknown", "a database connection lost after the rename cannot erase the durable uncertainty record");
        assert!(app.root.join("guest").join(result.target_path.trim_start_matches('/')).is_file());
    })).await;
}
