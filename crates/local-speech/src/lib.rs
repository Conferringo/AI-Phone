use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperError,
    WhisperState,
};

#[derive(Debug)]
pub enum Error {
    UnsupportedLanguage,
    Cancelled,
    Engine(WhisperError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedLanguage => formatter.write_str("本机识别不支持此语言"),
            Self::Cancelled => formatter.write_str("本机识别已取消"),
            Self::Engine(error) => write!(formatter, "本机识别失败：{error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<WhisperError> for Error {
    fn from(error: WhisperError) -> Self {
        Self::Engine(error)
    }
}

pub struct LocalSpeech {
    state: WhisperState,
    language: &'static str,
    cancelled: Arc<AtomicBool>,
}

impl LocalSpeech {
    pub fn new(model_path: impl AsRef<Path>, language: &str) -> Result<Self, Error> {
        let primary = language.split('-').next().unwrap_or_default();
        if !primary.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            return Err(Error::UnsupportedLanguage);
        }
        let language = whisper_rs::get_lang_id(&primary.to_ascii_lowercase())
            .and_then(whisper_rs::get_lang_str)
            .ok_or(Error::UnsupportedLanguage)?;
        let mut parameters = WhisperContextParameters::default();
        parameters.use_gpu(false);
        let context = WhisperContext::new_with_params(model_path, parameters)?;
        let state = context.create_state()?;
        Ok(Self {
            state,
            language,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    /// Accepts 16 kHz mono signed PCM16. The loaded model and state are reused.
    pub fn transcribe(&mut self, samples: &[i16]) -> Result<String, Error> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let audio: Vec<f32> = samples
            .iter()
            .map(|sample| f32::from(*sample) / 32768.0)
            .collect();
        let mut parameters = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        parameters.set_n_threads(4);
        parameters.set_language(Some(self.language));
        parameters.set_translate(false);
        parameters.set_no_context(true);
        parameters.set_no_timestamps(true);
        parameters.set_print_special(false);
        parameters.set_print_progress(false);
        parameters.set_print_realtime(false);
        parameters.set_print_timestamps(false);
        let result = self.state.full(parameters, &audio);
        if self.cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        result?;
        let mut transcript = String::new();
        for segment in self.state.as_iter() {
            transcript.push_str(segment.to_str()?);
        }
        Ok(transcript.trim().to_owned())
    }
}
