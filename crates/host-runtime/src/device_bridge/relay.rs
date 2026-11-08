use super::{config::Config, transport};
use ai_phone_api::wire::{DeviceBridgeOpen, DeviceBridgeOpened, DeviceBridgeService};
use sha2::{Digest, Sha256};
use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
};

const ADMISSION: Duration = Duration::from_secs(5);
const CONNECTIONS: usize = 32;

pub(super) enum Route {
    Host {
        config: Arc<Config>,
        targets: [SocketAddr; 2],
        key_hash: [u8; 32],
    },
    Guest {
        config: Arc<Config>,
        service: DeviceBridgeService,
    },
}
impl Route {
    pub fn host(config: Arc<Config>) -> Self {
        let key_hash = Sha256::digest(&config.secret).into();
        Self::Host {
            config,
            targets: [
                SocketAddr::from((Ipv4Addr::LOCALHOST, 15037)),
                SocketAddr::from((Ipv4Addr::LOCALHOST, 8558)),
            ],
            key_hash,
        }
    }
    async fn open(&self, client: &mut TcpStream) -> io::Result<TcpStream> {
        match self {
            Self::Host {
                config,
                targets,
                key_hash,
            } => {
                let result = authorize(client, config, key_hash, targets).await;
                transport::write(
                    client,
                    &DeviceBridgeOpened {
                        accepted: result.is_ok(),
                    },
                )
                .await?;
                result
            }
            Self::Guest { config, service } => connect(config, *service).await,
        }
    }
}
async fn authorize(
    client: &mut TcpStream,
    config: &Config,
    key_hash: &[u8; 32],
    targets: &[SocketAddr; 2],
) -> io::Result<TcpStream> {
    let request: DeviceBridgeOpen = transport::read(client).await?;
    let presented: [u8; 32] = Sha256::digest(&request.secret).into();
    if request.version != 1 || request.bridge_id != config.id || presented != *key_hash {
        return Err(transport::invalid());
    }
    let index = match DeviceBridgeService::try_from(request.service) {
        Ok(DeviceBridgeService::Adb) => 0,
        Ok(DeviceBridgeService::Emulator) => 1,
        _ => return Err(transport::invalid()),
    };
    TcpStream::connect(targets[index]).await
}
pub(super) async fn connect(
    config: &Config,
    service: DeviceBridgeService,
) -> io::Result<TcpStream> {
    let mut stream = TcpStream::connect(config.endpoint).await?;
    stream.set_nodelay(true)?;
    transport::write(
        &mut stream,
        &DeviceBridgeOpen {
            version: 1,
            bridge_id: config.id.clone(),
            secret: config.secret.clone(),
            service: service as i32,
        },
    )
    .await?;
    if !transport::read::<DeviceBridgeOpened>(&mut stream)
        .await?
        .accepted
    {
        return Err(transport::invalid());
    }
    Ok(stream)
}
async fn copy(mut client: TcpStream, route: Arc<Route>) -> io::Result<()> {
    let mut target = tokio::time::timeout(ADMISSION, route.open(&mut client))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "device_bridge_admission"))??;
    client.set_nodelay(true)?;
    target.set_nodelay(true)?;
    copy_bidirectional(&mut client, &mut target).await?;
    Ok(())
}
pub(super) async fn probe(config: &Config) -> io::Result<()> {
    for service in [DeviceBridgeService::Adb, DeviceBridgeService::Emulator] {
        tokio::time::timeout(ADMISSION, connect(config, service))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "device_bridge_admission"))??;
    }
    Ok(())
}
pub(super) async fn serve(
    listener: TcpListener,
    route: Route,
    mut stop: watch::Receiver<bool>,
) -> io::Result<()> {
    if *stop.borrow() {
        return Ok(());
    }
    let route = Arc::new(route);
    let mut work = JoinSet::new();
    let result = loop {
        tokio::select! { biased;
            _=stop.changed()=>break Ok(()),
            _=work.join_next(),if !work.is_empty()=>{},
            accepted=listener.accept()=>match accepted {
                Ok((stream,_))=>{if work.len()<CONNECTIONS { let route=route.clone();work.spawn(copy(stream,route)); }},
                Err(error)=>break Err(error),
            }
        }
    };
    work.shutdown().await;
    result
}
