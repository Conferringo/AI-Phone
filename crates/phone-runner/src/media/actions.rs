use super::{
    Duration, Media, Ordering, Result,
    grpc::{
        Session,
        wire::{SensorValue, Touch, TouchEvent},
    },
    now, wire,
};
use tokio::time::Instant;
use wire::device_action_request::Action;
type Contact = (u32, u32, u32);
fn physical(x: u32, y: u32, rotation: u32) -> (i32, i32) {
    let (x, y) = (x as i32, y as i32);
    match rotation {
        1 => (719 - y, x),
        2 => (719 - x, 1439 - y),
        3 => (y, 1439 - x),
        _ => (x, y),
    }
}
fn interpolate(points: &[wire::GesturePoint], time: u32) -> (u32, u32) {
    for pair in points.windows(2) {
        if time <= pair[1].at_ms {
            let a = &pair[0];
            let b = &pair[1];
            let progress = (time - a.at_ms) as f64 / (b.at_ms - a.at_ms) as f64;
            return (
                (a.x as f64 + (b.x as f64 - a.x as f64) * progress).round() as u32,
                (a.y as f64 + (b.y as f64 - a.y as f64) * progress).round() as u32,
            );
        }
    }
    let last = points.last().unwrap();
    (last.x, last.y)
}
async fn send(
    media: &Media,
    session: &mut Session,
    contacts: &[Contact],
    pressure: i32,
    rotation: u32,
    began: Instant,
) -> Result<()> {
    let touches = contacts
        .iter()
        .map(|&(x, y, id)| {
            let (x, y) = physical(x, y, rotation);
            Touch {
                x,
                y,
                identifier: id as i32,
                pressure,
                ..Default::default()
            }
        })
        .collect();
    session
        .client
        .send_touch(session.command(TouchEvent {
            touches,
            display: 0,
        }))
        .await?;
    let mut cursor = media.cursor.lock().await;
    let at_ms = began.elapsed().as_millis() as u32;
    for &(x, y, pointer_id) in contacts {
        let index =
            if let Some(index) = cursor.paths.iter().position(|p| p.pointer_id == pointer_id) {
                index
            } else {
                cursor.paths.push(wire::FingerPath {
                    pointer_id,
                    points: vec![],
                });
                cursor.paths.len() - 1
            };
        let points = &mut cursor.paths[index].points;
        let point = wire::GesturePoint { x, y, at_ms };
        if points.last().is_some_and(|v| v.at_ms == at_ms) {
            *points.last_mut().unwrap() = point;
        } else {
            points.push(point);
        }
        if points.len() > 128 {
            points.remove(1);
        }
    }
    cursor.input_confirmed = !cursor.paths.is_empty();
    cursor.duration_ms = at_ms;
    Ok(())
}
pub async fn execute(media: &Media, request: wire::DeviceActionRequest) -> Result<()> {
    let rotation = media.orientation().await?;
    let mut session = media.session()?;
    let action = request.action.ok_or("action missing")?;
    let kind = match &action {
        Action::Tap(_) => "tap",
        Action::LongPress(_) => "hold",
        Action::Swipe(_) => "swipe",
        Action::Drag(_) => "drag",
        Action::Gesture(_) => "gesture",
        Action::Motion(_) => "motion",
        _ => return Err("unsupported media action".into()),
    };
    *media.cursor.lock().await = wire::DeviceCursor {
        device_id: request.phone_id,
        action_id: request.action_id,
        actor: request.controller,
        kind: kind.into(),
        status: "running".into(),
        started_at_ms: now(),
        width: media.width.load(Ordering::Relaxed),
        height: media.height.load(Ordering::Relaxed),
        ..Default::default()
    };
    let began = Instant::now();
    let mut active: Vec<Contact> = vec![];
    let result: Result<()> = async {
        match action {
            Action::Tap(v) => {
                active = vec![(v.x, v.y, 0)];
                send(media, &mut session, &active, 1024, rotation, began).await?;
                tokio::time::sleep(Duration::from_millis(12)).await;
            }
            Action::LongPress(v) => {
                active = vec![(v.x, v.y, 0)];
                send(media, &mut session, &active, 1024, rotation, began).await?;
                tokio::time::sleep(Duration::from_millis(v.duration_ms as u64)).await;
            }
            Action::Swipe(v) => {
                slide(media, &mut session, &mut active, &v, rotation, began, false).await?
            }
            Action::Drag(v) => {
                slide(
                    media,
                    &mut session,
                    &mut active,
                    &wire::SwipeAction {
                        x1: v.x1,
                        y1: v.y1,
                        x2: v.x2,
                        y2: v.y2,
                        duration_ms: v.duration_ms,
                    },
                    rotation,
                    began,
                    true,
                )
                .await?
            }
            Action::Gesture(v) => {
                let started = Instant::now();
                loop {
                    let time = (started.elapsed().as_millis() as u32).min(v.duration_ms);
                    active = v
                        .paths
                        .iter()
                        .map(|path| {
                            let (x, y) = interpolate(&path.points, time);
                            (x, y, path.pointer_id)
                        })
                        .collect();
                    send(
                        media,
                        &mut session,
                        &active,
                        (v.pressure * 1024.0).round() as i32,
                        rotation,
                        began,
                    )
                    .await?;
                    if time >= v.duration_ms {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(16)).await;
                }
            }
            Action::Motion(v) => {
                let original = session
                    .client
                    .get_sensor(session.command(SensorValue::default()))
                    .await?
                    .into_inner();
                if original.status != 0 {
                    return Err("acceleration sensor unavailable".into());
                }
                let base = original
                    .value
                    .as_ref()
                    .ok_or("sensor values missing")?
                    .data
                    .clone();
                let moving: Result<()> = async {
                    let started = Instant::now();
                    while started.elapsed().as_millis() < (v.duration_ms as u128) {
                        let mut value = original.clone();
                        value.value.as_mut().unwrap().data[0] = base[0]
                            + v.amplitude
                                * (started.elapsed().as_secs_f32()
                                    * v.frequency_hz
                                    * std::f32::consts::TAU)
                                    .sin();
                        session.client.set_sensor(session.command(value)).await?;
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    Ok(())
                }
                .await;
                let restored = session.client.set_sensor(session.command(original)).await;
                moving?;
                restored?;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    .await;
    let released = if active.is_empty() {
        Ok(())
    } else {
        send(media, &mut session, &active, 0, rotation, began).await
    };
    result?;
    released?;
    media.cursor.lock().await.status = "completed".into();
    Ok(())
}
async fn slide(
    media: &Media,
    session: &mut Session,
    active: &mut Vec<Contact>,
    value: &wire::SwipeAction,
    rotation: u32,
    began: Instant,
    drag: bool,
) -> Result<()> {
    *active = vec![(value.x1, value.y1, 0)];
    send(media, session, active, 1024, rotation, began).await?;
    if drag {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let started = Instant::now();
    let duration = value.duration_ms.clamp(100, 1500);
    loop {
        let progress = (started.elapsed().as_millis() as f64 / duration as f64).min(1.0);
        let x = (value.x1 as f64 + (value.x2 as f64 - value.x1 as f64) * progress).round() as u32;
        let y = (value.y1 as f64 + (value.y2 as f64 - value.y1 as f64) * progress).round() as u32;
        *active = vec![(x, y, 0)];
        send(media, session, active, 1024, rotation, began).await?;
        if progress >= 1.0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(16)).await;
    }
    Ok(())
}
