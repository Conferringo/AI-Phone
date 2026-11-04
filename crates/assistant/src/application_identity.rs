//! Stable application keys, independent of private window/document titles.
pub(crate) fn phone(foreground: &str) -> Option<&str> {
    let value = foreground.split('/').next()?.trim();
    (value.len() < 180
        && value.contains('.')
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.as_bytes()[0].is_ascii_alphabetic())
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._".contains(&c)))
    .then_some(value)
}
fn class(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}
pub(crate) fn desktop(wm_class: &str) -> Option<String> {
    class(wm_class).then(|| format!("linux:{}", wm_class.to_ascii_lowercase()))
}
pub(crate) fn valid(value: &str) -> bool {
    match value.strip_prefix("linux:") {
        Some(value) => class(value) && !value.bytes().any(|c| c.is_ascii_uppercase()),
        None => phone(value) == Some(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_use_machine_fields_and_distinct_os_namespaces() {
        assert_eq!(
            desktop("LibreOffice-Writer").as_deref(),
            Some("linux:libreoffice-writer")
        );
        assert!(valid("linux:libreoffice-writer"));
        assert!(!valid("linux:LibreOffice-Writer"));
        assert!(desktop("private-document.odt — LibreOffice Writer").is_none());
        assert!(desktop("/home/ai/private").is_none());
        assert!(desktop("").is_none());
        assert_eq!(phone("com.tencent.mm/.Home"), Some("com.tencent.mm"));
        assert!(!valid("com.tencent.mm/.Home"));
    }
}
