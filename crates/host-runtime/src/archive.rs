use crate::{
    Error, Result,
    capsule::Reader,
    control::{CHUNK, Control},
    manifest::{self, Inventory, MAX_FILE, MAX_FILES, MAX_MANIFEST, MAX_TOTAL},
};
use sha2::Sha256;
use std::{collections::BTreeMap, fs::File, io::Read};

const MAX_EXTENSION: u64 = 64 * 1024;
const MAX_EXTENSIONS: u64 = 16 * 1024 * 1024;

#[derive(Default)]
struct Extra {
    path: Option<String>,
    size: Option<u64>,
}

#[derive(Default)]
struct Extensions {
    global: Extra,
    next: Extra,
    bytes: u64,
    headers: usize,
}

fn limited(entry: &mut dyn Read, size: u64, maximum: u64) -> Result<Vec<u8>> {
    if size > maximum {
        return Err(Error::Invalid("source_resource_metadata_size"));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    entry.take(size + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size {
        return Err(Error::Invalid("source_resource_truncated"));
    }
    Ok(bytes)
}

fn apply_pax(extra: &mut Extra, bytes: &[u8]) -> Result<()> {
    for item in tar::PaxExtensions::new(bytes) {
        let item = item?;
        let key = item
            .key()
            .map_err(|_| Error::Invalid("source_resource_pax"))?;
        let value = item
            .value()
            .map_err(|_| Error::Invalid("source_resource_pax"))?;
        apply_pax_field(extra, key, value)?;
    }
    Ok(())
}

fn apply_pax_field(extra: &mut Extra, key: &str, value: &str) -> Result<()> {
    match key {
        "path" => extra.path = Some(value.to_owned()),
        "size" => {
            extra.size = Some(
                value
                    .parse()
                    .map_err(|_| Error::Invalid("source_resource_pax_size"))?,
            )
        }
        key if key.starts_with("GNU.sparse") => {
            return Err(Error::Invalid("source_resource_entry_type"));
        }
        _ => {}
    }
    Ok(())
}

impl Extensions {
    fn metadata<R: Read>(&mut self, entry: &mut tar::Entry<'_, R>) -> Result<bool> {
        self.headers += 1;
        if self.headers > MAX_FILES * 2 + 16 {
            return Err(Error::Invalid("source_resource_entry_count"));
        }
        let kind = entry.header().entry_type().as_byte();
        if !matches!(kind, b'x' | b'g' | b'L') {
            return Ok(false);
        }
        let size = entry.size();
        let bytes = limited(entry, size, MAX_EXTENSION)?;
        self.bytes += bytes.len() as u64;
        if self.bytes > MAX_EXTENSIONS {
            return Err(Error::Invalid("source_resource_metadata_size"));
        }
        self.apply(kind, &bytes)?;
        Ok(true)
    }

    fn apply(&mut self, kind: u8, bytes: &[u8]) -> Result<()> {
        match kind {
            b'x' => apply_pax(&mut self.next, bytes),
            b'g' => apply_pax(&mut self.global, bytes),
            b'L' => {
                let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
                self.next.path = Some(
                    std::str::from_utf8(bytes)
                        .map_err(|_| Error::Invalid("source_resource_path"))?
                        .to_owned(),
                );
                Ok(())
            }
            _ => Err(Error::Invalid("source_resource_entry_type")),
        }
    }

    fn name(&mut self, header: &tar::Header) -> Result<String> {
        if !matches!(header.entry_type().as_byte(), 0 | b'0' | b'7') {
            return Err(Error::Invalid("source_resource_entry_type"));
        }
        let next = std::mem::take(&mut self.next);
        if next
            .size
            .or(self.global.size)
            .is_some_and(|size| header.size().ok() != Some(size))
        {
            return Err(Error::Invalid("source_resource_pax_size"));
        }
        let name = match next.path.or_else(|| self.global.path.clone()) {
            Some(name) => name,
            None => header_name(header)?,
        };
        manifest::canonical(&name)?;
        Ok(name)
    }
}

fn header_name(header: &tar::Header) -> Result<String> {
    // Inspect raw path bytes before tar can normalize a backslash into '/'.
    let raw = header.as_bytes();
    if raw[..100].contains(&b'\\') || raw[345..500].contains(&b'\\') {
        return Err(Error::Invalid("source_resource_path"));
    }
    String::from_utf8(header.path_bytes().into_owned())
        .map_err(|_| Error::Invalid("source_resource_path"))
}

fn add_header(
    headers: &mut BTreeMap<String, u64>,
    name: &str,
    size: u64,
    total: &mut u64,
    limits: &Limits,
) -> Result<()> {
    if headers.len() >= MAX_FILES
        || size > limits.file
        || headers.insert(name.to_owned(), size).is_some()
    {
        return Err(Error::Invalid("source_resource_inventory"));
    }
    *total = total
        .checked_add(size)
        .ok_or(Error::Invalid("source_resource_size"))?;
    if *total > limits.total {
        return Err(Error::Invalid("source_resource_size"));
    }
    Ok(())
}

pub(crate) struct Limits {
    pub manifest: &'static str,
    pub file: u64,
    pub total: u64,
    pub metadata: u64,
}

pub(crate) struct RawInventory {
    pub raw: Vec<u8>,
    pub headers: BTreeMap<String, u64>,
}

#[derive(Default)]
struct SourceInventory {
    extensions: Extensions,
    headers: BTreeMap<String, u64>,
    total: u64,
    raw: Option<Vec<u8>>,
}

impl SourceInventory {
    fn scan<R: Read>(
        &mut self,
        archive: &mut tar::Archive<R>,
        control: &Control<'_>,
        limits: &Limits,
        body: &mut impl FnMut(&str, u64, &mut dyn Read) -> Result<()>,
    ) -> Result<()> {
        for entry in archive.entries()?.raw(true) {
            control.check()?;
            self.entry(&mut entry?, body, limits)?;
        }
        if self.extensions.next.path.is_some() || self.extensions.next.size.is_some() {
            return Err(Error::Invalid("source_resource_extension_without_file"));
        }
        Ok(())
    }

    fn entry<R: Read>(
        &mut self,
        entry: &mut tar::Entry<'_, R>,
        body: &mut impl FnMut(&str, u64, &mut dyn Read) -> Result<()>,
        limits: &Limits,
    ) -> Result<()> {
        if self.extensions.metadata(entry)? {
            return Ok(());
        }
        let name = self.extensions.name(entry.header())?;
        let size = entry.size();
        add_header(&mut self.headers, &name, size, &mut self.total, limits)?;
        if name == limits.manifest {
            self.raw = Some(limited(entry, size, limits.metadata)?);
        } else {
            body(&name, size, entry)?;
        }
        Ok(())
    }

    fn finish(self) -> Result<RawInventory> {
        let raw = self
            .raw
            .ok_or(Error::Invalid("source_resource_manifest_missing"))?;
        Ok(RawInventory {
            raw,
            headers: self.headers,
        })
    }
}

pub(crate) fn visit(
    file: &mut File,
    control: &Control<'_>,
    digest: Option<&mut Sha256>,
    body: impl FnMut(&str, u64, &mut dyn Read) -> Result<()>,
) -> Result<Inventory> {
    let limits = Limits {
        manifest: manifest::HOST_MANIFEST,
        file: MAX_FILE,
        total: MAX_TOTAL,
        metadata: MAX_MANIFEST,
    };
    let raw = visit_bounded(file, control, &limits, digest, body)?;
    let manifest =
        serde_json::from_slice(&raw.raw).map_err(|_| Error::Invalid("source_resource_manifest"))?;
    let inventory = Inventory {
        manifest,
        raw: raw.raw,
        headers: raw.headers,
    };
    manifest::validate(&inventory)?;
    Ok(inventory)
}

pub(crate) fn visit_component(
    file: &mut File,
    control: &Control<'_>,
    limits: &Limits,
    body: impl FnMut(&str, u64, &mut dyn Read) -> Result<()>,
) -> Result<RawInventory> {
    visit_bounded(file, control, limits, None, body)
}

fn visit_bounded(
    file: &mut File,
    control: &Control<'_>,
    limits: &Limits,
    digest: Option<&mut Sha256>,
    mut body: impl FnMut(&str, u64, &mut dyn Read) -> Result<()>,
) -> Result<RawInventory> {
    let mut archive = tar::Archive::new(Reader::with_limit(
        file,
        control,
        limits.total + 64 * 1024 * 1024,
        digest,
    )?);
    let mut inventory = SourceInventory::default();
    inventory.scan(&mut archive, control, limits, &mut body)?;
    archive.into_inner().finish_tar()?;
    inventory.finish()
}

pub(crate) fn first_manifest(
    file: &mut File,
    control: &Control<'_>,
    limits: &Limits,
) -> Result<Vec<u8>> {
    let mut archive = tar::Archive::new(Reader::with_limit(
        file,
        control,
        limits.metadata + 512,
        None,
    )?);
    let mut entry = archive
        .entries()?
        .raw(true)
        .next()
        .ok_or(Error::Invalid("source_resource_manifest_missing"))??;
    if Extensions::default().name(entry.header())? != limits.manifest {
        return Err(Error::Invalid("source_resource_manifest_missing"));
    }
    let size = entry.size();
    limited(&mut entry, size, limits.metadata)
}

pub(crate) fn drain(source: &mut dyn Read) -> Result<()> {
    let mut bytes = [0; CHUNK];
    while source.read(&mut bytes)? != 0 {}
    Ok(())
}
