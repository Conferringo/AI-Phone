use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::IpAddr,
    sync::{
        RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{Manager, Url, WebviewUrl, WebviewWindowBuilder};
mod bootstrap;

#[cfg(any(desktop, test))]
const DEFAULT_SERVER: &str = "http://127.0.0.1:17860";

#[derive(Serialize, Deserialize)]
struct Connection {
    server_url: String,
}
struct Connecting(AtomicBool);
#[derive(Default)]
struct Navigation {
    startup_cancelled: AtomicBool,
    server: RwLock<Option<Url>>,
}

fn local_connection_url(url: &Url) -> bool {
    let origin = (url.scheme(), url.host_str(), url.port());
    matches!(
        origin,
        ("tauri", Some("localhost"), None) | ("http" | "https", Some("tauri.localhost"), None)
    ) && url.username().is_empty()
        && url.password().is_none()
        && matches!(url.path(), "" | "/" | "/index.html")
        && url.query().is_none()
        && url.fragment().is_none()
}

impl Navigation {
    fn permits(&self, url: &Url) -> bool {
        url.username().is_empty()
            && url.password().is_none()
            && self.server.read().is_ok_and(|server| {
                server
                    .as_ref()
                    .is_some_and(|server| server.origin() == url.origin())
            })
    }
    fn replace(&self, server: Option<Url>) -> Result<Option<Url>, String> {
        self.server
            .write()
            .map(|mut current| std::mem::replace(&mut *current, server))
            .map_err(|_| "Unable to update the connection".into())
    }
}

fn server_origin(value: &str) -> Result<Url, String> {
    let mut url = Url::parse(value.trim()).map_err(|_| "Invalid server address")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err("Use an HTTP(S) server address without credentials or a path".into());
    }
    let host = url.host_str().ok_or("Missing server host")?;
    let local = host == "localhost"
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| match ip {
                IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
                IpAddr::V6(ip) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
            });
    if !local {
        return Err("This preview connects only to localhost or a private LAN address".into());
    }
    url.set_path("/");
    Ok(url)
}

fn load_connection(app: &tauri::AppHandle) -> Result<Url, String> {
    let path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("server.json");
    let raw = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Connection>(&bytes)
            .map_err(|_| "Invalid saved server configuration")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(desktop)]
            {
                Connection {
                    server_url: DEFAULT_SERVER.into(),
                }
            }
            #[cfg(mobile)]
            {
                return Err("Server not configured".into());
            }
        }
        Err(e) => return Err(e.to_string()),
    };
    server_origin(&raw.server_url)
}

fn save_connection(app: &tauri::AppHandle, url: &Url) -> Result<(), String> {
    let folder = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join("server.json");
    let temp = folder.join("server.json.tmp");
    let value = serde_json::to_vec(&Connection {
        server_url: url.origin().ascii_serialization(),
    })
    .map_err(|e| e.to_string())?;
    fs::write(&temp, value).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    fs::File::open(&temp)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}

async fn probe(url: &Url) -> Result<(), String> {
    let result = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(6))
        .build()
        .map_err(|e| e.to_string())?
        .head(url.join("phone").map_err(|e| e.to_string())?)
        .send()
        .await
        .map_err(|_| "Cannot reach the server. Start your deployment, then retry.")?;
    if !result.status().is_success()
        || !result
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/html"))
    {
        return Err(format!(
            "The workbench is unavailable (HTTP {})",
            result.status().as_u16()
        ));
    }
    Ok(())
}

fn show_connection(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("connection") {
        #[cfg(mobile)]
        window
            .navigate(Url::parse("http://tauri.localhost/").unwrap())
            .map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    #[cfg(mobile)]
    let handle = app.clone();
    WebviewWindowBuilder::new(app, "connection", WebviewUrl::App("index.html".into()))
        .title("AI Phone")
        .inner_size(680., 620.)
        .min_inner_size(360., 520.)
        .resizable(true)
        .on_navigation(move |url| {
            #[cfg(mobile)]
            if handle.state::<Navigation>().permits(url) {
                return true;
            }
            local_connection_url(url)
        })
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn checked_locale(locale: Option<String>) -> Result<Option<String>, String> {
    match locale.as_deref() {
        None => Ok(None),
        Some("zh-CN" | "en" | "zh-TW" | "ja" | "ko" | "de" | "fr" | "es") => Ok(locale),
        _ => Err("Unsupported interface language".into()),
    }
}

fn workbench_url(server: &Url, locale: Option<&str>) -> Result<Url, String> {
    let mut url = server.join("phone").map_err(|e| e.to_string())?;
    if let Some(locale) = locale {
        url.query_pairs_mut().append_pair("language", locale);
    }
    Ok(url)
}

fn show_workbench(
    app: &tauri::AppHandle,
    server: &Url,
    locale: Option<&str>,
) -> Result<(), String> {
    let url = workbench_url(server, locale)?;
    let navigation = app.state::<Navigation>();
    let previous = navigation.replace(Some(server.clone()))?;
    #[cfg(desktop)]
    let label = "workbench";
    #[cfg(mobile)]
    let label = "connection";
    let result = if let Some(window) = app.get_webview_window(label) {
        window
            .navigate(url)
            .and_then(|_| window.set_focus())
            .map_err(|e| e.to_string())
    } else {
        let handle = app.clone();
        #[cfg(desktop)]
        let (width, height) = app
            .primary_monitor()
            .ok()
            .flatten()
            .map(|monitor| {
                let area = monitor.work_area();
                let scale = monitor.scale_factor();
                (
                    (area.size.width as f64 / scale - 48.).clamp(700., 1440.),
                    (area.size.height as f64 / scale - 64.).clamp(600., 980.),
                )
            })
            .unwrap_or((1440., 980.));
        #[cfg(mobile)]
        let (width, height) = (1440., 980.);
        WebviewWindowBuilder::new(app, label, WebviewUrl::External(url))
            .title("AI Phone")
            .inner_size(width, height)
            .min_inner_size(700., 600.)
            .on_navigation(move |url| {
                (cfg!(mobile) && local_connection_url(url))
                    || handle.state::<Navigation>().permits(url)
            })
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .build()
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    if let Err(error) = result {
        navigation.replace(previous)?;
        return Err(error);
    }
    #[cfg(desktop)]
    if let Some(window) = app.get_webview_window("connection") {
        let _ = window.close();
    }
    Ok(())
}

fn require_local_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|e| e.to_string())?;
    if window.label() != "connection" || !local_connection_url(&url) {
        return Err("Native configuration is available only in the local connection window".into());
    }
    Ok(())
}

#[tauri::command]
fn current_server(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<String, String> {
    require_local_window(&window)?;
    app.state::<Navigation>()
        .startup_cancelled
        .store(true, Ordering::SeqCst);
    match load_connection(&app) {
        Ok(url) => Ok(url.origin().ascii_serialization()),
        #[cfg(mobile)]
        Err(error) if error == "Server not configured" => Ok(String::new()),
        Err(error) => Err(error),
    }
}

#[tauri::command]
fn deployment_status(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<bootstrap::Snapshot, String> {
    require_local_window(&window)?;
    app.state::<Navigation>()
        .startup_cancelled
        .store(true, Ordering::SeqCst);
    bootstrap::snapshot(&app)
}
#[tauri::command]
fn start_deployment(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    request: bootstrap::Request,
) -> Result<(), String> {
    require_local_window(&window)?;
    bootstrap::start(app, request)
}
#[tauri::command]
fn cancel_deployment(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    require_local_window(&window)?;
    bootstrap::cancel(&app);
    Ok(())
}
#[tauri::command]
fn accept_deployment_license(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    require_local_window(&window)?;
    bootstrap::accept_license(&app)
}

#[tauri::command]
async fn connect_server(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    server_url: String,
    locale: Option<String>,
    connecting: tauri::State<'_, Connecting>,
) -> Result<(), String> {
    require_local_window(&window)?;
    app.state::<Navigation>()
        .startup_cancelled
        .store(true, Ordering::SeqCst);
    if connecting.0.swap(true, Ordering::SeqCst) {
        return Err("Connection already in progress".into());
    }
    let result = async {
        let url = server_origin(&server_url)?;
        let locale = checked_locale(locale)?;
        probe(&url).await?;
        save_connection(&app, &url)?;
        show_workbench(&app, &url, locale.as_deref())
    }
    .await;
    connecting.0.store(false, Ordering::SeqCst);
    result
}

async fn resume_local_host(app: &tauri::AppHandle) -> Option<Url> {
    if !bootstrap::resume_installed(app).unwrap_or(false) {
        return None;
    }
    let setup = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = show_connection(&setup);
    });
    let waiting = app.clone();
    tauri::async_runtime::spawn_blocking(move || bootstrap::wait_until_idle(&waiting))
        .await
        .ok()?;
    if !bootstrap::ready(app) {
        return None;
    }
    let url = server_origin("http://127.0.0.1:17860").ok()?;
    probe(&url).await.ok()?;
    save_connection(app, &url).ok()?;
    Some(url)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(Connecting(AtomicBool::new(false)))
        .manage(Navigation::default())
        .manage(bootstrap::Bootstrap::default())
        .invoke_handler(tauri::generate_handler![
            current_server,
            connect_server,
            deployment_status,
            start_deployment,
            cancel_deployment,
            accept_deployment_license
        ])
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
                let connection =
                    MenuItem::with_id(app, "connection", "Setup…", true, Some("CmdOrCtrl+,"))?;
                let app_menu = Submenu::with_items(
                    app,
                    "AI Phone",
                    true,
                    &[
                        &connection,
                        &PredefinedMenuItem::separator(app)?,
                        &PredefinedMenuItem::quit(app, Some("Quit AI Phone"))?,
                    ],
                )?;
                let edit = Submenu::with_items(
                    app,
                    "Edit",
                    true,
                    &[
                        &PredefinedMenuItem::undo(app, None)?,
                        &PredefinedMenuItem::redo(app, None)?,
                        &PredefinedMenuItem::separator(app)?,
                        &PredefinedMenuItem::cut(app, None)?,
                        &PredefinedMenuItem::copy(app, None)?,
                        &PredefinedMenuItem::paste(app, None)?,
                        &PredefinedMenuItem::select_all(app, None)?,
                    ],
                )?;
                app.set_menu(Menu::with_items(app, &[&app_menu, &edit])?)?;
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut reachable = match load_connection(&handle) {
                    Ok(url) if probe(&url).await.is_ok() => Some(url),
                    _ => None,
                };
                if reachable.is_none() {
                    reachable = resume_local_host(&handle).await;
                }
                let ui = handle.clone();
                // Commit on the UI thread: opening Connection while probing wins.
                let _ = handle.run_on_main_thread(move || {
                    if ui
                        .state::<Navigation>()
                        .startup_cancelled
                        .load(Ordering::SeqCst)
                    {
                        return;
                    }
                    // A normal restart must preserve the language chosen later in the workbench.
                    if reachable
                        .as_ref()
                        .is_none_or(|url| show_workbench(&ui, url, None).is_err())
                    {
                        let _ = show_connection(&ui);
                    }
                });
            });
            Ok(())
        });
    #[cfg(desktop)]
    let builder = builder.on_menu_event(|app, event| {
        if event.id().as_ref() == "connection" {
            app.state::<Navigation>()
                .startup_cancelled
                .store(true, Ordering::SeqCst);
            let _ = show_connection(app);
        }
    });
    builder
        .build(tauri::generate_context!())
        .expect("Unable to launch AI Phone client")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event
                && bootstrap::is_running(app)
            {
                api.prevent_exit();
                bootstrap::cancel(app);
                let app = app.clone();
                std::thread::spawn(move || {
                    bootstrap::wait_until_idle(&app);
                    app.exit(0);
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    #[test]
    fn language_handoff_is_bounded_and_preserves_server_authority() {
        for locale in ["zh-CN", "en", "zh-TW", "ja", "ko", "de", "fr", "es"] {
            assert_eq!(
                checked_locale(Some(locale.into())).unwrap().as_deref(),
                Some(locale)
            );
            let origin = server_origin(DEFAULT_SERVER).unwrap();
            let url = workbench_url(&origin, Some(locale)).unwrap();
            assert_eq!(url.origin(), origin.origin());
            assert_eq!(url.path(), "/phone");
            assert_eq!(
                url.query_pairs().collect::<Vec<_>>(),
                vec![("language".into(), locale.into())]
            );
        }
        assert!(checked_locale(None).unwrap().is_none());
        for invalid in ["", "de-DE", "en; Path=/", "../../en"] {
            assert!(checked_locale(Some(invalid.into())).is_err());
        }
        let legacy: Connection =
            serde_json::from_str(r#"{"server_url":"http://127.0.0.1:17860"}"#).unwrap();
        assert_eq!(legacy.server_url, DEFAULT_SERVER);
        assert!(
            workbench_url(&server_origin(DEFAULT_SERVER).unwrap(), None)
                .unwrap()
                .query()
                .is_none()
        );
    }
    #[test]
    fn native_configuration_rejects_lookalike_or_non_asset_urls() {
        for address in [
            "tauri://localhost",
            "tauri://localhost/index.html",
            "http://tauri.localhost/",
        ] {
            assert!(local_connection_url(&Url::parse(address).unwrap()));
        }
        for address in [
            "tauri://attacker/",
            "about:blank",
            "https://tauri.localhost.evil.example/",
            "http://tauri.localhost:17860/",
            "file://tauri.localhost/",
            "tauri://localhost/other.html",
            "tauri://localhost/?server=untrusted",
            "http://user@tauri.localhost/",
        ] {
            assert!(
                !local_connection_url(&Url::parse(address).unwrap()),
                "{address}"
            );
        }
    }

    #[test]
    fn switching_servers_replaces_navigation_authority_and_can_roll_back() {
        let policy = Navigation::default();
        let first = server_origin(DEFAULT_SERVER).unwrap();
        let next = server_origin("http://localhost:17860").unwrap();
        assert!(!policy.permits(&first));
        policy.replace(Some(first.clone())).unwrap();
        assert!(policy.permits(&first.join("phone").unwrap()));
        let previous = policy.replace(Some(next.clone())).unwrap();
        assert!(policy.permits(&next.join("phone").unwrap()));
        assert!(!policy.permits(&first));
        assert!(!policy.permits(&Url::parse("http://user@localhost:17860/").unwrap()));
        assert!(!policy.permits(&Url::parse("https://example.com/").unwrap()));
        policy.replace(previous).unwrap();
        assert!(policy.permits(&first));
        assert!(!policy.permits(&next));
    }
    #[test]
    fn connection_is_a_private_origin_without_embedded_secrets() {
        for address in [
            DEFAULT_SERVER,
            "http://localhost:17860",
            "https://10.1.2.3:17860",
            "http://[::1]:17860",
        ] {
            assert!(server_origin(address).is_ok());
        }
        for address in [
            "https://example.com",
            "http://8.8.8.8",
            "file:///etc/passwd",
            "http://user:secret@127.0.0.1:17860",
            "http://127.0.0.1:17860/phone",
            "http://127.0.0.1:17860?token=secret",
        ] {
            assert!(server_origin(address).is_err());
        }
    }

    #[test]
    fn probe_checks_real_http_status_type_and_refuses_redirects() {
        for (status, content_type, expected) in [
            (200, "text/html; charset=utf-8", true),
            (200, "application/json", false),
            (503, "text/html", false),
            (302, "text/html", false),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = [0; 4096];
                let length = stream.read(&mut bytes).unwrap();
                assert!(
                    String::from_utf8_lossy(&bytes[..length]).starts_with("HEAD /phone HTTP/1.1")
                );
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: {content_type}\r\nLocation: /other\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            });
            let url = server_origin(&format!("http://{address}")).unwrap();
            assert_eq!(
                tauri::async_runtime::block_on(probe(&url)).is_ok(),
                expected
            );
            server.join().unwrap();
        }
    }
}
