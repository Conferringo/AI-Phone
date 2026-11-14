use crate::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageEncoder, ImageFormat, ImageReader, Limits, RgbImage};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::Cursor;

pub fn png(image: &RgbImage) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes).write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(bytes)
}

#[derive(Deserialize)]
struct Zoom {
    png: String,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

fn decode(value: &str) -> Result<DynamicImage> {
    if value.len() > 12 * 1024 * 1024 {
        return Err("观察图片过大".into());
    }
    let bytes = STANDARD.decode(value)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(1280);
    limits.max_image_height = Some(800);
    limits.max_alloc = Some(16 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    reader.limits(limits);
    Ok(reader.decode()?)
}

fn region(request: &Zoom, image: &DynamicImage) -> Result<(u32, u32, u32, u32)> {
    if request.width <= 0 || request.height <= 0 {
        return Err("放大区域宽高必须为正整数像素".into());
    }
    if request.x < 0 || request.y < 0 {
        return Err("放大区域超出原始观察".into());
    }
    let right = request
        .x
        .checked_add(request.width)
        .ok_or("放大区域超出原始观察")?;
    let bottom = request
        .y
        .checked_add(request.height)
        .ok_or("放大区域超出原始观察")?;
    if right > i64::from(image.width()) || bottom > i64::from(image.height()) {
        return Err("放大区域超出原始观察".into());
    }
    Ok((
        request.x as u32,
        request.y as u32,
        request.width as u32,
        request.height as u32,
    ))
}

pub fn zoom(value: Value) -> Result<Value> {
    let request: Zoom = serde_json::from_value(value)?;
    let image = decode(&request.png)?;
    let (x, y, width, height) = region(&request, &image)?;
    let scale = 2.0_f64
        .min(1280.0 / f64::from(width))
        .min(800.0 / f64::from(height));
    let output_width = (f64::from(width) * scale).round_ties_even() as u32;
    let output_height = (f64::from(height) * scale).round_ties_even() as u32;
    let output = image.crop_imm(x, y, width, height).resize_exact(
        output_width,
        output_height,
        image::imageops::FilterType::Lanczos3,
    );
    let mut encoded = Cursor::new(Vec::new());
    output.write_to(&mut encoded, ImageFormat::Png)?;
    Ok(
        json!({"success": true, "png": STANDARD.encode(encoded.into_inner()), "scale": scale, "width": output_width, "height": output_height}),
    )
}
