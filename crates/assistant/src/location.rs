//! One owner for location configuration, motion and device receipts. Disabled means no lookup.
use super::location_devices::apply;
use super::location_motion as motion;
use crate::*;
use axum::extract::{Path, State};
use sha2::{Digest, Sha256};
static MUTATION: Mutex<()> = Mutex::const_new(());
pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
async fn load(app: &App) -> Result<wire::LocationState, String> {
    let bytes: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT state FROM simulated_location WHERE owner_id='local-user'")
            .fetch_optional(&app.db)
            .await
            .map_err(|e| e.to_string())?;
    bytes
        .map(|v| wire::LocationState::decode(v.as_slice()).map_err(|_| "位置记录无法读取".into()))
        .unwrap_or_else(|| {
            Ok(wire::LocationState {
                speed_kph: 5.,
                ..Default::default()
            })
        })
}
async fn save(app: &App, s: &wire::LocationState) -> Result<(), String> {
    sqlx::query("INSERT INTO simulated_location(owner_id,revision,state) VALUES('local-user',$1,$2) ON CONFLICT(owner_id) DO UPDATE SET revision=EXCLUDED.revision,state=EXCLUDED.state,updated_at=now()")
 .bind(s.revision as i64).bind(s.encode_to_vec()).execute(&app.db).await.map_err(|e|e.to_string())?;
    Ok(())
}
async fn host(app: &App) -> Result<wire::GeoPoint, String> {
    // External provider speaks JSON; self-owned business transport stays protobuf.
    let value: serde_json::Value = app
        .local
        .get("https://ipwho.is/")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|_| "网络定位不可用，请在地图选点")?
        .error_for_status()
        .map_err(|_| "网络定位不可用，请在地图选点")?
        .json()
        .await
        .map_err(|_| "网络定位返回无效内容")?;
    let p = wire::GeoPoint {
        latitude: value["latitude"]
            .as_f64()
            .ok_or("无法获得宿主位置，请在地图选点")?,
        longitude: value["longitude"]
            .as_f64()
            .ok_or("无法获得宿主位置，请在地图选点")?,
        accuracy_m: 50_000.,
    };
    if !motion::valid(&p) {
        return Err("网络定位坐标无效".into());
    }
    Ok(p)
}
fn validate_route(r: &wire::LocationUpdate) -> Result<(), String> {
    if r.route.len() > 200 || r.route.iter().any(|p| !motion::valid(p)) {
        return Err("路线坐标无效，最多 200 个点".into());
    }
    if r.moving && (r.route.len() < 2 || motion::length(&r.route) < 1.) {
        return Err("请设置至少两个不同的路线点".into());
    }
    Ok(())
}
fn validate(r: &wire::LocationUpdate) -> Result<(), String> {
    Uuid::parse_str(&r.request_id).map_err(|_| "无效操作编号")?;
    if !r.enabled {
        return Ok(());
    }
    if !(0.5..=500.).contains(&r.speed_kph) || !r.speed_kph.is_finite() {
        return Err("速度范围为 0.5–500 km/h".into());
    }
    validate_route(r)?;
    if !r.use_host && !r.point.as_ref().is_some_and(motion::valid) {
        return Err("请选择位置".into());
    }
    Ok(())
}
fn restarted_route(s: &wire::LocationState, r: &wire::LocationUpdate) -> bool {
    s.route != r.route || (r.moving && !r.repeat && s.distance_m >= motion::length(&r.route))
}
fn next_point(
    s: &wire::LocationState,
    r: &wire::LocationUpdate,
    point: Option<wire::GeoPoint>,
) -> Option<wire::GeoPoint> {
    if s.moving && !r.moving && s.route == r.route {
        s.point
    } else if r.use_host {
        point
    } else {
        point.or(s.point)
    }
}
fn configure(
    mut s: wire::LocationState,
    r: wire::LocationUpdate,
    point: Option<wire::GeoPoint>,
    fingerprint: Vec<u8>,
) -> wire::LocationState {
    let restart = restarted_route(&s, &r);
    s.point = next_point(&s, &r, point);
    s.enabled = r.enabled;
    s.source = if r.use_host { "network" } else { "manual" }.into();
    if r.use_host {
        s.host = point;
    }
    s.route = r.route;
    s.speed_kph = r.speed_kph;
    s.moving = r.enabled && r.moving;
    s.repeat = r.repeat;
    s.request_id = r.request_id;
    s.request_hash = fingerprint;
    s.revision += 1;
    if restart {
        s.distance_m = 0.;
    }
    if s.moving {
        s.point = motion::at(&s.route, s.distance_m, s.repeat).map(|v| v.0);
    }
    s.updated_at_ms = now();
    s
}
async fn requested_point(app: &App, r: &wire::LocationUpdate) -> (Option<wire::GeoPoint>, String) {
    if !r.enabled || !r.use_host {
        return (r.point, String::new());
    }
    match host(app).await {
        Ok(p) => (Some(p), String::new()),
        Err(e) => (None, e),
    }
}
fn repeated(
    s: &wire::LocationState,
    r: &wire::LocationUpdate,
    fingerprint: &[u8],
) -> Result<bool, String> {
    if s.request_id != r.request_id {
        return Ok(false);
    }
    if s.request_hash != fingerprint {
        return Err("操作编号已用于其他位置请求".into());
    }
    Ok(true)
}
async fn update(app: &App, r: wire::LocationUpdate) -> Result<wire::LocationState, String> {
    validate(&r)?;
    let _guard = MUTATION.lock().await;
    let current = load(app).await?;
    let fingerprint = Sha256::digest(r.encode_to_vec()).to_vec();
    if repeated(&current, &r, &fingerprint)? {
        return Ok(current);
    }
    if current.revision != r.expected_revision {
        return Err("位置已被更新，请刷新后重试".into());
    }
    let (point, lookup_error) = requested_point(app, &r).await;
    let mut state = configure(current, r, point, fingerprint);
    if !lookup_error.is_empty() {
        state.moving = false;
        state.error = lookup_error;
        state.devices.clear();
        save(app, &state).await?;
        return Ok(state);
    }
    commit_devices(app, &mut state).await?;
    Ok(state)
}
async fn commit_devices(app: &App, s: &mut wire::LocationState) -> Result<(), String> {
    // Persist intent before touching devices. Unknown motion must never resume itself.
    s.error = "位置更新中，设备结果待确认".into();
    save(app, s).await?;
    apply(app, s).await?;
    save(app, s).await
}
pub async fn route(
    State(app): State<Shared>,
    Path(op): Path<String>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let result = match op.as_str() {
        "state" => {
            if let Err(e) = decode::<wire::LocationRequest>(&app, &headers, body) {
                return e.into();
            }
            load(&app).await
        }
        "update" => {
            let r = match decode::<wire::LocationUpdate>(&app, &headers, body) {
                Ok(v) => v,
                Err(e) => return e.into(),
            };
            update(&app, r).await
        }
        _ => return error(StatusCode::NOT_FOUND, "not_found", "操作不存在"),
    };
    match result {
        Ok(s) => pb(StatusCode::OK, s),
        Err(e) => error(StatusCode::CONFLICT, "location_error", &e),
    }
}
pub async fn work(app: Shared) {
    // Interrupted motion requires a deliberate resume. Static overrides remain in guests.
    if let Ok(mut s) = load(&app).await
        && s.moving
    {
        s.moving = false;
        s.revision += 1;
        s.error = "位置运动已中断，请检查设备后继续".into();
        let _ = save(&app, &s).await;
    }
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let Ok(_guard) = MUTATION.try_lock() else {
            continue;
        };
        let Ok(mut s) = load(&app).await else {
            continue;
        };
        if !s.enabled || !s.moving || !s.error.is_empty() {
            continue;
        }
        let elapsed = now().saturating_sub(s.updated_at_ms).min(5000) as f64 / 1000.;
        s.distance_m += elapsed * s.speed_kph / 3.6;
        if let Some((p, ended)) = motion::at(&s.route, s.distance_m, s.repeat) {
            s.point = Some(p);
            s.moving = !ended;
        }
        s.updated_at_ms = now();
        s.error = "位置更新中，设备结果待确认".into();
        if save(&app, &s).await.is_err() {
            continue;
        }
        if let Err(e) = apply(&app, &mut s).await {
            s.moving = false;
            s.error = e;
        }
        let _ = save(&app, &s).await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_needs_no_location_or_network() {
        let r = wire::LocationUpdate {
            request_id: Uuid::new_v4().to_string(),
            ..Default::default()
        };
        assert!(validate(&r).is_ok());
    }
    #[test]
    fn rejects_invalid_routes_before_any_effect() {
        let mut r = wire::LocationUpdate {
            enabled: true,
            request_id: Uuid::new_v4().to_string(),
            speed_kph: 5.,
            use_host: true,
            moving: true,
            ..Default::default()
        };
        assert!(validate(&r).is_err());
        r.moving = false;
        assert!(validate(&r).is_ok());
        r.speed_kph = f64::NAN;
        assert!(validate(&r).is_err());
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use futures_util::FutureExt;
    use sqlx::Connection;
    use std::{panic::AssertUnwindSafe, str::FromStr};
    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL on a non-production port"]
    async fn location_defaults_off_and_configuration_revision_survives_runtime_receipts() {
        let url = std::env::var("AI_PHONE_TEST_DATABASE_URL").expect("isolated fixture required");
        let parsed = reqwest::Url::parse(&url).unwrap();
        assert_eq!(parsed.host_str(), Some("127.0.0.1"));
        assert_ne!(parsed.port(), Some(55482));
        let mut admin = sqlx::PgConnection::connect(&url).await.unwrap();
        let name = format!("aiphone_location_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut admin)
            .await
            .unwrap();
        let db = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect_with(
                sqlx::postgres::PgConnectOptions::from_str(&url)
                    .unwrap()
                    .database(&name),
            )
            .await
            .unwrap();
        let root = std::env::temp_dir().join(&name);
        let app = App {
            db: db.clone(),
            root: root.clone(),
            token: "location-fixture".into(),
            phone_token: "must-not-call".into(),
            settings: settings::SettingsStore::test_configured(&root),
            local: reqwest::Client::builder().no_proxy().build().unwrap(),
            remote: reqwest::Client::new(),
            active: Mutex::new(None),
            login_attempts: Mutex::new(vec![]),
        };
        let result = AssertUnwindSafe(async {
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            // This fixture owns no live runtimes, so no guest process may be called.
            sqlx::query("UPDATE devices SET runtime_state='planned'")
                .execute(&db)
                .await
                .unwrap();
            let initial = load(&app).await.unwrap();
            assert!(!initial.enabled);
            assert!(initial.host.is_none());
            assert!(initial.point.is_none());
            assert_eq!(initial.revision, 0);
            let request = wire::LocationUpdate {
                request_id: Uuid::new_v4().to_string(),
                ..Default::default()
            };
            let first = update(&app, request.clone()).await.unwrap();
            assert_eq!(first.revision, 1);
            assert!(first.devices.is_empty());
            assert_eq!(update(&app, request.clone()).await.unwrap(), first);
            let mut conflict = request.clone();
            conflict.enabled = true;
            conflict.use_host = true;
            conflict.speed_kph = 5.;
            assert!(update(&app, conflict).await.is_err());
            let mut receipt = first.clone();
            receipt.updated_at_ms = now();
            save(&app, &receipt).await.unwrap();
            assert_eq!(load(&app).await.unwrap().revision, 1);
            let stale = wire::LocationUpdate {
                request_id: Uuid::new_v4().to_string(),
                ..Default::default()
            };
            assert!(update(&app, stale).await.is_err());
            let bytes = load(&app).await.unwrap().encode_to_vec();
            assert_eq!(
                wire::LocationState::decode(bytes.as_slice())
                    .unwrap()
                    .revision,
                1
            );
        })
        .catch_unwind()
        .await;
        db.close().await;
        let _ = std::fs::remove_dir_all(root);
        sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .execute(&mut admin)
            .await
            .unwrap();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
