use ai_phone_guest_runtime::{
    DesktopRuntime, Response, Result, Settings,
    actions::{self, Action},
    cursor::Trace,
    files, imaging,
    process::{Capture, Output, Request, Runner},
    screen::{PixelFormat, Screen, pixels},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{Rgb, RgbImage};
use serde_json::{Value, json};
use std::sync::Mutex;

#[derive(Default)]
struct State {
    moves: Vec<(i16, i16)>,
    buttons: Vec<(u8, bool)>,
    fail_move_after: Option<usize>,
}
#[derive(Default)]
struct FakeScreen {
    state: Mutex<State>,
    reported: Option<(i32, i32)>,
    fail_capture: bool,
}
impl Screen for FakeScreen {
    fn capture(&self) -> Result<RgbImage> {
        if self.fail_capture {
            return Err("fixture capture lost after input".into());
        }
        Ok(RgbImage::from_pixel(1280, 800, Rgb([31, 63, 127])))
    }
    fn title(&self) -> Result<String> {
        Ok("Fixture 窗口".into())
    }
    fn move_to(&self, x: i16, y: i16) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state
            .fail_move_after
            .is_some_and(|limit| state.moves.len() >= limit)
        {
            return Err("fixture motion interruption".into());
        }
        state.moves.push((x, y));
        Ok(())
    }
    fn button(&self, button: u8, down: bool) -> Result<()> {
        self.state.lock().unwrap().buttons.push((button, down));
        Ok(())
    }
    fn pointer(&self) -> Result<(i32, i32)> {
        Ok(self.reported.unwrap_or_else(|| {
            let (x, y) = self
                .state
                .lock()
                .unwrap()
                .moves
                .last()
                .copied()
                .unwrap_or_default();
            (i32::from(x), i32::from(y))
        }))
    }
}

#[derive(Default)]
struct FakeRunner {
    requests: Mutex<Vec<Request>>,
    output: Mutex<Option<Output>>,
}
impl Runner for FakeRunner {
    async fn execute(&self, request: Request) -> Result<Output> {
        self.requests.lock().unwrap().push(request);
        Ok(self.output.lock().unwrap().take().unwrap_or(Output {
            code: 0,
            stdout: vec![],
            stderr: vec![],
        }))
    }
}
fn value(response: Response) -> Value {
    match response {
        Response::Json(value) => value,
        Response::Bytes(_) => panic!("expected JSON"),
    }
}
fn runtime(home: &std::path::Path) -> DesktopRuntime<FakeScreen, FakeRunner> {
    std::fs::create_dir(home.join("Files")).unwrap();
    DesktopRuntime {
        settings: Settings::for_home(home),
        screen: FakeScreen::default(),
        runner: FakeRunner::default(),
    }
}
const ACTION: &str = "1b729d02-7301-4672-9ac1-36f278fa5c60";

#[test]
fn coordinates_keys_and_unicode_limits_validate_before_input() {
    let settings = Settings::from_env();
    for (x, y) in [(0, 0), (1279, 799)] {
        assert!(actions::point(x, y, &settings).is_ok());
    }
    for (x, y) in [(-1, 0), (1280, 0), (0, 800)] {
        assert!(actions::point(x, y, &settings).is_err());
    }
    for key in ["ctrl+Return", "shift+Home", "Page_Down", "a b", "_"] {
        actions::key(key).unwrap();
    }
    for key in ["", "ctrl;touch", "$(whoami)", "中文"] {
        assert!(actions::key(key).is_err());
    }
    let good: Action =
        serde_json::from_value(json!({"kind":"text","text":"鲸".repeat(16000)})).unwrap();
    good.validate(&settings).unwrap();
    let bad: Action =
        serde_json::from_value(json!({"kind":"text","text":"鲸".repeat(16001)})).unwrap();
    assert!(bad.validate(&settings).is_err());
}

#[test]
fn drag_rounding_matches_legacy_ties_even() {
    assert_eq!(actions::step(0, 6, 1), 0);
    assert_eq!(actions::step(0, 6, 3), 2);
    assert_eq!(actions::step(6, 0, 1), 6);
    assert_eq!(actions::step(0, 120, 12), 120);
}

#[tokio::test]
async fn click_uses_real_cursor_ack_and_returns_fresh_png() {
    let home = tempfile::tempdir().unwrap();
    let mut app = runtime(home.path());
    app.screen.reported = Some((77, 66));
    let result = value(
        app.dispatch(
            "action",
            json!({"action_id":ACTION,"kind":"click","x":100,"y":200,"button":3,"clicks":2}),
        )
        .await
        .unwrap(),
    );
    assert_eq!(result["success"], true);
    assert_eq!(result["title"], "Fixture 窗口");
    let png = STANDARD.decode(result["png"].as_str().unwrap()).unwrap();
    let image = image::load_from_memory(&png).unwrap();
    assert_eq!((image.width(), image.height()), (1280, 800));
    assert_eq!(
        app.screen.state.lock().unwrap().buttons,
        [(3, true), (3, false), (3, true), (3, false)]
    );
    let cursor: Value =
        serde_json::from_slice(&std::fs::read(home.path().join(".aiphone/cursor.json")).unwrap())
            .unwrap();
    assert_eq!(cursor["action_id"], ACTION);
    assert_eq!(cursor["points"].as_array().unwrap().len(), 2);
    assert_eq!(
        (
            cursor["points"][0]["x"].as_i64(),
            cursor["points"][0]["y"].as_i64()
        ),
        (Some(77), Some(66))
    );
}

#[tokio::test]
async fn drag_is_twelve_measured_moves_and_releases_on_partial_failure() {
    let home = tempfile::tempdir().unwrap();
    let app = runtime(home.path());
    app.screen.state.lock().unwrap().fail_move_after = Some(5);
    let error = app
        .dispatch(
            "action",
            json!({"action_id":ACTION,"kind":"drag","x":10,"y":20,"to_x":100,"to_y":200}),
        )
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("interruption"));
    let state = app.screen.state.lock().unwrap();
    assert_eq!(state.moves.len(), 5);
    assert_eq!(state.buttons, [(1, true), (1, false)]);
    let cursor: Value =
        serde_json::from_slice(&std::fs::read(home.path().join(".aiphone/cursor.json")).unwrap())
            .unwrap();
    assert_eq!(cursor["points"].as_array().unwrap().len(), 5);
    assert_ne!(cursor["points"][4]["x"], 100);
}

#[tokio::test]
async fn invalid_drag_endpoint_has_zero_side_effects() {
    let home = tempfile::tempdir().unwrap();
    let app = runtime(home.path());
    assert!(
        app.dispatch(
            "action",
            json!({"kind":"drag","x":10,"y":20,"to_x":1280,"to_y":200})
        )
        .await
        .is_err()
    );
    let state = app.screen.state.lock().unwrap();
    assert!(state.moves.is_empty());
    assert!(state.buttons.is_empty());
    assert!(!home.path().join(".aiphone").exists());
}

#[tokio::test]
async fn scroll_clamps_count_and_preserves_direction() {
    let home = tempfile::tempdir().unwrap();
    let app = runtime(home.path());
    actions::execute(
        &app.screen,
        &app.runner,
        &app.settings,
        json!({"kind":"scroll","x":50,"y":60,"amount":-99}),
    )
    .await
    .unwrap();
    let state = app.screen.state.lock().unwrap();
    assert_eq!(state.buttons.len(), 16);
    assert!(state.buttons.iter().all(|value| value.0 == 4));
}

#[tokio::test]
async fn chinese_paste_has_exact_utf8_bytes_and_no_shell_interpolation() {
    let home = tempfile::tempdir().unwrap();
    let app = runtime(home.path());
    let text = "你好，鲸鱼！\n$(touch should-not-run) 😀";
    actions::execute(
        &app.screen,
        &app.runner,
        &app.settings,
        json!({"kind":"text","text":text}),
    )
    .await
    .unwrap();
    let requests = app.runner.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].program, "xclip");
    assert_eq!(requests[0].input, text.as_bytes());
    assert!(matches!(requests[0].capture, Capture::Discard));
    assert_eq!(requests[1].program, "xdotool");
    assert_eq!(requests[1].args, ["key", "--clearmodifiers", "ctrl+v"]);
    assert!(app.screen.state.lock().unwrap().moves.is_empty());
}

#[tokio::test]
async fn observation_failure_after_click_never_replays_input() {
    let home = tempfile::tempdir().unwrap();
    let mut app = runtime(home.path());
    app.screen.fail_capture = true;
    assert!(
        app.dispatch("action", json!({"kind":"click","x":50,"y":60}))
            .await
            .is_err()
    );
    assert_eq!(
        app.screen.state.lock().unwrap().buttons,
        [(1, true), (1, false)]
    );
}

#[tokio::test]
async fn guest_run_is_bounded_and_returns_nonzero_as_result() {
    let home = tempfile::tempdir().unwrap();
    let app = runtime(home.path());
    *app.runner.output.lock().unwrap() = Some(Output {
        code: 7,
        stdout: [vec![b'x'; 70000], "结束".as_bytes().to_vec()].concat(),
        stderr: vec![],
    });
    let result = value(
        app.dispatch("run", json!({"script":"printf 'work'; exit 7"}))
            .await
            .unwrap(),
    );
    assert_eq!(result["exit_code"], 7);
    assert_eq!(result["success"], false);
    let output = result["output"].as_str().unwrap();
    assert_eq!(output.len(), 65536);
    assert!(output.ends_with("结束"));
    let requests = app.runner.requests.lock().unwrap();
    let request = &requests[0];
    assert_eq!(request.program, "timeout");
    assert_eq!(request.args[0], "85");
    assert_eq!(request.timeout.as_secs(), 90);
    assert_eq!(
        request.cwd.as_deref(),
        Some(home.path().join("Files").canonicalize().unwrap().as_path())
    );
}

#[test]
fn files_are_sorted_capped_and_escape_is_rejected() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join("Files")).unwrap();
    for number in 0..105 {
        std::fs::write(
            home.path().join("Files").join(format!("{number:03}.txt")),
            b"fixture",
        )
        .unwrap();
    }
    let result = files::list(home.path(), "").unwrap();
    let rows = result["files"].as_array().unwrap();
    assert_eq!(rows.len(), 100);
    assert_eq!(rows[0]["name"], "000.txt");
    assert_eq!(rows[0]["bytes"], 7);
    assert!(files::work_path(home.path(), "../").is_err());
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), home.path().join("Files/escape")).unwrap();
    assert!(files::work_path(home.path(), "escape").is_err());
}

#[test]
fn zoom_decodes_real_png_crops_lanczos_and_enforces_bounds() {
    let image = RgbImage::from_pixel(100, 80, Rgb([42, 88, 123]));
    let input = STANDARD.encode(imaging::png(&image).unwrap());
    let result = imaging::zoom(json!({"png":input,"x":10,"y":12,"width":32,"height":32})).unwrap();
    assert_eq!(
        (result["width"].as_u64(), result["height"].as_u64()),
        (Some(64), Some(64))
    );
    assert_eq!(result["scale"], 2.0);
    let output =
        image::load_from_memory(&STANDARD.decode(result["png"].as_str().unwrap()).unwrap())
            .unwrap()
            .to_rgb8();
    assert_eq!(output.get_pixel(10, 10), &Rgb([42, 88, 123]));
    for (x, y, w, h) in [
        (-1, 0, 32, 32),
        (0, 0, 31, 32),
        (90, 0, 32, 32),
        (0, 70, 32, 32),
    ] {
        assert!(imaging::zoom(json!({"png":input,"x":x,"y":y,"width":w,"height":h})).is_err());
    }
}

#[test]
fn x11_pixel_formats_include_stride_endianness_and_rgb565() {
    let format = PixelFormat {
        bits_per_pixel: 32,
        scanline_pad: 32,
        little_endian: true,
        red: 0xff0000,
        green: 0xff00,
        blue: 0xff,
    };
    assert_eq!(
        pixels(&[3, 2, 1, 0], 1, 1, format).unwrap().get_pixel(0, 0),
        &Rgb([1, 2, 3])
    );
    assert_eq!(
        pixels(
            &[0, 1, 2, 3],
            1,
            1,
            PixelFormat {
                little_endian: false,
                ..format
            }
        )
        .unwrap()
        .get_pixel(0, 0),
        &Rgb([1, 2, 3])
    );
    let rgb565 = PixelFormat {
        bits_per_pixel: 16,
        scanline_pad: 32,
        little_endian: true,
        red: 0xf800,
        green: 0x7e0,
        blue: 0x1f,
    };
    assert_eq!(
        pixels(&[0, 0xf8, 0, 0], 1, 1, rgb565)
            .unwrap()
            .get_pixel(0, 0),
        &Rgb([255, 0, 0])
    );
    assert!(pixels(&[], 0, 1, format).is_err());
    assert!(pixels(&[0], 1, 1, format).is_err());
}

#[test]
fn cursor_receipts_are_atomic_bounded_and_preserve_action_id() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let screen = FakeScreen::default();
    let mut trace = Trace::new(home.path(), Some(ACTION));
    for _ in 0..140 {
        trace.acknowledge(&screen).unwrap();
    }
    let folder = home.path().join(".aiphone");
    let file = folder.join("cursor.json");
    let value: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(value["points"].as_array().unwrap().len(), 128);
    assert_eq!(value["action_id"], ACTION);
    assert_eq!(
        std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(std::fs::read_dir(folder).unwrap().count(), 1);
}
