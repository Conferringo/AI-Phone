//! One owner for the private executable and its independent runtime directories.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) struct Toolchain {
    pub binary: PathBuf,
    environment: Vec<(&'static str, PathBuf)>,
}

impl Toolchain {
    pub fn new(folder: &Path) -> Self {
        let tools = folder.join("tools");
        Self {
            binary: tools.join("bin/mise"),
            environment: [
                ("MISE_DATA_DIR", "mise/data"),
                ("MISE_CACHE_DIR", "mise/cache"),
                ("MISE_CONFIG_DIR", "mise/config"),
                ("CARGO_HOME", "cargo"),
                ("RUSTUP_HOME", "rustup"),
            ]
            .into_iter()
            .map(|(key, suffix)| (key, tools.join(suffix)))
            .collect(),
        }
    }
    pub fn prepare(&self) -> Result<(), String> {
        let bin = self.binary.parent().ok_or("invalid_tool_policy")?;
        fs::create_dir_all(bin).map_err(|error| error.to_string())?;
        let legacy = bin.parent().ok_or("invalid_tool_policy")?.join("mise");
        if legacy.is_file() {
            let destination = if self.binary.exists() {
                self.binary.with_extension("previous")
            } else {
                self.binary.clone()
            };
            if destination.exists() {
                return Err("tool_cache_conflict".into());
            }
            fs::rename(legacy, destination).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    pub fn apply(&self, command: &mut Command) {
        for (key, path) in &self.environment {
            command.env(key, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn executable_and_runtime_directories_can_exist_at_the_same_time() {
        let folder = std::env::temp_dir().join(format!("ai-phone-layout-{}", std::process::id()));
        let layout = Toolchain::new(&folder);
        layout.prepare().unwrap();
        fs::write(&layout.binary, b"fixture executable").unwrap();
        let result = layout
            .environment
            .iter()
            .try_for_each(|(_, path)| fs::create_dir_all(path));
        fs::remove_dir_all(folder).unwrap();
        assert!(
            result.is_ok(),
            "the installed program must not be the parent of its own runtime directories"
        );
    }
    #[test]
    fn old_executable_cache_moves_without_overwriting_new_runtime_or_user_data() {
        for newer in [false, true] {
            let folder = std::env::temp_dir().join(format!(
                "ai-phone-layout-migration-{}-{newer}",
                std::process::id()
            ));
            fs::create_dir_all(folder.join("tools")).unwrap();
            fs::write(folder.join("tools/mise"), b"old cached binary").unwrap();
            fs::create_dir_all(folder.join("user")).unwrap();
            fs::write(folder.join("user/keep"), b"user fixture").unwrap();
            let layout = Toolchain::new(&folder);
            if newer {
                fs::create_dir_all(layout.binary.parent().unwrap()).unwrap();
                fs::write(&layout.binary, b"new cached binary").unwrap();
            }
            layout.prepare().unwrap();
            let old = if newer {
                layout.binary.with_extension("previous")
            } else {
                layout.binary.clone()
            };
            assert_eq!(fs::read(old).unwrap(), b"old cached binary");
            if newer {
                assert_eq!(fs::read(&layout.binary).unwrap(), b"new cached binary");
            }
            for (_, path) in &layout.environment {
                fs::create_dir_all(path).unwrap();
            }
            layout.prepare().unwrap();
            assert_eq!(fs::read(folder.join("user/keep")).unwrap(), b"user fixture");
            fs::remove_dir_all(folder).unwrap();
        }
    }
}
