use crate::{
    Result, Settings, actions, files, imaging, paint,
    process::{Capture, OsRunner, Request, Runner},
    screen::{NativeScreen, Screen},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::RgbImage;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub enum Response {
    Json(Value),
    Bytes(Vec<u8>),
}

pub struct DesktopRuntime<S = NativeScreen, R = OsRunner> {
    pub settings: Settings,
    pub screen: S,
    pub runner: R,
}

impl DesktopRuntime {
    pub fn new(settings: Settings) -> Self {
        let screen = NativeScreen::new(settings.display.clone());
        Self {
            settings,
            screen,
            runner: OsRunner,
        }
    }
}

#[derive(Deserialize)]
struct FilesRequest {
    #[serde(default)]
    path: String,
}
#[derive(Deserialize)]
struct RunRequest {
    #[serde(default)]
    script: String,
}

impl<S: Screen, R: Runner> DesktopRuntime<S, R> {
    pub fn observe(&self) -> Result<Value> {
        self.frame(&self.screen.capture()?)
    }

    fn frame(&self, image: &RgbImage) -> Result<Value> {
        let identity = self.screen.window().unwrap_or_default();
        let title = if identity.title.is_empty() {
            "Linux desktop"
        } else {
            &identity.title
        };
        let bytes = imaging::png(image)?;
        Ok(
            json!({"width": self.settings.width, "height": self.settings.height, "title": title, "application_class":identity.application_class,"window_id":identity.window_id,"process_id":identity.process_id,"png": STANDARD.encode(bytes)}),
        )
    }

    pub async fn dispatch(&self, operation: &str, args: Value) -> Result<Response> {
        match operation {
            "controls" => crate::accessibility::find(&self.screen, &self.settings, args)
                .await
                .map(Response::Json),
            "action" => self.perform_action(args).await.map(Response::Json),
            "run" => self.run(args).await.map(Response::Json),
            "notifications" => self.notifications(args).await.map(Response::Json),
            _ => self.read(operation, args),
        }
    }

    fn read(&self, operation: &str, args: Value) -> Result<Response> {
        match operation {
            "snapshot" => self.snapshot().map(Response::Bytes),
            "observe" => self.observe().map(Response::Json),
            "zoom" => imaging::zoom(args).map(Response::Json),
            "files" => self.files(args).map(Response::Json),
            _ => Err("未知操作".into()),
        }
    }

    fn snapshot(&self) -> Result<Vec<u8>> {
        imaging::png(&self.screen.capture()?)
    }

    fn files(&self, args: Value) -> Result<Value> {
        let request: FilesRequest = serde_json::from_value(args)?;
        files::list(&self.settings.home, &request.path)
    }

    async fn perform_action(&self, args: Value) -> Result<Value> {
        let baseline = if args["wait_for_paint"] == true {
            let image = self.screen.capture()?;
            let focus = self.screen.paint_focus().unwrap_or_default();
            let mousepad_text = matches!(args["kind"].as_str(), Some("text" | "replace_text"))
                && self.screen.window().is_ok_and(|window| {
                    window.window_id == focus.window_id
                        && window.application_class.eq_ignore_ascii_case("mousepad")
                });
            Some((image, focus, mousepad_text))
        } else {
            None
        };
        let input = actions::execute(&self.screen, &self.runner, &self.settings, args).await?;
        let mut observation = if let Some((before, focus, mousepad_text)) = baseline {
            let frame = paint::after(&self.screen, before, &focus, mousepad_text).await?;
            if input == actions::InputEvidence::PasteDispatched && frame.state == "unchanged" {
                return Err(actions::InputUnconfirmed.into());
            }
            let mut value = self.frame(&frame.image)?;
            value["frame_state"] = json!(frame.state);
            value["frame_wait_ms"] = json!(frame.wait_ms);
            value
        } else {
            tokio::time::sleep(Duration::from_millis(120)).await;
            self.observe()?
        };
        observation["success"] = json!(true);
        Ok(observation)
    }

    async fn notifications(&self, args: Value) -> Result<Value> {
        let mut feed = crate::notifications::list(
            &self.runner,
            &self.settings.home.join(".aiphone"),
            rustix::process::getuid().as_raw(),
        )
        .await?;
        if args["include_window"] == true
            && let Ok(window) = self.screen.window()
            && window.window_id != 0
            && window.process_id != 0
            && !window.application_class.is_empty()
            && !window.title.trim().is_empty()
        {
            feed["foreground"] = json!([
                window.window_id,
                window.process_id,
                window.application_class,
                format!("{:x}", Sha256::digest(window.title.trim().as_bytes())),
            ]);
        }
        Ok(feed)
    }

    async fn run(&self, value: Value) -> Result<Value> {
        let request: RunRequest = serde_json::from_value(value)?;
        if request.script.is_empty() || request.script.chars().count() > 24000 {
            return Err("命令为空或过长".into());
        }
        // This shell redirection preserves stderr/stdout order without an extra shell/process.
        let script = format!("exec 2>&1\n{}", request.script);
        let mut command = Request::new("timeout", &["85", "bash", "-lc", &script]);
        command.cwd = Some(files::work_path(&self.settings.home, "")?);
        command.env = self.settings.environment()?;
        command.timeout = Some(Duration::from_secs(90));
        command.capture = Capture::Tail(65536);
        let output = self.runner.execute(command).await?;
        let mut bytes = output.stderr;
        bytes.extend(output.stdout);
        let tail = &bytes[bytes.len().saturating_sub(65536)..];
        Ok(
            json!({"success": output.code == 0, "exit_code": output.code, "output": String::from_utf8_lossy(tail)}),
        )
    }
}
