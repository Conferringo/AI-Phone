//! One source-publication command for standalone and bundled native installers.
use crate::{Error, Installation, Result};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub const CONTRACT: &str = "ai-phone-source-staging-v1";

fn arguments(values: &[OsString]) -> Result<(PathBuf, PathBuf, String)> {
    let [payload, home, identity] = values else {
        return Err(Error::Invalid("source_staging_arguments"));
    };
    let identity = identity
        .to_str()
        .ok_or(Error::Invalid("source_resource_identity"))?;
    if identity.len() != 64
        || !identity
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Invalid("source_resource_identity"));
    }
    Ok((
        Path::new(payload).into(),
        Path::new(home).into(),
        identity.into(),
    ))
}

pub async fn run(values: &[OsString]) -> Result<()> {
    let (payload, home, identity) = arguments(values)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let mut worker = tokio::task::spawn_blocking(move || {
        Installation::lock(&home)?.stage_source(
            &payload,
            &identity,
            &|path| fs2::available_space(path).unwrap_or(0),
            &|| stop.load(Ordering::Relaxed),
        )
    });
    let result = tokio::select! {
        result = &mut worker => result,
        signal = crate::component::interrupt() => {
            cancel.store(true, Ordering::Relaxed);
            // Returning or killing the worker early could leave a writer alive.
            let result = worker.await;
            signal?;
            result
        }
    };
    let source = result.map_err(|_| Error::Invalid("source_staging_worker"))??;
    println!("{} {}", source.identity, source.directory.display());
    Ok(())
}
