use super::{Arc, Bytes, Duration, Media, Ordering, Result, now};
use axum::body::Body;
use futures_util::stream;
use image::{ImageBuffer, RgbImage, codecs::jpeg::JpegEncoder};
use std::{io, io::Cursor, path::PathBuf};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::watch,
};

#[allow(unsafe_code)]
mod mapping;
use mapping::Mapping;

const FRAME_HEADER: usize = 16384;
const AUDIO_HEADER: usize = 4096;
const AUDIO_SLOTS: usize = 512;
const AUDIO_SLOT: usize = 8192;

struct EncodedFrame {
    layout: Arc<Display>,
    sequence: u64,
    rotation: u32,
    jpeg: Vec<u8>,
    captured: u64,
}

pub(super) struct Avo {
    path: PathBuf,
    frame: watch::Sender<Option<Arc<Display>>>,
    audio: watch::Sender<Option<Arc<Audio>>>,
}

impl Avo {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            frame: watch::channel(None).0,
            audio: watch::channel(None).0,
        }
    }

    pub(super) async fn connect(&self) -> Result<Control> {
        Ok(Control(BufReader::new(
            UnixStream::connect(&self.path).await?,
        )))
    }

    async fn events(&self) -> Result<()> {
        let mut control = self.connect().await?.0;
        control.get_mut().write_all(b"subscribe\n").await?;
        let mut line = String::new();
        while control.read_line(&mut line).await? != 0 {
            let words: Vec<_> = line.split_whitespace().collect();
            match words.as_slice() {
                ["frames", "0", _, _, _, _, _, name] => {
                    self.frame
                        .send_replace(Some(Arc::new(Display::open(name)?)));
                }
                ["frame", "0", _] => self.frame.send_modify(|_| {}),
                ["frames-off", "0"] => {
                    self.frame.send_replace(None);
                }
                ["audio", "playback", _, name] => {
                    self.audio.send_replace(Some(Arc::new(Audio::open(name)?)));
                }
                _ => {}
            }
            line.clear();
        }
        Ok(())
    }

    pub(super) async fn subscribe(&self) {
        loop {
            if let Err(error) = self.events().await {
                eprintln!("Avo media: {error}");
            }
            self.frame.send_replace(None);
            self.audio.send_replace(None);
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    pub(super) async fn snapshot(&self, media: &Media) -> Result<(Vec<u8>, u64)> {
        let mut frames = self.frame.subscribe();
        let frame = tokio::time::timeout(
            Duration::from_secs(1),
            frames.wait_for(|frame| frame.as_ref().is_some_and(|frame| frame.sequence() != 0)),
        )
        .await??
        .clone()
        .ok_or("Avo screen has no frame")?;
        let rotation = media.orientation().await?;
        tokio::task::spawn_blocking(move || {
            let (image, captured) = frame.read(rotation)?;
            let mut png = Cursor::new(Vec::new());
            image.write_to(&mut png, image::ImageFormat::Png)?;
            Ok((png.into_inner(), captured))
        })
        .await?
    }

    pub(super) async fn capture(&self, media: &Media) {
        let mut frames = self.frame.subscribe();
        loop {
            media.screen.demand().await;
            let mut heartbeat = tokio::time::interval(Duration::from_secs(1));
            heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut last: Option<EncodedFrame> = None;
            while media.screen.viewers.load(Ordering::Relaxed) != 0 {
                let frame = frames.borrow_and_update().clone();
                if let Some(frame) = frame.filter(|frame| frame.sequence() != 0) {
                    let sequence = frame.sequence();
                    let rotation = media.rotation.load(Ordering::Relaxed);
                    if let Some(last) = &last
                        && Arc::ptr_eq(&frame, &last.layout)
                        && sequence == last.sequence
                        && rotation == last.rotation
                    {
                        media.screen.publish(last.jpeg.clone(), last.captured);
                    } else {
                        let source = frame.clone();
                        match tokio::task::spawn_blocking(move || source.jpeg(rotation)).await {
                            Ok(Ok((jpeg, captured))) => {
                                media.screen.publish(jpeg.clone(), captured);
                                last = Some(EncodedFrame {
                                    layout: frame,
                                    sequence,
                                    rotation,
                                    jpeg,
                                    captured,
                                });
                            }
                            Ok(Err(error)) => eprintln!("Avo frame: {error}"),
                            Err(error) => eprintln!("Avo encoder: {error}"),
                        }
                    }
                } else if last.take().is_some() {
                    media.screen.end();
                }
                tokio::select! {
                    _ = frames.changed() => tokio::time::sleep(Duration::from_millis(33)).await,
                    _ = heartbeat.tick() => { let _ = media.orientation().await; }
                    _ = media.screen.changed.notified() => {}
                }
            }
        }
    }

    pub(super) fn audio(&self) -> Body {
        let mut rings = self.audio.subscribe();
        let current = rings.borrow_and_update().clone();
        let sequence = current.as_ref().map_or(0, |ring| ring.map.sequence(32));
        let mut tick = tokio::time::interval(Duration::from_millis(20));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        Body::from_stream(stream::unfold(
            (rings, current, sequence, tick, true),
            |(mut rings, mut current, mut sequence, mut tick, first)| async move {
                let bytes = if first {
                    Bytes::from_static(&[0; 4])
                } else {
                    loop {
                        tick.tick().await;
                        if rings.has_changed().ok()? {
                            current = rings.borrow_and_update().clone();
                            sequence = 0;
                        }
                        if let Some(ring) = &current {
                            let pcm = ring.read(&mut sequence);
                            if !pcm.is_empty() {
                                break Bytes::from(pcm);
                            }
                        }
                    }
                };
                Some((
                    Ok::<_, io::Error>(bytes),
                    (rings, current, sequence, tick, false),
                ))
            },
        ))
    }
}

pub(super) struct Control(BufReader<UnixStream>);

impl Control {
    pub(super) async fn send(&mut self, command: &str) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(2), async {
            self.0.get_mut().write_all(command.as_bytes()).await?;
            self.0.get_mut().write_all(b"\n").await?;
            let mut reply = String::new();
            self.0.read_line(&mut reply).await?;
            if reply.trim_end() != "ok" {
                return Err(format!("Avo command rejected: {}", reply.trim_end()).into());
            }
            Ok(())
        })
        .await?
    }
}

fn open(name: &str, bytes: usize, magic: [u8; 4]) -> Result<Mapping> {
    let name = name
        .strip_prefix("shm:")
        .filter(|name| name.starts_with("/avo."))
        .ok_or("invalid Avo memory name")?;
    let map = Mapping::open(name)?;
    if map.len() < bytes || map.word(0) != u32::from_le_bytes(magic) || map.word(4) != 1 {
        return Err("unexpected Avo shared memory format".into());
    }
    Ok(map)
}

struct Display {
    map: Mapping,
    width: u32,
    height: u32,
    stride: usize,
    format: u32,
    buffer: usize,
}

impl Display {
    fn open(name: &str) -> Result<Self> {
        let map = open(name, FRAME_HEADER, *b"AVOF")?;
        let width = map.word(12);
        let height = map.word(16);
        let stride = map.word(20) as usize;
        let format = map.word(24);
        let buffer = map.word(36) as usize;
        if !(1..=4096).contains(&width)
            || !(1..=4096).contains(&height)
            || stride != width as usize * 4
            || format > 1
            || map.word(28) != 3
            || map.word(32) as usize != FRAME_HEADER
            || buffer < stride * height as usize
            || buffer > (map.len() - FRAME_HEADER) / 3
        {
            return Err("invalid Avo frame layout".into());
        }
        Ok(Self {
            map,
            width,
            height,
            stride,
            format,
            buffer,
        })
    }

    fn sequence(&self) -> u64 {
        self.map.sequence(48)
    }

    fn jpeg(&self, rotation: u32) -> Result<(Vec<u8>, u64)> {
        let (image, captured) = self.read(rotation)?;
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 80).encode_image(&image)?;
        Ok((jpeg, captured))
    }

    fn read(&self, rotation: u32) -> Result<(RgbImage, u64)> {
        let mut raw = vec![0; self.stride * self.height as usize];
        let (sequence, mut captured) = self.copy(&mut raw);
        if self.sequence() > sequence + 1 {
            let (sequence, at) = self.copy(&mut raw);
            if self.sequence() > sequence + 1 {
                return Err("Avo frame changed while copying".into());
            }
            captured = at;
        }
        let order = if self.format == 0 {
            [2, 1, 0]
        } else {
            [0, 1, 2]
        };
        let rgb = raw
            .chunks_exact(4)
            .flat_map(|p| [p[order[0]], p[order[1]], p[order[2]]])
            .collect();
        let image =
            ImageBuffer::from_raw(self.width, self.height, rgb).ok_or("invalid RGB frame")?;
        let portrait = self.width < self.height;
        Ok((
            match rotation {
                1 if portrait => image::imageops::rotate270(&image),
                2 => image::imageops::rotate180(&image),
                3 if portrait => image::imageops::rotate90(&image),
                _ => image,
            },
            captured,
        ))
    }

    fn copy(&self, bytes: &mut [u8]) -> (u64, u64) {
        let sequence = self.sequence();
        let index = (sequence % 3) as usize;
        self.map.copy(FRAME_HEADER + self.buffer * index, bytes);
        (sequence, now())
    }
}

struct Audio {
    map: Mapping,
}

impl Audio {
    fn open(name: &str) -> Result<Self> {
        let map = open(name, AUDIO_HEADER + AUDIO_SLOTS * AUDIO_SLOT, *b"AVOA")?;
        if map.word(8) as usize != AUDIO_SLOTS || map.word(12) as usize != AUDIO_SLOT {
            return Err("invalid Avo PCM layout".into());
        }
        Ok(Self { map })
    }

    fn read(&self, sequence: &mut u64) -> Vec<u8> {
        let latest = self.map.sequence(32);
        let since = (*sequence).max(latest.saturating_sub(5));
        *sequence = latest;
        let mut pcm = Vec::new();
        for seq in (since + 1)..=latest {
            let offset = AUDIO_HEADER + seq as usize % AUDIO_SLOTS * AUDIO_SLOT;
            if self.map.sequence(offset) != seq
                || self.map.word(offset + 16) != 48000
                || self.map.word(offset + 20) != 0x1_0002
            {
                continue;
            }
            let len = (self.map.word(offset + 24) as usize) * 4;
            if len > AUDIO_SLOT - 32 {
                continue;
            }
            let start = pcm.len();
            pcm.resize(start + len, 0);
            self.map.copy(offset + 32, &mut pcm[start..]);
            if self.map.sequence(offset) != seq {
                pcm.truncate(start);
            }
        }
        pcm
    }
}
