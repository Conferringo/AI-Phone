//! Streaming image encoder; the existing XZ envelope and 128 MiB reader limit remain.
use std::{
    io::{self, Read, Write},
    path::Path,
};
use xz2::{
    stream::{Check, Filters, LzmaOptions, Stream},
    write::XzEncoder,
};

const RESERVE: u64 = 4 * 1024 * 1024 * 1024;
const SPACE_INTERVAL: usize = 32 * 1024 * 1024;

struct ReservedWriter<'a, W> {
    output: W,
    folder: &'a Path,
    since_check: usize,
}
impl<W: Write> Write for ReservedWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.since_check + bytes.len() >= SPACE_INTERVAL {
            if fs2::available_space(self.folder)? < RESERVE {
                return Err(io::Error::other("Keep 4 GiB free while exporting images"));
            }
            self.since_check = 0;
        }
        let count = self.output.write(bytes)?;
        self.since_check += count;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

pub fn encode(
    input: &mut impl Read,
    output: impl Write,
    architecture: &str,
    preset: u32,
    folder: &Path,
) -> crate::Result<()> {
    let mut filters = Filters::new();
    match architecture {
        "amd64" => {
            filters.x86();
        }
        "arm64" => {}
        _ => return Err("Unknown image architecture".into()),
    }
    filters.lzma2(&LzmaOptions::new_preset(preset)?);
    let stream = Stream::new_stream_encoder(&filters, Check::Crc64)?;
    let output = ReservedWriter {
        output,
        folder,
        since_check: 0,
    };
    let mut encoder = XzEncoder::new_stream(output, stream);
    io::copy(input, &mut encoder)?;
    encoder.finish()?.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_output_directory_cannot_bypass_the_existing_reserve_check() {
        let temp = tempfile::tempdir().unwrap();
        let mut writer = ReservedWriter {
            output: Vec::new(),
            folder: &temp.path().join("missing"),
            since_check: SPACE_INTERVAL - 1,
        };
        assert_eq!(
            writer.write(b"x").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(writer.output.is_empty());
    }
}
