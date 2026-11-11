//! Bounded SSE framing shared by model protocols. EOF never dispatches a partial event.
const MAX_TOTAL: usize = 32 * 1024 * 1024;
const MAX_EVENT: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct SseDecoder {
    line: Vec<u8>,
    data: String,
    total: usize,
    skip_lf: bool,
    started: bool,
}

impl SseDecoder {
    pub(crate) fn feed(&mut self, chunk: &[u8]) -> Result<Vec<String>, String> {
        self.total = self.total.saturating_add(chunk.len());
        if self.total > MAX_TOTAL {
            return Err("模型输出超过限制".into());
        }
        let mut events = Vec::new();
        for &byte in chunk {
            if std::mem::take(&mut self.skip_lf) && byte == b'\n' {
                continue;
            }
            if matches!(byte, b'\r' | b'\n') {
                self.finish_line(&mut events)?;
                self.skip_lf = byte == b'\r';
            } else {
                self.line.push(byte);
                if self.line.len() > MAX_EVENT {
                    return Err("模型流单帧超过限制".into());
                }
            }
        }
        Ok(events)
    }

    fn finish_line(&mut self, events: &mut Vec<String>) -> Result<(), String> {
        let line = std::str::from_utf8(&self.line).map_err(|_| "模型流编码无效")?;
        let line = if self.started {
            line
        } else {
            self.started = true;
            line.strip_prefix('\u{feff}').unwrap_or(line)
        };
        if line.is_empty() {
            if !self.data.is_empty() {
                self.data.pop(); // SSE appends one LF per data field.
                events.push(std::mem::take(&mut self.data));
            }
        } else {
            let (field, value) = line.split_once(':').unwrap_or((line, ""));
            if field == "data" {
                let value = value.strip_prefix(' ').unwrap_or(value);
                if self.data.len().saturating_add(value.len() + 1) > MAX_EVENT {
                    return Err("模型流单帧超过限制".into());
                }
                self.data.push_str(value);
                self.data.push('\n');
            }
        }
        self.line.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cr_lf_crlf_bom_comments_and_fragmented_unicode_follow_event_boundaries() {
        let bytes =
            "\u{feff}: heartbeat\r\nevent: message\rdata: 成功\r\ndata: receipt\r\rdata: next\n\n"
                .as_bytes();
        let mut decoder = SseDecoder::default();
        let mut events = Vec::new();
        for byte in bytes {
            events.extend(decoder.feed(&[*byte]).unwrap());
        }
        assert_eq!(events, ["成功\nreceipt", "next"]);
    }
    #[test]
    fn incomplete_events_and_ignored_fields_do_not_emit_output() {
        let mut decoder = SseDecoder::default();
        assert!(
            decoder
                .feed(b"id: 1\nretry: 50\n\ndata: pending\n")
                .unwrap()
                .is_empty()
        );
        assert_eq!(decoder.feed(b"\n").unwrap(), ["pending"]);
        assert_eq!(decoder.feed(b"data\n\n").unwrap(), [""]);
    }
    #[test]
    fn invalid_utf8_and_total_output_limit_are_not_silently_accepted() {
        assert!(SseDecoder::default().feed(b"data: \xff\n\n").is_err());
        let mut decoder = SseDecoder {
            total: MAX_TOTAL,
            ..Default::default()
        };
        assert!(decoder.feed(b"").unwrap().is_empty());
        assert!(decoder.feed(b"x").is_err());
    }
    #[test]
    fn a_bom_is_only_ignored_at_the_beginning_of_the_stream() {
        let mut decoder = SseDecoder::default();
        assert_eq!(
            decoder
                .feed("data: ok\n\n\u{feff}data: ignored\n\n".as_bytes())
                .unwrap(),
            ["ok"]
        );
    }
    #[test]
    fn exact_line_and_multiline_event_budgets_are_enforced() {
        let mut line = SseDecoder::default();
        assert!(line.feed(&vec![b'x'; MAX_EVENT]).unwrap().is_empty());
        assert!(line.feed(b"x").is_err());
        let half = "x".repeat(MAX_EVENT / 2 - 1);
        for overflow in [false, true] {
            let mut decoder = SseDecoder::default();
            for chunk in [
                b"data: ".as_slice(),
                half.as_bytes(),
                b"\ndata: ",
                half.as_bytes(),
                b"\n",
            ] {
                assert!(decoder.feed(chunk).unwrap().is_empty());
            }
            if overflow {
                assert!(decoder.feed(b"data:\n").is_err());
            } else {
                assert_eq!(decoder.feed(b"\n").unwrap()[0].len(), MAX_EVENT - 1);
            }
        }
    }
}
