//! Pinned, direct downloads. Cancellation polls one future without replaying it.
use super::ManagerState;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

pub(super) fn hash(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let length = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if length == 0 {
            break;
        }
        hash.update(&bytes[..length]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

async fn download_step<T>(
    manager: &ManagerState,
    future: impl std::future::Future<Output = Result<T, reqwest::Error>>,
) -> Result<T, String> {
    let mut future = Box::pin(future);
    loop {
        manager.check_cancelled()?;
        // Poll the same request; a timeout must not issue another HTTP request.
        if let Ok(result) = tokio::time::timeout(Duration::from_millis(100), future.as_mut()).await
        {
            return result.map_err(|_| "download_failed".into());
        }
    }
}

async fn receive_body(
    manager: &ManagerState,
    mut response: reqwest::Response,
    partial: &Path,
) -> Result<(), String> {
    let mut file = fs::File::create(partial).map_err(|e| e.to_string())?;
    let mut total = 0;
    while let Some(chunk) = download_step(manager, response.chunk()).await? {
        manager.check_cancelled()?;
        total += chunk.len();
        if total > 200 * 1024 * 1024 {
            return Err("invalid_download_size".into());
        }
        file.write_all(&chunk).map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())
}

async fn fetch(manager: &ManagerState, url: &str, partial: &Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(900))
        .build()
        .map_err(|_| "download_failed")?;
    let response = download_step(manager, client.get(url).send())
        .await?
        .error_for_status()
        .map_err(|_| "download_failed")?;
    receive_body(manager, response, partial).await
}

fn activate(partial: &Path, destination: &Path, digest: &str) -> Result<(), String> {
    if hash(partial)? != digest {
        let _ = fs::remove_file(partial);
        return Err("download_checksum".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(partial, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    fs::rename(partial, destination).map_err(|e| e.to_string())
}

pub(super) async fn mise(
    manager: &ManagerState,
    destination: &Path,
    record: &serde_json::Value,
) -> Result<(), String> {
    let digest = record["sha256"].as_str().ok_or("invalid_tool_policy")?;
    if destination.exists() && hash(destination)? == digest {
        return Ok(());
    }
    let partial = destination.with_extension("partial");
    fetch(
        manager,
        record["url"].as_str().ok_or("invalid_tool_policy")?,
        &partial,
    )
    .await?;
    activate(&partial, destination, digest)
}
