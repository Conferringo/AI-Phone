//! A paired transport, not guest authority or an Android-readiness shortcut.
mod cli;
mod config;
mod connection;
pub(crate) mod discovery;
pub(crate) mod guest;
pub use guest::paired_phone;
mod peer;
mod relay;
#[cfg(test)]
mod tests;
mod transport;
use crate::Result;
use ai_phone_api::wire::{
    DeviceBridgeConfig, DeviceBridgeRole, DeviceBridgeService, DeviceBridgeStarted,
};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{net::TcpListener, sync::watch, task::JoinSet};

pub const CONTRACT: &str = "ai-phone-device-bridge-v1";
pub use cli::run;
pub use connection::Connection;
pub use peer::{CONTRACT as PEER_CONTRACT, run as run_peer};

pub struct Bridge {
    receipt: DeviceBridgeStarted,
    routes: Vec<(TcpListener, relay::Route)>,
}
impl Bridge {
    pub async fn bind(request: DeviceBridgeConfig) -> Result<Self> {
        Self::bind_config(config::Config::try_from(request)?).await
    }
    async fn bind_config(config: config::Config) -> Result<Self> {
        let config = Arc::new(config);
        let routes = match config.role {
            DeviceBridgeRole::Host => {
                let listener = TcpListener::bind(config.endpoint).await?;
                vec![(listener, relay::Route::host(config.clone()))]
            }
            _ => {
                relay::probe(&config).await?;
                bind_guest(config.clone(), config.ports).await?
            }
        };
        let endpoint = if config.role == DeviceBridgeRole::Host {
            routes[0].0.local_addr()?
        } else {
            config.endpoint
        };
        let receipt = DeviceBridgeStarted {
            version: 1,
            role: config.role as i32,
            bridge_id: config.id.clone(),
            endpoint: endpoint.to_string(),
            adb_port: config.ports[0].into(),
            emulator_port: config.ports[1].into(),
        };
        Ok(Self { receipt, routes })
    }
    pub fn receipt(&self) -> &DeviceBridgeStarted {
        &self.receipt
    }
    pub async fn serve(self, mut stop: watch::Receiver<bool>) -> Result<()> {
        if *stop.borrow() {
            return Ok(());
        }
        let (end, worker_stop) = watch::channel(false);
        let mut work = JoinSet::new();
        for (listener, route) in self.routes {
            work.spawn(relay::serve(listener, route, worker_stop.clone()));
        }
        let result = tokio::select! { biased;
            _=stop.changed()=>Ok(()),
            result=work.join_next()=>match result {
                Some(Ok(result))=>result.map_err(Into::into),
                _=>Err(crate::Error::Invalid("device_bridge_worker")),
            }
        };
        let _ = end.send(true);
        // Every listener drains its accepted connections before this owner returns.
        while work.join_next().await.is_some() {}
        result
    }
}
async fn bind_guest(
    config: Arc<config::Config>,
    ports: [u16; 2],
) -> Result<Vec<(TcpListener, relay::Route)>> {
    let mut result = Vec::new();
    for (port, service) in ports
        .into_iter()
        .zip([DeviceBridgeService::Adb, DeviceBridgeService::Emulator])
    {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await?;
        result.push((
            listener,
            relay::Route::Guest {
                config: config.clone(),
                service,
            },
        ));
    }
    Ok(result)
}
