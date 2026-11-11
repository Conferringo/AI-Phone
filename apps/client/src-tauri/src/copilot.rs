use ai_phone_api::wire;
use ai_phone_local_speech::LocalSpeech;
use prost::Message;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tauri::{
    Manager,
    ipc::{InvokeBody, Request, Response},
};

struct Session {
    id: String,
    decoder: LocalSpeech,
}
#[derive(Default)]
pub(crate) struct Voice {
    session: Mutex<Option<Session>>,
    cancellation: Mutex<Option<(String, Arc<AtomicBool>)>>,
}
impl Voice {
    fn cancel(&self, id: Option<&str>) -> Result<(), String> {
        let mut cancellation = self.cancellation.lock().map_err(|e| e.to_string())?;
        if cancellation
            .as_ref()
            .is_some_and(|(owner, _)| id.is_none_or(|id| id == owner))
        {
            let (_, control) = cancellation.take().unwrap();
            control.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}

#[tauri::command]
pub(crate) async fn native_voice(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    request: Request<'_>,
) -> Result<Response, String> {
    let url = window.url().map_err(|e| e.to_string())?;
    if window.label() != "workbench"
        || url.path() != "/phone"
        || !app.state::<super::Navigation>().permits(&url)
    {
        return Err("请从工作台操作".into());
    }
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("语音输入格式错误".into());
    };
    let request = wire::NativeVoiceRequest::decode(bytes.as_slice()).map_err(|e| e.to_string())?;
    if request.session_id.is_empty() {
        return Err("语音会话已关闭".into());
    }
    let model = app
        .path()
        .resolve(
            "host/speech/ggml-tiny-q5_1.bin",
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| e.to_string())?;
    if matches!(
        request.action,
        Some(wire::native_voice_request::Action::Stop(true))
    ) {
        app.state::<Voice>().cancel(Some(&request.session_id))?;
    }
    let output = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Voice>();
        let mut current = state.session.lock().map_err(|e| e.to_string())?;
        let mut output = wire::NativeVoiceResponse::default();
        match request.action {
            Some(wire::native_voice_request::Action::Start(start)) => {
                state.cancel(None)?;
                *current = None;
                let decoder =
                    LocalSpeech::new(&model, &start.language).map_err(|e| e.to_string())?;
                *state.cancellation.lock().map_err(|e| e.to_string())? =
                    Some((request.session_id.clone(), decoder.cancel_handle()));
                *current = Some(Session {
                    id: request.session_id,
                    decoder,
                });
            }
            Some(wire::native_voice_request::Action::Audio(audio)) => {
                let session = current
                    .as_mut()
                    .filter(|s| s.id == request.session_id)
                    .ok_or("语音会话已关闭")?;
                if !audio.pcm16.len().is_multiple_of(2) || audio.pcm16.len() > 30 * 16000 * 2 {
                    return Err("语音输入格式错误".to_string());
                }
                let samples = audio
                    .pcm16
                    .chunks_exact(2)
                    .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
                    .collect::<Vec<_>>();
                let began = Instant::now();
                output.text = session
                    .decoder
                    .transcribe(&samples)
                    .map_err(|e| e.to_string())?;
                output.inference_ms = began.elapsed().as_millis() as u32;
            }
            Some(wire::native_voice_request::Action::Stop(true)) => {
                if current.as_ref().is_some_and(|s| s.id == request.session_id) {
                    *current = None;
                }
            }
            _ => return Err("语音输入格式错误".to_string()),
        }
        Ok(output)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(Response::new(output.encode_to_vec()))
}
