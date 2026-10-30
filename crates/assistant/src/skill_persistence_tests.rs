use super::*;

fn text(body: &str) -> String {
    format!(
        "---\nname: persistence-fixture\ndescription: Isolated persistence behavior.\n---\n\n{body}\n"
    )
}

fn builtin_fixtures(root: &std::path::Path) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/builtin");
    for id in [
        "phone-use",
        "wechat",
        "computer-use",
        "device-transfer",
        "autonomous-learning",
        "browser-android",
        "files-android",
    ] {
        let folder = root.join("skills/builtin").join(id);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(source.join(id).join("SKILL.md"), folder.join("SKILL.md")).unwrap();
    }
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
        let edit = skill_edit(app, wire::SkillEditRequest { id: item.id.clone(), content: text("uncommitted draft"), expected_content_sha256: item.content_sha256 });
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
            builtin_fixtures(&app.root);
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
                    expected_content_sha256: item.content_sha256,
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

async fn forward_postgres_until_commit_ack(
    mut client: tokio::net::TcpStream,
    target: std::net::SocketAddr,
    dropped: std::sync::Arc<std::sync::atomic::AtomicBool>,
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
            if kind == b'C' && body == b"COMMIT\0" && !dropped.swap(true, Ordering::SeqCst) {
                // PostgreSQL has committed. Close before its success acknowledgement
                // reaches SQLx, reproducing an actually ambiguous COMMIT result.
                return Ok::<(), std::io::Error>(());
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
                        let _ = forward_postgres_until_commit_ack(socket, target, dropped).await;
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
                active: Mutex::new(None),
                login_attempts: Mutex::new(vec![]),
            };
            let latest = text("committed despite lost acknowledgement");
            let result = skill_edit(
                &proxy_app,
                wire::SkillEditRequest {
                    id: original.id.clone(),
                    content: latest.clone(),
                    expected_content_sha256: original.content_sha256,
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
            builtin_fixtures(&app.root);
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
