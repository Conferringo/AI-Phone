//! CPU OCR pipeline. Tesseract is a third-party native tool; no Python is executed.
use crate::{Bounds, Capture, Error, Region, process_output};
use image::{ImageFormat, ImageReader, imageops::FilterType};
use std::{io::Cursor, time::Duration};
use tokio::process::Command;

const HEADER: &str =
    "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext";

pub async fn recognize(png: &[u8]) -> Result<Capture, Error> {
    let image = decode(png)?;
    let (width, height) = (image.width(), image.height());
    let bytes = process_output(command(), png, 2_097_152, Duration::from_secs(4)).await?;
    let regions = regions(&bytes, width, height)?;
    let grid =
        image::imageops::resize(&image.into_luma8(), 32, 64, FilterType::Triangle).into_raw();
    Ok(Capture {
        width,
        height,
        regions,
        grid,
    })
}

fn decode(png: &[u8]) -> Result<image::DynamicImage, Error> {
    if png.is_empty() || png.len() > 2_097_152 {
        return Err(Error::InvalidData);
    }
    let mut reader = ImageReader::with_format(Cursor::new(png), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| Error::InvalidData)
}

fn bundled_command() -> Option<Command> {
    let executable = std::env::current_exe().ok()?;
    let root = executable.parent()?.parent()?.join("ocr");
    let loader = root.join("lib/ld-linux-x86-64.so.2");
    if !cfg!(target_os = "linux") || !loader.is_file() {
        return None;
    }
    let mut command = Command::new(loader);
    command
        .arg("--library-path")
        .arg(root.join("lib"))
        .arg(root.join("bin/tesseract"));
    command.env("TESSDATA_PREFIX", root.join("tessdata"));
    Some(command)
}

fn command() -> Command {
    let mut command = if let Some(program) = std::env::var_os("AI_PHONE_TESSERACT") {
        Command::new(program)
    } else if let Some(command) = bundled_command() {
        command
    } else {
        let mut command = Command::new("docker");
        command.args(["exec", "-i", "-e", "OMP_THREAD_LIMIT=1"]);
        command
            .arg(std::env::var("AI_PHONE_OCR_CONTAINER").unwrap_or_else(|_| "ai-phone-ocr".into()));
        command.args(["timeout", "-k", "0.2", "4", "tesseract"]);
        command
    };
    command.args([
        "stdin",
        "stdout",
        "-l",
        "chi_sim+eng",
        "--psm",
        "11",
        "-c",
        "tessedit_create_tsv=1",
    ]);
    command.env("OMP_THREAD_LIMIT", "1");
    command
}

struct Word {
    line: [u32; 4],
    region: Region,
}
fn score(value: &str) -> Option<f32> {
    let confidence: f32 = value.parse().ok()?;
    (confidence.is_finite() && (70.0..=100.0).contains(&confidence)).then_some(confidence / 100.0)
}
fn text_bounds(columns: &[&str], width: u32, height: u32) -> Option<Bounds> {
    let numbers: Vec<u32> = columns
        .iter()
        .map(|value| value.parse())
        .collect::<Result<_, _>>()
        .ok()?;
    let (left, top, w, h) = (numbers[0], numbers[1], numbers[2], numbers[3]);
    let (right, bottom) = (left.checked_add(w)?, top.checked_add(h)?);
    if w == 0 || h == 0 || right > width || bottom > height {
        return None;
    }
    Some(Bounds {
        left,
        top,
        right,
        bottom,
    })
}
fn word(row: &str, width: u32, height: u32) -> Option<Word> {
    let columns: Vec<_> = row.splitn(12, '\t').collect();
    if columns.len() != 12 || columns[0] != "5" || columns[11].trim().is_empty() {
        return None;
    }
    let confidence = score(columns[10])?;
    let bounds = text_bounds(&columns[6..10], width, height)?;
    let line: Vec<u32> = columns[1..5]
        .iter()
        .map(|value| value.parse())
        .collect::<Result<_, _>>()
        .ok()?;
    Some(Word {
        line: line.try_into().ok()?,
        region: Region {
            text: columns[11].chars().take(400).collect(),
            confidence,
            bounds,
        },
    })
}

fn append_line(words: &mut Vec<Word>, regions: &mut Vec<Region>) {
    if words.len() > 1 {
        let first = &words[0].region;
        let mut bounds = first.bounds.clone();
        let mut confidence = first.confidence;
        for word in &words[1..] {
            let b = &word.region.bounds;
            bounds.left = bounds.left.min(b.left);
            bounds.top = bounds.top.min(b.top);
            bounds.right = bounds.right.max(b.right);
            bounds.bottom = bounds.bottom.max(b.bottom);
            confidence = confidence.min(word.region.confidence);
        }
        regions.push(Region {
            text: words
                .iter()
                .map(|w| w.region.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(400)
                .collect(),
            confidence,
            bounds,
        });
    }
    words.clear();
}

fn regions(bytes: &[u8], width: u32, height: u32) -> Result<Vec<Region>, Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::InvalidData)?;
    let mut rows = text.lines();
    if rows.next() != Some(HEADER) {
        return Err(Error::InvalidData);
    }
    let mut regions = vec![];
    let mut line: Vec<Word> = vec![];
    for row in rows {
        let Some(word) = word(row, width, height) else {
            append_line(&mut line, &mut regions);
            continue;
        };
        if line
            .last()
            .is_some_and(|previous| previous.line != word.line)
        {
            append_line(&mut line, &mut regions);
        }
        regions.push(word.region.clone());
        line.push(word);
        if regions.len() >= 200 {
            break;
        }
    }
    append_line(&mut line, &mut regions);
    regions.truncate(200);
    Ok(regions)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(word: u32, left: u32, confidence: &str, text: &str) -> String {
        format!("5\t1\t1\t1\t1\t{word}\t{left}\t30\t40\t25\t{confidence}\t{text}\n")
    }
    fn capture(body: &str) -> Capture {
        Capture {
            width: 300,
            height: 200,
            regions: regions(format!("{HEADER}\n{body}").as_bytes(), 300, 200).unwrap(),
            grid: vec![0; 2048],
        }
    }
    #[test]
    fn adjacent_words_form_a_visible_chinese_label_without_inventing_letters() {
        let capture = capture(&(row(1, 20, "96", "周二") + &row(2, 65, "97", "计划")));
        let matches = capture.find_text("周二计划");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "周二 计划");
        assert_eq!((matches[0].bounds.left, matches[0].bounds.right), (20, 105));
        assert_eq!(matches[0].confidence, 0.96);
        assert_eq!(
            capture.find_text("周二")[0].bounds.right,
            60,
            "individual words remain precise"
        );
    }
    #[test]
    fn unknown_words_break_a_line_and_invalid_geometry_is_ignored() {
        let body = row(1, 20, "96", "甲")
            + &row(2, 65, "12", "?")
            + &row(3, 110, "97", "乙")
            + &row(4, u32::MAX, "99", "越界")
            + &row(5, 200, "NaN", "无效");
        let capture = capture(&body);
        assert!(capture.find_text("甲乙").is_empty());
        assert_eq!(capture.regions.len(), 2);
        assert!(regions(b"not a TSV header", 300, 200).is_err());
        assert!(regions(&[0xff], 300, 200).is_err());
    }
    #[test]
    fn plain_text_or_missing_tsv_columns_are_explicit_failures() {
        for bytes in [
            b"7319\n".as_slice(),
            b"".as_slice(),
            b"level\ttext\n5\t7319\n".as_slice(),
        ] {
            assert!(matches!(regions(bytes, 320, 112), Err(Error::InvalidData)));
        }
    }
    #[test]
    fn native_regions_preserve_confidence_and_image_bounds() {
        let row = "5\t1\t1\t1\t1\t1\t10\t20\t60\t30\t96.5\t7319\n";
        let data = format!("{HEADER}\n{row}");
        let accepted = regions(data.as_bytes(), 320, 112).unwrap();
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].text, "7319");
        assert_eq!(accepted[0].confidence, 0.965);
        assert_eq!(
            (
                accepted[0].bounds.left,
                accepted[0].bounds.top,
                accepted[0].bounds.right,
                accepted[0].bounds.bottom
            ),
            (10, 20, 70, 50)
        );
        assert_eq!(
            regions(data.as_bytes(), 70, 50).unwrap().len(),
            1,
            "inclusive screen edges remain visible"
        );
        for body in [
            row.replace("\t60\t30\t", "\t0\t30\t"),
            row.replace("\t60\t30\t", "\t60\t0\t"),
            row.replace("7319", "   "),
            row.replacen("5\t", "4\t", 1),
        ] {
            assert!(
                regions(format!("{HEADER}\n{body}").as_bytes(), 320, 112)
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(regions(data.as_bytes(), 30, 112).unwrap().is_empty());
        assert!(
            regions(data.replace("96.5", "69.9").as_bytes(), 320, 112)
                .unwrap()
                .is_empty()
        );
        assert!(
            regions(format!("{HEADER}\n").as_bytes(), 320, 112)
                .unwrap()
                .is_empty()
        );
        let exact = regions(data.replace("96.5", "70").as_bytes(), 320, 112).unwrap();
        assert_eq!(exact[0].confidence, 0.7);
    }
    #[tokio::test]
    async fn invalid_images_fail_before_calling_the_ocr_engine() {
        assert!(matches!(
            recognize(b"not PNG").await,
            Err(Error::InvalidData)
        ));
        assert!(matches!(
            recognize(&vec![0; 2_097_153]).await,
            Err(Error::InvalidData)
        ));
    }
}
