use ai_phone_api::wire;
pub fn valid(v: &wire::GestureAction, w: u32, h: u32) -> bool {
    if v.paths.is_empty()
        || v.paths.len() > 5
        || !(100..=3000).contains(&v.duration_ms)
        || !v.pressure.is_finite()
        || !(0.1..=1.0).contains(&v.pressure)
    {
        return false;
    }
    let mut ids = std::collections::HashSet::new();
    v.paths.iter().all(|p| {
        p.pointer_id < 5
            && ids.insert(p.pointer_id)
            && p.points.len() >= 2
            && p.points.len() <= 64
            && p.points.first().is_some_and(|v| v.at_ms == 0)
            && p.points.last().is_some_and(|x| x.at_ms == v.duration_ms)
            && p.points.iter().all(|x| x.x < w && x.y < h)
            && p.points.windows(2).all(|x| x[0].at_ms < x[1].at_ms)
    })
}
pub fn motion(v: &wire::MotionAction) -> bool {
    v.kind == "shake"
        && v.amplitude.is_finite()
        && (0.5..=15.0).contains(&v.amplitude)
        && v.frequency_hz.is_finite()
        && (0.5..=8.0).contains(&v.frequency_hz)
        && (200..=3000).contains(&v.duration_ms)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_paths_and_pressure() {
        let mut v = wire::GestureAction {
            paths: vec![wire::FingerPath {
                pointer_id: 0,
                points: vec![
                    wire::GesturePoint {
                        x: 10,
                        y: 10,
                        at_ms: 0,
                    },
                    wire::GesturePoint {
                        x: 100,
                        y: 200,
                        at_ms: 500,
                    },
                ],
            }],
            duration_ms: 500,
            pressure: 0.7,
        };
        assert!(valid(&v, 720, 1440));
        v.paths.push(v.paths[0].clone());
        assert!(!valid(&v, 720, 1440));
        v.paths.pop();
        v.paths[0].points[1].x = 720;
        assert!(!valid(&v, 720, 1440));
        v.paths[0].points[1].x = 100;
        v.pressure = f32::NAN;
        assert!(!valid(&v, 720, 1440));
    }
    #[test]
    fn bounds_sensor_motion() {
        let v = wire::MotionAction {
            kind: "shake".into(),
            amplitude: 3.,
            frequency_hz: 2.,
            duration_ms: 800,
        };
        assert!(motion(&v));
        assert!(!motion(&wire::MotionAction {
            amplitude: 100.,
            ..v
        }));
    }
}
