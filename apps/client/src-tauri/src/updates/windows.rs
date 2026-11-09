use super::Platform;
use semver::Version;
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    ptr::null_mut,
};
use tauri::{AppHandle, Manager};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    Storage::FileSystem::FILE_SHARE_READ,
    System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, INFINITE, OpenProcess,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW,
        WaitForSingleObject,
    },
    UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_ICONWARNING, MessageBoxW},
};

const MODE: &str = "--install-client-update";
const HELPER: &str = "ai-phone-update-helper.exe";
const LAUNCHER: &str = "ai-phone-client.exe";

pub(super) fn launch(app: &AppHandle, downloaded: &Path, version: &Version) -> Result<(), String> {
    let folder = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("client-updates");
    if downloaded != folder.join(Platform::Windows.installer(version)) {
        return Err("Unexpected Windows installer path".into());
    }
    let _locked = lock_installer(downloaded).map_err(|error| error.to_string())?;
    spawn_helper(&folder, version).map_err(|error| error.to_string())
}

fn lock_installer(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
}

fn installed_folder(path: &Path) -> io::Result<&Path> {
    let folder = path.parent().ok_or_else(invalid_path)?;
    if !path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(LAUNCHER))
        || !folder.join("host/source.payload").is_file()
    {
        return Err(invalid_path());
    }
    Ok(folder)
}

fn invalid_path() -> io::Error {
    io::Error::other("Windows update paths do not match the installed AI Phone client")
}

fn spawn_helper(folder: &Path, version: &Version) -> io::Result<()> {
    let installed = std::env::current_exe()?;
    installed_folder(&installed)?;
    let helper = folder.join(HELPER);
    match fs::remove_file(&helper) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut copy = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&helper)?;
    io::copy(&mut File::open(installed)?, &mut copy)?;
    drop(copy);

    let mut child = Command::new(helper)
        .arg(MODE)
        .arg(std::process::id().to_string())
        .arg(version.to_string())
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut ready = [0];
    if child
        .stdout
        .take()
        .ok_or_else(invalid_path)?
        .read_exact(&mut ready)
        .is_err()
        || ready != [1]
    {
        let _ = child.wait();
        return Err(io::Error::other("Windows update helper did not start"));
    }
    Ok(())
}

pub(crate) fn run(args: &[OsString]) -> Option<i32> {
    if args.first().is_none_or(|arg| arg != MODE) {
        return None;
    }
    let ready = match args {
        [_, pid, version] => prepare(pid, version),
        _ => Err(invalid_path()),
    };
    let (parent, installed, downloaded, locked) = match ready {
        Ok(ready) => ready,
        Err(_) => return Some(1),
    };
    if io::stdout()
        .write_all(&[1])
        .and_then(|()| io::stdout().flush())
        .is_err()
    {
        return Some(1);
    }
    if unsafe { WaitForSingleObject(parent.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0 {
        notice("AI Phone could not close for the update.", MB_ICONWARNING);
        return Some(1);
    }
    drop(parent);
    Some(install(&installed, &downloaded, locked))
}

fn prepare(pid: &OsStr, version: &OsStr) -> io::Result<(OwnedHandle, PathBuf, PathBuf, File)> {
    let pid = pid
        .to_str()
        .and_then(|value| value.parse().ok())
        .ok_or_else(invalid_path)?;
    let tag = version.to_str().ok_or_else(invalid_path)?;
    let version = Version::parse(tag).map_err(|_| invalid_path())?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(invalid_path());
    }
    let helper = std::env::current_exe()?;
    let folder = helper.parent().ok_or_else(invalid_path)?;
    if helper.file_name() != Some(OsStr::new(HELPER))
        || !folder.ends_with(Path::new("dev.aiphone.client/client-updates"))
    {
        return Err(invalid_path());
    }

    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let parent = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut path = vec![0; 32_768];
    let mut length = path.len() as u32;
    if unsafe {
        QueryFullProcessImageNameW(parent.as_raw_handle(), 0, path.as_mut_ptr(), &mut length)
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let installed = PathBuf::from(OsString::from_wide(&path[..length as usize]));
    installed_folder(&installed)?;
    let downloaded = folder.join(Platform::Windows.installer(&version));
    if !fs::symlink_metadata(&downloaded)?.is_file() {
        return Err(invalid_path());
    }
    let locked = lock_installer(&downloaded)?;
    Ok((parent, installed, downloaded, locked))
}

fn install(installed: &Path, downloaded: &Path, locked: File) -> i32 {
    let Some(folder) = installed.parent() else {
        return 1;
    };
    let mut directory = OsString::from("/DIR=");
    directory.push(folder);
    let status = Command::new(downloaded)
        .args([
            "/SILENT",
            "/SP-",
            "/NORESTART",
            "/RESTARTEXITCODE=3010",
            "/NORESTARTAPPLICATIONS",
        ])
        .arg(directory)
        .current_dir(folder)
        .status();
    drop(locked);
    let code = status.as_ref().ok().and_then(|status| status.code());
    if code == Some(3010) {
        if reopen_after_restart(installed).is_ok() {
            notice(
                "Restart Windows to finish the AI Phone update.",
                MB_ICONINFORMATION,
            );
        } else {
            notice(
                "Restart Windows, then open AI Phone to finish the update.",
                MB_ICONINFORMATION,
            );
        }
        return 3010;
    }
    if let Err(error) = Command::new(installed).current_dir(folder).spawn() {
        notice(
            &format!("Could not reopen AI Phone. {error}"),
            MB_ICONWARNING,
        );
        return 1;
    }
    match status {
        Ok(status) if status.success() => 0,
        Ok(status) if status.code() == Some(2) => 2,
        Ok(status) => {
            notice(
                &format!("AI Phone update did not finish ({status})."),
                MB_ICONWARNING,
            );
            1
        }
        Err(error) => {
            notice(
                &format!("Could not start the AI Phone installer. {error}"),
                MB_ICONWARNING,
            );
            1
        }
    }
}

fn reopen_after_restart(installed: &Path) -> io::Result<()> {
    let system = PathBuf::from(std::env::var_os("SystemRoot").ok_or_else(invalid_path)?);
    let mut command = OsString::from("\"");
    command.push(installed);
    command.push("\"");
    let status = Command::new(system.join("System32/reg.exe"))
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\RunOnce",
            "/v",
            "AIPhoneUpdate",
            "/t",
            "REG_SZ",
            "/d",
        ])
        .arg(command)
        .arg("/f")
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| io::Error::other("Could not schedule AI Phone to reopen"))
}

fn notice(message: &str, icon: u32) {
    let wide = |value: &str| {
        OsStr::new(value)
            .encode_wide()
            .chain([0])
            .collect::<Vec<_>>()
    };
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(message).as_ptr(),
            wide("AI Phone").as_ptr(),
            icon,
        )
    };
}
