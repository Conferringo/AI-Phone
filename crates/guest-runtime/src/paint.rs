//! Bounded application redraw sampling; encode only the final frame.
use crate::{Result, screen::Screen};
use image::RgbImage;
use std::time::Duration;
use tokio::time::Instant;

const SAMPLE: Duration = Duration::from_millis(40);
const QUIET: Duration = Duration::from_millis(80);
const DEADLINE: Duration = Duration::from_millis(800);

pub struct Frame {
    pub image: RgbImage,
    pub state: &'static str,
    pub wait_ms: u32,
}

fn changed(before: &RgbImage, after: &RgbImage) -> bool {
    if before.dimensions() != after.dimensions() {
        return true;
    }
    // A text caret or a few antialiased pixels must not reset the quiet window.
    before
        .pixels()
        .zip(after.pixels())
        .filter(|(a, b)| {
            a.0.iter()
                .zip(b.0)
                .map(|(a, b)| u16::from(a.abs_diff(b)))
                .sum::<u16>()
                > 30
        })
        .take(64)
        .count()
        == 64
}

pub async fn after(
    screen: &impl Screen,
    mut previous: RgbImage,
    before_title: &str,
) -> Result<Frame> {
    let start = Instant::now();
    let mut last_change = start;
    let mut painted = false;
    loop {
        let image = screen.capture()?;
        let now = Instant::now();
        let different = changed(&previous, &image);
        if different {
            painted = true;
            last_change = now;
        }
        // A new title can precede document layout by several quiet intervals.
        // Use the existing budget for transitions, without delaying ordinary input.
        let transition = screen.title().is_ok_and(|title| title != before_title);
        let expired = now.duration_since(start) >= DEADLINE;
        let state =
            if painted && now.duration_since(last_change) >= QUIET && (!transition || expired) {
                Some("stable")
            } else if expired {
                Some(if painted { "changing" } else { "unchanged" })
            } else {
                None
            };
        if let Some(state) = state {
            return Ok(Frame {
                image,
                state,
                wait_ms: now
                    .duration_since(start)
                    .as_millis()
                    .min(u128::from(u32::MAX)) as u32,
            });
        }
        if different {
            previous = image;
        }
        tokio::time::sleep(SAMPLE).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Frames {
        count: AtomicUsize,
        busy: bool,
        transition: bool,
    }
    impl Screen for Frames {
        fn capture(&self) -> Result<RgbImage> {
            let count = self.count.fetch_add(1, Ordering::Relaxed);
            let mut image = RgbImage::new(32, 32);
            if self.busy {
                image.fill(if count.is_multiple_of(2) { 255 } else { 0 });
            } else if self.transition {
                for p in image.pixels_mut().take(if count < 5 { 64 } else { 128 }) {
                    *p = Rgb([255, 255, 255]);
                }
            } else {
                for p in image.pixels_mut().take((count * 32).min(128)) {
                    *p = Rgb([255, 255, 255]);
                }
            }
            Ok(image)
        }
        fn title(&self) -> Result<String> {
            Ok(if self.transition {
                "New document".into()
            } else {
                String::new()
            })
        }
        fn move_to(&self, _: i16, _: i16) -> Result<()> {
            unreachable!()
        }
        fn button(&self, _: u8, _: bool) -> Result<()> {
            unreachable!()
        }
        fn pointer(&self) -> Result<(i32, i32)> {
            unreachable!()
        }
    }

    #[test]
    fn caret_noise_is_ignored_but_material_and_size_changes_are_retained() {
        let before = RgbImage::new(32, 32);
        let mut after = before.clone();
        for p in after.pixels_mut().take(32) {
            *p = Rgb([255, 255, 255]);
        }
        assert!(!changed(&before, &after));
        for p in after.pixels_mut().take(64) {
            *p = Rgb([255, 255, 255]);
        }
        assert!(changed(&before, &after));
        assert!(changed(&before, &RgbImage::new(64, 32)));
    }

    #[tokio::test]
    async fn incremental_small_redraws_accumulate_before_being_called_unchanged() {
        let screen = Frames {
            count: AtomicUsize::new(0),
            busy: false,
            transition: false,
        };
        let frame = after(&screen, RgbImage::new(32, 32), "").await.unwrap();
        assert_eq!(frame.state, "stable");
        assert_eq!(*frame.image.get_pixel(31, 3), Rgb([255, 255, 255]));
        assert!(frame.wait_ms < 800);
    }

    #[tokio::test]
    async fn continuous_animation_returns_latest_frame_at_the_bound() {
        let screen = Frames {
            count: AtomicUsize::new(0),
            busy: true,
            transition: false,
        };
        let frame = after(&screen, RgbImage::new(32, 32), "").await.unwrap();
        assert_eq!(frame.state, "changing");
        assert!(frame.wait_ms >= 800);
        assert!(frame.wait_ms < 1500);
    }

    #[tokio::test]
    async fn no_visual_change_returns_without_claiming_application_completion() {
        let screen = Frames {
            count: AtomicUsize::new(0),
            busy: false,
            transition: false,
        };
        screen.count.store(4, Ordering::Relaxed);
        let baseline = screen.capture().unwrap();
        let same = after(&screen, baseline, "").await.unwrap();
        assert_eq!(same.state, "unchanged");
        assert!(same.wait_ms >= 800);
    }

    #[tokio::test]
    async fn window_transition_waits_through_the_initial_frame_before_document_layout() {
        let screen = Frames {
            count: AtomicUsize::new(0),
            busy: false,
            transition: true,
        };
        let frame = after(&screen, RgbImage::new(32, 32), "Previous document")
            .await
            .unwrap();
        assert_eq!(frame.state, "stable");
        assert_eq!(*frame.image.get_pixel(31, 3), Rgb([255, 255, 255]));
        assert!(frame.wait_ms >= 800 && frame.wait_ms < 1500);
    }
}
