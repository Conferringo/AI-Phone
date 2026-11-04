use ai_phone_guest_runtime::{DesktopRuntime, Response, Result, Settings};
use serde_json::json;
use std::{
    io::{Read, Write},
    path::Path,
    process::ExitCode,
};

const MAX_INPUT: u64 = 16 * 1024 * 1024;

fn request(operation: &str) -> Result<serde_json::Value> {
    if matches!(operation, "observe" | "snapshot") {
        return Ok(json!({}));
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_INPUT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT {
        return Err("输入超出大小限制".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn emit(response: Response) -> Result<()> {
    let mut output = std::io::stdout().lock();
    match response {
        Response::Json(value) => {
            serde_json::to_writer(&mut output, &value)?;
            output.write_all(b"\n")?;
        }
        Response::Bytes(bytes) => output.write_all(&bytes)?,
    }
    output.flush()?;
    Ok(())
}

async fn interrupted() -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    tokio::select! { _ = term.recv() => (), _ = interrupt.recv() => () }
    Ok(())
}

async fn run(operation: &str) -> Result<()> {
    match operation {
        "audio-supervise" => return supervise_audio().await,
        "home-identity" => return ai_phone_guest_runtime::home_identity::run().await,
        "record-notification" => return ai_phone_guest_runtime::notifications::record_from_env(),
        "capture" => {
            let mode = std::env::args().nth(2).ok_or("缺少采集模式")?;
            return ai_phone_guest_runtime::media::run(&mode).await;
        }
        _ => (),
    }
    let request = request(operation)?;
    let runtime = DesktopRuntime::new(Settings::from_env());
    let result = tokio::select! {
        result = runtime.dispatch(operation, request) => result,
        result = interrupted() => { result?; Err("操作已中断，结果可能未知，请先观察".into()) }
    }?;
    emit(result)
}

async fn supervise_audio() -> Result<()> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/tmp/ai-runtime".into());
    let config = std::env::args_os()
        .nth(2)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/etc/ai-phone-pulse.pa".into());
    ai_phone_guest_runtime::audio::run(&runtime, &config).await
}

fn operation() -> Result<String> {
    let mut args = std::env::args();
    let executable = args.next().ok_or("缺少程序名")?;
    if Path::new(&executable)
        .file_name()
        .and_then(|name| name.to_str())
        == Some("desktop-notification-record")
    {
        return Ok("record-notification".into());
    }
    args.next().ok_or_else(|| "未知操作".into())
}

#[tokio::main]
async fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("desktop-bridge {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let operation = operation();
    let stderr_only = operation.as_ref().is_ok_and(|value| {
        matches!(
            value.as_str(),
            "capture" | "audio-supervise" | "record-notification" | "home-identity"
        )
    });
    let result = match operation {
        Ok(value) => run(&value).await,
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        if stderr_only {
            eprintln!("{error}");
        } else {
            let mut value = json!({"success": false, "error": error.to_string()});
            if error.is::<ai_phone_guest_runtime::actions::InputRejected>() {
                value["not_executed"] = json!(true);
                value["code"] = json!("field_focus");
            }
            let _ = emit(Response::Json(value));
        }
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
