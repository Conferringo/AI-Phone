//! Read only owned emulator advertisements; never search unrelated SDK profiles.
use crate::{Error, Result, filesystem};
use std::{collections::BTreeMap, path::Path};

fn values(bytes: Vec<u8>) -> Result<BTreeMap<String, String>> {
    let text =
        String::from_utf8(bytes).map_err(|_| Error::Invalid("emulator_discovery_invalid"))?;
    let mut rows = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if ["grpc.port", "port.serial", "grpc.token", "aiphone.bridge"].contains(&key)
            && rows.insert(key.into(), value.into()).is_some()
        {
            return Err(Error::Invalid("emulator_discovery_invalid"));
        }
    }
    Ok(rows)
}
pub(crate) fn token(
    directory: &Path,
    console: u16,
    grpc: u16,
    owner: Option<&str>,
) -> Result<Option<String>> {
    if !directory.try_exists()? {
        return Ok(None);
    }
    filesystem::existing_directory(directory)?;
    let mut found = None;
    for (index, entry) in std::fs::read_dir(directory)?.enumerate() {
        if index >= 128 {
            return Err(Error::Invalid("emulator_discovery_budget"));
        }
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(pid) = name
            .strip_prefix("pid_")
            .and_then(|v| v.strip_suffix(".ini"))
        else {
            continue;
        };
        if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let row = values(filesystem::bounded_file_with_limit(&entry.path(), 16384)?)?;
        if row.get("grpc.port") != Some(&grpc.to_string())
            || row.get("port.serial") != Some(&console.to_string())
        {
            continue;
        }
        if let Some(owner) = owner
            && row.get("aiphone.bridge").map(String::as_str) != Some(owner)
        {
            return Err(Error::Invalid("phone_peer_identity"));
        }
        let token = row
            .get("grpc.token")
            .ok_or(Error::Invalid("emulator_auth_unavailable"))?;
        if token.is_empty()
            || token.len() > 4096
            || token.chars().any(char::is_control)
            || found.is_some()
        {
            return Err(Error::Invalid("emulator_discovery_invalid"));
        }
        found = Some(token.clone());
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_exact_owned_ports_and_rejects_duplicates_and_wrong_pair() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("pid_0.ini");
        std::fs::write(&path,"grpc.port=8558\nport.serial=5564\ngrpc.token=private-fixture\naiphone.bridge=expected\n").unwrap();
        assert_eq!(
            token(root.path(), 5564, 8558, Some("expected"))
                .unwrap()
                .as_deref(),
            Some("private-fixture")
        );
        assert!(token(root.path(), 5564, 8558, Some("other")).is_err());
        assert!(token(root.path(), 5566, 8558, None).unwrap().is_none());
        std::fs::copy(&path, root.path().join("pid_1.ini")).unwrap();
        assert!(token(root.path(), 5564, 8558, None).is_err());
        std::fs::remove_file(root.path().join("pid_1.ini")).unwrap();
        std::fs::write(
            &path,
            "grpc.port=8558\nport.serial=5564\ngrpc.token=one\ngrpc.token=two\n",
        )
        .unwrap();
        assert!(token(root.path(), 5564, 8558, None).is_err());
    }
}
