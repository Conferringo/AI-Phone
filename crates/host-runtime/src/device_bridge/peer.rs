//! Guest lifetime and private discovery for the existing SDK media consumer.
use super::{Bridge, config::Config};
use crate::{Error, Result, filesystem, native_ipc};
use ai_phone_api::wire::{
    DeviceBridgePeerOwner, DeviceBridgePhone, DeviceBridgeRole, DeviceBridgeStarted,
};
use prost::Message;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tokio::sync::watch;

pub const CONTRACT: &str = "ai-phone-phone-peer-v2";
const LIMIT: u64 = 16384;
struct Claim {
    initial_owner: bool,
    advertisement: Option<Vec<u8>>,
}
struct Plan {
    request: DeviceBridgePhone,
    home: PathBuf,
    owner: DeviceBridgePeerOwner,
}
impl Plan {
    fn parse(mut request: DeviceBridgePhone) -> Result<(Self, Config)> {
        let config = Config::try_from(
            request
                .transport
                .take()
                .ok_or(Error::Invalid("phone_peer_configuration"))?,
        )?;
        if config.role != DeviceBridgeRole::Guest
            || !valid_serial(&request.serial)
            || !(1..=8192).contains(&request.grpc_token.len())
            || !request.grpc_token.bytes().all(|v| v.is_ascii_graphic())
        {
            return Err(Error::Invalid("phone_peer_configuration"));
        }
        let home = runtime_home(&request.runtime_directory)?;
        let owner = DeviceBridgePeerOwner {
            version: 1,
            bridge_id: config.id.clone(),
            secret: config.secret.clone(),
        };
        Ok((
            Self {
                request,
                home,
                owner,
            },
            config,
        ))
    }
    fn owner_path(&self) -> PathBuf {
        self.home.join(".phone-peer.pb")
    }
    fn advertisement(&self) -> PathBuf {
        self.home.join("avd/running/pid_0.ini")
    }
    fn marker(&self) -> String {
        format!("aiphone.bridge={}\n", self.owner.bridge_id)
    }
    fn claim(&self) -> Result<(filesystem::Lease, Claim)> {
        let lease = filesystem::lease(&self.home.join(".phone-peer.lock"))?.ok_or(Error::Busy)?;
        let initial = self.initial_owner()?;
        let advertisement = self.read_advertisement()?;
        if let Some(bytes) = &advertisement
            && (initial || !bytes.starts_with(self.marker().as_bytes()))
        {
            return Err(Error::Invalid("phone_peer_foreign_metadata"));
        }
        Ok((
            lease,
            Claim {
                initial_owner: initial,
                advertisement,
            },
        ))
    }
    fn initial_owner(&self) -> Result<bool> {
        let Some(bytes) = read_optional(&self.owner_path())? else {
            return Ok(true);
        };
        let owner = DeviceBridgePeerOwner::decode(bytes.as_slice())
            .map_err(|_| Error::Invalid("phone_peer_owner"))?;
        if owner != self.owner {
            return Err(Error::Invalid("phone_peer_owner_conflict"));
        }
        Ok(false)
    }
    fn publish(&self, transport: &DeviceBridgeStarted, claim: Claim) -> Result<Vec<u8>> {
        for path in [self.home.join("avd"), self.home.join("avd/running")] {
            filesystem::directory(&path, false)?;
        }
        if self.read_advertisement()? != claim.advertisement {
            return Err(Error::Invalid("phone_peer_foreign_metadata"));
        }
        let metadata = self.metadata(transport);
        if claim.initial_owner {
            publish(&self.owner_path(), &self.owner.encode_to_vec(), true)?;
        }
        // An absent advertisement stays a create-only publication, even when
        // this owner is resuming. A failed publication must never clean up a
        // file another writer may have created (even with identical bytes).
        publish(
            &self.advertisement(),
            &metadata,
            claim.advertisement.is_none(),
        )?;
        Ok(metadata)
    }
    fn metadata(&self, transport: &DeviceBridgeStarted) -> Vec<u8> {
        format!(
            "{}port.serial={}\ngrpc.port={}\ngrpc.token={}\n",
            self.marker(),
            &self.request.serial[9..],
            transport.emulator_port,
            self.request.grpc_token
        )
        .into_bytes()
    }
    fn read_advertisement(&self) -> Result<Option<Vec<u8>>> {
        for path in [self.home.join("avd"), self.home.join("avd/running")] {
            match fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.into()),
                Ok(_) => filesystem::existing_directory(&path)?,
            }
        }
        read_optional(&self.advertisement())
    }
}
fn runtime_home(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if !path.is_absolute() {
        return Err(Error::Invalid("phone_peer_directory"));
    }
    filesystem::existing_directory(path)?;
    Ok(path.canonicalize()?)
}
fn valid_serial(value: &str) -> bool {
    value
        .strip_prefix("emulator-")
        .and_then(|v| v.parse::<u16>().ok())
        .is_some_and(|v| {
            (5554..=5682).contains(&v) && v.is_multiple_of(2) && value == format!("emulator-{v}")
        })
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
        Ok(_) => filesystem::bounded_file_with_limit(path, LIMIT).map(Some),
    }
}
fn publish(path: &Path, bytes: &[u8], create_new: bool) -> Result<()> {
    let parent = path
        .parent()
        .ok_or(Error::Invalid("phone_peer_directory"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    if create_new {
        file.persist_noclobber(path)
    } else {
        file.persist(path)
    }
    .map_err(|e| Error::Io(e.error))?;
    filesystem::sync_directory(parent)
}
pub(super) struct PhonePeer {
    bridge: Bridge,
    plan: Plan,
    metadata: Vec<u8>,
    _lease: filesystem::Lease,
}
impl PhonePeer {
    pub async fn bind(request: DeviceBridgePhone) -> Result<Self> {
        let (plan, config) = Plan::parse(request)?;
        let (lease, claim) = plan.claim()?;
        let bridge = Bridge::bind_config(config).await?;
        let metadata = plan.publish(bridge.receipt(), claim)?;
        Ok(Self {
            bridge,
            plan,
            metadata,
            _lease: lease,
        })
    }
    pub fn receipt(&self) -> &DeviceBridgeStarted {
        self.bridge.receipt()
    }
    pub async fn serve(self, stop: watch::Receiver<bool>) -> Result<()> {
        let result = self.bridge.serve(stop).await;
        let cleanup = remove_advertisement(&self.plan, &self.metadata);
        cleanup.and(result)
    }
}
fn remove_advertisement(plan: &Plan, expected: &[u8]) -> Result<()> {
    let path = plan.advertisement();
    let bytes = plan
        .read_advertisement()
        .map_err(|_| Error::Invalid("phone_peer_cleanup_unknown"))?;
    let Some(bytes) = bytes else {
        return Ok(());
    };
    if bytes != expected {
        return Err(Error::Invalid("phone_peer_cleanup_unknown"));
    }
    fs::remove_file(&path)?;
    filesystem::sync_directory(
        path.parent()
            .ok_or(Error::Invalid("phone_peer_directory"))?,
    )
}
pub async fn run(arguments: &[std::ffi::OsString]) -> Result<()> {
    if !arguments.is_empty() {
        return Err(Error::Invalid("phone_peer_arguments"));
    }
    let request: DeviceBridgePhone =
        native_ipc::read_bounded(&mut tokio::io::stdin(), LIMIT as usize).await?;
    let input = if request.owner_pipe {
        Some(owner_input()?)
    } else {
        None
    };
    let peer = PhonePeer::bind(request).await?;
    let reply = native_ipc::write(&mut tokio::io::stdout(), peer.receipt()).await;
    if let Err(error) = reply {
        let (_end, stop) = watch::channel(true);
        peer.serve(stop).await?;
        return Err(error.into());
    }
    let (end, stop) = watch::channel(false);
    let control = input.map(|mut input| {
        let end = end.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            // The owner keeps an idle pipe open. EOF, an I/O failure or a
            // control byte ends ownership; none can leave a detached peer.
            let _ = input.read_u8().await;
            let _ = end.send(true);
        })
    });
    let signal_end = end.clone();
    let signal = tokio::spawn(async move {
        if crate::component::interrupt().await.is_ok() {
            let _ = signal_end.send(true);
        }
    });
    let result = peer.serve(stop).await;
    signal.abort();
    let _ = signal.await;
    if let Some(control) = control {
        control.abort();
        let _ = control.await;
    }
    result
}

#[cfg(unix)]
fn owner_input() -> Result<tokio::net::unix::pipe::Receiver> {
    use std::os::fd::AsFd;
    // A cancellable pipe avoids Tokio's blocking stdin worker keeping the
    // process alive after SIGTERM while the native owner still holds stdin.
    let input = std::io::stdin().as_fd().try_clone_to_owned()?;
    Ok(tokio::net::unix::pipe::Receiver::from_owned_fd(input)?)
}
#[cfg(not(unix))]
fn owner_input() -> Result<tokio::io::Empty> {
    // Managed peers are Linux/Unix guests; the Windows side owns Connection.
    Err(Error::Invalid("phone_peer_owner_pipe_unsupported"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_phone_api::wire::DeviceBridgeConfig;

    fn fixture(root: &Path) -> (Plan, DeviceBridgeStarted) {
        let id = "a".repeat(32);
        let (plan, _) = Plan::parse(DeviceBridgePhone {
            transport: Some(DeviceBridgeConfig {
                version: 1,
                role: DeviceBridgeRole::Guest as i32,
                bridge_id: id.clone(),
                secret: vec![7; 32],
                endpoint: "127.0.0.1:23000".into(),
                ..Default::default()
            }),
            runtime_directory: root.to_str().unwrap().into(),
            serial: "emulator-5564".into(),
            grpc_token: "test-token".into(),
            owner_pipe: false,
        })
        .unwrap();
        (
            plan,
            DeviceBridgeStarted {
                version: 1,
                role: DeviceBridgeRole::Guest as i32,
                bridge_id: id,
                endpoint: "127.0.0.1:23000".into(),
                adb_port: 15037,
                emulator_port: 8558,
            },
        )
    }
    #[test]
    fn owner_publication_preserves_a_file_created_after_the_claim() {
        let root = tempfile::tempdir().unwrap();
        let (plan, receipt) = fixture(root.path());
        let (_lease, initial) = plan.claim().unwrap();
        std::fs::write(plan.owner_path(), b"manual owner").unwrap();
        assert!(plan.publish(&receipt, initial).is_err());
        assert_eq!(std::fs::read(plan.owner_path()).unwrap(), b"manual owner");
        assert!(!plan.advertisement().exists());
    }
    #[test]
    fn advertisement_changed_during_pairing_is_preserved() {
        for (owned, advertised) in [(false, false), (true, false), (true, true)] {
            let root = tempfile::tempdir().unwrap();
            let (plan, receipt) = fixture(root.path());
            filesystem::directory(&plan.home.join("avd"), false).unwrap();
            filesystem::directory(&plan.home.join("avd/running"), false).unwrap();
            if owned {
                std::fs::write(plan.owner_path(), plan.owner.encode_to_vec()).unwrap();
            }
            if advertised {
                std::fs::write(plan.advertisement(), plan.metadata(&receipt)).unwrap();
            }
            let (_lease, initial) = plan.claim().unwrap();
            let changed = b"manual replacement during authentication\n";
            std::fs::write(plan.advertisement(), changed).unwrap();
            assert!(
                plan.publish(&receipt, initial).is_err(),
                "owned={owned},advertised={advertised}"
            );
            assert_eq!(std::fs::read(plan.advertisement()).unwrap(), changed);
        }
    }
    #[test]
    fn unchanged_claims_publish_new_and_resumed_discovery_metadata() {
        for (owned, advertised) in [(false, false), (true, false), (true, true)] {
            let root = tempfile::tempdir().unwrap();
            let (plan, mut receipt) = fixture(root.path());
            filesystem::directory(&plan.home.join("avd"), false).unwrap();
            filesystem::directory(&plan.home.join("avd/running"), false).unwrap();
            if owned {
                std::fs::write(plan.owner_path(), plan.owner.encode_to_vec()).unwrap();
            }
            if advertised {
                std::fs::write(plan.advertisement(), plan.metadata(&receipt)).unwrap();
            }
            let (_lease, claim) = plan.claim().unwrap();
            receipt.emulator_port += 2;
            let metadata = plan.publish(&receipt, claim).unwrap();
            assert_eq!(metadata, plan.metadata(&receipt));
            assert_eq!(std::fs::read(plan.advertisement()).unwrap(), metadata);
            assert_eq!(
                std::fs::read(plan.owner_path()).unwrap(),
                plan.owner.encode_to_vec()
            );
        }
    }
    #[test]
    fn removed_or_edited_owned_advertisement_invalidates_the_claim() {
        for removed in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let (plan, receipt) = fixture(root.path());
            filesystem::directory(&plan.home.join("avd"), false).unwrap();
            filesystem::directory(&plan.home.join("avd/running"), false).unwrap();
            std::fs::write(plan.owner_path(), plan.owner.encode_to_vec()).unwrap();
            std::fs::write(plan.advertisement(), plan.metadata(&receipt)).unwrap();
            let (_lease, claim) = plan.claim().unwrap();
            if removed {
                std::fs::remove_file(plan.advertisement()).unwrap();
            } else {
                std::fs::write(
                    plan.advertisement(),
                    format!("{}edited=true\n", plan.marker()),
                )
                .unwrap();
            }
            let expected = plan.read_advertisement().unwrap();
            assert!(plan.publish(&receipt, claim).is_err());
            assert_eq!(plan.read_advertisement().unwrap(), expected);
        }
    }
}
