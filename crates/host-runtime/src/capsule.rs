use crate::{
    Error, Result,
    control::{CHUNK, Control},
    manifest::identity,
};
use flate2::bufread::GzDecoder;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom},
};
use xz2::{bufread::XzDecoder, stream::Stream};

const XZ_MEMORY: u64 = 128 * 1024 * 1024;

#[cfg(test)]
thread_local! {
    pub(crate) static HASHED_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

struct Compressed<'a> {
    file: &'a mut File,
    digest: Option<&'a mut Sha256>,
}

impl Read for Compressed<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let length = self.file.read(buffer)?;
        if let Some(digest) = self.digest.as_deref_mut() {
            hash_chunk(digest, &buffer[..length]);
        }
        Ok(length)
    }
}

enum Decoder<'a> {
    Gzip(GzDecoder<BufReader<Compressed<'a>>>),
    Xz(XzDecoder<BufReader<Compressed<'a>>>),
}

pub(crate) struct Reader<'a, 'b> {
    decoder: Decoder<'a>,
    control: &'b Control<'b>,
    decoded: u64,
    maximum: u64,
}

impl<'a, 'b> Reader<'a, 'b> {
    pub fn with_limit(
        file: &'a mut File,
        control: &'b Control<'b>,
        maximum: u64,
        digest: Option<&'a mut Sha256>,
    ) -> Result<Self> {
        file.rewind()?;
        let mut magic = [0; 6];
        file.read_exact(&mut magic)?;
        file.rewind()?;
        let input = BufReader::with_capacity(CHUNK, Compressed { file, digest });
        let decoder = match magic {
            [0x1f, 0x8b, ..] => Decoder::Gzip(GzDecoder::new(input)),
            [0xfd, b'7', b'z', b'X', b'Z', 0] => {
                let stream = Stream::new_stream_decoder(XZ_MEMORY, 0)
                    .map_err(|_| Error::Invalid("source_resource_codec"))?;
                Decoder::Xz(XzDecoder::new_stream(input, stream))
            }
            _ => return Err(Error::Invalid("source_resource_codec")),
        };
        Ok(Self {
            decoder,
            control,
            decoded: 0,
            maximum,
        })
    }

    pub fn finish_tar(mut self) -> Result<()> {
        let mut remaining = 0u64;
        let mut buffer = [0; CHUNK];
        loop {
            let length = self.read(&mut buffer)?;
            if length == 0 {
                break;
            }
            remaining += length as u64;
            if buffer[..length].iter().any(|byte| *byte != 0) {
                return Err(Error::Invalid("source_resource_tar_tail"));
            }
        }
        if remaining < 512 {
            return Err(Error::Invalid("source_resource_tar_footer"));
        }
        self.finish_codec()
    }

    fn finish_codec(self) -> Result<()> {
        let mut input = match self.decoder {
            Decoder::Gzip(decoder) => decoder.into_inner(),
            Decoder::Xz(decoder) => decoder.into_inner(),
        };
        if !input.fill_buf()?.is_empty() {
            return Err(Error::Invalid("source_resource_codec_tail"));
        }
        Ok(())
    }
}

impl Read for Reader<'_, '_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.control.check().map_err(io::Error::other)?;
        let limit = buffer.len().min(CHUNK);
        let length = match &mut self.decoder {
            Decoder::Gzip(decoder) => decoder.read(&mut buffer[..limit])?,
            Decoder::Xz(decoder) => decoder.read(&mut buffer[..limit])?,
        };
        self.decoded = self
            .decoded
            .checked_add(length as u64)
            .ok_or_else(|| io::Error::other(Error::Invalid("source_resource_size")))?;
        if self.decoded > self.maximum {
            return Err(io::Error::other(Error::Invalid("source_resource_size")));
        }
        Ok(length)
    }
}

pub(crate) fn check_identity(file: &File, expected: &str, maximum: u64) -> Result<()> {
    if !identity(expected) || file.metadata()?.len() > maximum {
        return Err(Error::Invalid("source_resource_identity"));
    }
    Ok(())
}

pub(crate) fn verify_bounded_identity(
    file: &mut File,
    expected: &str,
    control: &Control<'_>,
    maximum: u64,
) -> Result<()> {
    check_identity(file, expected, maximum)?;
    if hash(file, control)? != expected {
        return Err(Error::Invalid("source_resource_changed"));
    }
    Ok(())
}

pub(crate) fn hash(file: &mut File, control: &Control<'_>) -> Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut buffer = [0; CHUNK];
    loop {
        control.check()?;
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        hash_chunk(&mut hash, &buffer[..length]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn hash_chunk(hash: &mut Sha256, bytes: &[u8]) {
    hash.update(bytes);
    #[cfg(test)]
    HASHED_BYTES.with(|count| count.set(count.get() + bytes.len() as u64));
}
