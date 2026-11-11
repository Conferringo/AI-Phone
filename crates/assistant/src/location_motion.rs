//! Pure, WGS84 route interpolation. Motion advances only after device receipts.
use crate::wire::GeoPoint;
pub fn valid(p: &GeoPoint) -> bool {
    p.latitude.is_finite()
        && (-90.0..=90.0).contains(&p.latitude)
        && p.longitude.is_finite()
        && (-180.0..=180.0).contains(&p.longitude)
        && p.accuracy_m.is_finite()
        && (0.0..=100_000.0).contains(&p.accuracy_m)
}
fn delta(a: f64, b: f64) -> f64 {
    (b - a + 180.).rem_euclid(360.) - 180.
}
pub fn distance(a: &GeoPoint, b: &GeoPoint) -> f64 {
    let (x, y) = (a.latitude.to_radians(), b.latitude.to_radians());
    let h = ((y - x) / 2.).sin().powi(2)
        + x.cos()
            * y.cos()
            * (delta(a.longitude, b.longitude).to_radians() / 2.)
                .sin()
                .powi(2);
    12_742_000. * h.clamp(0., 1.).sqrt().asin()
}
pub fn length(route: &[GeoPoint]) -> f64 {
    route.windows(2).map(|p| distance(&p[0], &p[1])).sum()
}
pub fn at(route: &[GeoPoint], metres: f64, repeat: bool) -> Option<(GeoPoint, bool)> {
    let total = length(route);
    if total < 0.1 {
        return route.first().cloned().map(|p| (p, true));
    }
    let mut left = if repeat {
        metres.rem_euclid(total)
    } else {
        metres.min(total)
    };
    for pair in route.windows(2) {
        let d = distance(&pair[0], &pair[1]);
        if left <= d && d > 0. {
            let f = left / d;
            return Some((
                GeoPoint {
                    latitude: pair[0].latitude + (pair[1].latitude - pair[0].latitude) * f,
                    longitude: (pair[0].longitude
                        + delta(pair[0].longitude, pair[1].longitude) * f
                        + 180.)
                        .rem_euclid(360.)
                        - 180.,
                    accuracy_m: pair[0].accuracy_m.max(5.),
                },
                !repeat && metres >= total,
            ));
        }
        left -= d;
    }
    route.last().cloned().map(|p| (p, !repeat))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint {
            latitude: lat,
            longitude: lon,
            accuracy_m: 5.,
        }
    }
    #[test]
    fn interpolation_is_distance_based_and_crosses_dateline_short_way() {
        let route = [p(0., 179.999), p(0., -179.999)];
        let total = length(&route);
        assert!((220.0..224.0).contains(&total));
        let (mid, end) = at(&route, total / 2., false).unwrap();
        assert!(mid.longitude.abs() > 179.9999);
        assert!(!end);
        assert!(at(&route, total + 1., false).unwrap().1);
        assert!(!at(&route, total + 1., true).unwrap().1);
    }
    #[test]
    fn bounds_and_empty_routes() {
        assert!(at(&[], 2., true).is_none());
        assert!(!valid(&p(f64::NAN, 1.)));
        assert!(!valid(&p(91., 1.)));
        assert!(valid(&p(-90., -180.)));
    }
}
