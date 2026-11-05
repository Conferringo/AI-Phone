use crate::{Error, Result};

pub(super) fn component(value: &str) -> bool {
    let Some((package, class)) = value.split_once('/') else {
        return false;
    };
    !package.is_empty()
        && !class.is_empty()
        && !class.contains('/')
        && [package, class].into_iter().all(|s| {
            s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.$".contains(&b))
        })
}
pub(super) fn components(value: &str, ime: bool) -> Result<Vec<&str>> {
    if value.is_empty() || value == "null" {
        return Ok(Vec::new());
    }
    let result = value
        .split(':')
        .map(|v| {
            if ime {
                v.split(';').next().unwrap_or("")
            } else {
                v
            }
        })
        .collect::<Vec<_>>();
    if !result.iter().all(|v| component(v)) {
        return Err(Error::Invalid("phone_input_observation"));
    }
    Ok(result)
}
pub(super) fn contains(rows: &[&str], wanted: &str) -> bool {
    fn expand(value: &str) -> String {
        let (package, class) = value.split_once('/').unwrap_or(("", ""));
        if class.starts_with('.') {
            format!("{package}/{package}{class}")
        } else {
            value.into()
        }
    }
    rows.iter().any(|v| expand(v) == expand(wanted))
}
pub(super) fn package(value: &str, wanted: &str) -> Result<bool> {
    for row in value.lines() {
        let name = row
            .strip_prefix("package:")
            .ok_or(Error::Invalid("phone_input_observation"))?;
        if !name.contains('.')
            || !name.split('.').all(|part| {
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
        {
            return Err(Error::Invalid("phone_input_observation"));
        }
    }
    Ok(value.lines().any(|v| v == format!("package:{wanted}")))
}
