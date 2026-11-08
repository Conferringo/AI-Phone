//! Bounded, serial-scoped ADB server transport. Never starts a host executable or retries a command.
pub mod sync;
use std::{fmt, io, net::Ipv4Addr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Rejected(String),
    Protocol,
    Limit,
    Timeout,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(message) => write!(f, "ADB rejected: {message}"),
            Self::Io(_) => f.write_str("ADB connection interrupted; outcome unknown"),
            Self::Protocol => f.write_str("Invalid ADB response; outcome unknown"),
            Self::Limit => f.write_str("ADB output limit exceeded; outcome unknown"),
            Self::Timeout => f.write_str("ADB timeout; outcome unknown"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct Client {
    port: u16,
    serial: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Output {
    pub code: u8,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
#[derive(Clone, Copy)]
pub struct Limits {
    pub stdout: usize,
    pub stderr: usize,
}

impl Client {
    pub fn new(port: u16, serial: String) -> Result<Self> {
        if port == 0
            || serial.is_empty()
            || serial.len() > 256
            || !serial.bytes().all(|v| v.is_ascii_graphic())
        {
            return Err(Error::Protocol);
        }
        Ok(Self { port, serial })
    }
    async fn connect(&self) -> Result<TcpStream> {
        let socket = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).await?;
        socket.set_nodelay(true)?;
        Ok(socket)
    }
    async fn device(&self, service: &str) -> Result<TcpStream> {
        let mut socket = self.connect().await?;
        command(&mut socket, &format!("host:transport:{}", self.serial)).await?;
        command(&mut socket, service).await?;
        Ok(socket)
    }
    pub async fn shell(&self, text: &str, limits: Limits, deadline: Duration) -> Result<Output> {
        bounded(deadline, async {
            let mut socket = self.device(&format!("shell,v2,raw:{text}")).await?;
            socket.write_all(&[4, 0, 0, 0, 0]).await?; // This command never consumes stdin.
            shell_output(&mut socket, limits).await
        })
        .await
    }
    pub async fn raw(&self, service: &str, limit: usize, deadline: Duration) -> Result<Vec<u8>> {
        bounded(deadline, async {
            let mut socket = self.device(service).await?;
            let mut bytes = Vec::new();
            (&mut socket)
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .await?;
            if bytes.len() > limit {
                return Err(Error::Limit);
            }
            Ok(bytes)
        })
        .await
    }
    pub async fn state(&self, limit: usize, deadline: Duration) -> Result<Vec<u8>> {
        self.query(
            &format!("host-serial:{}:get-state", self.serial),
            limit,
            deadline,
        )
        .await
    }
    pub async fn version(&self, deadline: Duration) -> Result<Vec<u8>> {
        self.query("host:version", 16, deadline).await
    }
    async fn query(&self, service: &str, limit: usize, deadline: Duration) -> Result<Vec<u8>> {
        bounded(deadline, async {
            let mut socket = self.connect().await?;
            command(&mut socket, service).await?;
            string(&mut socket, limit).await
        })
        .await
    }
}
async fn bounded<T>(
    deadline: Duration,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(deadline, future)
        .await
        .map_err(|_| Error::Timeout)?
}
async fn string(socket: &mut TcpStream, limit: usize) -> Result<Vec<u8>> {
    let mut header = [0; 4];
    socket.read_exact(&mut header).await?;
    if !header.iter().all(u8::is_ascii_hexdigit) {
        return Err(Error::Protocol);
    }
    let count = std::str::from_utf8(&header)
        .ok()
        .and_then(|s| usize::from_str_radix(s, 16).ok())
        .ok_or(Error::Protocol)?;
    if count > limit {
        return Err(Error::Limit);
    }
    let mut bytes = vec![0; count];
    socket.read_exact(&mut bytes).await?;
    Ok(bytes)
}
async fn command(socket: &mut TcpStream, service: &str) -> Result<()> {
    if service.len() > u16::MAX as usize || service.contains('\0') {
        return Err(Error::Protocol);
    }
    socket
        .write_all(format!("{:04x}{service}", service.len()).as_bytes())
        .await?;
    let mut status = [0; 4];
    socket.read_exact(&mut status).await?;
    match &status {
        b"OKAY" => Ok(()),
        b"FAIL" => Err(Error::Rejected(
            String::from_utf8_lossy(&string(socket, 65535).await?).into_owned(),
        )),
        _ => Err(Error::Protocol),
    }
}
async fn shell_output(socket: &mut TcpStream, limits: Limits) -> Result<Output> {
    let mut output = Output {
        code: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    loop {
        let mut header = [0; 5];
        socket.read_exact(&mut header).await?;
        let count =
            u32::from_le_bytes(header[1..].try_into().map_err(|_| Error::Protocol)?) as usize;
        let (buffer, limit) = match header[0] {
            1 => (&mut output.stdout, limits.stdout),
            2 => (&mut output.stderr, limits.stderr),
            3 if count == 1 => {
                output.code = socket.read_u8().await?;
                return Ok(output);
            }
            _ => return Err(Error::Protocol),
        };
        if count > limit.saturating_sub(buffer.len()) {
            return Err(Error::Limit);
        }
        let start = buffer.len();
        buffer.resize(start + count, 0);
        socket.read_exact(&mut buffer[start..]).await?;
    }
}

#[cfg(test)]
mod tests;
