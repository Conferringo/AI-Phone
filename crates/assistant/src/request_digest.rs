use sha2::{Digest, Sha256, Sha512};

pub fn encode(bytes: &[u8]) -> String {
    format!("{:x}", Sha512::digest(bytes))
}

pub fn matches(stored: &str, current: &str, bytes: &[u8]) -> bool {
    stored == current || (stored.len() == 64 && stored == format!("{:x}", Sha256::digest(bytes)))
}
