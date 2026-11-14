//! The trace is measured from X11 acknowledgements, never predicted motion.
use crate::{Result, screen::Screen};
use serde::Serialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Serialize)]
struct Point {
    x: i32,
    y: i32,
    at_ms: u64,
}

pub struct Trace {
    action_id: Option<String>,
    folder: PathBuf,
    started: Instant,
    points: Vec<Point>,
}

impl Trace {
    pub fn new(home: &Path, id: Option<&str>) -> Self {
        let action_id = id.filter(|value| valid_id(value)).map(str::to_owned);
        Self {
            action_id,
            folder: home.join(".aiphone"),
            started: Instant::now(),
            points: vec![],
        }
    }

    pub fn acknowledge(&mut self, screen: &impl Screen) -> Result<()> {
        let Some(action_id) = &self.action_id else {
            return Ok(());
        };
        let (x, y) = screen.pointer()?;
        self.points.push(Point {
            x,
            y,
            at_ms: u64::try_from(self.started.elapsed().as_millis())?,
        });
        if self.points.len() > 128 {
            self.points.remove(0);
        }
        write(&self.folder, action_id, &self.points)
    }
}

fn valid_id(value: &str) -> bool {
    value.len() == 36
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte) || byte == b'-')
}

fn write(folder: &Path, action_id: &str, points: &[Point]) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true).mode(0o700).create(folder)?;
    let mut temporary = tempfile::NamedTempFile::new_in(folder)?;
    serde_json::to_writer(
        &mut temporary,
        &serde_json::json!({"action_id": action_id, "points": points}),
    )?;
    temporary.flush()?;
    temporary.persist(folder.join("cursor.json"))?;
    Ok(())
}
