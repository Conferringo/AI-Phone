//! Linux maintainer build workspace. The caller enters a private mount namespace;
//! exec replaces this process so the existing build-session owner also owns cleanup.
use std::{
    ffi::OsString,
    fs::{self, File},
    os::unix::{fs::chown, process::CommandExt},
    path::Path,
    process::Command,
};

pub fn run(arguments: &[OsString]) -> crate::Result<()> {
    let [project, work, uid, gid, command @ ..] = arguments else {
        return Err("source-sdk-build requires project, work, uid, gid and command".into());
    };
    if command.is_empty() {
        return Err("source-sdk-build requires a command".into());
    }
    let uid: u32 = uid.to_str().ok_or("Invalid build uid")?.parse()?;
    let gid: u32 = gid.to_str().ok_or("Invalid build gid")?.parse()?;
    let work = Path::new(work);
    let upper = work.join("sdk-upper");
    let overlay_work = work.join("sdk-overlay-work");
    fs::create_dir(&upper)?;
    fs::create_dir(&overlay_work)?;
    chown(&upper, Some(uid), Some(gid))?;
    let mut options = OsString::from("lowerdir=");
    options.push(project);
    options.push(",upperdir=");
    options.push(&upper);
    options.push(",workdir=");
    options.push(&overlay_work);
    let status = Command::new("mount")
        .args(["-t", "overlay", "overlay", "-o"])
        .arg(options)
        .arg(project)
        .status()?;
    if !status.success() {
        return Err("Cannot mount the private source SDK overlay".into());
    }
    // This is the initial input-copy footprint, not later compiler copy-up usage.
    serde_json::to_writer(
        File::create_new(work.join("sdk-copy.json"))?,
        &serde_json::json!({"backend":"overlay","files":0,"logical_bytes":0,"allocated_bytes":0}),
    )?;
    Err(Command::new("setpriv")
        .arg(format!("--reuid={uid}"))
        .arg(format!("--regid={gid}"))
        .args(["--init-groups", "--"])
        .args(command)
        .exec()
        .into())
}
