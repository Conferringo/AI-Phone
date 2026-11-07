use super::{Result, now};
use axum::body::{Body, Bytes};
use futures_util::stream;
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use tokio::sync::{Notify, watch};

#[derive(Clone)]
pub enum Frame {
    Waiting,
    Image(Bytes),
    End,
}
pub struct Hub {
    pub frames: watch::Sender<Frame>,
    pub viewers: AtomicUsize,
    pub changed: Notify,
    pub sequence: AtomicU64,
    pub verified: AtomicU64,
}
impl Hub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            frames: watch::channel(Frame::Waiting).0,
            viewers: AtomicUsize::new(0),
            changed: Notify::new(),
            sequence: AtomicU64::new(0),
            verified: AtomicU64::new(0),
        })
    }
    pub fn publish(&self, jpeg: Vec<u8>, captured: u64) {
        let verified = now();
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let mut packet = format!("--phone\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nX-Capture-Time: {captured}\r\nX-Verified-Time: {verified}\r\nX-Frame-Sequence: {sequence}\r\n\r\n",jpeg.len()).into_bytes();
        packet.extend(jpeg);
        packet.extend(b"\r\n");
        self.verified.store(verified, Ordering::Relaxed);
        self.frames.send_replace(Frame::Image(packet.into()));
    }
    pub fn end(&self) {
        self.frames.send_replace(Frame::End);
    }
    pub fn body(self: &Arc<Self>) -> Body {
        struct Viewer(Arc<Hub>);
        impl Drop for Viewer {
            fn drop(&mut self) {
                if self.0.viewers.fetch_sub(1, Ordering::Relaxed) == 1 {
                    self.0.frames.send_replace(Frame::Waiting);
                    self.0.changed.notify_one();
                }
            }
        }
        if self.viewers.fetch_add(1, Ordering::Relaxed) == 0 {
            self.frames.send_replace(Frame::Waiting);
        }
        self.changed.notify_one();
        let receiver = self.frames.subscribe();
        Body::from_stream(stream::unfold(
            (receiver, Viewer(self.clone()), true),
            |(mut receiver, viewer, first)| async move {
                if !first {
                    receiver.changed().await.ok()?;
                }
                loop {
                    let frame = receiver.borrow_and_update().clone();
                    match frame {
                        Frame::Image(bytes) => {
                            return Some((
                                Ok::<_, std::io::Error>(bytes),
                                (receiver, viewer, false),
                            ));
                        }
                        Frame::End => return None,
                        Frame::Waiting => receiver.changed().await.ok()?,
                    }
                }
            },
        ))
    }
    pub async fn demand(&self) {
        while self.viewers.load(Ordering::Relaxed) == 0 {
            self.changed.notified().await;
        }
    }
}
pub fn jpeg(frame: &super::grpc::wire::Image, angle: u32) -> Result<Vec<u8>> {
    use image::{ImageBuffer, Rgb, codecs::jpeg::JpegEncoder};
    let format = frame.format.as_ref().ok_or("image format missing")?;
    let raw =
        ImageBuffer::<Rgb<u8>, _>::from_raw(format.width, format.height, frame.image.as_slice())
            .ok_or("invalid image dimensions")?;
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, 80);
    match angle % 4 {
        0 => encoder.encode_image(&raw)?,
        1 => encoder.encode_image(&image::imageops::rotate270(&raw))?,
        2 => encoder.encode_image(&image::imageops::rotate180(&raw))?,
        3 => encoder.encode_image(&image::imageops::rotate90(&raw))?,
        _ => unreachable!(),
    }
    Ok(bytes)
}
