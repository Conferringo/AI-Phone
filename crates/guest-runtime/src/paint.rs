//! Bounded application redraw sampling; encode only the final frame.
use crate::{
    Result,
    screen::{PaintFocus, Screen},
};
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
    const CHANNELS: usize = 3;
    const BLOCK_BYTES: usize = 256 * CHANNELS;
    let visible_bytes = before.pixels().len() * CHANNELS;
    // A text caret or a few antialiased pixels must not reset the quiet window.
    before.as_raw()[..visible_bytes]
        .chunks(BLOCK_BYTES)
        .zip(after.as_raw()[..visible_bytes].chunks(BLOCK_BYTES))
        .filter(|(a, b)| a != b)
        .flat_map(|(a, b)| a.chunks_exact(CHANNELS).zip(b.chunks_exact(CHANNELS)))
        .filter(|(a, b)| {
            a.iter()
                .zip(*b)
                .map(|(a, b)| u16::from(a.abs_diff(*b)))
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
    before: &PaintFocus,
    mousepad_text: bool,
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
        let elapsed = now.duration_since(start);
        let expired = elapsed >= DEADLINE;
        let quiet = now.duration_since(last_change) >= QUIET;
        // A new document title can precede layout. A confirmed desktop reveal has no app to load.
        let state = if painted
            && quiet
            && (expired
                || !screen.paint_focus().is_ok_and(|focus| {
                    focus.title != before.title
                        && !(mousepad_text && mousepad_dirty_title(before, &focus))
                        && !screen.confirmed_desktop_reveal(before.window_id, focus.window_id)
                })) {
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
                wait_ms: elapsed.as_millis().min(u128::from(u32::MAX)) as u32,
            });
        }
        if different {
            previous = image;
        }
        tokio::time::sleep(SAMPLE).await;
    }
}

fn mousepad_dirty_title(before: &PaintFocus, after: &PaintFocus) -> bool {
    before.window_id != 0
        && before.window_id == after.window_id
        && (before.title.strip_prefix('*') == Some(after.title.as_str())
            || after.title.strip_prefix('*') == Some(before.title.as_str()))
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
        desktop_reveal: bool,
    }
    impl Screen for Frames {
        fn window(&self) -> Result<crate::screen::WindowIdentity> {
            Ok(crate::screen::WindowIdentity {
                title: self.title()?,
                application_class: String::new(),
                window_id: if self.desktop_reveal { 2 } else { 1 },
                ..Default::default()
            })
        }
        fn capture(&self) -> Result<RgbImage> {
            let count = self.count.fetch_add(1, Ordering::Relaxed);
            let mut image = RgbImage::new(32, 32);
            if self.busy {
                image.fill(if count.is_multiple_of(2) { 255 } else { 0 });
            } else if self.desktop_reveal {
                image.fill(255);
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
            Ok(if self.desktop_reveal {
                "Desktop".into()
            } else if self.transition {
                "New document".into()
            } else {
                String::new()
            })
        }
        fn confirmed_desktop_reveal(&self, previous: u32, focused: u32) -> bool {
            self.desktop_reveal && (previous, focused) == (1, 2)
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

    fn focus(title: &str) -> PaintFocus {
        PaintFocus {
            title: title.into(),
            window_id: 1,
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
        for p in after.pixels_mut().skip(256).take(32) {
            *p = Rgb([255, 255, 255]);
        }
        assert!(changed(&before, &after));
        assert!(changed(&before, &RgbImage::new(64, 32)));
        let padded = RgbImage::from_raw(1, 1, vec![0; 3 + 64 * 3]).unwrap();
        let mut raw = vec![255; 3 + 64 * 3];
        raw[..3].fill(0);
        let changed_padding = RgbImage::from_raw(1, 1, raw).unwrap();
        assert!(!changed(&padded, &changed_padding));
    }

    #[tokio::test]
    async fn incremental_small_redraws_accumulate_before_being_called_unchanged() {
        let screen = Frames {
            count: AtomicUsize::new(0),
            busy: false,
            transition: false,
            desktop_reveal: false,
        };
        let frame = after(&screen, RgbImage::new(32, 32), &focus(""), false)
            .await
            .unwrap();
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
            desktop_reveal: false,
        };
        let frame = after(&screen, RgbImage::new(32, 32), &focus(""), false)
            .await
            .unwrap();
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
            desktop_reveal: false,
        };
        screen.count.store(4, Ordering::Relaxed);
        let baseline = screen.capture().unwrap();
        let same = after(&screen, baseline, &focus(""), false).await.unwrap();
        assert_eq!(same.state, "unchanged");
        assert!(same.wait_ms >= 800);
    }

    #[tokio::test]
    async fn window_transition_waits_through_the_initial_frame_before_document_layout() {
        let screen = Frames {
            count: AtomicUsize::new(0),
            busy: false,
            transition: true,
            desktop_reveal: false,
        };
        let frame = after(
            &screen,
            RgbImage::new(32, 32),
            &focus("Previous document"),
            false,
        )
        .await
        .unwrap();
        assert_eq!(frame.state, "stable");
        assert_eq!(*frame.image.get_pixel(31, 3), Rgb([255, 255, 255]));
        assert!(frame.wait_ms >= 800 && frame.wait_ms < 1500);
    }

    #[tokio::test]
    async fn closing_a_window_to_reveal_the_desktop_returns_after_the_quiet_period() {
        let screen = Frames {
            count: AtomicUsize::new(0),
            busy: false,
            transition: false,
            desktop_reveal: true,
        };
        let frame = after(
            &screen,
            RgbImage::new(32, 32),
            &focus("Closed window"),
            false,
        )
        .await
        .unwrap();
        assert_eq!(frame.state, "stable");
        assert_eq!(*frame.image.get_pixel(0, 0), Rgb([255, 255, 255]));
        assert!((80..800).contains(&frame.wait_ms), "{} ms", frame.wait_ms);
    }

    #[test]
    fn mousepad_dirty_marker_is_local_to_the_same_window_and_document() {
        let clean = focus("Untitled 1 - Mousepad");
        let dirty = focus("*Untitled 1 - Mousepad");
        assert!(mousepad_dirty_title(&clean, &dirty));
        assert!(mousepad_dirty_title(&dirty, &clean));
        assert!(!mousepad_dirty_title(
            &clean,
            &focus("*Untitled 2 - Mousepad")
        ));
        assert!(!mousepad_dirty_title(
            &clean,
            &PaintFocus {
                window_id: 2,
                ..dirty
            }
        ));
    }
}
