//! Offline image admission and import share one open capsule; no image-sized staging copy.
mod engine;
mod inventory;
mod manifest;
mod normalize;
use crate::{Error, Result, filesystem};
use manifest::{MAX_ARCHIVE, MAX_METADATA, Manifest};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{File, Metadata},
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};
use tokio::sync::watch;

pub(crate) const CONTRACT: &str = "ai-phone-image-activation-v1";
pub(crate) fn environment(root: &Path) -> Result<BTreeMap<String, String>> {
    let bytes = match std::fs::read(root.join("data/deployment/image-runtime.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.into()),
    };
    Ok(Manifest::parse(&bytes)?.environment())
}
pub(super) struct Prepared {
    file: File,
    manifest: Manifest,
    offset: u64,
    before: Metadata,
    layout: Option<normalize::Layout>,
}

struct Hashed<R> {
    input: R,
    hash: Sha256,
}
impl<R: Read> Read for Hashed<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.input.read(bytes)?;
        self.hash.update(&bytes[..count]);
        Ok(count)
    }
}
pub(super) struct Cancelled<R> {
    input: R,
    cancel: watch::Receiver<bool>,
}
impl<R: Read> Read for Cancelled<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if *self.cancel.borrow() {
            return Err(io::Error::other(Error::Cancelled));
        }
        let limit = bytes.len().min(65536);
        self.input.read(&mut bytes[..limit])
    }
}
fn prepare(path: &Path, cancel: watch::Receiver<bool>) -> Result<Prepared> {
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    let mut entries = BTreeMap::new();
    let mut metadata = None;
    {
        let mut archive = tar::Archive::new(&mut file);
        for entry in archive.entries_with_seek()? {
            let mut entry = entry?;
            let name = String::from_utf8(entry.path_bytes().into_owned())
                .map_err(|_| Error::Invalid("image_capsule_member"))?;
            let limit = match name.as_str() {
                "IMAGES-MANIFEST.json" => MAX_METADATA,
                "images.tar.gz" | "images.tar.xz" => MAX_ARCHIVE,
                _ => return Err(Error::Invalid("image_capsule_member")),
            };
            if !entry.header().entry_type().is_file()
                || !(1..=limit).contains(&entry.size())
                || entries
                    .insert(name.clone(), (entry.raw_file_position(), entry.size()))
                    .is_some()
            {
                return Err(Error::Invalid("image_capsule_member"));
            }
            if name == "IMAGES-MANIFEST.json" {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                metadata = Some(bytes);
            }
        }
    }
    let mut manifest = Manifest::parse(&metadata.ok_or(Error::Invalid("image_capsule_manifest"))?)?;
    let (offset, size) = *entries
        .get(&manifest.archive.path)
        .ok_or(Error::Invalid("image_capsule_archive"))?;
    if entries.len() != 2 || size != manifest.archive.size {
        return Err(Error::Invalid("image_capsule_archive"));
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut compressed = Hashed {
        input: (&mut file).take(size),
        hash: Sha256::new(),
    };
    let layout = {
        let decoder = inventory::decoder(&mut compressed, manifest.xz())?;
        inventory::inspect(
            inventory::Bounded::new(Cancelled {
                input: decoder,
                cancel: cancel.clone(),
            }),
            &mut manifest,
        )?
    };
    io::copy(
        &mut Cancelled {
            input: &mut compressed,
            cancel,
        },
        &mut io::sink(),
    )?;
    if format!("{:x}", compressed.hash.finalize()) != manifest.archive.sha256 {
        return Err(Error::Invalid("image_archive_digest"));
    }
    Ok(Prepared {
        file,
        manifest,
        offset,
        before,
        layout,
    })
}

async fn activate(
    root: &Path,
    mut prepared: Prepared,
    cancel: watch::Receiver<bool>,
) -> Result<()> {
    let architecture = engine::architecture(cancel.clone()).await?;
    if architecture != prepared.manifest.architecture {
        return Err(Error::Invalid("image_engine_architecture"));
    }
    let mut ready = true;
    for image in prepared.manifest.images.values() {
        ready &= engine::present(image, &architecture, cancel.clone()).await?;
    }
    if !ready {
        if fs2::available_space(root)? < 4 * 1024 * 1024 * 1024 {
            return Err(Error::LowSpace);
        }
        let current = prepared.file.metadata()?;
        if (current.len(), current.modified()?)
            != (prepared.before.len(), prepared.before.modified()?)
        {
            return Err(Error::Invalid("image_capsule_changed"));
        }
        engine::load(&mut prepared, cancel.clone()).await?;
    }
    for image in prepared.manifest.images.values() {
        if !engine::present(image, &architecture, cancel.clone()).await? {
            return Err(Error::Invalid("image_import_unconfirmed"));
        }
    }
    let directory = root.join("data/deployment");
    filesystem::directory(&directory, true)?;
    filesystem::publish_private(
        &directory,
        "image-runtime.json",
        &serde_json::to_vec(&prepared.manifest).map_err(|_| Error::Invalid("image_manifest"))?,
        false,
    )?;
    println!(
        "{}",
        serde_json::to_string(&prepared.manifest.environment())
            .map_err(|_| Error::Invalid("image_manifest"))?
    );
    Ok(())
}

pub(crate) async fn run(arguments: &[OsString]) -> Result<()> {
    let [root, payload] = arguments else {
        return Err(Error::Invalid("image_activation_arguments"));
    };
    let root = Path::new(root);
    let payload = Path::new(payload).to_owned();
    let (signal, cancel) = watch::channel(false);
    let input_cancel = cancel.clone();
    let interrupt = crate::component::interrupt();
    tokio::pin!(interrupt);
    let mut worker = tokio::task::spawn_blocking(move || prepare(&payload, input_cancel));
    let prepared = tokio::select! {
        result=&mut worker=>result.map_err(|_|Error::Invalid("image_worker"))??,
        result=&mut interrupt=>{signal.send_replace(true);let _=worker.await;result?;return Err(Error::Cancelled);}
    };
    let operation = activate(root, prepared, cancel);
    tokio::pin!(operation);
    tokio::select! {
        result=&mut operation=>result,
        result=&mut interrupt=>{signal.send_replace(true);let outcome=operation.await;result?;outcome}
    }
}
