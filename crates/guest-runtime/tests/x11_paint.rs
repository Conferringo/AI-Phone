#![cfg(target_os = "linux")]

use ai_phone_guest_runtime::{DesktopRuntime, Response, Result, Settings};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, Rectangle, WindowClass},
    },
};

fn exercise(wait: bool) -> (Value, image::RgbImage, Duration) {
    std::thread::scope(|scope| {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let releases = Arc::new(AtomicUsize::new(0));
        let events = releases.clone();
        let client = scope.spawn(move || -> Result<()> {
            let (conn, index) = x11rb::connect(None)?;
            let screen = &conn.setup().roots[index];
            let window = conn.generate_id()?;
            let gc = conn.generate_id()?;
            conn.create_window(
                screen.root_depth,
                window,
                screen.root,
                400,
                300,
                200,
                140,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &CreateWindowAux::new()
                    .background_pixel(0x1f3f7f)
                    .override_redirect(1)
                    // Select the press too: otherwise Openbox receives it on the
                    // root and its implicit grab owns the matching release.
                    .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE),
            )?
            .check()?;
            conn.create_gc(gc, window, &CreateGCAux::new().foreground(0x00b428))?
                .check()?;
            conn.map_window(window)?.check()?;
            conn.get_input_focus()?.reply()?;
            ready_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline
                && matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty))
            {
                if let Some(Event::ButtonRelease(_)) = conn.poll_for_event()? {
                    events.fetch_add(1, Ordering::SeqCst);
                    if !wait {
                        // A manual acknowledgement must not depend on application repaint.
                        // Gate it explicitly so a loaded CI host cannot race the 220 ms case.
                        done_rx.recv_timeout(Duration::from_secs(3)).unwrap();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(220));
                    conn.poly_fill_rectangle(
                        window,
                        gc,
                        &[Rectangle {
                            x: 0,
                            y: 0,
                            width: 200,
                            height: 140,
                        }],
                    )?
                    .check()?;
                } else {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            conn.destroy_window(window)?.check()?;
            Ok(())
        });
        ready_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("isolated X11 application did not start");
        let home = tempfile::tempdir().unwrap();
        let mut settings = Settings::for_home(home.path());
        settings.display =
            std::env::var("DISPLAY").expect("explicit isolated X11 DISPLAY required");
        let runtime = DesktopRuntime::new(settings);
        let executor = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let started = Instant::now();
        let result = executor
            .block_on(runtime.dispatch(
                "action",
                json!({"kind":"click","x":450,"y":330,"wait_for_paint":wait}),
            ))
            .unwrap();
        let elapsed = started.elapsed();
        let Response::Json(value) = result else {
            panic!("expected frame")
        };
        let png = STANDARD.decode(value["png"].as_str().unwrap()).unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgb8();
        done_tx.send(()).unwrap();
        client.join().unwrap().unwrap();
        assert_eq!(
            releases.load(Ordering::SeqCst),
            1,
            "paint polling must not repeat the actual click"
        );
        println!(
            "paint_wait={wait} action_ms={} frame_wait_ms={}",
            elapsed.as_millis(),
            value["frame_wait_ms"]
        );
        (value, image, elapsed)
    })
}

#[test]
#[ignore = "requires an explicitly isolated Linux X11 display"]
fn automated_action_returns_the_real_delayed_redraw() {
    let (value, image, _) = exercise(true);
    assert_eq!(*image.get_pixel(450, 330), image::Rgb([0, 180, 40]));
    assert_eq!(value["frame_state"], "stable");
}

#[test]
#[ignore = "requires an explicitly isolated Linux X11 display"]
fn streamed_manual_input_keeps_its_fast_acknowledgement() {
    let (value, image, _) = exercise(false);
    assert_eq!(*image.get_pixel(450, 330), image::Rgb([31, 63, 127]));
    assert!(value["frame_state"].is_null());
}
