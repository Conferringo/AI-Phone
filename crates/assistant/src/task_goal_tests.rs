use super::*;
use crate::worker::steering_regressions::database_case;

const DESKTOP: &str = "goal-test-desktop";

fn headers(app: &App) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", app.token).parse().unwrap(),
    );
    headers.insert("content-type", MEDIA.parse().unwrap());
    headers
}

async fn goal(app: &Shared, revision: u64, text: &str) -> Response {
    route(
        State(app.clone()),
        Path("goal".into()),
        headers(app),
        Ok(wire::GoalUpdateRequest {
            expected_revision: revision,
            goal: text.into(),
            device_id: registry::PHONE.into(),
        }
        .encode_to_vec()
        .into()),
    )
    .await
}

async fn goal_state(response: Response) -> wire::ProactiveState {
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    wire::ProactiveState::decode(body).unwrap()
}

async fn chat(app: &Shared, device: &str, text: &str) -> wire::ChatState {
    let request = wire::ChatSendRequest {
        device_id: device.into(),
        text: text.into(),
        request_id: Uuid::now_v7().to_string(),
        ..Default::default()
    };
    let response = crate::http::send(
        State(app.clone()),
        headers(app),
        Ok(request.encode_to_vec().into()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    wire::ChatState::decode(body).unwrap()
}

#[tokio::test]
#[ignore = "real disposable PostgreSQL; no model or device calls"]
async fn goal_steers_one_run_and_blocks_liberty_until_stopped_or_completed() {
    database_case(|app| async move {
        registry::initialize(&app.db, &app.settings).await.unwrap();
        sqlx::query(
            "INSERT INTO devices(id,name,kind,system,runtime_state)
             VALUES($1,'Computer','desktop','fixture','managed')",
        )
        .bind(DESKTOP)
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO agent_device_bindings(agent_id,device_id)
             SELECT default_agent_id,$1 FROM devices WHERE id=$2",
        )
        .bind(DESKTOP)
        .bind(registry::PHONE)
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE devices d SET default_agent_id=p.default_agent_id
             FROM devices p WHERE d.id=$1 AND p.id=$2",
        )
        .bind(DESKTOP)
        .bind(registry::PHONE)
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO device_runtimes(device_id,provider,instance_ref) VALUES($1,'docker-desktop','ai-phone-desktop')")
            .bind(DESKTOP).execute(&app.db).await.unwrap();
        let desktop_liberty = wire::DesktopExploreUpdate {
            device_id: DESKTOP.into(),
            enabled: true,
            interval_seconds: 45,
        };
        crate::desktop_explore::update(&app, desktop_liberty.clone())
            .await
            .unwrap();
        let unrelated = chat(&app, DESKTOP, "保留电脑任务").await;
        schedule(&app, &["phone"]).await.unwrap();
        let before = state(&app).await.unwrap();
        assert!(before.exploring);
        assert_eq!(before.status, "exploring");

        let started = goal_state(goal(&app, before.revision, "检查消息").await).await;
        assert!(started.goal_active && !started.exploring);
        assert_eq!(started.goal_device_id, registry::PHONE);
        assert_ne!(started.active_run_id, before.active_run_id);
        assert!(
            !crate::desktop_explore::state(&app, DESKTOP)
                .await
                .unwrap()
                .enabled
        );

        let manual = chat(&app, registry::PHONE, "只看今天的消息").await;
        assert_eq!(manual.run_id, started.active_run_id);
        let edited = goal_state(goal(&app, started.revision, "检查今天的工作消息").await).await;
        assert_eq!(edited.active_run_id, started.active_run_id);
        assert_eq!(edited.goal, "检查今天的工作消息");
        assert!(edited.goal_active);
        let phone_liberty = route(
            State(app.clone()),
            Path("update".into()),
            headers(&app),
            Ok(wire::ProactiveUpdateRequest {
                expected_revision: edited.revision,
                exploring: true,
                interval_seconds: 45,
                ..Default::default()
            }
            .encode_to_vec()
            .into()),
        )
        .await;
        assert_eq!(phone_liberty.status(), StatusCode::CONFLICT);
        let error = axum::body::to_bytes(phone_liberty.into_body(), 1024).await.unwrap();
        assert_eq!(wire::ApiError::decode(error).unwrap().code, "goal_active");
        assert_eq!(
            crate::desktop_explore::update(&app, desktop_liberty.clone())
                .await
                .unwrap_err(),
            "请先关闭当前目标"
        );
        assert_eq!(
            goal(&app, started.revision, "").await.status(),
            StatusCode::CONFLICT
        );

        let stopped = goal_state(goal(&app, edited.revision, "").await).await;
        assert!(!stopped.goal_active);
        assert!(stopped.goal.is_empty());
        let run_status = |run| {
            sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id=$1").bind(run)
        };
        let goal_run = Uuid::parse_str(&edited.active_run_id).unwrap();
        let unrelated_run = Uuid::parse_str(&unrelated.run_id).unwrap();
        assert_eq!(run_status(goal_run).fetch_one(&app.db).await.unwrap(), "canceled");
        assert_eq!(run_status(unrelated_run).fetch_one(&app.db).await.unwrap(), "queued");

        let again = goal_state(goal(&app, stopped.revision, "复核消息").await).await;
        let run = Uuid::parse_str(&again.active_run_id).unwrap();
        sqlx::query("UPDATE runs SET status='succeeded' WHERE id=$1")
            .bind(run)
            .execute(&app.db)
            .await
            .unwrap();
        let finished = state(&app).await.unwrap();
        assert!(!finished.goal_active);
        assert_eq!(finished.goal_status, "succeeded");
        assert!(
            crate::desktop_explore::update(&app, desktop_liberty)
                .await
                .unwrap()
                .enabled
        );
    })
    .await;
}
