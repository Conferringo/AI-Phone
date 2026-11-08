//! One native owner for a bridge and its guest process; no model/action policy.
use super::{Bridge, config::Config};
use crate::{Error, Result, native_ipc, vm_control};
use ai_phone_api::wire::{
    DeviceBridgeConfig, DeviceBridgePhone, DeviceBridgeRole, DeviceBridgeStarted, LinuxVmOperation,
};
use prost::Message;
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::Path,
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    process::{Child, ChildStdin, Command},
    sync::watch,
    task::JoinHandle,
};

pub struct Connection {
    peer: Peer,
    end: watch::Sender<bool>,
    bridge: Option<JoinHandle<Result<()>>>,
}

pub(crate) enum Guest<'a> {
    Wsl(Command),
    Vm { port: u16, key: &'a Path },
}

enum Peer {
    Process {
        child: Child,
        input: Option<ChildStdin>,
    },
    Stream(TcpStream),
}

impl Connection {
    pub(crate) async fn start(
        mut guest: DeviceBridgePhone,
        address: Ipv4Addr,
        launch: Guest<'_>,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<Self> {
        if *stop.borrow() {
            return Err(Error::Cancelled);
        }
        let config = guest
            .transport
            .as_mut()
            .ok_or(Error::Invalid("phone_peer_configuration"))?;
        let host = Bridge::bind(DeviceBridgeConfig {
            version: config.version,
            role: DeviceBridgeRole::Host as i32,
            bridge_id: config.bridge_id.clone(),
            secret: config.secret.clone(),
            endpoint: SocketAddr::from((address, 0)).to_string(),
            adb_port: 0,
            emulator_port: 0,
        })
        .await?;
        config.endpoint = match &launch {
            Guest::Wsl(_) => host.receipt().endpoint.clone(),
            Guest::Vm { .. } => {
                let address: SocketAddr = host
                    .receipt()
                    .endpoint
                    .parse()
                    .map_err(|_| Error::Invalid("phone_peer_configuration"))?;
                SocketAddr::from((Ipv4Addr::new(10, 0, 2, 2), address.port())).to_string()
            }
        };
        let checked = Config::try_from(config.clone())?;
        if checked.role != DeviceBridgeRole::Guest {
            return Err(Error::Invalid("phone_peer_configuration"));
        }
        let expected = DeviceBridgeStarted {
            version: 1,
            role: DeviceBridgeRole::Guest as i32,
            bridge_id: checked.id,
            endpoint: checked.endpoint.to_string(),
            adb_port: checked.ports[0].into(),
            emulator_port: checked.ports[1].into(),
        };
        guest.owner_pipe = true;
        let (end, receiver) = watch::channel(false);
        let bridge = tokio::spawn(host.serve(receiver));
        match launch_peer(launch, &guest, &expected, stop).await {
            Ok(peer) => Ok(Self {
                peer,
                end,
                bridge: Some(bridge),
            }),
            Err(error) => {
                let _ = end.send(true);
                let _ = bridge.await;
                Err(error)
            }
        }
    }
    pub async fn wait(&mut self) -> Result<()> {
        let bridge = self
            .bridge
            .as_mut()
            .ok_or(Error::Invalid("device_bridge_worker"))?;
        tokio::select! {
            result=self.peer.wait()=>result,
            result=bridge=>{
                self.bridge=None;
                result.map_err(|_|Error::Invalid("device_bridge_worker"))??;
                Err(Error::Invalid("device_bridge_worker"))
            }
        }
    }
    pub async fn shutdown(&mut self) -> Result<()> {
        let peer = self.peer.shutdown().await;
        let _ = self.end.send(true);
        let bridge = if let Some(task) = self.bridge.take() {
            task.await
                .map_err(|_| Error::Invalid("device_bridge_worker"))?
        } else {
            Ok(())
        };
        peer.and(bridge)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.end.send(true);
    }
}

async fn launch_peer(
    launch: Guest<'_>,
    guest: &DeviceBridgePhone,
    expected: &DeviceBridgeStarted,
    stop: &mut watch::Receiver<bool>,
) -> Result<Peer> {
    match launch {
        Guest::Wsl(mut command) => {
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()?;
            let mut input = child
                .stdin
                .take()
                .ok_or(Error::Invalid("phone_peer_start_unknown"))?;
            let mut output = child
                .stdout
                .take()
                .ok_or(Error::Invalid("phone_peer_start_unknown"))?;
            let result = bounded_start(stop, async {
                native_ipc::write_bounded(&mut input, guest, 16384).await?;
                let reply = native_ipc::read_bounded(&mut output, 512).await?;
                verify_receipt(reply, expected)
            })
            .await;
            let mut peer = Peer::Process {
                child,
                input: Some(input),
            };
            if let Err(error) = result {
                peer.shutdown().await?;
                return Err(error);
            }
            Ok(peer)
        }
        Guest::Vm { port, key } => {
            bounded_start(stop, async {
                let (stream, bytes) = vm_control::open(
                    port,
                    key,
                    LinuxVmOperation::PhonePeer,
                    guest.encode_to_vec(),
                )
                .await?;
                let reply = DeviceBridgeStarted::decode(bytes.as_slice())
                    .map_err(|_| Error::Invalid("phone_peer_start_receipt"))?;
                verify_receipt(reply, expected)?;
                Ok(Peer::Stream(stream))
            })
            .await
        }
    }
}

fn verify_receipt(reply: DeviceBridgeStarted, expected: &DeviceBridgeStarted) -> Result<()> {
    (&reply == expected)
        .then_some(())
        .ok_or(Error::Invalid("phone_peer_start_receipt"))
}

async fn bounded_start<T>(
    stop: &mut watch::Receiver<bool>,
    exchange: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! { biased;
        _=stop.changed()=>Err(Error::Cancelled),
        result=tokio::time::timeout(Duration::from_secs(15),exchange)=>result.map_err(|_|Error::Invalid("phone_peer_start_unknown"))?,
    }
}

impl Peer {
    async fn wait(&mut self) -> Result<()> {
        match self {
            Self::Process { child, .. } => {
                child.wait().await?;
            }
            Self::Stream(stream) => {
                let _ = stream.read_u8().await;
            }
        }
        Err(Error::Invalid("phone_peer_exited"))
    }

    async fn shutdown(&mut self) -> Result<()> {
        // EOF lets the guest drain streams before its process or socket closes.
        let timeout = Duration::from_secs(15);
        match self {
            Self::Process { child, input } => {
                drop(input.take());
                match tokio::time::timeout(timeout, child.wait()).await {
                    Ok(result) => result?
                        .success()
                        .then_some(())
                        .ok_or(Error::Invalid("phone_peer_exit_failed")),
                    Err(_) => {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        Err(Error::Invalid("phone_peer_stop_unknown"))
                    }
                }
            }
            Self::Stream(stream) => {
                stream.shutdown().await?;
                let mut byte = [0];
                if matches!(
                    tokio::time::timeout(timeout, stream.read(&mut byte)).await,
                    Ok(Ok(0))
                ) {
                    Ok(())
                } else {
                    Err(Error::Invalid("phone_peer_stop_unknown"))
                }
            }
        }
    }
}
