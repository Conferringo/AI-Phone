//! Real disposable PostgreSQL only; no device, model or scheduler is started.
use super::*;
use crate::worker::steering_regressions::database_case;

const COMPUTER: &str = "explore-configuration-fixture";

async fn register(app: &App) {
    sqlx::query("INSERT INTO devices(id,name,kind,system,runtime_state) VALUES($1,'Computer','desktop','fixture','managed')")
        .bind(COMPUTER).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO device_runtimes(device_id,provider,instance_ref) VALUES($1,'docker-desktop','ai-phone-desktop')")
        .bind(COMPUTER).execute(&app.db).await.unwrap();
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no model or device calls"]
async fn invalid_interval_does_not_create_exploration_state() {
    database_case(|app| async move {
        register(&app).await;
        let result = update(
            &app,
            wire::DesktopExploreUpdate {
                device_id: COMPUTER.into(),
                enabled: false,
                interval_seconds: 0,
            },
        )
        .await;
        assert_eq!(result.unwrap_err(), "间隔应为30–3600秒");
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM desktop_exploration WHERE device_id=$1)",
        )
        .bind(COMPUTER)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert!(
            !exists,
            "invalid input must not initialize a mode with the default enabled state"
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no model or device calls"]
async fn updates_return_committed_state_and_keep_runtime_validation() {
    database_case(|app| async move {
        register(&app).await;
        sqlx::query("SELECT 'entropy_explore_begin'")
            .execute(&app.db)
            .await
            .unwrap();
        let started = std::time::Instant::now();
        for i in 0..25 {
            let enabled = i % 2 == 0;
            let result = update(
                &app,
                wire::DesktopExploreUpdate {
                    device_id: COMPUTER.into(),
                    enabled,
                    interval_seconds: 30 + i,
                },
            )
            .await
            .unwrap();
            assert_eq!(result.device_id, COMPUTER);
            assert_eq!(result.enabled, enabled);
            assert_eq!(result.interval_seconds, 30 + i);
            assert!(result.run_id.is_empty());
        }
        eprintln!("explore_update_25_us={}", started.elapsed().as_micros());
        sqlx::query("SELECT 'entropy_explore_end'")
            .execute(&app.db)
            .await
            .unwrap();
        let result = state(&app, COMPUTER).await.unwrap();
        assert!(result.enabled);
        assert_eq!(result.interval_seconds, 54);
        sqlx::query("UPDATE device_runtimes SET instance_ref='not-attached' WHERE device_id=$1")
            .bind(COMPUTER)
            .execute(&app.db)
            .await
            .unwrap();
        assert!(
            update(
                &app,
                wire::DesktopExploreUpdate {
                    device_id: COMPUTER.into(),
                    enabled: false,
                    interval_seconds: 45,
                }
            )
            .await
            .is_err(),
            "a stored mode cannot substitute for a registered runtime"
        );
        assert!(
            state(&app, registry::PHONE).await.is_err(),
            "primary phone keeps its own scheduler"
        );
    })
    .await;
}
