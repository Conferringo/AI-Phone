//! Guest-only durable Dunst inbox. Notification text is data, never a command.
use crate::{Error, Result, process};
use fs2::FileExt;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::BTreeMap,
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const MAX_EVENTS: usize = 256;
const MAX_BYTES: usize = 2_000_000;
const ENV_FIELDS: [&str; 7] = [
    "DBUS_SESSION_BUS_ADDRESS",
    "DUNST_ID",
    "DUNST_DESKTOP_ENTRY",
    "DUNST_APP_NAME",
    "DUNST_SUMMARY",
    "DUNST_BODY",
    "HOME",
];

/// The bridge passes its actual user ID; no fixed guest UID is assumed.
pub async fn list(runner: &impl process::Runner, folder: &Path, uid: u32) -> Result<Value> {
    let uid = uid.to_string();
    let mut request = process::Request::new("pgrep", &["-u", &uid, "-x", "dunst"]);
    request.capture = process::Capture::Discard;
    let connected = runner.execute(request).await?.code == 0;
    let notifications = read_events(&folder.join("notifications.json"))?;
    Ok(json!({"connected": connected, "notifications": notifications}))
}

pub fn record_from_env() -> Result<()> {
    let env = ENV_FIELDS
        .into_iter()
        .map(read_env)
        .collect::<Result<BTreeMap<_, _>>>()?;
    let home = env
        .get("HOME")
        .filter(|home| !home.is_empty())
        .ok_or("HOME is missing")?;
    record(&PathBuf::from(home).join(".aiphone"), &env)
}

fn read_env(key: &str) -> Result<(String, String)> {
    let value = match std::env::var(key) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(error) => return Err(error.into()),
    };
    Ok((key.into(), value))
}

pub fn record(folder: &Path, env: &BTreeMap<String, String>) -> Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)?;
    let _lock = inbox_lock(folder)?;
    let path = folder.join("notifications.json");
    let mut events = read_events(&path)?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let event = event(env, &events, Uuid::new_v4(), u64::try_from(timestamp)?);
    events.push(event);
    let bytes = encode_events(events)?;
    replace_inbox(folder, &path, &bytes)
}

fn inbox_lock(folder: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(folder.join("notifications.lock"))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.lock_exclusive()?;
    Ok(file)
}

fn read_events(path: &Path) -> Result<Vec<Value>> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn field(env: &BTreeMap<String, String>, key: &str, limit: usize) -> String {
    env.get(key)
        .into_iter()
        .flat_map(|value| value.chars())
        .take(limit)
        .collect()
}

fn event(env: &BTreeMap<String, String>, previous: &[Value], id: Uuid, timestamp: u64) -> Value {
    let session = env
        .get("DBUS_SESSION_BUS_ADDRESS")
        .map(String::as_str)
        .unwrap_or("");
    let notification_id = field(env, "DUNST_ID", 80);
    let mut previous_keys = previous
        .iter()
        .filter_map(|event| {
            let key = event["notification_key"].as_str()?;
            let (session, id) = key.strip_prefix("desktop:")?.split_once(':')?;
            (session != "v2" && id == notification_id).then_some(key)
        })
        .peekable();
    let retained_key = previous_keys
        .peek()
        .is_some()
        .then(|| {
            let hash = format!("{:x}", Sha256::digest(session.as_bytes()));
            format!("desktop:{}:{notification_id}", &hash[..12])
        })
        .filter(|old| previous_keys.any(|key| key == old));
    let notification_key = retained_key.unwrap_or_else(|| {
        let hash = format!("{:x}", Sha512::digest(session.as_bytes()));
        format!("desktop:v2:{}:{notification_id}", &hash[..12])
    });
    let package = env
        .get("DUNST_DESKTOP_ENTRY")
        .filter(|value| !value.is_empty())
        .or_else(|| env.get("DUNST_APP_NAME"));
    json!({
        "event_id": format!("00000000000000000000000000000000{}", id.simple()),
        "notification_key": notification_key,
        "package_name": package.into_iter().flat_map(|value| value.chars()).take(240).collect::<String>(),
        "app_name": field(env, "DUNST_APP_NAME", 240),
        "title": field(env, "DUNST_SUMMARY", 1000),
        "text": field(env, "DUNST_BODY", 12000),
        "posted_at_ms": timestamp,
    })
}

// Match Python json.dumps(..., ensure_ascii=False) spacing for the retention budget.
// Only the outer event array uses compact commas, as the original hook did.
struct InboxFormatter;
impl InboxFormatter {
    fn separator<W: ?Sized + Write>(writer: &mut W, first: bool) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }
}
impl serde_json::ser::Formatter for InboxFormatter {
    fn begin_array_value<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        Self::separator(writer, first)
    }
    fn begin_object_key<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        Self::separator(writer, first)
    }
    fn begin_object_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b": ")
    }
}

fn encode_event(event: &Value) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    event.serialize(&mut serde_json::Serializer::with_formatter(
        &mut bytes,
        InboxFormatter,
    ))?;
    Ok(bytes)
}

fn encode_events(events: Vec<Value>) -> Result<Vec<u8>> {
    let skip = events.len().saturating_sub(MAX_EVENTS);
    let mut encoded = events
        .into_iter()
        .skip(skip)
        .map(|event| encode_event(&event))
        .collect::<Result<std::collections::VecDeque<_>>>()?;
    let mut size = encoded.iter().map(Vec::len).sum::<usize>() + encoded.len() + 1;
    while size > MAX_BYTES && encoded.len() > 1 {
        size -= encoded.pop_front().ok_or("missing inbox event")?.len() + 1;
    }
    let mut output = Vec::with_capacity(size);
    output.push(b'[');
    for (index, bytes) in encoded.into_iter().enumerate() {
        if index != 0 {
            output.push(b',');
        }
        output.extend(bytes);
    }
    output.push(b']');
    Ok(output)
}

fn close_after_error(file: tempfile::NamedTempFile, error: Error) -> Error {
    match file.close() {
        Ok(()) => error,
        Err(cleanup) => {
            format!("{error}; notification temporary-file cleanup failed: {cleanup}").into()
        }
    }
}

fn replace_inbox(folder: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = tempfile::Builder::new()
        .prefix("notifications-")
        .tempfile_in(folder)?;
    if let Err(error) = write_inbox(file.as_file_mut(), bytes) {
        return Err(close_after_error(file, error));
    }
    match file.persist(path) {
        Ok(_) => Ok(()),
        Err(error) => Err(close_after_error(error.file, error.error.into())),
    }
}

fn write_inbox(file: &mut File, bytes: &[u8]) -> Result<()> {
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn env(values: &[(&str, &str)]) -> BTreeMap<String, String> {
        values
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into()))
            .collect()
    }

    #[test]
    fn deterministic_event_preserves_unicode_and_stable_identifiers() {
        let values = env(&[
            ("DBUS_SESSION_BUS_ADDRESS", "fixture-session"),
            ("DUNST_ID", "通知"),
            ("DUNST_APP_NAME", "微信"),
            ("DUNST_DESKTOP_ENTRY", ""),
            ("DUNST_BODY", "$(touch never)🐳"),
        ]);
        let output = event(&values, &[], Uuid::nil(), 123);
        assert_eq!(output["event_id"], "0".repeat(64));
        assert_eq!(output["package_name"], "微信");
        assert_eq!(
            output["notification_key"],
            format!(
                "desktop:v2:{}:通知",
                &format!("{:x}", Sha512::digest(b"fixture-session"))[..12]
            )
        );
        assert_eq!(output["text"], "$(touch never)🐳");
        assert_eq!(output["posted_at_ms"], 123);
        let previous = [json!({
            "notification_key": format!("desktop:{}:通知",
                &format!("{:x}", Sha256::digest(b"fixture-session"))[..12])
        })];
        let repeated = event(&values, &previous, Uuid::from_u128(1), 124);
        assert_eq!(
            repeated["notification_key"],
            previous[0]["notification_key"]
        );
        assert_ne!(repeated["event_id"], output["event_id"]);
        let mut other_session = values;
        other_session.insert("DBUS_SESSION_BUS_ADDRESS".into(), "new-session".into());
        assert_ne!(
            event(&other_session, &previous, Uuid::from_u128(2), 125)["notification_key"],
            previous[0]["notification_key"]
        );
    }

    #[test]
    fn encoded_budget_uses_original_unicode_spacing_and_retains_unknown_values() {
        let encoded = encode_events(vec![
            json!({"extra": ["🐳", 3], "nested": {"k": true}}),
            json!(null),
        ])
        .unwrap();
        assert_eq!(
            String::from_utf8(encoded).unwrap(),
            "[{\"extra\": [\"🐳\", 3], \"nested\": {\"k\": true}},null]"
        );
        let large = "1234567890123456789012345678901234567890";
        let value = serde_json::from_str(large).unwrap();
        assert_eq!(
            String::from_utf8(encode_events(vec![value]).unwrap()).unwrap(),
            format!("[{large}]")
        );
    }

    #[test]
    fn parallel_notifications_are_durable_private_and_never_execute_content() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("inbox");
        std::thread::scope(|scope| {
            for index in 0..40 {
                let folder = &folder;
                scope.spawn(move || {
                    record(
                        folder,
                        &env(&[
                            ("DUNST_ID", &index.to_string()),
                            ("DUNST_BODY", "$(touch never)"),
                        ]),
                    )
                    .unwrap()
                });
            }
        });
        let events = read_events(&folder.join("notifications.json")).unwrap();
        assert_eq!(events.len(), 40);
        assert_eq!(
            events
                .iter()
                .map(|event| event["event_id"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            40
        );
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 2);
        for entry in fs::read_dir(&folder).unwrap() {
            assert_eq!(entry.unwrap().metadata().unwrap().mode() & 0o077, 0);
        }
        assert_eq!(fs::metadata(&folder).unwrap().mode() & 0o777, 0o700);
    }

    #[test]
    fn existing_home_identity_and_mode_are_unchanged() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o750)).unwrap();
        let before = fs::metadata(root.path()).unwrap();
        record(root.path(), &BTreeMap::new()).unwrap();
        let after = fs::metadata(root.path()).unwrap();
        assert_eq!(
            (before.ino(), before.uid(), before.gid(), before.mode()),
            (after.ino(), after.uid(), after.gid(), after.mode())
        );
        assert_eq!(
            fs::metadata(root.path().join("notifications.json"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn corrupt_or_non_array_inbox_is_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("notifications.json");
        for bytes in [b"broken".as_slice(), b"{}".as_slice()] {
            fs::write(&path, bytes).unwrap();
            assert!(record(root.path(), &BTreeMap::new()).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
        }
    }

    #[test]
    fn replacement_failure_keeps_old_target_and_removes_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("notifications.json");
        fs::create_dir(&path).unwrap();
        assert!(replace_inbox(root.path(), &path, b"[]").is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn count_byte_and_unicode_limits_match_original_retention_policy() {
        let short = (0..270)
            .map(|index| json!({"text": index.to_string()}))
            .collect();
        let events: Vec<Value> = serde_json::from_slice(&encode_events(short).unwrap()).unwrap();
        assert_eq!(events.len(), 256);
        assert_eq!(events[0]["text"], "14");
        let body = "🐳".repeat(14000);
        let values = env(&[
            ("DUNST_ID", &"汉".repeat(100)),
            ("DUNST_BODY", &body),
            ("DUNST_SUMMARY", &"🐳".repeat(1100)),
        ]);
        let item = event(&values, &[], Uuid::nil(), 1);
        assert_eq!(item["text"].as_str().unwrap().chars().count(), 12000);
        assert_eq!(item["title"].as_str().unwrap().chars().count(), 1000);
        assert_eq!(
            item["notification_key"].as_str().unwrap().chars().count(),
            "desktop:".len() + 12 + 1 + 80
        );
        let bytes = encode_events(vec![item; 50]).unwrap();
        assert!(bytes.len() <= MAX_BYTES);
        let events: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
        assert!(events.len() < 50);
    }

    struct FakeRunner(i32);
    impl process::Runner for FakeRunner {
        async fn execute(&self, request: process::Request) -> Result<process::Output> {
            assert_eq!(request.program, "pgrep");
            assert_eq!(
                request.args,
                ["-u", "1234", "-x", "dunst"].map(std::ffi::OsString::from)
            );
            assert!(matches!(request.capture, process::Capture::Discard));
            Ok(process::Output {
                code: self.0,
                stdout: vec![],
                stderr: vec![],
            })
        }
    }

    #[tokio::test]
    async fn list_reads_guest_inbox_with_actual_uid_and_connection_status() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            list(&FakeRunner(1), root.path(), 1234).await.unwrap(),
            json!({"connected": false, "notifications": []})
        );
        fs::write(
            root.path().join("notifications.json"),
            b"[{\"unknown\":true}]",
        )
        .unwrap();
        assert_eq!(
            list(&FakeRunner(0), root.path(), 1234).await.unwrap(),
            json!({"connected": true, "notifications": [{"unknown": true}]})
        );
    }
}
