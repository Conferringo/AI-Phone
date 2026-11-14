use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Settings {
    pub display: String,
    pub home: PathBuf,
    pub width: u32,
    pub height: u32,
}

impl Settings {
    pub fn from_env() -> Self {
        Self {
            display: std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into()),
            home: PathBuf::from("/home/ai"),
            width: 1280,
            height: 800,
        }
    }

    pub fn for_home(home: &Path) -> Self {
        Self {
            home: home.into(),
            ..Self::from_env()
        }
    }

    pub fn files(&self) -> PathBuf {
        self.home.join("Files")
    }
}

impl Settings {
    pub fn environment(&self) -> crate::Result<Vec<(std::ffi::OsString, std::ffi::OsString)>> {
        let mut env = vec![
            ("DISPLAY".into(), self.display.clone().into()),
            ("HOME".into(), self.home.as_os_str().into()),
        ];
        match std::fs::read_to_string("/tmp/ai-runtime/dbus-address") {
            Ok(value) => env.push(("DBUS_SESSION_BUS_ADDRESS".into(), value.into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        Ok(env)
    }
}
