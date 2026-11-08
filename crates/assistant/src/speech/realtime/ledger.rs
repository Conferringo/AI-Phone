//! One transcription session owns turn correlation; audio is never retained here.
use super::*;
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct Ledger {
    turns: VecDeque<Turn>,
    finished: VecDeque<(String, String)>,
    pcm: Pcm24,
}
struct Turn {
    local: String,
    remote: Option<String>,
    committed: bool,
    text: String,
    revision: u32,
}
impl Turn {
    fn update(&mut self, text: String, done: bool) -> Result<wire::VoiceStreamTranscript> {
        if done && (!self.committed || text.trim().is_empty()) {
            return Err("语音服务未完成有效转录".into());
        }
        if done {
            self.text = text;
        } else {
            self.text.push_str(&text);
        }
        if self.text.len() > 48000 {
            return Err("语音转录过长".into());
        }
        self.revision = self.revision.checked_add(1).ok_or("语音转录版本溢出")?;
        Ok(wire::VoiceStreamTranscript {
            utterance_id: self.local.clone(),
            revision: self.revision,
            text: self.text.clone(),
            r#final: done,
        })
    }
}
struct Patch {
    remote: String,
    text: Option<String>,
    done: bool,
}
impl Patch {
    fn parse(event: Value) -> Result<Option<Self>> {
        let kind = event["type"].as_str().unwrap_or("");
        let (field, done) = match kind {
            "error" | "conversation.item.input_audio_transcription.failed" => {
                return Err("语音服务拒绝实时识别，请检查协议、模型与服务权限".into());
            }
            "input_audio_buffer.committed" => (None, false),
            "conversation.item.input_audio_transcription.delta" => (Some("delta"), false),
            "conversation.item.input_audio_transcription.completed" => (Some("transcript"), true),
            _ => return Ok(None),
        };
        let text = field
            .map(|key| {
                event[key]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("语音服务未返回转录")
            })
            .transpose()?;
        Ok(Some(Self {
            remote: event["item_id"].as_str().unwrap_or("").into(),
            text,
            done,
        }))
    }
}
impl Ledger {
    fn begin(&mut self, local: &str) -> Result<()> {
        if self.turns.len() >= 8
            || self.turns.iter().any(|t| t.local == local)
            || self.finished.iter().any(|t| t.0 == local)
        {
            return Err("语音识别积压或片段重复".into());
        }
        self.turns.push_back(Turn {
            local: local.into(),
            remote: None,
            committed: false,
            text: String::new(),
            revision: 0,
        });
        Ok(())
    }
    fn append(&mut self, audio: wire::VoiceStreamAudio) -> Result<Value> {
        if audio.pcm16.is_empty()
            || audio.pcm16.len() > 3200
            || !audio.pcm16.len().is_multiple_of(2)
        {
            return Err("实时音频帧无效".into());
        }
        uuid::Uuid::parse_str(&audio.utterance_id).map_err(|_| "语音片段标识无效")?;
        if self.turns.back().is_none_or(|t| t.committed) {
            self.begin(&audio.utterance_id)?;
        }
        if self
            .turns
            .back()
            .is_none_or(|t| t.local != audio.utterance_id)
        {
            return Err("语音片段交错".into());
        }
        let pcm = self.pcm.push(&audio.pcm16);
        Ok(json!({"type":"input_audio_buffer.append","audio":STANDARD.encode(pcm)}))
    }
    pub(super) fn input(&mut self, event: wire::voice_stream_input::Event) -> Result<Vec<Value>> {
        use wire::voice_stream_input::Event;
        match event {
            Event::Audio(audio) => self.append(audio).map(|v| vec![v]),
            Event::Commit(commit) => {
                let turn = self
                    .turns
                    .back_mut()
                    .filter(|t| !t.committed && t.local == commit.utterance_id)
                    .ok_or("语音片段不能重复提交")?;
                turn.committed = true;
                let tail = self.pcm.finish();
                let mut events = Vec::with_capacity(2);
                if !tail.is_empty() {
                    events.push(
                        json!({"type":"input_audio_buffer.append","audio":STANDARD.encode(tail)}),
                    );
                }
                events.push(json!({"type":"input_audio_buffer.commit"}));
                Ok(events)
            }
            Event::Start(_) => Err("语音会话已经开始".into()),
        }
    }
    fn turn(&mut self, remote: &str) -> Result<Option<usize>> {
        if remote.is_empty() || remote.len() > 256 {
            return Err("语音服务片段标识无效".into());
        }
        if self.finished.iter().any(|t| t.1 == remote) {
            return Ok(None);
        }
        if let Some(index) = self
            .turns
            .iter()
            .position(|t| t.remote.as_deref() == Some(remote))
        {
            return Ok(Some(index));
        }
        let index = self
            .turns
            .iter()
            .position(|t| t.remote.is_none())
            .ok_or("语音服务返回未知片段")?;
        self.turns[index].remote = Some(remote.into());
        Ok(Some(index))
    }
    pub(super) fn output(&mut self, event: Value) -> Result<Option<wire::VoiceStreamTranscript>> {
        let Some(patch) = Patch::parse(event)? else {
            return Ok(None);
        };
        let Some(index) = self.turn(&patch.remote)? else {
            return Ok(None);
        };
        let turn = &mut self.turns[index];
        let Some(text) = patch.text else {
            if !turn.committed {
                return Err("语音服务提前提交片段".into());
            }
            return Ok(None);
        };
        let output = turn.update(text, patch.done)?;
        if patch.done {
            self.finish(index);
        }
        Ok(Some(output))
    }
    fn finish(&mut self, index: usize) {
        let turn = self.turns.remove(index).expect("the indexed turn exists");
        self.finished
            .push_back((turn.local, turn.remote.unwrap_or_default()));
        if self.finished.len() > 32 {
            self.finished.pop_front();
        }
    }
}

/// Retains interpolation phase across arbitrary frame boundaries, not raw audio.
#[derive(Default)]
struct Pcm24 {
    previous: Option<i32>,
    phase: u8,
}
impl Pcm24 {
    fn push(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(bytes.len() * 3 / 2 + 2);
        for sample in bytes.chunks_exact(2) {
            let right = i16::from_le_bytes([sample[0], sample[1]]) as i32;
            if let Some(left) = self.previous {
                while self.phase <= 3 {
                    let phase = i32::from(self.phase);
                    let value = ((left * (3 - phase) + right * phase) / 3) as i16;
                    output.extend_from_slice(&value.to_le_bytes());
                    self.phase += 2;
                }
                self.phase -= 3;
            } else {
                output.extend_from_slice(&(right as i16).to_le_bytes());
                self.phase = 2;
            }
            self.previous = Some(right);
        }
        output
    }
    fn finish(&mut self) -> Vec<u8> {
        let tail = if self.phase == 1 {
            self.previous
                .map(|value| (value as i16).to_le_bytes().to_vec())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        *self = Self::default();
        tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn audio(id: &str) -> wire::voice_stream_input::Event {
        wire::voice_stream_input::Event::Audio(wire::VoiceStreamAudio {
            utterance_id: id.into(),
            pcm16: [0x10, 0x20].repeat(800),
        })
    }
    fn commit(ledger: &mut Ledger, id: &str) {
        ledger
            .input(wire::voice_stream_input::Event::Commit(
                wire::VoiceStreamCommit {
                    utterance_id: id.into(),
                },
            ))
            .unwrap();
    }
    #[test]
    fn resampling_does_not_depend_on_pcm_frame_boundaries() {
        let pcm: Vec<u8> = [-8000i16, 4000, -2000, 8000, 1000, -5000]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect();
        let mut a = Pcm24::default();
        let mut b = Pcm24::default();
        let whole: Vec<u8> = a.push(&pcm).into_iter().chain(a.finish()).collect();
        let mut split: Vec<u8> = pcm.chunks(2).flat_map(|chunk| b.push(chunk)).collect();
        split.extend(b.finish());
        assert_eq!(
            whole, split,
            "a continuous stream must not extrapolate at every frame boundary"
        );
    }
    #[test]
    fn deltas_precede_commit_and_out_of_order_finals_keep_their_turn() {
        let mut ledger = Ledger::default();
        let a = Uuid::new_v4().to_string();
        let b = Uuid::new_v4().to_string();
        let append = ledger.input(audio(&a)).unwrap();
        assert_eq!(
            STANDARD
                .decode(append[0]["audio"].as_str().unwrap())
                .unwrap(),
            [0x10, 0x20].repeat(1199)
        );
        let partial=ledger.output(json!({"type":"conversation.item.input_audio_transcription.delta","item_id":"a","delta":"打开"})).unwrap().unwrap();
        assert_eq!(partial.utterance_id, a);
        assert!(!partial.r#final);
        commit(&mut ledger, &a);
        ledger
            .output(json!({"type":"input_audio_buffer.committed","item_id":"a"}))
            .unwrap();
        ledger.input(audio(&b)).unwrap();
        commit(&mut ledger, &b);
        ledger
            .output(json!({"type":"input_audio_buffer.committed","item_id":"b"}))
            .unwrap();
        for (remote, local) in [("b", &b), ("a", &a)] {
            let final_=ledger.output(json!({"type":"conversation.item.input_audio_transcription.completed","item_id":remote,"transcript":"打开天气"})).unwrap().unwrap();
            assert_eq!(&final_.utterance_id, local);
            assert!(final_.r#final);
        }
        assert!(ledger.output(json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"a","transcript":"duplicate"})).unwrap().is_none());
        assert!(ledger.input(audio(&a)).is_err());
    }
    #[test]
    fn invalid_frames_interleaving_duplicate_commit_and_unfinished_results_fail() {
        let mut ledger = Ledger::default();
        let a = Uuid::new_v4().to_string();
        let b = Uuid::new_v4().to_string();
        assert!(
            ledger
                .input(wire::voice_stream_input::Event::Audio(
                    wire::VoiceStreamAudio {
                        utterance_id: a.clone(),
                        pcm16: vec![1]
                    }
                ))
                .is_err()
        );
        ledger.input(audio(&a)).unwrap();
        assert!(ledger.input(audio(&b)).is_err());
        assert!(ledger.output(json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"a","transcript":"early"})).is_err());
        commit(&mut ledger, &a);
        assert!(
            ledger
                .input(wire::voice_stream_input::Event::Commit(
                    wire::VoiceStreamCommit { utterance_id: a }
                ))
                .is_err()
        );
    }
}
