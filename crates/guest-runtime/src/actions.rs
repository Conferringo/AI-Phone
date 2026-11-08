use crate::{
    Result, Settings,
    cursor::Trace,
    process::{self, Capture, Request, Runner},
    screen::Screen,
};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

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
            Self::Text { text: value } | Self::ReplaceText { text: value, .. } => text(value),
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

pub async fn execute(
    screen: &impl Screen,
    runner: &impl Runner,
    settings: &Settings,
    value: Value,
) -> Result<()> {
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
) -> Result<()> {
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
        Action::Text { text } => paste(runner, settings, &text, false).await,
        Action::ReplaceText { text, .. } => paste(runner, settings, &text, true).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
