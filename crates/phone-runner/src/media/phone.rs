use super::{
    Arc, Body, Bytes, Duration, Media, Ordering, Result,
    grpc::{
        Session,
        wire::{AudioFormat, Image, ImageFormat},
    },
    hub,
};
use futures_util::stream;
use tokio::{sync::watch, time::Instant};
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
pub async fn capture(media: Arc<Media>) {
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
