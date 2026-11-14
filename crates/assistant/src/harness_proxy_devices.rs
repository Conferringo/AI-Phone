use crate::{App, desktop};
use ai_phone_adb::{Client, Limits};
use anyhow::{Context, Result, anyhow, ensure};
use std::time::Duration;

const PHONE_PROXY: &str = "10.0.2.2:17906";
const PHONE_PROXY_COMMAND: &str = r#"
set -eu
setting() { settings get global "$1"; }
host_port() {
    printf '%s:%s' "$(setting global_http_proxy_host)" "$(setting global_http_proxy_port)"
}
current=$(host_port)
legacy=$(setting http_proxy)
case "$(setting global_proxy_pac_url)" in
    ''|null) ;;
    *) [ "$1" = off ] && exit 0; exit 2 ;;
esac
case "$current" in
    null:null|null:0|:0|"$2") ;;
    *) [ "$1" = off ] && exit 0; exit 2 ;;
esac
case "$legacy" in
    ''|null|:0|"$2") ;;
    *) [ "$1" = off ] && exit 0; exit 2 ;;
esac
if [ "$1" = on ]; then
    [ "$current" != "$2" ] || exit 0
    settings put global http_proxy "$2"
else
    [ "$current" = "$2" ] || [ "$legacy" = "$2" ] || exit 0
    settings put global http_proxy :0
fi
for attempt in 1 2 3 4 5 6 7 8 9 10; do
    host=$(setting global_http_proxy_host)
    if [ "$1" = on ]; then
        [ "$host:$(setting global_http_proxy_port)" != "$2" ] || exit 0
    else
        [ "$host" = "${2%:*}" ] || exit 0
    fi
    sleep 0.1
done
exit 3
"#;

const DESKTOP_PROXY_COMMAND: &str = r#"
set -eu
policy=/etc/chromium/policies/managed/aimanity-workbench-proxy.json
if [ "$1" = on ]; then
    mkdir -p "${policy%/*}"
    staged=$(mktemp "${policy%/*}/.aimanity-workbench-proxy.XXXXXX")
    trap 'rm -f "$staged"' EXIT
    cat > "$staged"
    chmod 644 "$staged"
    mv -f "$staged" "$policy"
    cat "$policy"
else
    rm -f "$policy"
    test ! -e "$policy"
fi
"#;

async fn phone(enabled: bool) -> Result<()> {
    let adb = Client::new(15037, std::env::var("AI_PHONE_SERIAL")?)?;
    let mode = if enabled { "on" } else { "off" };
    let command = format!("set -- {mode} {PHONE_PROXY}\n{PHONE_PROXY_COMMAND}");
    let output = adb
        .shell(
            &command,
            Limits {
                stdout: 4096,
                stderr: 4096,
            },
            Duration::from_secs(5),
        )
        .await?;
    match output.code {
        0 => Ok(()),
        2 => Err(anyhow!("harness_phone_proxy_conflict")),
        _ => Err(anyhow!("harness_phone_proxy_not_applied")),
    }
}

pub(crate) async fn apply_desktop(app: &App, enabled: bool) -> Result<()> {
    let desktop_host = if std::env::var("AI_PHONE_ANDROID_QEMU").as_deref() == Ok("1") {
        "10.0.2.15"
    } else if cfg!(target_os = "macos") && std::env::var_os("AI_PHONE_AVO_CONTROL_SOCKET").is_some()
    {
        "192.168.5.2"
    } else {
        return Err(anyhow!("harness_device_proxy_runtime"));
    };

    let policy = if enabled {
        serde_json::to_vec(&serde_json::json!({
            "ProxySettings": {
                "ProxyMode": "fixed_servers",
                "ProxyServer": format!("http://{desktop_host}:17906"),
            }
        }))?
    } else {
        Vec::new()
    };
    let mut command = desktop::docker();
    command.current_dir(&app.root).args([
        "exec",
        "-i",
        "-u",
        "0",
        "ai-phone-desktop",
        "sh",
        "-c",
        DESKTOP_PROXY_COMMAND,
        "sh",
        if enabled { "on" } else { "off" },
    ]);
    let actual = desktop::process(command, policy.clone(), 4096, 5)
        .await
        .map_err(|error| anyhow!("harness_desktop_proxy: {error}"))?;
    ensure!(actual == policy, "harness_desktop_proxy_not_applied");
    Ok(())
}

pub(crate) async fn apply(app: &App, enabled: bool) -> Result<()> {
    phone(enabled).await.context("harness_phone_proxy")?;
    apply_desktop(app, enabled).await
}
