use super::{Cancelled, Prepared, inventory, manifest::Image};
use crate::{Error, Result};
use serde_json::Value;
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    sync::watch,
};

async fn stop(child: &mut Child) -> io::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    if let Some(pid) = child
        .id()
        .and_then(|pid| rustix::process::Pid::from_raw(pid as i32))
    {
        rustix::process::kill_process(pid, rustix::process::Signal::TERM)?;
    }
    match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
        Ok(result) => {
            result?;
            Ok(())
        }
        Err(_) => child.kill().await,
    }
}
async fn cleanup(child: &mut Child, outcome: Result<()>) -> Result<()> {
    match outcome {
        Ok(()) => Ok(()),
        Err(primary) => match stop(child).await {
            Ok(()) => Err(primary),
            Err(cleanup) => Err(Error::Cleanup {
                primary: Box::new(primary),
                cleanup,
            }),
        },
    }
}
async fn query(arguments: &[&str], mut cancel: watch::Receiver<bool>) -> Result<Option<Vec<u8>>> {
    if *cancel.borrow() {
        return Err(Error::Cancelled);
    }
    let mut child = Command::new("docker")
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut bytes = Vec::new();
    let mut present = false;
    let outcome = {
        let complete = async {
            stdout.read_to_end(&mut bytes).await?;
            present = child.wait().await?.success();
            Ok(())
        };
        tokio::pin!(complete);
        tokio::select! {result=&mut complete=>result,_=cancel.wait_for(|value|*value)=>Err(Error::Cancelled)}
    };
    drop(stdout);
    cleanup(&mut child, outcome).await?;
    Ok(present.then_some(bytes))
}
pub(super) async fn architecture(cancel: watch::Receiver<bool>) -> Result<String> {
    let bytes = query(&["info", "--format", "{{.Architecture}}"], cancel)
        .await?
        .ok_or(Error::Invalid("image_engine_unavailable"))?;
    let value = match std::str::from_utf8(&bytes).map(str::trim) {
        Ok("arm64" | "aarch64") => "arm64",
        Ok("amd64" | "x86_64") => "amd64",
        _ => return Err(Error::Invalid("image_engine_architecture")),
    };
    Ok(value.into())
}
pub(super) async fn present(
    image: &Image,
    architecture: &str,
    cancel: watch::Receiver<bool>,
) -> Result<bool> {
    let Some(bytes) = query(&["image", "inspect", &image.reference], cancel.clone()).await? else {
        return Ok(false);
    };
    let mut value: Value =
        serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("image_engine_response"))?;
    let mut row = one(&value)?;
    if row
        .get("Descriptor")
        .and_then(|value| value.get("mediaType"))
        .and_then(Value::as_str)
        == Some("application/vnd.oci.image.index.v1+json")
    {
        let bytes = query(
            &[
                "image",
                "inspect",
                &image.reference,
                "--platform",
                &format!("linux/{architecture}"),
            ],
            cancel,
        )
        .await?
        .ok_or(Error::Invalid("image_platform_unconfirmed"))?;
        value =
            serde_json::from_slice(&bytes).map_err(|_| Error::Invalid("image_engine_response"))?;
        row = one(&value)?;
    }
    if !row
        .get("Id")
        .and_then(Value::as_str)
        .is_some_and(|id| id == image.export_digest || id == image.config_digest)
        || row.get("Architecture").and_then(Value::as_str) != Some(architecture)
        || row.get("Os").and_then(Value::as_str) != Some("linux")
    {
        return Err(Error::Invalid("image_reference_conflict"));
    }
    Ok(true)
}
fn one(value: &Value) -> Result<&Value> {
    value
        .as_array()
        .filter(|values| values.len() == 1)
        .map(|values| &values[0])
        .ok_or(Error::Invalid("image_engine_response"))
}

async fn wait(child: &mut Child, mut cancel: watch::Receiver<bool>) -> Result<()> {
    tokio::select! {
        result=child.wait()=>{if result?.success(){Ok(())}else{Err(Error::Invalid("image_import_unknown"))}},
        _=cancel.wait_for(|value|*value)=>Err(Error::Cancelled)
    }
}
pub(super) async fn load(prepared: &mut Prepared, cancel: watch::Receiver<bool>) -> Result<()> {
    let log = tempfile::tempfile()?;
    let mut child = Command::new("docker")
        .args(["image", "load", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(log.try_clone()?)
        .stderr(log)
        .kill_on_drop(true)
        .spawn()?;
    let setup = (|| -> Result<_> {
        let input = child.stdin.take().expect("piped stdin").into_owned_fd()?;
        let pipe = File::from(input);
        let mut file = prepared.file.try_clone()?;
        file.seek(SeekFrom::Start(prepared.offset))?;
        Ok((pipe, file))
    })();
    let (mut pipe, file) = match setup {
        Ok(value) => value,
        Err(error) => return cleanup(&mut child, Err(error)).await,
    };
    let xz = prepared.manifest.xz();
    let layout = prepared.layout.take();
    let size = prepared.manifest.archive.size;
    let write_cancel = cancel.clone();
    let mut writer = tokio::task::spawn_blocking(move || -> Result<()> {
        let input = file.take(size);
        if let Some(layout) = layout {
            let decoder = inventory::decoder(input, xz)?;
            layout.write(
                inventory::Bounded::new(Cancelled {
                    input: decoder,
                    cancel: write_cancel,
                }),
                &mut pipe,
            )?;
        } else if xz {
            let decoder = inventory::decoder(input, true)?;
            io::copy(
                &mut inventory::Bounded::new(Cancelled {
                    input: decoder,
                    cancel: write_cancel,
                }),
                &mut pipe,
            )?;
        } else {
            io::copy(
                &mut Cancelled {
                    input,
                    cancel: write_cancel,
                },
                &mut pipe,
            )?;
        }
        Ok(())
    });
    let mut cancelled = cancel.clone();
    let written =
        tokio::select! {result=&mut writer=>Some(result),_=cancelled.wait_for(|value|*value)=>None};
    let joined = written.is_some();
    let outcome = match written {
        Some(Ok(Ok(()))) => wait(&mut child, cancel).await,
        Some(Ok(Err(error))) => Err(error),
        Some(Err(_)) => Err(Error::Invalid("image_worker")),
        None => Err(Error::Cancelled),
    };
    let outcome = cleanup(&mut child, outcome).await;
    if !joined {
        // Cancellation is already the primary outcome; joining completes pipe/file ownership.
        let _ = writer.await;
    }
    outcome
}
