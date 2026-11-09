use super::Platform;
use semver::Version;
use serde::Deserialize;
use std::{
    env,
    ffi::{CString, OsString, c_char, c_int, c_uint},
    fs::{self, DirBuilder, File},
    io::{self, Read},
    os::unix::{ffi::OsStrExt, fs::DirBuilderExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{self, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

const HELPER: &str = "--install-macos-update";
const BUNDLE: &str = "AI Phone.app";
const RENAME_SWAP: c_uint = 0x0000_0002;

unsafe extern "C" {
    fn getppid() -> c_int;
    fn renamex_np(first: *const c_char, second: *const c_char, flags: c_uint) -> c_int;
}

#[derive(Deserialize)]
struct BundleInfo {
    #[serde(rename = "CFBundleIdentifier")]
    identifier: String,
    #[serde(rename = "CFBundleShortVersionString")]
    version: String,
    #[serde(rename = "CFBundleExecutable")]
    executable: String,
}

fn installed_bundle() -> Result<PathBuf, String> {
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let bundle = executable
        .ancestors()
        .nth(3)
        .ok_or("Install AI Phone first.")?;
    if bundle.extension().is_some_and(|ext| ext == "app")
        && executable.starts_with(bundle.join("Contents/MacOS"))
    {
        Ok(bundle.to_path_buf())
    } else {
        Err("Install AI Phone before updating.".into())
    }
}

fn checked(command: &mut Command) -> Result<Output, String> {
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(output);
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let program = Path::new(command.get_program())
        .file_name()
        .unwrap_or(command.get_program())
        .to_string_lossy();
    Err(format!("{program} {} {}", output.status, detail.trim()))
}

fn info(bundle: &Path) -> Result<BundleInfo, String> {
    let output = checked(
        Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-"])
            .arg(bundle.join("Contents/Info.plist")),
    )?;
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

fn workspace(bundle: &Path, pid: u32) -> PathBuf {
    bundle.with_file_name(format!(".ai-phone-update-{pid}"))
}

pub(super) fn launch(app: &AppHandle, downloaded: &Path, version: &Version) -> Result<(), String> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?;
    if downloaded
        != cache
            .join("client-updates")
            .join(Platform::Mac.installer(version))
    {
        return Err("Unexpected macOS installer path.".into());
    }
    let bundle = installed_bundle()?;
    if info(&bundle)?.identifier != app.config().identifier {
        return Err("The installed app has a different bundle identifier.".into());
    }
    let pid = process::id();
    let work = workspace(&bundle, pid);
    DirBuilder::new()
        .mode(0o700)
        .create(&work)
        .map_err(|error| format!("Cannot update AI Phone in this folder. {error}"))?;

    if let Err(error) = Command::new(env::current_exe().map_err(|error| error.to_string())?)
        .arg(HELPER)
        .arg(downloaded)
        .arg(version.to_string())
        .arg(pid.to_string())
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        let _ = fs::remove_dir(&work);
        return Err(error.to_string());
    }
    Ok(())
}

fn wait_for_parent(pid: u32) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(120);
    while unsafe { getppid() } as u32 == pid {
        if Instant::now() >= deadline {
            return Err("AI Phone did not quit. Close it and try the update again.".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn validate(candidate: &Path, old: &BundleInfo, version: &Version) -> Result<(), String> {
    let new = info(candidate)?;
    let current = Version::parse(&old.version).map_err(|error| error.to_string())?;
    if new.identifier != old.identifier
        || new.version != version.to_string()
        || version <= &current
        || new.executable != old.executable
    {
        return Err("The update does not match the installed app or requested version.".into());
    }
    let executable = candidate.join("Contents/MacOS").join(new.executable);
    let mut header = [0; 8];
    File::open(executable)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|error| error.to_string())?;
    if header != [0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 0x01] {
        return Err("The update must contain an Apple Silicon executable.".into());
    }
    Ok(())
}

fn stage(
    downloaded: &Path,
    mount: &Path,
    staged: &Path,
    old: &BundleInfo,
    version: &Version,
) -> Result<(), String> {
    fs::create_dir(mount).map_err(|error| error.to_string())?;
    checked(
        Command::new("/usr/bin/hdiutil")
            .args([
                "attach",
                "-readonly",
                "-nobrowse",
                "-noverify",
                "-owners",
                "off",
                "-mountpoint",
            ])
            .arg(mount)
            .arg(downloaded),
    )?;
    let candidate = mount.join(BUNDLE);
    let copied = validate(&candidate, old, version).and_then(|()| {
        checked(Command::new("/usr/bin/ditto").arg(&candidate).arg(staged)).map(|_| ())
    });
    let detached = checked(Command::new("/usr/bin/hdiutil").arg("detach").arg(mount)).map(|_| ());
    copied.and(detached)
}

fn swap(first: &Path, second: &Path) -> Result<(), String> {
    let first = CString::new(first.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    let second = CString::new(second.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
    // Both paths are on the same volume and remain valid for the call.
    if unsafe { renamex_np(first.as_ptr(), second.as_ptr(), RENAME_SWAP) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error().to_string())
    }
}

fn reopen(bundle: &Path) -> Result<(), String> {
    checked(Command::new("/usr/bin/open").arg("-n").arg(bundle)).map(|_| ())
}

fn restart_after_error(bundle: &Path, error: String) -> String {
    match reopen(bundle) {
        Ok(()) => error,
        Err(restart) => format!("{error}\nCould not reopen AI Phone. {restart}"),
    }
}

fn install(bundle: &Path, work: &Path, downloaded: &Path, version: &Version) -> Result<(), String> {
    let mount = work.join("image");
    let staged = work.join(BUNDLE);
    let applied = info(bundle)
        .and_then(|old| stage(downloaded, &mount, &staged, &old, version))
        .and_then(|()| swap(bundle, &staged));
    let result = match applied {
        Ok(()) => match reopen(bundle) {
            Ok(()) => Ok(()),
            Err(error) => match swap(bundle, &staged) {
                Ok(()) => Err(restart_after_error(bundle, error)),
                Err(rollback) => {
                    return Err(restart_after_error(
                        &staged,
                        format!(
                            "{error}\nCould not restore the app at its original location. {rollback}\n{}",
                            staged.display()
                        ),
                    ));
                }
            },
        },
        Err(error) => Err(restart_after_error(bundle, error)),
    };
    let _ = fs::remove_dir_all(&staged);
    let _ = fs::remove_dir(&mount);
    result
}

fn alert(message: &str) {
    let _ = Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display alert \"AI Phone update failed\" message (item 1 of argv) as critical",
            "-e",
            "end run",
            "--",
            message,
        ])
        .spawn();
}

pub(crate) fn run(args: &[OsString]) -> Option<i32> {
    if args.first().is_none_or(|arg| arg != HELPER) {
        return None;
    }
    let result = (|| {
        let [_, downloaded, version, pid] = args else {
            return Err("Invalid macOS update arguments.".into());
        };
        let pid = pid
            .to_str()
            .and_then(|pid| pid.parse().ok())
            .ok_or("Invalid macOS update process.")?;
        let version = Version::parse(version.to_str().ok_or("Invalid macOS update version.")?)
            .map_err(|error| error.to_string())?;
        let downloaded = Path::new(downloaded);
        if downloaded.file_name() != Some(std::ffi::OsStr::new(&Platform::Mac.installer(&version)))
            || !downloaded
                .parent()
                .is_some_and(|folder| folder.ends_with("dev.aiphone.client/client-updates"))
            || !fs::symlink_metadata(downloaded).is_ok_and(|metadata| metadata.is_file())
        {
            return Err("Unexpected macOS installer path.".into());
        }
        let bundle = installed_bundle()?;
        let work = workspace(&bundle, pid);
        let result =
            wait_for_parent(pid).and_then(|()| install(&bundle, &work, downloaded, &version));
        let _ = fs::remove_dir(&work);
        result
    })();
    Some(match result {
        Ok(()) => 0,
        Err(error) => {
            alert(&error);
            1
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_and_rollback_touch_only_the_application_bundle() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        let old = root.join("Applications").join(BUNDLE);
        let candidate = root.join("Applications").join("candidate").join(BUNDLE);
        for (bundle, version) in [(&old, "0.1.3"), (&candidate, "0.1.4")] {
            let executable = bundle.join("Contents/MacOS");
            fs::create_dir_all(&executable).unwrap();
            fs::write(
                bundle.join("Contents/Info.plist"),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.aiphone.client</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
<key>CFBundleExecutable</key><string>ai-phone-client</string>
</dict></plist>"#
                ),
            )
            .unwrap();
            fs::write(
                executable.join("ai-phone-client"),
                [0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 0x01],
            )
            .unwrap();
        }
        let data = root.join("host/user/data");
        fs::create_dir_all(&data).unwrap();
        for name in ["device", "model", "chat"] {
            fs::write(data.join(name), name).unwrap();
        }
        validate(&candidate, &info(&old).unwrap(), &Version::new(0, 1, 4)).unwrap();
        swap(&old, &candidate).unwrap();
        assert_eq!(info(&old).unwrap().version, "0.1.4");
        for name in ["device", "model", "chat"] {
            assert_eq!(fs::read_to_string(data.join(name)).unwrap(), name);
        }
        swap(&old, &candidate).unwrap();
        assert_eq!(info(&old).unwrap().version, "0.1.3");
    }
}
