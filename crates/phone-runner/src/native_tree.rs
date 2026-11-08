//! One live, DUMP-protected accessibility reply; never a cached file.
use base64::{Engine, engine::general_purpose::STANDARD};

const MAX_XML: usize = 262_144;

pub(super) fn command(request: &str) -> Vec<String> {
    vec![
        "shell".into(),
        format!(
            "am broadcast --receiver-foreground --user 0 -a dev.aiphone.input.DUMP_UI -n dev.aiphone.input/.UiTreeReceiver --es request_id {request}"
        ),
    ]
}

pub(super) fn from_output(output: &[u8], request: &str) -> Option<String> {
    let prefix = format!("Broadcast completed: result=-1, data=\"{request}:");
    let text = std::str::from_utf8(output).ok()?;
    let encoded = text.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .and_then(|value| value.strip_suffix('"'))
    })?;
    if encoded.len() > MAX_XML.div_ceil(3) * 4 {
        return None;
    }
    let bytes = STANDARD.decode(encoded).ok()?;
    if bytes.len() > MAX_XML {
        return None;
    }
    let xml = String::from_utf8(bytes).ok()?;
    let document = roxmltree::Document::parse(&xml).ok()?;
    (document.root_element().tag_name().name() == "hierarchy").then_some(xml)
}

#[cfg(test)]
mod tests {
    use super::*;
    const REQUEST: &str = "e7517f0be3854ed9852193959e102dec";
    fn response(xml: &str) -> Vec<u8> {
        format!(
            "Broadcast completed: result=-1, data=\"{REQUEST}:{}\"\n",
            STANDARD.encode(xml)
        )
        .into_bytes()
    }
    #[test]
    fn accepts_only_the_current_complete_hierarchy() {
        let xml = "<?xml version=\"1.0\"?><hierarchy><node text=\"当前\" /></hierarchy>";
        assert_eq!(from_output(&response(xml), REQUEST).as_deref(), Some(xml));
        assert!(from_output(&response(xml), "older-request").is_none());
        assert!(from_output(&response("<hierarchy><node"), REQUEST).is_none());
        assert!(from_output(&response("<other />"), REQUEST).is_none());
        assert!(from_output(b"Broadcast completed: result=0\n", REQUEST).is_none());
    }
    #[test]
    fn rejects_broken_transport_and_oversized_trees() {
        assert!(from_output(b"\xff", REQUEST).is_none());
        assert!(from_output(&response(&"x".repeat(MAX_XML + 1)), REQUEST).is_none());
        let invalid = format!("Broadcast completed: result=-1, data=\"{REQUEST}:!!!\"");
        assert!(from_output(invalid.as_bytes(), REQUEST).is_none());
        assert!(from_output(&response("<hierarchy />"), "").is_none());
    }
    #[test]
    fn accepts_the_size_boundary_and_rejects_a_larger_valid_document() {
        let xml = format!(
            "<hierarchy>{}</hierarchy>",
            " ".repeat(MAX_XML - "<hierarchy></hierarchy>".len())
        );
        assert_eq!(
            from_output(&response(&xml), REQUEST).unwrap().len(),
            MAX_XML
        );
        let larger = xml.replacen("<hierarchy>", "<hierarchy> ", 1);
        assert!(from_output(&response(&larger), REQUEST).is_none());
    }
}
