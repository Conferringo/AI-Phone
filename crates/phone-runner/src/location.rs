//! Mock providers are owned by this feature and removed when it is disabled.
use ai_phone_api::wire;
pub fn command(fix: &wire::LocationFix) -> Option<Vec<String>> {
    let body = if fix.enabled {
        let p = fix.point.as_ref()?;
        if !p.latitude.is_finite()
            || !(-90.0..=90.0).contains(&p.latitude)
            || !p.longitude.is_finite()
            || !(-180.0..=180.0).contains(&p.longitude)
            || !p.accuracy_m.is_finite()
            || !(0.0..=100_000.0).contains(&p.accuracy_m)
        {
            return None;
        }
        format!(
            r#"set -e
marker=/data/local/tmp/ai-phone-location-before
if [ ! -f "$marker" ]; then
  cmd location is-location-enabled > "$marker"
  appops get 0 android:mock_location | sed -n 's/.*MOCK_LOCATION: \([a-z]*\).*/\1/p' | head -1 > "$marker.appop"
  chmod 600 "$marker"
  appops set 0 android:mock_location allow
  cmd location providers add-test-provider gps --requiresSatellite --supportsAltitude --supportsSpeed --supportsBearing
  cmd location providers add-test-provider network --requiresNetwork
fi
cmd location set-location-enabled true
for provider in gps network; do
  cmd location providers set-test-provider-enabled "$provider" true
  cmd location providers set-test-provider-location "$provider" --location {},{} --accuracy {}
done
printf 'location-applied'
"#,
            p.latitude,
            p.longitude,
            p.accuracy_m.max(1.0)
        )
    } else {
        r#"set -e
marker=/data/local/tmp/ai-phone-location-before
if [ -f "$marker" ]; then
  cmd location providers remove-test-provider gps
  cmd location providers remove-test-provider network
  cmd location set-location-enabled "$(cat "$marker")"
  previous="$(cat "$marker.appop" 2>/dev/null)"
  case "$previous" in allow|ignore|deny|foreground) ;; *) previous=default ;; esac
  appops set 0 android:mock_location "$previous"
  rm -f "$marker.appop"
  rm "$marker"
fi
printf 'location-disabled'
"#
        .to_string()
    };
    Some(vec!["shell".into(), body])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_nonfinite_and_out_of_bounds_fixes() {
        let mut fix = wire::LocationFix {
            enabled: true,
            point: Some(wire::GeoPoint {
                latitude: 31.2,
                longitude: 121.5,
                accuracy_m: 5.,
            }),
            ..Default::default()
        };
        assert!(command(&fix).unwrap()[1].contains("31.2,121.5"));
        fix.point.as_mut().unwrap().longitude = 181.;
        assert!(command(&fix).is_none());
        fix.point.as_mut().unwrap().longitude = f64::NAN;
        assert!(command(&fix).is_none());
        fix.enabled = false;
        assert!(command(&fix).unwrap()[1].contains("remove-test-provider"));
    }
}
