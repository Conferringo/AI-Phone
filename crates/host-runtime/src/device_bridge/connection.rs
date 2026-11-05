//! One native owner for a bridge and its guest process; no model/action policy.
use super::{Bridge, config::Config};
use crate::{Error, Result, native_ipc};
use ai_phone_api::wire::{
    DeviceBridgeConfig, DeviceBridgePhone, DeviceBridgeRole, DeviceBridgeStarted,
};
use std::{
    net::{Ipv4Addr, SocketAddr},
    process::Stdio,
    time::Duration,
};
use tokio::{
    process::{Child, ChildStdin, Command},
    sync::watch,
    task::JoinHandle,
};

pub struct Connection {
    child: Child,
    input: Option<ChildStdin>,
    end: watch::Sender<bool>,
    bridge: Option<JoinHandle<Result<()>>>,
}

impl Connection {
    pub async fn start(
        mut guest: DeviceBridgePhone,
        address: Ipv4Addr,
        mut command: Command,
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
        config.endpoint = host.receipt().endpoint.clone();
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
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let child = command.spawn()?;
        let (end, receiver) = watch::channel(false);
        let bridge = tokio::spawn(host.serve(receiver));
        let mut value = Self {
            child,
            input: None,
            end,
            bridge: Some(bridge),
        };
        value.input = value.child.stdin.take();
        let result = value.exchange(&guest, &expected, stop).await;
        if let Err(error) = result {
            value.shutdown().await?;
            return Err(error);
        }
        Ok(value)
    }
    async fn exchange(
        &mut self,
        guest: &DeviceBridgePhone,
        expected: &DeviceBridgeStarted,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        let input = self
            .input
            .as_mut()
            .ok_or(Error::Invalid("phone_peer_start_unknown"))?;
        let mut output = self
            .child
            .stdout
            .take()
            .ok_or(Error::Invalid("phone_peer_start_unknown"))?;
        let exchange = async {
            native_ipc::write_bounded(input, guest, 16384).await?;
            let reply: DeviceBridgeStarted = native_ipc::read_bounded(&mut output, 512).await?;
            if &reply != expected {
                return Err(Error::Invalid("phone_peer_start_receipt"));
            }
            Ok(())
        };
        tokio::select! { biased;
            _=stop.changed()=>Err(Error::Cancelled),
            result=tokio::time::timeout(Duration::from_secs(15),exchange)=>result.map_err(|_|Error::Invalid("phone_peer_start_unknown"))?,
        }
    }
    pub async fn wait(&mut self) -> Result<()> {
        let bridge = self
            .bridge
            .as_mut()
            .ok_or(Error::Invalid("device_bridge_worker"))?;
        tokio::select! {
            status=self.child.wait()=>{status?;Err(Error::Invalid("phone_peer_exited"))},
            result=bridge=>{
                self.bridge=None;
                result.map_err(|_|Error::Invalid("device_bridge_worker"))??;
                Err(Error::Invalid("device_bridge_worker"))
            }
        }
    }
    pub async fn shutdown(&mut self) -> Result<()> {
        // EOF first lets the guest drain active streams and remove its own ad.
        drop(self.input.take());
        let child = match tokio::time::timeout(Duration::from_secs(15), self.child.wait()).await {
            Ok(result) => result.map_err(Error::from).and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(Error::Invalid("phone_peer_exit_failed"))
                }
            }),
            Err(_) => {
                let _ = self.child.kill().await;
                let _ = self.child.wait().await;
                Err(Error::Invalid("phone_peer_stop_unknown"))
            }
        };
        let _ = self.end.send(true);
        let bridge = if let Some(task) = self.bridge.take() {
            task.await
                .map_err(|_| Error::Invalid("device_bridge_worker"))?
        } else {
            Ok(())
        };
        child.and(bridge)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        drop(self.input.take());
        let _ = self.end.send(true);
    }
}
