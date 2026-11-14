//! Native process fixture; explicit production builders do not include this binary.
#[cfg(target_os = "linux")]
use std::{fs, io::Write, path::PathBuf, time::Duration};

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::var_os("AI_PHONE_SERVICE_TEST_ROOT").ok_or("fixture root missing")?,
    );
    let mode = std::env::args().nth(1).ok_or("fixture mode missing")?;
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    fs::write(
        root.join("worker.stdin"),
        fs::read_link("/proc/self/fd/0")?
            .as_os_str()
            .as_encoded_bytes(),
    )?;
    fs::write(root.join("worker.next"), std::process::id().to_string())?;
    fs::rename(root.join("worker.next"), root.join("worker.pid"))?;
    if mode == "exit" {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("exits"))?
            .write_all(b"exit\n")?;
        std::process::exit(7);
    }
    if mode == "ignore" {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    term.recv().await;
    fs::write(root.join("term-seen"), b"ready")?;
    if mode == "hold" {
        while !root.join("release").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The native service process fixture runs on Linux");
    std::process::exit(2);
}
