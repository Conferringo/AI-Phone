use crate::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) const ROLES: [&str; 4] = ["workspace", "desktop", "postgres", "redis"];
pub(super) const MAX_ARCHIVE: u64 = 8 * 1024 * 1024 * 1024;
pub(super) const MAX_METADATA: u64 = 8 * 1024 * 1024;
pub(super) const MAX_RAW: u64 = MAX_ARCHIVE + 64 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
pub(super) struct Image {
    pub source_digest: String,
    pub export_digest: String,
    pub config_digest: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub size: u64,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}
#[derive(Deserialize, Serialize)]
pub(super) struct Archive {
    pub path: String,
    #[serde(
        default,
        deserialize_with = "codec",
        skip_serializing_if = "Option::is_none"
    )]
    pub codec: Option<String>,
    pub size: u64,
    pub sha256: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}
fn codec<'de, D: Deserializer<'de>>(input: D) -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(input).map(Some)
}
#[derive(Deserialize, Serialize)]
pub(super) struct Manifest {
    format: u32,
    os: String,
    pub architecture: String,
    redistribution: String,
    pub images: BTreeMap<String, Image>,
    pub archive: Archive,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
pub(super) fn identity(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
pub(super) fn digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(identity)
}
impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let value: Self =
            serde_json::from_slice(bytes).map_err(|_| Error::Invalid("image_manifest"))?;
        let path = match value.format {
            1 => "images.tar.gz",
            2 => "images.tar.xz",
            _ => return Err(Error::Invalid("image_manifest")),
        };
        if value.os != "linux"
            || !matches!(value.architecture.as_str(), "arm64" | "amd64")
            || value.redistribution != "review-pending"
            || value.images.len() != ROLES.len()
            || !ROLES.iter().all(|role| value.images.contains_key(*role))
            || value.archive.path != path
            || !(1..=MAX_ARCHIVE).contains(&value.archive.size)
            || !identity(&value.archive.sha256)
            || ((value.xz() || value.archive.codec.is_some())
                && value.archive.codec.as_deref() != Some(if value.xz() { "xz" } else { "gzip" }))
        {
            return Err(Error::Invalid("image_manifest"));
        }
        for (role, image) in &value.images {
            if ![
                &image.source_digest,
                &image.export_digest,
                &image.config_digest,
            ]
            .into_iter()
            .all(|value| digest(value))
                || image.reference
                    != format!("ai-phone-bundle/{role}:sha-{}", &image.export_digest[7..])
                || !(1..=32 * 1024 * 1024 * 1024).contains(&image.size)
            {
                return Err(Error::Invalid("image_manifest"));
            }
        }
        Ok(value)
    }
    pub fn xz(&self) -> bool {
        self.format == 2
    }
    pub fn environment(&self) -> BTreeMap<String, String> {
        let mut values: BTreeMap<_, _> = self
            .images
            .iter()
            .map(|(role, image)| {
                (
                    format!("AI_PHONE_{}_IMAGE", role.to_uppercase()),
                    image.reference.clone(),
                )
            })
            .collect();
        values.insert("AI_PHONE_IMAGE_PULL_POLICY".into(), "never".into());
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn document() -> Value {
        let id = "a".repeat(64);
        let mut images = json!({});
        for role in ROLES {
            images[role] = json!({"source_digest":format!("sha256:{id}"),"config_digest":format!("sha256:{id}"),"export_digest":format!("sha256:{id}"),"ref":format!("ai-phone-bundle/{role}:sha-{id}"),"size":32_u64*1024*1024*1024});
        }
        json!({"format":1,"os":"linux","architecture":"arm64","redistribution":"review-pending","images":images,"archive":{"path":"images.tar.gz","size":MAX_ARCHIVE,"sha256":id},"extra":"preserved"})
    }
    fn parse(value: &Value) -> Result<Manifest> {
        Manifest::parse(&serde_json::to_vec(value).unwrap())
    }
    #[test]
    fn documented_versions_limits_architectures_and_extra_metadata_are_retained() {
        for architecture in ["arm64", "amd64"] {
            for xz in [false, true] {
                let mut value = document();
                value["architecture"] = json!(architecture);
                if xz {
                    value["format"] = json!(2);
                    value["archive"]["path"] = json!("images.tar.xz");
                    value["archive"]["codec"] = json!("xz");
                }
                let manifest = parse(&value).unwrap();
                assert_eq!(serde_json::to_value(&manifest).unwrap(), value);
                assert_eq!(manifest.environment().len(), 5);
            }
        }
    }
    #[test]
    fn malformed_shapes_types_aliases_codecs_and_limits_are_rejected() {
        let changes = [
            ("/format", json!(true)),
            ("/format", json!(3)),
            ("/os", json!("windows")),
            ("/architecture", json!("x86_64")),
            ("/redistribution", json!("approved")),
            ("/images", json!({})),
            ("/archive/size", json!(0)),
            ("/archive/size", json!(MAX_ARCHIVE + 1)),
            ("/archive/size", json!(true)),
            ("/archive/sha256", json!("bad")),
            ("/archive/path", json!("private/file")),
            ("/archive/codec", Value::Null),
            (
                "/images/desktop/size",
                json!(32_u64 * 1024 * 1024 * 1024 + 1),
            ),
            ("/images/desktop/size", json!(true)),
            ("/images/desktop/source_digest", json!("bad")),
            ("/images/desktop/config_digest", json!("bad")),
            ("/images/desktop/ref", json!("foreign/image:latest")),
        ];
        for (pointer, changed) in changes {
            let mut value = document();
            if pointer == "/archive/codec" {
                value["archive"]["codec"] = changed;
            } else {
                *value.pointer_mut(pointer).unwrap() = changed;
            }
            assert!(parse(&value).is_err(), "{pointer}");
        }
    }
}
