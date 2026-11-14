use super::*;

fn text(body: &str) -> String {
    format!(
        "---\nname: persistence-fixture\ndescription: Isolated persistence behavior.\n---\n\n{body}\n"
    )
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; no production Skills or workspace"]
async fn skill_edit_never_publishes_uncommitted_text() {
    database_case(|app| Box::pin(async move {
        let original = text("committed original");
        let item = skill_import(app, "", &original, "").await.unwrap();
        let destination = app.root.join("data/runtime-skills").join(&item.id).join("SKILL.md");
        sqlx::raw_sql("CREATE FUNCTION reject_skill_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.35); RAISE EXCEPTION 'isolated rejected commit'; END $$; CREATE CONSTRAINT TRIGGER reject_skill_commit AFTER UPDATE ON runtime_skills DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_skill_commit();")
            .execute(&app.db).await.unwrap();
        let edit = skill_edit(app, wire::SkillEditRequest { id: item.id.clone(), content: text("uncommitted draft"), expected_content_version: item.content_version });
        let observe = async {
            tokio::time::sleep(Duration::from_millis(150)).await;
            std::fs::read_to_string(&destination).unwrap()
        };
        let (result, visible) = tokio::join!(edit, observe);
        assert!(result.is_err());
        assert_eq!(visible, original, "workspace must not see a draft before SQL COMMIT");
        assert_eq!(std::fs::read_to_string(destination).unwrap(), original);
    })).await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; no production Skills or workspace"]
async fn skill_startup_repairs_interrupted_custom_materialization() {
    database_case(|app| {
        Box::pin(async move {
            crate::builtin_guides::test_files(&app.root);
            let original = text("committed original");
            let item = skill_import(app, "", &original, "").await.unwrap();
            let folder = app.root.join("data/runtime-skills").join(&item.id);
            std::fs::write(folder.join("helper.py"), "retained auxiliary resource").unwrap();
            // Simulate process death between a committed canonical update and file publication.
            let latest = text("committed newer revision");
            sqlx::query("UPDATE runtime_skills SET content=$2 WHERE id=$1")
                .bind(&item.id)
                .bind(&latest)
                .execute(&app.db)
                .await
                .unwrap();
            initialize(&app.db, &app.root).await.unwrap();
            assert_eq!(
                std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
                latest
            );
            assert_eq!(
                std::fs::read_to_string(folder.join("helper.py")).unwrap(),
                "retained auxiliary resource"
            );
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; no production Skills or workspace"]
async fn skill_materialization_failure_retains_committed_edit_and_repairs_on_read() {
    database_case(|app| {
        Box::pin(async move {
            let item = skill_import(app, "", &text("original"), "").await.unwrap();
            let destination = app
                .root
                .join("data/runtime-skills")
                .join(&item.id)
                .join("SKILL.md");
            std::fs::remove_file(&destination).unwrap();
            std::fs::create_dir(&destination).unwrap();
            let latest = text("durable edit despite cache write failure");
            let result = skill_edit(
                app,
                wire::SkillEditRequest {
                    id: item.id.clone(),
                    content: latest.clone(),
                    expected_content_version: item.content_version,
                },
            )
            .await;
            assert!(
                result.is_err(),
                "resource publication failure must be explicit"
            );
            let stored: String =
                sqlx::query_scalar("SELECT content FROM runtime_skills WHERE id=$1")
                    .bind(&item.id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            assert_eq!(
                stored, latest,
                "a recoverable file failure must not discard the canonical edit"
            );
            std::fs::remove_dir(&destination).unwrap();
            let read = skill_text(app, &item.id).await.unwrap();
            assert!(read.contains(&latest));
            assert_eq!(std::fs::read_to_string(destination).unwrap(), latest);
        })
    })
    .await;
}

#[derive(Clone)]
enum PostgresReplyFault {
    DropCommitAck(std::sync::Arc<std::sync::atomic::AtomicBool>),
    DelayRow {
        armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
        delayed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    },
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; delayed row reply exceeds the unchanged idle transaction timeout"]
async fn skill_projection_survives_slow_reply_and_a_concurrent_committed_edit() {
    database_case(|app| {
        Box::pin(async move {
            use std::sync::{
                Arc,
                atomic::{AtomicBool, Ordering},
            };
            let id = "slow-projection";
            let original = text("first committed revision");
            let latest = text("newer committed revision");
            sqlx::query("INSERT INTO runtime_skills(id,name,description,source,content) VALUES($1,'slow-projection','Isolated projection fixture','upload',$2)")
                .bind(id).bind(&original).execute(&app.db).await.unwrap();
            let database: String = sqlx::query_scalar("SELECT current_database()")
                .fetch_one(&app.db).await.unwrap();
            let options = PgConnectOptions::from_str(
                &std::env::var("AI_PHONE_TEST_PERSISTENCE_DATABASE_URL").unwrap(),
            ).unwrap().database(&database);
            let target = std::net::SocketAddr::from(([127, 0, 0, 1], options.get_port()));
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let armed = Arc::new(AtomicBool::new(false));
            let delayed = Arc::new(AtomicBool::new(false));
            let fault = PostgresReplyFault::DelayRow { armed: armed.clone(), delayed: delayed.clone() };
            let proxy = tokio::spawn(async move {
                loop {
                    let (socket, _) = listener.accept().await.unwrap();
                    let fault = fault.clone();
                    tokio::spawn(async move { let _ = forward_postgres_with_fault(socket, target, fault).await; });
                }
            });
            let db = PgPoolOptions::new().max_connections(2)
                .after_connect(|c, _| Box::pin(async move {
                    sqlx::query("SET idle_in_transaction_session_timeout='100ms'").execute(c).await?;
                    Ok(())
                }))
                .connect_with(options.port(port).ssl_mode(sqlx::postgres::PgSslMode::Disable))
                .await.unwrap();
            let timeout: String = sqlx::query_scalar("SHOW idle_in_transaction_session_timeout")
                .fetch_one(&db).await.unwrap();
            assert_eq!(timeout, "100ms");
            armed.store(true, Ordering::SeqCst);
            let publish = skill_resources::publish(&db, &app.root, id);
            let edit = async {
                tokio::time::timeout(Duration::from_secs(2), async {
                    while !delayed.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                }).await.unwrap();
                sqlx::query("UPDATE runtime_skills SET content=$2 WHERE id=$1")
                    .bind(id).bind(&latest).execute(&app.db).await.unwrap();
                skill_resources::publish(&app.db, &app.root, id).await
            };
            let (first, second) = tokio::join!(publish, edit);
            db.close().await;
            proxy.abort();
            assert!(delayed.load(Ordering::SeqCst), "fault must actually delay a returned row");
            assert_eq!(first.unwrap().get::<String, _>("content"), original);
            assert_eq!(second.unwrap().get::<String, _>("content"), latest);
            assert_eq!(std::fs::read_to_string(app.root.join("data/runtime-skills").join(id).join("SKILL.md")).unwrap(), latest);
        })
    }).await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; real file lock and canceled publisher"]
async fn canceled_skill_publisher_releases_its_queued_projection_lock() {
    database_case(|app| {
        Box::pin(async move {
            let id = "canceled-projection";
            let content = text("committed after canceled projection");
            sqlx::query("INSERT INTO runtime_skills(id,name,description,source,content) VALUES($1,$1,'Isolated lock fixture','upload',$2)")
                .bind(id).bind(&content).execute(&app.db).await.unwrap();
            let locks = app.root.join("data/skill-locks");
            std::fs::create_dir_all(&locks).unwrap();
            let held = std::fs::File::create(locks.join(format!("{id}.lock"))).unwrap();
            held.lock().unwrap();
            let db = app.db.clone();
            let root = app.root.clone();
            let pending = tokio::spawn(async move { skill_resources::publish(&db, &root, id).await });
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(!pending.is_finished(), "a competing publisher must wait for the file owner");
            let folder = app.root.join("data/runtime-skills").join(id);
            assert!(!folder.exists());
            pending.abort();
            assert!(pending.await.unwrap_err().is_cancelled());
            drop(held);
            let row = tokio::time::timeout(Duration::from_secs(2), skill_resources::publish(&app.db, &app.root, id))
                .await.unwrap().unwrap();
            assert_eq!(row.get::<String, _>("content"), content);
            assert_eq!(std::fs::read_to_string(folder.join("SKILL.md")).unwrap(), content);
            assert_eq!(std::fs::read_dir(folder).unwrap().count(), 1, "no abandoned temporary publication");
        })
    }).await;
}

async fn forward_postgres_with_fault(
    mut client: tokio::net::TcpStream,
    target: std::net::SocketAddr,
    fault: PostgresReplyFault,
) -> std::io::Result<()> {
    use std::sync::atomic::Ordering;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut server = tokio::net::TcpStream::connect(target).await?;
    // Tests disable TLS; forward the untyped PostgreSQL startup message first.
    let length = client.read_u32().await?;
    if !(4..=65536).contains(&length) {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut startup = vec![0; (length - 4) as usize];
    client.read_exact(&mut startup).await?;
    server.write_u32(length).await?;
    server.write_all(&startup).await?;
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();
    let requests = async { tokio::io::copy(&mut client_read, &mut server_write).await };
    let responses = async {
        loop {
            let kind = server_read.read_u8().await?;
            let length = server_read.read_u32().await?;
            if !(4..=16 * 1024 * 1024).contains(&length) {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            let mut body = vec![0; (length - 4) as usize];
            server_read.read_exact(&mut body).await?;
            match &fault {
                PostgresReplyFault::DropCommitAck(dropped)
                    if kind == b'C'
                        && body == b"COMMIT\0"
                        && !dropped.swap(true, Ordering::SeqCst) =>
                {
                    // PostgreSQL has committed; close before SQLx receives its acknowledgement.
                    return Ok::<(), std::io::Error>(());
                }
                PostgresReplyFault::DelayRow { armed, delayed }
                    if kind == b'D' && armed.swap(false, Ordering::SeqCst) =>
                {
                    delayed.store(true, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(350)).await;
                }
                _ => {}
            }
            client_write.write_u8(kind).await?;
            client_write.write_u32(length).await?;
            client_write.write_all(&body).await?;
        }
    };
    tokio::select! { result = requests => { result?; }, result = responses => { result?; } }
    Ok(())
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; loopback PostgreSQL acknowledgement fault proxy"]
async fn committed_skill_survives_lost_commit_acknowledgement() {
    database_case(|app| {
        Box::pin(async move {
            use std::sync::{
                Arc,
                atomic::{AtomicBool, Ordering},
            };
            let original = skill_import(app, "", &text("original"), "").await.unwrap();
            let options = PgConnectOptions::from_str(
                &std::env::var("AI_PHONE_TEST_PERSISTENCE_DATABASE_URL").unwrap(),
            )
            .unwrap();
            let target = std::net::SocketAddr::from(([127, 0, 0, 1], options.get_port()));
            let database: String = sqlx::query_scalar("SELECT current_database()")
                .fetch_one(&app.db)
                .await
                .unwrap();
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let dropped = Arc::new(AtomicBool::new(false));
            let observed = dropped.clone();
            let proxy = tokio::spawn(async move {
                loop {
                    let (socket, _) = listener.accept().await.unwrap();
                    let dropped = observed.clone();
                    tokio::spawn(async move {
                        let _ = forward_postgres_with_fault(
                            socket,
                            target,
                            PostgresReplyFault::DropCommitAck(dropped),
                        )
                        .await;
                    });
                }
            });
            let db = PgPoolOptions::new()
                .max_connections(2)
                .connect_with(
                    options
                        .database(&database)
                        .port(port)
                        .ssl_mode(sqlx::postgres::PgSslMode::Disable),
                )
                .await
                .unwrap();
            let proxy_app = App {
                db: db.clone(),
                root: app.root.clone(),
                token: "fixture".into(),
                phone_token: "unused".into(),
                settings: settings::SettingsStore::open(&app.root).unwrap(),
                local: reqwest::Client::new(),
                remote: reqwest::Client::new(),
                active: Mutex::new(Default::default()),
                run_signals: Default::default(),
                login_attempts: Mutex::new(vec![]),
                workspace_init: Mutex::new(None),
                location_mutation: Mutex::new(()),
                network_mutation: Mutex::new(()),
            };
            let latest = text("committed despite lost acknowledgement");
            let result = skill_edit(
                &proxy_app,
                wire::SkillEditRequest {
                    id: original.id.clone(),
                    content: latest.clone(),
                    expected_content_version: original.content_version,
                },
            )
            .await;
            let stored: String =
                sqlx::query_scalar("SELECT content FROM runtime_skills WHERE id=$1")
                    .bind(&original.id)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            let cached = std::fs::read_to_string(
                app.root
                    .join("data/runtime-skills")
                    .join(&original.id)
                    .join("SKILL.md"),
            )
            .unwrap();
            db.close().await;
            proxy.abort();
            assert!(dropped.load(Ordering::SeqCst));
            assert!(
                result.is_err(),
                "a lost acknowledgement must not fabricate success"
            );
            assert_eq!(stored, latest);
            assert_eq!(
                cached, latest,
                "commit errors must not compensate with stale pre-edit text"
            );
        })
    })
    .await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; no production Skills or workspace"]
async fn skill_import_staging_recovers_resources_without_exposing_invalid_imports() {
    database_case(|app| {
        Box::pin(async move {
            crate::builtin_guides::test_files(&app.root);
            assert!(
                skill_import(app, "", "not a valid Skill", "")
                    .await
                    .is_err()
            );
            let mount = app.root.join("data/runtime-skills");
            assert!(!mount.exists() || std::fs::read_dir(&mount).unwrap().next().is_none());
            let original = skill_import(app, "", &text("committed import"), "")
                .await
                .unwrap();
            let folder = mount.join(&original.id);
            std::fs::write(folder.join("helper.py"), "persisted resource").unwrap();
            let staged = app.root.join("data/skill-staging").join(&original.id);
            std::fs::rename(&folder, &staged).unwrap();
            initialize(&app.db, &app.root).await.unwrap();
            assert!(!staged.exists());
            assert_eq!(
                std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
                original.content
            );
            assert_eq!(
                std::fs::read_to_string(folder.join("helper.py")).unwrap(),
                "persisted resource"
            );
        })
    })
    .await;
}
