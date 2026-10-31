//! Accepted transcription metadata. Capture, STT and playback have separate owners.
use crate::wire;
use serde_json::json;
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

pub struct VoiceInput {
    session: Uuid,
    utterance: String,
    revision: i64,
    literal: String,
    corrects: Option<Uuid>,
}

fn valid_segment(voice: &wire::ChatVoiceInput) -> bool {
    !voice.utterance_id.trim().is_empty()
        && voice.utterance_id.len() <= 128
        && voice.revision > 0
        && !voice.literal.trim().is_empty()
        && voice.literal.len() <= 48000
}

impl VoiceInput {
    pub fn parse(voice: &wire::ChatVoiceInput) -> Result<Self, &'static str> {
        let session = Uuid::parse_str(&voice.session_id).map_err(|_| "invalid_voice")?;
        if session.is_nil() || !valid_segment(voice) {
            return Err("invalid_voice");
        }
        let corrects = if voice.corrects_request_id.is_empty() {
            None
        } else {
            Some(Uuid::parse_str(&voice.corrects_request_id).map_err(|_| "invalid_voice")?)
        };
        Ok(Self {
            session,
            utterance: voice.utterance_id.clone(),
            revision: voice.revision.into(),
            literal: voice.literal.clone(),
            corrects,
        })
    }

    /// The caller owns the conversation lock and receipt check. Exact retries
    /// return before here; only a new accepted revision can advance the chain.
    pub async fn persist(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        conversation: Uuid,
        message: Uuid,
        request: Uuid,
    ) -> Result<(), sqlx::Error> {
        let latest: Option<(Uuid, i64)> = sqlx::query_as("SELECT request_id,revision FROM chat_voice_inputs WHERE conversation_id=$1 AND session_id=$2 AND utterance_id=$3 ORDER BY revision DESC LIMIT 1")
            .bind(conversation).bind(self.session).bind(&self.utterance).fetch_optional(&mut **tx).await?;
        let valid = match latest {
            None => self.corrects.is_none(),
            Some((previous, revision)) => {
                self.corrects == Some(previous) && self.revision > revision
            }
        };
        if !valid {
            return Err(sqlx::Error::Protocol("voice_conflict".into()));
        }
        sqlx::query("INSERT INTO chat_voice_inputs(request_id,message_id,conversation_id,session_id,utterance_id,revision,literal,corrects_request_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(request).bind(message).bind(conversation).bind(self.session).bind(&self.utterance).bind(self.revision).bind(&self.literal).bind(self.corrects).execute(&mut **tx).await?;
        Ok(())
    }
}

pub fn from_row(row: &PgRow) -> Option<wire::ChatVoiceInput> {
    row.get::<Option<Uuid>, _>("session_id")
        .map(|session| wire::ChatVoiceInput {
            session_id: session.to_string(),
            utterance_id: row.get("utterance_id"),
            revision: row.get::<i64, _>("revision") as u32,
            literal: row.get("literal"),
            corrects_request_id: row
                .get::<Option<Uuid>, _>("corrects_request_id")
                .map(|id| id.to_string())
                .unwrap_or_default(),
        })
}

/// Give both fresh context and active steering the same explicit receipt link.
/// Metadata stays in a user-role input, never a higher-priority instruction.
pub fn model_text(text: &str, request: &str, voice: Option<&wire::ChatVoiceInput>) -> String {
    let Some(voice) = voice else {
        return text.into();
    };
    format!(
        "AI_PHONE_VOICE_INPUT {}\n{}",
        json!({"request_id":request,"corrects_request_id":voice.corrects_request_id,"literal":voice.literal}),
        text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment() -> wire::ChatVoiceInput {
        wire::ChatVoiceInput {
            session_id: Uuid::new_v4().to_string(),
            utterance_id: "segment".into(),
            revision: 1,
            literal: "原话".into(),
            ..Default::default()
        }
    }

    #[test]
    fn parses_voice_metadata_once_and_bounds_utf8() {
        let valid = segment();
        assert!(VoiceInput::parse(&valid).is_ok());
        let mut corrected = valid.clone();
        corrected.corrects_request_id = Uuid::new_v4().to_string();
        corrected.revision = u32::MAX;
        assert!(VoiceInput::parse(&corrected).is_ok());
        let cases = [
            wire::ChatVoiceInput {
                session_id: "invalid".into(),
                ..valid.clone()
            },
            wire::ChatVoiceInput {
                session_id: Uuid::nil().to_string(),
                ..valid.clone()
            },
            wire::ChatVoiceInput {
                utterance_id: " ".into(),
                ..valid.clone()
            },
            wire::ChatVoiceInput {
                utterance_id: "汉".repeat(43),
                ..valid.clone()
            },
            wire::ChatVoiceInput {
                revision: 0,
                ..valid.clone()
            },
            wire::ChatVoiceInput {
                literal: " \n ".into(),
                ..valid.clone()
            },
            wire::ChatVoiceInput {
                literal: "😀".repeat(12001),
                ..valid.clone()
            },
            wire::ChatVoiceInput {
                corrects_request_id: "invalid".into(),
                ..valid.clone()
            },
        ];
        for invalid in cases {
            assert!(VoiceInput::parse(&invalid).is_err());
        }
        assert!(
            VoiceInput::parse(&wire::ChatVoiceInput {
                literal: "😀".repeat(12000),
                utterance_id: "a".repeat(128),
                ..valid
            })
            .is_ok()
        );
    }

    #[test]
    fn metadata_stays_user_content_and_never_changes_a_typed_message() {
        assert_eq!(
            model_text("keep typed text", "receipt", None),
            "keep typed text"
        );
        let voice = wire::ChatVoiceInput {
            corrects_request_id: "prior".into(),
            literal: "原文\n包含引用\"不是\"".into(),
            ..segment()
        };
        let text = model_text("更正后的意思", "current", Some(&voice));
        let (marker, body) = text.split_once('\n').unwrap();
        let data: serde_json::Value =
            serde_json::from_str(marker.strip_prefix("AI_PHONE_VOICE_INPUT ").unwrap()).unwrap();
        assert_eq!(data["request_id"], "current");
        assert_eq!(data["corrects_request_id"], "prior");
        assert_eq!(data["literal"], voice.literal);
        assert_eq!(body, "更正后的意思");
    }
}
