//! Bounded, cancellable invocation of the bundled OCR engine.
use serde::{Deserialize, Serialize};
use std::{fmt, io, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

const OUTPUT_LIMIT: usize = 262_144;
#[cfg(feature = "native")]
pub mod tesseract;

#[derive(Debug)]
pub enum Error {
    Launch(io::Error),
    Read(io::Error),
    Timeout,
    Failed,
    InvalidData,
    Dimensions,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Launch(error) => write!(f, "本机OCR启动失败（{:?}: {error}）", error.kind()),
            Self::Read(error) => write!(f, "本机OCR读取失败：{error}"),
            Self::Timeout => f.write_str("本机OCR超时"),
            Self::Failed => f.write_str("本机OCR失败"),
            Self::InvalidData => f.write_str("OCR数据无效"),
            Self::Dimensions => f.write_str("OCR画面尺寸不匹配"),
        }
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Bounds {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Region {
    pub text: String,
    pub confidence: f32,
    pub bounds: Bounds,
}
impl Region {
    /// Reliable visible text, without claiming it is an actionable UI control.
    pub fn usable(&self, width: u32, height: u32) -> bool {
        let b = &self.bounds;
        self.confidence.is_finite()
            && self.confidence >= 0.80
            && !self.text.trim().is_empty()
            && b.right <= width
            && b.bottom <= height
            && b.right.saturating_sub(b.left) > 3
            && b.bottom.saturating_sub(b.top) > 3
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub regions: Vec<Region>,
    pub grid: Vec<u8>,
}

/// The installer selects a bundled helper; development uses the same Rust executable.
pub fn program(root: &Path) -> std::path::PathBuf {
    std::env::var_os("AI_PHONE_OCR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let bundled = root.join("runtime/services/bin/vision");
            if bundled.is_file() {
                bundled
            } else {
                root.join("target/debug/ai-phone-vision")
            }
        })
}

pub async fn recognize(
    program: &Path,
    png: &[u8],
    width: u32,
    height: u32,
) -> Result<Capture, Error> {
    let bytes = output(Command::new(program), png).await?;
    let capture: Capture = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidData)?;
    if capture.width != width || capture.height != height || capture.grid.len() != 2048 {
        return Err(Error::Dimensions);
    }
    Ok(capture)
}

async fn output(command: Command, input: &[u8]) -> Result<Vec<u8>, Error> {
    process_output(command, input, OUTPUT_LIMIT, Duration::from_secs(5)).await
}
async fn process_output(
    command: Command,
    input: &[u8],
    limit: usize,
    budget: Duration,
) -> Result<Vec<u8>, Error> {
    read_child(spawn(command)?, input, limit, budget).await
}

fn spawn(mut command: Command) -> Result<tokio::process::Child, Error> {
    command
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(Error::Launch)
}

async fn read_child(
    mut child: tokio::process::Child,
    input: &[u8],
    limit: usize,
    budget: Duration,
) -> Result<Vec<u8>, Error> {
    let mut stdin = child.stdin.take().expect("piped OCR stdin");
    let stdout = child.stdout.take().expect("piped OCR stdout");
    let work = async {
        let write = async {
            stdin.write_all(input).await.map_err(Error::Read)?;
            stdin.shutdown().await.map_err(Error::Read)?;
            drop(stdin);
            Ok(())
        };
        let read = async {
            let mut bytes = vec![];
            stdout
                .take((limit + 1) as u64)
                .read_to_end(&mut bytes)
                .await
                .map_err(Error::Read)?;
            if bytes.len() > limit {
                return Err(Error::Failed);
            }
            Ok(bytes)
        };
        let (_, bytes, status) = tokio::try_join!(write, read, async {
            child.wait().await.map_err(Error::Read)
        })?;
        if !status.success() {
            return Err(Error::Failed);
        }
        Ok(bytes)
    };
    tokio::time::timeout(budget, work)
        .await
        .map_err(|_| Error::Timeout)?
}

fn search_text(text: &str) -> String {
    text.split_whitespace().collect::<String>().to_lowercase()
}

impl Capture {
    /// Return up to twenty full-frame candidates. Equal labels remain distinct.
    pub fn find_text(&self, query: &str) -> Vec<&Region> {
        let query = search_text(query);
        if query.is_empty() {
            return vec![];
        }
        let matches: Vec<_> = self
            .regions
            .iter()
            .filter(|r| r.usable(self.width, self.height) && search_text(&r.text).contains(&query))
            .collect();
        let exact: Vec<_> = matches
            .iter()
            .copied()
            .filter(|r| search_text(&r.text) == query)
            .take(20)
            .collect();
        if !exact.is_empty() {
            return exact;
        }
        matches.into_iter().take(20).collect()
    }
}

#[cfg(all(test, unix))]
mod lifecycle_tests {
    use super::*;

    fn sleeper() -> tokio::process::Child {
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        spawn(command).unwrap()
    }
    async fn assert_stopped(pid: &str) {
        for _ in 0..100 {
            let alive = std::process::Command::new("/bin/kill")
                .args(["-0", pid])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success();
            if !alive {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // A failing contract must not leave its own sleeper behind.
        std::process::Command::new("/bin/kill")
            .args(["-KILL", pid])
            .stderr(Stdio::null())
            .status()
            .unwrap();
        panic!("OCR child survived cancellation or timeout");
    }
    #[tokio::test]
    async fn canceling_recognition_stops_the_exact_child() {
        let child = sleeper();
        let pid = child.id().unwrap().to_string();
        let task = tokio::spawn(async move {
            read_child(child, b"", OUTPUT_LIMIT, Duration::from_secs(5)).await
        });
        tokio::task::yield_now().await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_stopped(&pid).await;
    }
    #[tokio::test]
    async fn deadline_stops_hung_recognition() {
        let child = sleeper();
        let pid = child.id().unwrap().to_string();
        assert!(matches!(
            read_child(child, b"", OUTPUT_LIMIT, Duration::from_secs(5)).await,
            Err(Error::Timeout)
        ));
        assert_stopped(&pid).await;
    }
}
