use super::{
    Arc, Body, Bytes, Duration, Media, Ordering, Result,
    grpc::{
        Session,
        wire::{AudioFormat, Image, ImageFormat},
    },
    hub,
};
use futures_util::stream;
use tokio::{
    io::AsyncReadExt,
    sync::watch,
    time::{self, Instant},
};
fn format(kind: i32) -> ImageFormat {
    ImageFormat {
        format: kind,
        width: if kind == 2 { 540 } else { 0 },
        height: if kind == 2 { 1080 } else { 0 },
        ..Default::default()
    }
}
fn angle(media: &Media, image: &Image) -> u32 {
    let delivered = image
        .format
        .as_ref()
        .and_then(|f| f.rotation.as_ref())
        .map_or(0, |v| v.rotation as u32);
    (media.rotation.load(Ordering::Relaxed) + 4 - delivered) % 4
}
pub async fn snapshot(media: &Media) -> Result<(Vec<u8>, u64)> {
    #[cfg(target_os = "macos")]
    if let Some(avo) = &media.avo {
        return avo.snapshot(media).await;
    }
    if media.adb_media {
        let bytes = adb_png(media).await?;
        return Ok((bytes, super::now()));
    }
    media.orientation().await?;
    let mut session = media.session()?;
    let image = session
        .client
        .get_screenshot(session.command(format(0)))
        .await?
        .into_inner();
    let at = image.timestamp_us / 1000;
    let rotation = angle(media, &image);
    if rotation == 0 {
        return Ok((image.image, at));
    }
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let input = image::load_from_memory_with_format(&image.image, image::ImageFormat::Png)?;
        let rotated = match rotation {
            1 => input.rotate270(),
            2 => input.rotate180(),
            3 => input.rotate90(),
            _ => unreachable!(),
        };
        let mut output = std::io::Cursor::new(Vec::new());
        rotated.write_to(&mut output, image::ImageFormat::Png)?;
        Ok(output.into_inner())
    })
    .await??;
    Ok((bytes, at))
}
pub async fn audio(media: &Media) -> Result<Body> {
    #[cfg(target_os = "macos")]
    if let Some(avo) = &media.avo {
        return Ok(avo.audio());
    }
    if media.adb_media {
        return adb_audio(&media.adb).await;
    }
    let session = media.session()?;
    Ok(Body::from_stream(stream::try_unfold(
        (session, None, true),
        |(mut session, packets, first): (
            Session,
            Option<tonic::Streaming<super::grpc::wire::AudioPacket>>,
            bool,
        )| async move {
            if first {
                return Ok::<_, std::io::Error>(Some((
                    Bytes::from_static(&[0; 4]),
                    (session, packets, false),
                )));
            }
            let mut packets = match packets {
                Some(packets) => packets,
                None => session
                    .client
                    .stream_audio(session.request(AudioFormat {
                        sampling_rate: 48000,
                        channels: 1,
                        format: 1,
                        mode: 1,
                    }))
                    .await
                    .map_err(std::io::Error::other)?
                    .into_inner(),
            };
            Ok(packets
                .message()
                .await
                .map_err(std::io::Error::other)?
                .map(|packet| (Bytes::from(packet.audio), (session, Some(packets), false))))
        },
    )))
}

async fn adb_audio(adb: &ai_phone_adb::Client) -> Result<Body> {
    let scid = uuid::Uuid::new_v4().as_fields().0 & 0x7fff_ffff;
    let service = format!("localabstract:scrcpy_{scid:08x}");
    let command = format!(
        "shell,v2,raw:exec su 2000 env CLASSPATH=/data/local/tmp/aimanity-audio.jar \
         app_process / com.genymobile.scrcpy.Server 3.3.4 scid={scid:08x} \
         audio=true audio_source=output audio_codec=raw video=false control=false \
         tunnel_forward=true send_dummy_byte=false send_device_meta=false \
         send_frame_meta=false cleanup=false >/dev/null 2>&1"
    );
    let mut owner = adb.open_stream(&command, Duration::from_secs(2)).await?;
    let mut socket = time::timeout(Duration::from_secs(4), async {
        loop {
            match adb.open_stream(&service, Duration::from_secs(1)).await {
                Ok(socket) => break Ok(socket),
                Err(ai_phone_adb::Error::Rejected(message)) if message == "closed" => {}
                Err(error) => break Err(error),
            }
            let mut closed = [0];
            tokio::select! {
                _ = owner.read_exact(&mut closed) => break Err(ai_phone_adb::Error::Protocol),
                _ = time::sleep(Duration::from_millis(20)) => {}
            }
        }
    })
    .await??;
    let mut codec = [0; 4];
    time::timeout(Duration::from_secs(2), socket.read_exact(&mut codec)).await??;
    if &codec != b"\0raw" {
        return Err("Android playback capture unavailable".into());
    }
    Ok(Body::from_stream(stream::try_unfold(
        (socket, owner),
        |(mut socket, owner)| async move {
            let mut buffer = vec![0; 4096];
            let length = socket.read(&mut buffer).await?;
            if length == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            buffer.truncate(length);
            Ok(Some((Bytes::from(buffer), (socket, owner))))
        },
    )))
}
async fn receive(
    media: &Media,
    mut session: Session,
    raw: &watch::Sender<Option<Arc<Image>>>,
) -> Result<()> {
    media.orientation().await?;
    let mut pictures = session
        .client
        .stream_screenshot(session.request(format(2)))
        .await?
        .into_inner();
    let mut last: Option<Arc<Image>> = None;
    let mut last_at = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    while media.screen.viewers.load(Ordering::Relaxed) > 0 {
        tokio::select! {
            image=pictures.message()=>{
                let image=Arc::new(image?.ok_or("emulator screen ended")?);
                last_at=Instant::now();last=Some(image.clone());raw.send_replace(Some(image));
            }
            _=media.screen.changed.notified()=>{}
            _=tick.tick()=>{
                media.orientation().await?;
                if last_at.elapsed()>=Duration::from_secs(1){
                    let image=Arc::new(session.client.get_screenshot(session.command(format(2))).await?.into_inner());
                    let changed=last.as_ref().is_some_and(|previous|previous.image!=image.image||previous.format!=image.format);
                    raw.send_replace(Some(image));
                    if changed{return Ok(());}
                }
            }
        }
    }
    Ok(())
}
async fn grpc_capture(media: Arc<Media>) {
    let (raw, mut pictures) = watch::channel::<Option<Arc<Image>>>(None);
    let encoder_media = media.clone();
    tokio::spawn(async move {
        let mut next = Instant::now();
        while pictures.changed().await.is_ok() {
            tokio::time::sleep_until(next).await;
            let image = pictures.borrow_and_update().clone();
            if encoder_media.screen.viewers.load(Ordering::Relaxed) == 0 {
                continue;
            }
            if let Some(image) = image {
                let rotation = angle(&encoder_media, &image);
                let at = image.timestamp_us / 1000;
                match tokio::task::spawn_blocking(move || hub::jpeg(&image, rotation)).await {
                    Ok(Ok(jpeg)) => encoder_media.screen.publish(jpeg, at),
                    error => {
                        eprintln!("phone frame: {error:?}");
                        encoder_media.screen.end();
                    }
                }
                next = Instant::now() + Duration::from_millis(33);
            }
        }
    });
    loop {
        media.screen.demand().await;
        let result = async { receive(&media, media.session()?, &raw).await }.await;
        if let Err(error) = result {
            eprintln!("phone screen: {error}");
            media.screen.end();
            media.screen.changed.notified().await;
        }
    }
}

async fn adb_png(media: &Media) -> Result<Vec<u8>> {
    Ok(media
        .adb
        .raw("exec:screencap -p", 8 * 1024 * 1024, Duration::from_secs(2))
        .await?)
}

async fn adb_capture(media: &Media) {
    loop {
        media.screen.demand().await;
        let mut frame = tokio::time::interval(Duration::from_millis(100));
        frame.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        while media.screen.viewers.load(Ordering::Relaxed) != 0 {
            frame.tick().await;
            let result: Result<()> = async {
                let png = adb_png(media).await?;
                let captured = super::now();
                let jpg = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
                    let source =
                        image::load_from_memory_with_format(&png, image::ImageFormat::Png)?;
                    let mut jpg = Vec::new();
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 80)
                        .encode_image(&source)?;
                    Ok(jpg)
                })
                .await??;
                media.screen.publish(jpg, captured);
                Ok(())
            }
            .await;
            if let Err(error) = result {
                eprintln!("Android QEMU screen: {error}");
                media.screen.end();
                media.screen.changed.notified().await;
                break;
            }
        }
    }
}

pub async fn capture(media: Arc<Media>) {
    #[cfg(target_os = "macos")]
    if let Some(avo) = &media.avo {
        tokio::join!(avo.subscribe(), avo.capture(&media));
        return;
    }
    if media.adb_media {
        adb_capture(&media).await;
    } else {
        grpc_capture(media).await;
    }
}
