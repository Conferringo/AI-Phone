use super::{Config, Result};
use std::{collections::BTreeMap, fs, path::PathBuf};
use tonic::{
    Request,
    metadata::{Ascii, MetadataValue},
    transport::Channel,
};
pub mod wire {
    tonic::include_proto!("android.emulation.control");
}
use wire::emulator_controller_client::EmulatorControllerClient;

#[derive(Clone)]
pub struct Session {
    pub client: EmulatorControllerClient<Channel>,
    token: MetadataValue<Ascii>,
}
impl Session {
    pub fn request<T>(&self, body: T) -> Request<T> {
        let mut request = Request::new(body);
        request
            .metadata_mut()
            .insert("authorization", self.token.clone());
        request
    }
    pub fn command<T>(&self, body: T) -> Request<T> {
        let mut request = self.request(body);
        request.set_timeout(std::time::Duration::from_secs(5));
        request
    }
}
pub fn session(config: &Config, channel: &Channel) -> Result<Session> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let roots = if cfg!(target_os = "macos") {
        vec![home.join("Library/Caches/TemporaryItems")]
    } else {
        let mut roots = Vec::new();
        for name in [
            "XDG_RUNTIME_DIR",
            "ANDROID_EMULATOR_HOME",
            "ANDROID_PREFS_ROOT",
            "ANDROID_SDK_HOME",
        ] {
            if let Some(value) = std::env::var_os(name) {
                roots.push(PathBuf::from(value));
            }
        }
        roots.extend([
            PathBuf::from(format!("/run/user/{}", rustix::process::getuid().as_raw())),
            home.join(".android"),
        ]);
        roots
    };
    let mut entries = Vec::new();
    for root in roots {
        if let Ok(directory) = fs::read_dir(root.join("avd/running")) {
            for entry in directory {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("pid_") && name.ends_with(".ini") {
                    let metadata = entry.metadata()?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        if metadata.uid() != rustix::process::getuid().as_raw() {
                            continue;
                        }
                    }
                    entries.push((metadata.modified()?, entry.path()));
                }
            }
        }
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (_, path) in entries {
        let text = fs::read_to_string(path)?;
        let values: BTreeMap<_, _> = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        if values.get("grpc.port").copied() != Some(config.grpc_port.to_string().as_str()) {
            continue;
        }
        if let Some(serial) = values.get("port.serial")
            && format!("emulator-{serial}") != config.serial
        {
            continue;
        }
        let token = values
            .get("grpc.token")
            .ok_or("emulator authentication unavailable")?;
        return Ok(Session {
            client: EmulatorControllerClient::new(channel.clone())
                .max_decoding_message_size(12 * 1024 * 1024),
            token: format!("Bearer {token}").parse()?,
        });
    }
    Err("emulator unavailable".into())
}
