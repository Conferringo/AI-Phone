use crate::{Error, Result, filesystem};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::process::Command;

struct Phone {
    program: PathBuf,
    serial: String,
    client: ai_phone_adb::Client,
}
impl Phone {
    async fn command(&self, args: &[&str]) -> Result<Vec<u8>> {
        if args.first() == Some(&"shell") {
            let command = args[1..]
                .iter()
                .map(|value| format!("'{}'", value.replace('\'', "'\\''")))
                .collect::<Vec<_>>()
                .join(" ");
            let output = self
                .client
                .shell(
                    &command,
                    ai_phone_adb::Limits {
                        stdout: 65536,
                        stderr: 4096,
                    },
                    Duration::from_secs(30),
                )
                .await
                .map_err(|_| Error::Invalid("android_maintenance_command_unknown"))?;
            if output.code != 0 {
                return Err(Error::Invalid("android_maintenance_command_unknown"));
            }
            return Ok(output.stdout);
        }
        let child = Command::new(&self.program)
            .args(["-P", "15037", "-s", &self.serial])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let result = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output())
            .await
            .map_err(|_| Error::Invalid("android_maintenance_timeout"))??;
        if !result.status.success() {
            return Err(Error::Invalid("android_maintenance_command"));
        }
        Ok(result.stdout)
    }
    async fn text(&self, args: &[&str]) -> Result<String> {
        String::from_utf8(self.command(args).await?)
            .map(|text| text.trim().to_owned())
            .map_err(|_| Error::Invalid("android_maintenance_encoding"))
    }
}
const OPTIONAL: &[&str] = &[
    "dev.aiphone.bridge",
    "com.android.camera2",
    "com.android.calendar",
    "com.android.deskclock",
    "com.android.gallery3d",
    "com.android.music",
    "com.android.musicfx",
    "com.android.contacts",
    "com.android.dialer",
    "com.android.messaging",
    "com.android.quicksearchbox",
    "com.android.stk",
    "org.chromium.webview_shell",
    "com.android.devicediagnostics",
    "com.android.traceur",
    "com.android.egg",
    "com.android.dreams.basic",
    "com.android.dreams.phototable",
    "com.android.emulator.multidisplay",
];
async fn cleanup(phone: &Phone) -> Result<Value> {
    if phone
        .text(&["shell", "settings", "get", "secure", "default_input_method"])
        .await?
        != "dev.aiphone.input/.UnicodeIme"
    {
        return Err(Error::Invalid("phone_cleanup_input_required"));
    }
    let installed = phone
        .text(&["shell", "pm", "list", "packages", "--user", "0"])
        .await?;
    if !installed
        .lines()
        .any(|line| line == "package:info.plateaukao.einkbro")
    {
        return Err(Error::Invalid("phone_cleanup_browser_required"));
    }
    let mut removed = Vec::new();
    for package in OPTIONAL {
        if installed
            .lines()
            .any(|line| line.strip_prefix("package:") == Some(*package))
        {
            if phone
                .text(&["shell", "pm", "uninstall", "-k", "--user", "0", package])
                .await?
                != "Success"
            {
                return Err(Error::Invalid("phone_cleanup_failed"));
            }
            removed.push(*package);
        }
    }
    phone
        .command(&[
            "shell",
            "cmd",
            "role",
            "add-role-holder",
            "--user",
            "0",
            "android.app.role.BROWSER",
            "info.plateaukao.einkbro",
        ])
        .await?;
    Ok(json!({"removed": removed, "data_preserved": true}))
}
async fn ready(
    phone: &Phone,
    observe_only: bool,
    stop: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<Value> {
    let value = super::session::wait_serial(phone.serial.clone(), observe_only, stop).await?;
    if !observe_only {
        phone
            .command(&["shell", "settings", "put", "system", "show_touches", "1"])
            .await?;
        if phone
            .text(&["shell", "settings", "get", "system", "show_touches"])
            .await?
            != "1"
        {
            return Err(Error::Invalid("phone_touch_indicators"));
        }
        let overlays = phone.text(&["shell", "cmd", "overlay", "list"]).await?;
        for line in overlays.lines() {
            if line.starts_with("[x] com.android.internal.display.cutout.emulation.")
                || [
                    "[x] com.android.internal.emulation.pixel_7",
                    "[x] com.android.systemui.emulation.pixel_7",
                ]
                .contains(&line)
            {
                let package = line
                    .split_whitespace()
                    .last()
                    .ok_or(Error::Invalid("phone_overlay_observation"))?;
                phone
                    .command(&["shell", "cmd", "overlay", "disable", "--user", "0", package])
                    .await?;
            }
        }
    }
    Ok(
        json!({"boot_completed": true, "root": value.root_verified, "settings_changed": !observe_only}),
    )
}
fn create(arguments: &[OsString]) -> Result<Value> {
    let [sdk, home, abi, cpus, memory, storage] = arguments else {
        return Err(Error::Invalid("android_create_arguments"));
    };
    let number = |value: &OsString| {
        value
            .to_str()
            .and_then(|text| text.parse().ok())
            .ok_or(Error::Invalid("invalid_device_resources"))
    };
    let resources = super::Resources {
        cpus: number(cpus)?,
        memory_mb: number(memory)?,
        storage_gb: number(storage)?,
    };
    resources.validate()?;
    let abi = abi
        .to_str()
        .filter(|value| ["arm64-v8a", "x86_64"].contains(value))
        .ok_or(Error::Invalid("android_device_abi"))?;
    let sdk = Path::new(sdk).canonicalize()?;
    for name in ["system.img", "vendor.img", "kernel-ranchu", "ramdisk.img"] {
        filesystem::regular(
            &sdk.join("system-images/android-35/default")
                .join(abi)
                .join(name),
        )?;
    }
    let control = crate::control::Control {
        available: &|path| fs2::available_space(path).unwrap_or(0),
        cancelled: &|| false,
    };
    let (_, reused) = super::device::create_at(Path::new(home), &sdk, abi, resources, &control)?;
    Ok(json!({"created": !reused}))
}
const PROPERTIES: &[&str] = &[
    "/system/build.prop",
    "/product/etc/build.prop",
    "/vendor/build.prop",
    "/system_ext/etc/build.prop",
    "/odm/etc/build.prop",
];
fn field(key: &str) -> Option<&str> {
    let value = key.strip_prefix("ro.product.")?;
    let field = if let Some((namespace, field)) = value.split_once('.') {
        if !["system", "system_ext", "vendor", "product", "odm"].contains(&namespace) {
            return None;
        }
        field
    } else {
        value
    };
    ["brand", "manufacturer", "model", "device", "name"]
        .contains(&field)
        .then_some(field)
}
fn fingerprint_key(key: &str) -> bool {
    key == "ro.build.fingerprint"
        || ["system", "system_ext", "vendor", "product", "odm"]
            .iter()
            .any(|part| key == format!("ro.{part}.build.fingerprint"))
}
fn properties(
    content: &[u8],
    values: &BTreeMap<&str, &str>,
    fingerprint: &str,
    build_type: &str,
) -> Result<Vec<u8>> {
    let source =
        std::str::from_utf8(content).map_err(|_| Error::Invalid("phone_profile_encoding"))?;
    let mut result = String::new();
    for line in source.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let updated = if let Some(field) = field(key) {
                values[field].to_owned()
            } else if key == "ro.build.product" {
                values["device"].to_owned()
            } else if fingerprint_key(key) {
                fingerprint.to_owned()
            } else if key == "ro.build.description" {
                let tail = value
                    .find(char::is_whitespace)
                    .map(|offset| &value[offset..])
                    .unwrap_or("");
                format!("{}-{}{tail}", values["device"], build_type)
            } else {
                value.to_owned()
            };
            result.push_str(key);
            result.push('=');
            result.push_str(&updated);
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    Ok(result.into_bytes())
}
async fn profile(root: &Path, phone: &Phone, id: &str) -> Result<Value> {
    let config: Value =
        serde_json::from_slice(&std::fs::read(root.join("config/device-presets.json"))?)
            .map_err(|_| Error::Invalid("phone_profile_configuration"))?;
    let selected = config["identities"]
        .as_array()
        .and_then(|profiles| profiles.iter().find(|item| item["id"] == id))
        .ok_or(Error::Invalid("phone_profile_unknown"))?;
    let mut values = BTreeMap::new();
    for field in ["brand", "manufacturer", "model", "device"] {
        values.insert(
            field,
            selected[field]
                .as_str()
                .ok_or(Error::Invalid("phone_profile_configuration"))?,
        );
    }
    values.insert("name", values["device"]);
    if phone.text(&["shell", "id", "-u"]).await? != "0" {
        return Err(Error::Invalid("phone_profile_root_required"));
    }
    let fingerprint = phone
        .text(&["shell", "getprop", "ro.build.fingerprint"])
        .await?;
    let (_, suffix) = fingerprint
        .split_once(':')
        .ok_or(Error::Invalid("phone_profile_fingerprint"))?;
    let fingerprint = format!(
        "{}/{}/{}:{suffix}",
        values["brand"].replace(' ', "_"),
        values["device"],
        values["device"]
    );
    let build_type = phone.text(&["shell", "getprop", "ro.build.type"]).await?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Invalid("phone_profile_clock"))?
        .as_nanos();
    let backup = root
        .join("data/android/backups")
        .join(format!("identity-{}-{stamp}", phone.serial));
    filesystem::directory(&backup, true)?;
    let mut original = BTreeMap::new();
    let mut hashes = BTreeMap::new();
    for path in PROPERTIES {
        let content = phone.command(&["exec-out", "cat", path]).await?;
        let target = backup.join(path.trim_start_matches('/'));
        filesystem::directory(
            target
                .parent()
                .ok_or(Error::Invalid("phone_profile_backup"))?,
            true,
        )?;
        std::fs::write(target, &content)?;
        hashes.insert(*path, format!("{:x}", Sha256::digest(&content)));
        original.insert(*path, content);
    }
    let mut names = BTreeMap::new();
    for key in ["device_name", "bluetooth_name"] {
        names.insert(
            key,
            phone
                .text(&["shell", "settings", "get", "global", key])
                .await?,
        );
    }
    std::fs::write(
        backup.join("manifest.json"),
        json!({"files": hashes, "names": names}).to_string(),
    )?;
    phone.command(&["remount"]).await?;
    let temporary = backup.join("next.prop");
    let local = temporary
        .to_str()
        .ok_or(Error::Invalid("phone_profile_path"))?;
    for (path, content) in original {
        let updated = properties(&content, &values, &fingerprint, &build_type)?;
        std::fs::write(&temporary, &updated)?;
        phone
            .command(&["push", local, "/data/local/tmp/aiphone-identity.prop"])
            .await?;
        phone
            .command(&[
                "shell",
                "sh",
                "-c",
                &format!("cat /data/local/tmp/aiphone-identity.prop > {path} && restorecon {path}"),
            ])
            .await?;
        if phone.command(&["exec-out", "cat", path]).await? != updated {
            return Err(Error::Invalid("phone_profile_readback"));
        }
    }
    phone
        .command(&["shell", "rm", "/data/local/tmp/aiphone-identity.prop"])
        .await?;
    std::fs::remove_file(temporary)?;
    let name = selected["name"]
        .as_str()
        .ok_or(Error::Invalid("phone_profile_configuration"))?;
    for key in names.keys() {
        phone
            .command(&["shell", "settings", "put", "global", key, name])
            .await?;
    }
    Ok(json!({"profile": values, "backup": backup, "restart_required": true}))
}
pub async fn run(arguments: &[OsString]) -> Result<()> {
    let [root, adb, serial, operation, rest @ ..] = arguments else {
        return Err(Error::Invalid("android_maintenance_arguments"));
    };
    let serial = serial
        .to_str()
        .ok_or(Error::Invalid("android_maintenance_serial"))?;
    let client = ai_phone_adb::Client::new(15037, serial.to_owned())
        .map_err(|_| Error::Invalid("android_maintenance_serial"))?;
    let root = Path::new(root);
    filesystem::existing_directory(root)?;
    let phone = Phone {
        program: PathBuf::from(adb),
        serial: serial.to_owned(),
        client,
    };
    let (send, mut stop) = tokio::sync::watch::channel(false);
    let signal = tokio::spawn(async move {
        crate::component::interrupt().await?;
        send.send_replace(true);
        Ok::<_, std::io::Error>(())
    });
    let mut interrupted = stop.clone();
    let action = async {
        match (operation.to_str(), rest) {
            (Some("create"), arguments) => create(arguments),
            (Some("ready"), []) => ready(&phone, false, &mut stop).await,
            (Some("ready"), [flag]) if flag == "--observe-only" => {
                ready(&phone, true, &mut stop).await
            }
            (Some("input"), [apk, probe]) => super::input::setup(
                ai_phone_api::wire::NativeAndroidInputSetup {
                    version: 1,
                    project_root: root
                        .to_str()
                        .ok_or(Error::Invalid("android_maintenance_arguments"))?
                        .into(),
                    adb_program: phone
                        .program
                        .to_str()
                        .ok_or(Error::Invalid("android_maintenance_arguments"))?
                        .into(),
                    serial: phone.serial.clone(),
                    apk_path: apk
                        .to_str()
                        .ok_or(Error::Invalid("android_maintenance_arguments"))?
                        .into(),
                    notification_probe_path: probe
                        .to_str()
                        .ok_or(Error::Invalid("android_maintenance_arguments"))?
                        .into(),
                    ..Default::default()
                },
                stop.clone(),
            )
            .await
            .map(|changed| json!({"version": 1, "changed": changed})),
            (Some("cleanup"), []) => cleanup(&phone).await,
            (Some("profile"), [id]) => {
                profile(
                    root,
                    &phone,
                    id.to_str().ok_or(Error::Invalid("phone_profile_unknown"))?,
                )
                .await
            }
            _ => Err(Error::Invalid("android_maintenance_arguments")),
        }
    };
    let result = tokio::select! { biased; result=action=>result, _=interrupted.changed()=>Err(Error::Cancelled) };
    signal.abort();
    let _ = signal.await;
    println!("{}", result?);
    Ok(())
}
