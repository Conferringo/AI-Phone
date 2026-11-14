use crate::{
    Result, Settings,
    cursor::Trace,
    process::{self, Capture, Request, Runner},
    screen::{Screen, WindowIdentity},
};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

mod terminal;

fn one() -> i64 {
    1
}
fn three() -> i64 {
    3
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Action {
    Click {
        #[serde(default)]
        x: i32,
        #[serde(default)]
        y: i32,
        #[serde(default = "one")]
        button: i64,
        #[serde(default = "one")]
        clicks: i64,
    },
    Drag {
        #[serde(default)]
        x: i32,
        #[serde(default)]
        y: i32,
        #[serde(default)]
        to_x: i32,
        #[serde(default)]
        to_y: i32,
    },
    Scroll {
        #[serde(default)]
        x: i32,
        #[serde(default)]
        y: i32,
        #[serde(default = "three")]
        amount: i64,
    },
    Key {
        key: String,
    },
    Text {
        text: String,
        window_id: Option<u32>,
        process_id: Option<u32>,
    },
    #[serde(rename = "replace_text")]
    ReplaceText {
        text: String,
        window_id: u32,
        process_id: u32,
    },
}

/// The focus check failed before any clipboard or keyboard side effect.
#[derive(Debug)]
pub struct InputRejected(pub String);
impl std::fmt::Display for InputRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for InputRejected {}

#[derive(Debug)]
pub struct InputUnconfirmed;
impl std::fmt::Display for InputUnconfirmed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("当前窗口的文本输入结果未确认，请先核对")
    }
}
impl std::error::Error for InputUnconfirmed {}

#[derive(Debug, PartialEq, Eq)]
pub enum InputEvidence {
    NoText,
    PasteDispatched,
    TerminalSelectionDelivered,
}

#[derive(Deserialize)]
pub struct Input {
    pub action_id: Option<String>,
    #[serde(flatten)]
    pub action: Action,
}

pub fn point(x: i32, y: i32, settings: &Settings) -> Result<(i16, i16)> {
    if !(0..settings.width as i32).contains(&x) || !(0..settings.height as i32).contains(&y) {
        return Err("坐标超出屏幕".into());
    }
    Ok((i16::try_from(x)?, i16::try_from(y)?))
}

fn click_point(x: i32, y: i32, button: i64, settings: &Settings) -> Result<()> {
    point(x, y, settings)?;
    if !(1..=3).contains(&button) {
        return Err("无效鼠标按键".into());
    }
    Ok(())
}

pub fn key(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 80 {
        return Err("无效按键".into());
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_+ -".contains(&byte))
    {
        return Err("无效按键".into());
    }
    Ok(())
}

fn text(value: &str) -> Result<()> {
    if value.chars().count() > 16000 {
        return Err("文本过长".into());
    }
    Ok(())
}

impl Action {
    pub fn validate(&self, settings: &Settings) -> Result<()> {
        match self {
            Self::Click { x, y, button, .. } => click_point(*x, *y, *button, settings),
            Self::Drag { x, y, to_x, to_y } => {
                point(*x, *y, settings)?;
                point(*to_x, *to_y, settings)?;
                Ok(())
            }
            Self::Scroll { x, y, .. } => point(*x, *y, settings).map(|_| ()),
            Self::Key { key: value } => key(value),
            Self::Text { text: value, .. } | Self::ReplaceText { text: value, .. } => text(value),
        }
    }
}

struct HeldButton<'a, S: Screen> {
    screen: &'a S,
    button: u8,
    held: bool,
}
impl<'a, S: Screen> HeldButton<'a, S> {
    fn press(screen: &'a S, button: u8) -> Result<Self> {
        screen.button(button, true)?;
        Ok(Self {
            screen,
            button,
            held: true,
        })
    }
    fn release(&mut self) -> Result<()> {
        self.screen.button(self.button, false)?;
        self.held = false;
        Ok(())
    }
}
impl<S: Screen> Drop for HeldButton<'_, S> {
    fn drop(&mut self) {
        if self.held {
            let _ = self.screen.button(self.button, false);
        }
    }
}

pub fn step(from: i32, to: i32, number: i32) -> i16 {
    (f64::from(from) + f64::from(to - from) * f64::from(number) / 12.0).round_ties_even() as i16
}

async fn drag(
    screen: &impl Screen,
    trace: &mut Trace,
    start: (i32, i32),
    end: (i32, i32),
) -> Result<()> {
    screen.move_to(start.0 as i16, start.1 as i16)?;
    let mut held = HeldButton::press(screen, 1)?;
    trace.acknowledge(screen)?;
    for number in 1..=12 {
        screen.move_to(step(start.0, end.0, number), step(start.1, end.1, number))?;
        trace.acknowledge(screen)?;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    held.release()
}

async fn repeat(screen: &impl Screen, button: u8, count: u64, delay: u64) -> Result<()> {
    for index in 0..count {
        let mut held = HeldButton::press(screen, button)?;
        held.release()?;
        if index + 1 < count {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }
    Ok(())
}

async fn click(
    screen: &impl Screen,
    trace: &mut Trace,
    location: (i32, i32),
    button: i64,
    clicks: i64,
) -> Result<()> {
    screen.move_to(location.0 as i16, location.1 as i16)?;
    trace.acknowledge(screen)?;
    repeat(screen, button as u8, clicks.clamp(1, 2) as u64, 110).await?;
    trace.acknowledge(screen)
}

async fn scroll(
    screen: &impl Screen,
    trace: &mut Trace,
    location: (i32, i32),
    amount: i64,
) -> Result<()> {
    screen.move_to(location.0 as i16, location.1 as i16)?;
    trace.acknowledge(screen)?;
    repeat(
        screen,
        if amount > 0 { 5 } else { 4 },
        amount.unsigned_abs().clamp(1, 8),
        50,
    )
    .await?;
    trace.acknowledge(screen)
}

async fn send_keys(runner: &impl Runner, settings: &Settings, keys: &[&str]) -> Result<()> {
    let mut request = Request::new("xdotool", &["key", "--clearmodifiers"]);
    request
        .args
        .extend(keys.iter().map(std::ffi::OsString::from));
    request.env = settings.environment()?;
    process::checked(runner.execute(request).await?)?;
    Ok(())
}

async fn paste(
    runner: &impl Runner,
    settings: &Settings,
    value: &str,
    replace: bool,
) -> Result<()> {
    let mut request = Request::new("xclip", &["-selection", "clipboard"]);
    request.input = value.as_bytes().to_vec();
    request.timeout = Some(Duration::from_secs(5));
    request.capture = Capture::Discard;
    request.env = settings.environment()?;
    process::checked(runner.execute(request).await?)?;
    let keys: &[&str] = if replace {
        &["ctrl+a", "ctrl+v"]
    } else {
        &["ctrl+v"]
    };
    send_keys(runner, settings, keys).await
}

fn text_target(
    screen: &impl Screen,
    window: Option<u32>,
    process: Option<u32>,
) -> Result<WindowIdentity> {
    let focus = screen
        .window()
        .map_err(|_| InputRejected("未确认当前输入窗口".into()))?;
    if focus.window_id == 0
        || window.is_some_and(|id| id != 0 && focus.window_id != id)
        || process.is_some_and(|id| id != 0 && focus.process_id != id)
    {
        return Err(InputRejected("当前输入窗口已变化，请重新观察".into()).into());
    }
    Ok(focus)
}

async fn type_text(
    screen: &impl Screen,
    runner: &impl Runner,
    settings: &Settings,
    text: &str,
    window_id: Option<u32>,
    process_id: Option<u32>,
) -> Result<InputEvidence> {
    let focus = text_target(screen, window_id, process_id)?;
    if !terminal::is_xterm(&focus.application_class) {
        paste(runner, settings, text, false).await?;
        return Ok(InputEvidence::PasteDispatched);
    }
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err(InputRejected("终端输入需要单行可打印文本".into()).into());
    }
    let selection = terminal::Selection::open(&settings.display)
        .map_err(|_| InputRejected("无法准备终端文本输入".into()))?;
    text_target(screen, Some(focus.window_id), Some(focus.process_id))?;
    send_keys(runner, settings, &["shift+Insert"])
        .await
        .map_err(|_| InputUnconfirmed)?;
    selection
        .confirm(focus.window_id, text)
        .await
        .map_err(|_| InputUnconfirmed)?;
    Ok(InputEvidence::TerminalSelectionDelivered)
}

pub async fn execute(
    screen: &impl Screen,
    runner: &impl Runner,
    settings: &Settings,
    value: Value,
) -> Result<InputEvidence> {
    let input: Input = serde_json::from_value(value)?;
    input.action.validate(settings)?;
    if let Action::ReplaceText {
        window_id,
        process_id,
        ..
    } = &input.action
    {
        crate::accessibility::require_field(screen, settings, *window_id, *process_id)
            .await
            .map_err(|e| InputRejected(e.to_string()))?;
    }
    let mut trace = Trace::new(&settings.home, input.action_id.as_deref());
    perform(screen, runner, settings, &mut trace, input.action).await
}

async fn perform(
    screen: &impl Screen,
    runner: &impl Runner,
    settings: &Settings,
    trace: &mut Trace,
    action: Action,
) -> Result<InputEvidence> {
    match action {
        Action::Click {
            x,
            y,
            button,
            clicks,
        } => click(screen, trace, (x, y), button, clicks).await,
        Action::Drag { x, y, to_x, to_y } => drag(screen, trace, (x, y), (to_x, to_y)).await,
        Action::Scroll { x, y, amount } => scroll(screen, trace, (x, y), amount).await,
        Action::Key { key } => send_keys(runner, settings, &[&key]).await,
        Action::Text {
            text,
            window_id,
            process_id,
        } => return type_text(screen, runner, settings, &text, window_id, process_id).await,
        Action::ReplaceText { text, .. } => {
            paste(runner, settings, &text, true).await?;
            return Ok(InputEvidence::PasteDispatched);
        }
    }?;
    Ok(InputEvidence::NoText)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;
    use serde_json::json;
    use std::sync::Mutex;
    #[derive(Default)]
    struct Recorder(Mutex<Vec<Request>>);
    impl Runner for Recorder {
        async fn execute(&self, request: Request) -> Result<process::Output> {
            self.0.lock().unwrap().push(request);
            Ok(process::Output {
                code: 0,
                stdout: vec![],
                stderr: vec![],
            })
        }
    }
    struct Focus(&'static str);
    impl Screen for Focus {
        fn capture(&self) -> Result<RgbImage> {
            unreachable!()
        }
        fn title(&self) -> Result<String> {
            unreachable!()
        }
        fn window(&self) -> Result<WindowIdentity> {
            Ok(WindowIdentity {
                application_class: self.0.into(),
                window_id: 11,
                process_id: 22,
                ..Default::default()
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

    #[tokio::test]
    async fn stale_focus_and_terminal_control_characters_never_send_input() {
        let settings = Settings::from_env();
        let runner = Recorder::default();
        for (class, payload) in [
            (
                "Mousepad",
                json!({"kind":"text","text":"test","window_id":12,"process_id":22}),
            ),
            (
                "Mousepad",
                json!({"kind":"text","text":"test","window_id":11,"process_id":23}),
            ),
            ("XTerm", json!({"kind":"text","text":"test\n"})),
            ("UXTerm", json!({"kind":"text","text":"test\u{001b}"})),
        ] {
            let error = execute(&Focus(class), &runner, &settings, payload)
                .await
                .unwrap_err();
            assert!(error.is::<InputRejected>());
        }
        assert!(runner.0.lock().unwrap().is_empty());
        execute(
            &Focus("Mousepad"),
            &runner,
            &settings,
            json!({"kind":"text","text":"test"}),
        )
        .await
        .unwrap();
        let calls = runner.0.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].program, "xclip");
        assert_eq!(calls[1].args, ["key", "--clearmodifiers", "ctrl+v"]);
    }

    #[tokio::test]
    async fn replacement_selects_and_pastes_in_one_keyboard_process() {
        let runner = Recorder::default();
        let text = "新文件 😀 $(not-a-command)";
        paste(&runner, &Settings::from_env(), text, true)
            .await
            .unwrap();
        let calls = runner.0.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].program, "xclip");
        assert_eq!(calls[0].input, text.as_bytes());
        assert_eq!(calls[1].program, "xdotool");
        assert_eq!(
            calls[1].args,
            ["key", "--clearmodifiers", "ctrl+a", "ctrl+v"]
        );
    }
}
