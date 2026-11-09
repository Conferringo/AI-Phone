//! Numeric transport timings; no prompts, credentials or model reasoning.
use serde_json::{Value, json};
use std::time::Instant;

pub struct Timing {
    started: Instant,
    request_at: Option<Instant>,
    preparation_ms: u64,
    headers_ms: Option<u64>,
    first_byte_ms: Option<u64>,
    first_text_ms: Option<u64>,
    request_bytes: u64,
}

fn millis(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u64::MAX as u128) as u64
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            request_at: None,
            preparation_ms: 0,
            headers_ms: None,
            first_byte_ms: None,
            first_text_ms: None,
            request_bytes: 0,
        }
    }
}

impl Timing {
    pub fn request_started(&mut self, bytes: usize) {
        self.preparation_ms = millis(self.started);
        self.request_bytes = bytes as u64;
        self.request_at = Some(Instant::now());
    }

    pub fn headers_received(&mut self) {
        self.headers_ms = self.request_at.map(millis);
    }

    pub fn first_bytes_received(&mut self) {
        if self.first_byte_ms.is_none() {
            self.first_byte_ms = self.request_at.map(millis);
        }
    }

    pub fn text_received(&mut self) {
        if self.first_text_ms.is_none() {
            self.first_text_ms = self.request_at.map(millis);
        }
    }

    pub fn snapshot(&self) -> Value {
        json!({"preparation_ms":if self.request_at.is_some(){self.preparation_ms}else{millis(self.started)},
            "headers_ms":self.headers_ms,"first_byte_ms":self.first_byte_ms,"first_text_ms":self.first_text_ms,
            "model_ms":self.request_at.map(millis).unwrap_or(0),"elapsed_ms":millis(self.started),
            "request_bytes":self.request_bytes})
    }
}

pub fn wire(value: &Value) -> Option<ai_phone_api::wire::ModelCallTiming> {
    Some(ai_phone_api::wire::ModelCallTiming {
        preparation_ms: value["preparation_ms"].as_u64()?,
        headers_ms: value["headers_ms"].as_u64(),
        first_byte_ms: value["first_byte_ms"].as_u64(),
        model_ms: value["model_ms"].as_u64()?,
        elapsed_ms: value["elapsed_ms"].as_u64()?,
        request_bytes: value["request_bytes"].as_u64()?,
        first_text_ms: value["first_text_ms"].as_u64(),
    })
}
