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
  umask 077
  cmd location is-location-enabled > "$marker.new"
  appops get 0 android:mock_location | sed -n 's/.*MOCK_LOCATION: \([a-z]*\).*/\1/p' | head -1 > "$marker.appop"
  mv "$marker.new" "$marker"
fi
boot="$(cat /proc/sys/kernel/random/boot_id)"
if [ ! -f "$marker.ready" ] || [ "$(cat "$marker.boot" 2>/dev/null || true)" != "$boot" ]; then
  appops set 0 android:mock_location allow
  cmd location providers add-test-provider gps --requiresSatellite --supportsAltitude --supportsSpeed --supportsBearing
  cmd location providers add-test-provider network --requiresNetwork
  printf '%s' "$boot" > "$marker.boot"
  touch "$marker.ready"
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
  rm -f "$marker.ready"
  cmd location providers remove-test-provider gps
  cmd location providers remove-test-provider network
  cmd location set-location-enabled "$(cat "$marker")"
  previous="$(cat "$marker.appop" 2>/dev/null || true)"
  case "$previous" in allow|ignore|deny|foreground) ;; *) previous=default ;; esac
  appops set 0 android:mock_location "$previous"
  rm -f "$marker.appop" "$marker.boot"
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
    fn location_legacy_marker_can_be_disabled_after_partial_cleanup() {
        use std::{fs, os::unix::fs::PermissionsExt, process::Command};
        let folder =
            std::env::temp_dir().join(format!("aiphone-location-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&folder).unwrap();
        let marker = folder.join("before");
        fs::write(&marker, "true\n").unwrap();
        for name in ["cmd", "appops"] {
            let script = folder.join(name);
            fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
            fs::set_permissions(script, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let body = command(&wire::LocationFix::default()).unwrap()[1].replace(
            "/data/local/tmp/ai-phone-location-before",
            marker.to_str().unwrap(),
        );
        let result = Command::new("/bin/sh")
            .arg("-c")
            .arg(body)
            .env(
                "PATH",
                format!("{}:{}", folder.display(), std::env::var("PATH").unwrap()),
            )
            .output()
            .unwrap();
        let retained = marker.exists();
        fs::remove_dir_all(folder).unwrap();
        assert!(
            result.status.success(),
            "optional legacy metadata must not interrupt cleanup"
        );
        assert!(!retained, "no stale ownership marker after disabling");
    }

    #[test]
    fn location_recovers_partial_provider_setup_and_guest_reboot_and_restores_original_settings() {
        use std::{fs, os::unix::fs::PermissionsExt, process::Command};
        let folder =
            std::env::temp_dir().join(format!("aiphone-gps-recovery-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&folder).unwrap();
        let source = r#"#!/usr/bin/env python3
import os,sys
from pathlib import Path
root=Path(os.environ['LOCATION_FIXTURE']);args=sys.argv[1:]
if Path(sys.argv[0]).name=='appops':
 if args[0]=='get': print('Uid mode: MOCK_LOCATION: '+(root/'appop').read_text())
 else:(root/'appop').write_text(args[-1])
elif args[1]=='is-location-enabled':print((root/'enabled').read_text())
elif args[1]=='set-location-enabled':(root/'enabled').write_text(args[2])
else:
 operation,provider=args[2:4];p=root/('provider-'+provider)
 if operation=='add-test-provider':
  if provider=='network' and (root/'fail-network').exists():(root/'fail-network').unlink();sys.exit(42)
  p.touch()
 elif operation=='remove-test-provider':p.unlink(missing_ok=True)
 elif not p.exists():sys.exit(43)
 elif operation=='set-test-provider-location':(root/'fix').write_text(args[5])
"#;
        for name in ["cmd", "appops"] {
            let path = folder.join(name);
            fs::write(&path, source).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(folder.join("enabled"), "false").unwrap();
        fs::write(folder.join("appop"), "ignore").unwrap();
        fs::write(folder.join("boot"), "first").unwrap();
        fs::write(folder.join("fail-network"), "").unwrap();
        let fix = wire::LocationFix {
            enabled: true,
            point: Some(wire::GeoPoint {
                latitude: 31.,
                longitude: 121.,
                accuracy_m: 5.,
            }),
            ..Default::default()
        };
        let run = |fix: &wire::LocationFix| {
            let body = command(fix).unwrap()[1]
                .replace(
                    "/data/local/tmp/ai-phone-location-before",
                    folder.join("before").to_str().unwrap(),
                )
                .replace(
                    "/proc/sys/kernel/random/boot_id",
                    folder.join("boot").to_str().unwrap(),
                );
            Command::new("/bin/sh")
                .args(["-c", &body])
                .env("LOCATION_FIXTURE", &folder)
                .env(
                    "PATH",
                    format!("{}:{}", folder.display(), std::env::var("PATH").unwrap()),
                )
                .output()
                .unwrap()
                .status
                .success()
        };
        assert!(!run(&fix));
        assert!(!folder.join("before.ready").exists());
        assert!(run(&fix));
        assert_eq!(fs::read_to_string(folder.join("fix")).unwrap(), "31,121");
        for name in ["provider-gps", "provider-network"] {
            fs::remove_file(folder.join(name)).unwrap();
        }
        fs::write(folder.join("boot"), "second").unwrap();
        assert!(run(&fix));
        assert!(run(&wire::LocationFix::default()));
        assert_eq!(
            fs::read_to_string(folder.join("enabled")).unwrap().trim(),
            "false"
        );
        assert_eq!(fs::read_to_string(folder.join("appop")).unwrap(), "ignore");
        assert!(!folder.join("before").exists());
        fs::remove_dir_all(folder).unwrap();
    }

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
