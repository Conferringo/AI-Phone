use super::{adb::Adb, plan::Plan, readiness};
use crate::{Error, Result, filesystem};
use ai_phone_api::wire::{NativeAndroidBooted, NativeAndroidSessionOwner};
use prost::Message;
use std::{fs::File, io::Write, path::Path, process::Stdio, time::Duration};
use tokio::{
    process::{Child, Command},
    sync::watch,
    time::Instant,
};

pub(super) enum Started {
    Existing(NativeAndroidBooted),
    Owned(Box<Phone>),
}
pub(super) struct Phone {
    plan: Plan,
    _lock: filesystem::Lease,
    log: File,
    adb: Option<Child>,
    emulator: Option<Child>,
    pub receipt: Option<NativeAndroidBooted>,
}
fn owner_path(plan: &Plan) -> std::path::PathBuf {
    plan.home.join("session.pb")
}
fn read_owner(plan: &Plan) -> Result<Option<NativeAndroidSessionOwner>> {
    let path = owner_path(plan);
    if !path.try_exists()? {
        return Ok(None);
    }
    let value = NativeAndroidSessionOwner::decode(
        filesystem::bounded_file_with_limit(&path, 16384)?.as_slice(),
    )
    .map_err(|_| Error::Invalid("android_session_owner"))?;
    if value != plan.owner() {
        return Err(Error::Invalid("android_session_conflict"));
    }
    Ok(Some(value))
}
fn save_owner(plan: &Plan) -> Result<()> {
    let mut out = tempfile::NamedTempFile::new_in(&plan.home)?;
    out.write_all(&plan.owner().encode_to_vec())?;
    out.as_file().sync_all()?;
    out.persist(owner_path(plan))
        .map_err(|e| Error::Io(e.error))?;
    filesystem::sync_directory(&plan.home)
}
fn client(plan: &Plan) -> Adb {
    Adb {
        port: plan.ports.adb,
        serial: plan.serial(),
    }
}
fn spawn(mut command: Command, log: &File) -> Result<Child> {
    command
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log.try_clone()?));
    Ok(command.spawn()?)
}
fn alive(child: &mut Option<Child>, reason: &'static str) -> Result<()> {
    if child
        .as_mut()
        .ok_or(Error::Invalid(reason))?
        .try_wait()?
        .is_some()
    {
        return Err(Error::Invalid(reason));
    }
    Ok(())
}
fn space(plan: &Plan) -> Result<()> {
    let disk = plan.avd.join("phone-local-01.avd/userdata-qemu.img");
    let initial = if disk.try_exists()? {
        0
    } else {
        plan.resources.storage_gb as u64 * 1024_u64.pow(3)
    };
    let image = plan.sdk.join(format!(
        "system-images/android-35/default/{}/system.img",
        plan.abi
    ));
    let needed = crate::control::RESERVE + initial + std::fs::metadata(image)?.len();
    if fs2::available_space(&plan.home)? < needed {
        return Err(Error::LowSpace);
    }
    Ok(())
}
pub(super) async fn start(plan: Plan, stop: &mut watch::Receiver<bool>) -> Result<Started> {
    readiness::cancelled(stop)?;
    let lease = filesystem::lease(&plan.home.join("session.lock"))?;
    let owner = read_owner(&plan)?;
    let Some(lease) = lease else {
        owner.ok_or(Error::Invalid("android_session_starting"))?;
        return Ok(Started::Existing(
            readiness::existing(&client(&plan), stop).await?,
        ));
    };
    let mut phone = Phone::new(plan, lease)?;
    if let Err(error) = phone.launch(stop).await {
        phone.shutdown().await.inspect_err(|_| {
            eprintln!("android_start_cleanup: {}", error.code());
        })?;
        return Err(error);
    }
    Ok(Started::Owned(Box::new(phone)))
}
impl Phone {
    fn new(plan: Plan, lease: filesystem::Lease) -> Result<Self> {
        let reservations = plan.ports.reserve()?;
        space(&plan)?;
        let log = plan.log()?;
        save_owner(&plan)?;
        let value = Self {
            plan,
            _lock: lease,
            log,
            adb: None,
            emulator: None,
            receipt: None,
        };
        drop(reservations);
        Ok(value)
    }
    async fn launch(&mut self, stop: &mut watch::Receiver<bool>) -> Result<()> {
        self.adb = Some(spawn(self.plan.adb_server()?, &self.log)?);
        await_server(&mut self.adb, &client(&self.plan), stop).await?;
        self.emulator = Some(spawn(self.plan.emulator()?, &self.log)?);
        self.receipt = Some(self.wait_boot(stop).await?);
        Ok(())
    }
    async fn wait_boot(&mut self, stop: &mut watch::Receiver<bool>) -> Result<NativeAndroidBooted> {
        let until = Instant::now() + Duration::from_secs(120);
        let mut root_requested = false;
        loop {
            alive(&mut self.adb, "android_adb_server_exited")?;
            alive(&mut self.emulator, "android_emulator_exited")?;
            readiness::cancelled(stop)?;
            if let Some(value) = boot_probe(&client(&self.plan), &mut root_requested).await? {
                return Ok(value);
            }
            if Instant::now() >= until {
                return Err(Error::Invalid("android_boot_timeout"));
            }
            readiness::pause(stop).await?;
        }
    }
    pub async fn wait(&mut self, stop: &mut watch::Receiver<bool>) -> Result<()> {
        let adb = self
            .adb
            .as_mut()
            .ok_or(Error::Invalid("android_adb_server_exited"))?;
        let emulator = self
            .emulator
            .as_mut()
            .ok_or(Error::Invalid("android_emulator_exited"))?;
        if *stop.borrow() {
            return Ok(());
        }
        tokio::select! {
            _=stop.changed()=>Ok(()),
            status=adb.wait()=>{status?;Err(Error::Invalid("android_adb_server_exited"))},
            status=emulator.wait()=>if status?.success(){Ok(())}else{Err(Error::Invalid("android_emulator_exited"))},
        }
    }
    async fn stop_emulator(&mut self) -> Result<()> {
        let Some(emulator) = self.emulator.as_mut() else {
            return Ok(());
        };
        let until = Instant::now() + Duration::from_secs(15);
        if !console_ready(emulator, self.plan.ports.console, until).await? {
            return Ok(());
        }
        let command = self.plan.stop().map_err(|_| stop_unknown())?;
        let mut stop = spawn(command, &self.log).map_err(|_| stop_unknown())?;
        if tokio::time::timeout_at(
            (Instant::now() + Duration::from_secs(5)).min(until),
            stop.wait(),
        )
        .await
        .is_err()
        {
            let _ = stop.kill().await;
            let _ = stop.wait().await;
        }
        match tokio::time::timeout_at(until, emulator.wait()).await {
            Ok(result) => {
                result.map_err(|_| stop_unknown())?;
                Ok(())
            }
            Err(_) => Err(stop_unknown()),
        }
    }
    pub async fn shutdown(&mut self) -> Result<()> {
        self.stop_emulator().await?;
        stop_adb(&mut self.adb).await?;
        match std::fs::remove_file(owner_path(&self.plan)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        filesystem::sync_directory(Path::new(&self.plan.home))
    }
}

async fn stop_adb(adb: &mut Option<Child>) -> Result<()> {
    if let Some(adb) = adb.as_mut() {
        if adb
            .try_wait()
            .map_err(|_| Error::Invalid("android_adb_stop_unknown"))?
            .is_none()
        {
            adb.kill()
                .await
                .map_err(|_| Error::Invalid("android_adb_stop_unknown"))?;
        }
        adb.wait()
            .await
            .map_err(|_| Error::Invalid("android_adb_stop_unknown"))?;
    }
    Ok(())
}

async fn await_server(
    child: &mut Option<Child>,
    adb: &Adb,
    stop: &mut watch::Receiver<bool>,
) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        alive(child, "android_adb_server_exited")?;
        readiness::cancelled(stop)?;
        if adb.server().await.is_ok() {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(Error::Invalid("android_adb_server_timeout"));
        }
        readiness::pause(stop).await?;
    }
}

async fn boot_probe(adb: &Adb, root_requested: &mut bool) -> Result<Option<NativeAndroidBooted>> {
    if adb.boot_completed().await.unwrap_or(false)
        && let Ok(uid) = adb.uid().await
    {
        if uid == 0 {
            return Ok(readiness::snapshot(adb, false).await.ok());
        }
        if !*root_requested {
            *root_requested = true;
            if let Err(error) = adb.root().await
                && error.code() == "android_root_unavailable"
            {
                return Err(error);
            }
        }
    }
    Ok(None)
}

fn stop_unknown() -> Error {
    Error::Invalid("android_emulator_stop_unknown")
}

// The process exists before its authenticated console. Cancellation in that
// interval must wait for the endpoint, then issue one stop within the same budget.
async fn console_ready(child: &mut Child, port: u16, until: Instant) -> Result<bool> {
    loop {
        if child.try_wait().map_err(|_| stop_unknown())?.is_some() {
            return Ok(false);
        }
        if matches!(
            tokio::time::timeout(
                Duration::from_millis(200),
                tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            )
            .await,
            Ok(Ok(_))
        ) {
            return Ok(true);
        }
        if Instant::now() >= until {
            return Err(stop_unknown());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
