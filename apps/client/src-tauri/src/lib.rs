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
mod updates;

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
    fn cancel_startup(&self) {
        self.startup_cancelled.store(true, Ordering::SeqCst);
    }

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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Connection {
            server_url: DEFAULT_SERVER.into(),
        },
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
        return window.set_focus().map_err(|e| e.to_string());
    }
    WebviewWindowBuilder::new(app, "connection", WebviewUrl::App("index.html".into()))
        .title("AI Phone")
        .inner_size(680., 620.)
        .min_inner_size(360., 260.)
        .resizable(true)
        .on_navigation(local_connection_url)
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn checked_locale(locale: Option<String>) -> Result<Option<String>, String> {
    match locale.as_deref() {
        None => Ok(None),
        Some("zh-CN" | "en" | "zh-TW" | "ja" | "ko" | "de" | "fr" | "es" | "ru" | "ar") => {
            Ok(locale)
        }
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
    let label = "workbench";
    let result = if let Some(window) = app.get_webview_window(label) {
        window
            .navigate(url)
            .and_then(|_| window.set_focus())
            .map_err(|e| e.to_string())
    } else {
        let handle = app.clone();
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
        WebviewWindowBuilder::new(app, label, WebviewUrl::External(url))
            .title("AI Phone")
            .inner_size(width, height)
            .min_inner_size(700., 600.)
            .on_navigation(move |url| handle.state::<Navigation>().permits(url))
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .build()
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    if let Err(error) = result {
        navigation.replace(previous)?;
        return Err(error);
    }
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

fn require_update_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|e| e.to_string())?;
    if window.label() == "workbench"
        && url.scheme() == "http"
        && url.port() == Some(17860)
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
        && url.path() == "/phone"
    {
        Ok(())
    } else {
        Err("Use the native update menu for this connection".into())
    }
}

#[tauri::command]
async fn check_client_update(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Option<updates::UpdateDetails>, String> {
    require_update_window(&window)?;
    updates::refresh(&app)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn install_client_update(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    version: String,
) -> Result<(), String> {
    require_update_window(&window)?;
    let version = semver::Version::parse(&version).map_err(|error| error.to_string())?;
    updates::apply(&app, Some(version))
        .await
        .map_err(|error| error.to_string())
}

// Only the local setup asset may request content-sized native chrome.
#[tauri::command]
fn fit_setup(window: tauri::WebviewWindow, height: f64) -> Result<(), String> {
    require_local_window(&window)?;
    if height.is_finite()
        && !window.is_maximized().unwrap_or(false)
        && !window.is_fullscreen().unwrap_or(false)
    {
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let size = window
            .inner_size()
            .map_err(|e| e.to_string())?
            .to_logical::<f64>(scale);
        let maximum = window
            .current_monitor()
            .ok()
            .flatten()
            .map(|m| m.work_area().size.height as f64 / m.scale_factor() - 80.)
            .unwrap_or(800.)
            .max(260.);
        let height = height.clamp(260., maximum);
        if (size.height - height).abs() > 2. {
            window
                .set_size(tauri::LogicalSize::new(size.width, height))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
fn current_server(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<String, String> {
    require_local_window(&window)?;
    load_connection(&app).map(|url| url.origin().ascii_serialization())
}

#[tauri::command]
fn deployment_status(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<bootstrap::Snapshot, String> {
    require_local_window(&window)?;
    bootstrap::snapshot(&app)
}
#[tauri::command]
fn start_deployment(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    request: bootstrap::Request,
) -> Result<(), String> {
    require_local_window(&window)?;
    app.state::<Navigation>().cancel_startup();
    bootstrap::start(app, request)
}
#[tauri::command]
fn cancel_deployment(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    require_local_window(&window)?;
    app.state::<Navigation>().cancel_startup();
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
    app.state::<Navigation>().cancel_startup();
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

async fn resume_local_host(app: &tauri::AppHandle, upgrade: bool) -> Option<Url> {
    if app
        .state::<Navigation>()
        .startup_cancelled
        .load(Ordering::SeqCst)
        || !bootstrap::resume_installed(app, upgrade).unwrap_or(false)
    {
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

pub fn run() {
    {
        let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
        if let Some(code) = updates::run(&arguments) {
            std::process::exit(code);
        }
        #[cfg(windows)]
        if arguments.len() == 1 && arguments[0] == "--stop-windows-backend" {
            let result = std::env::var_os("APPDATA")
                .map(std::path::PathBuf::from)
                .ok_or("windows_app_data")
                .and_then(|home| {
                    let home = home.join("dev.aiphone.client/host");
                    tauri::async_runtime::block_on(
                        ai_phone_host_runtime::qemu_host::stop_installed(&home),
                    )
                    .map_err(|error| error.code())
                });
            std::process::exit(if result.is_ok() { 0 } else { 1 });
        }
        #[cfg(windows)]
        if arguments.len() == 1 && arguments[0] == "--prepare-windows-host" {
            match bootstrap::prepare_windows_host() {
                Ok(code) => std::process::exit(code),
                Err(error) => {
                    println!("{error}");
                    std::process::exit(1);
                }
            }
        }
        if let Some(result) = ai_phone_host_runtime::component::run(&arguments) {
            match result {
                Ok(()) => std::process::exit(0),
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
            }
        }
    }
    let builder = tauri::Builder::default()
        .manage(Connecting(AtomicBool::new(false)))
        .manage(Navigation::default())
        .manage(bootstrap::Bootstrap::default())
        .invoke_handler(tauri::generate_handler![
            fit_setup,
            current_server,
            connect_server,
            deployment_status,
            start_deployment,
            cancel_deployment,
            accept_deployment_license,
            check_client_update,
            install_client_update
        ])
        .setup(|app| {
            updates::cleanup_current(app.handle());
            {
                use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
                let connection =
                    MenuItem::with_id(app, "connection", "Setup…", true, Some("CmdOrCtrl+,"))?;
                let update = MenuItem::with_id(
                    app,
                    updates::ID,
                    updates::CHECK,
                    updates::platform().is_some(),
                    None::<&str>,
                )?;
                app.manage(updates::Menu::new(update.clone()));
                let app_menu = Submenu::with_items(
                    app,
                    "AI Phone",
                    true,
                    &[
                        &connection,
                        &update,
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
                let saved = load_connection(&handle);
                #[cfg(any(target_os = "macos", windows))]
                let upgrade = saved.as_ref().is_ok_and(|url| {
                    url.port() == Some(17860)
                        && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
                }) && bootstrap::upgrade_needed(&handle).unwrap_or(false);
                #[cfg(not(any(target_os = "macos", windows)))]
                let upgrade = false;
                let reachable = match saved {
                    _ if upgrade => resume_local_host(&handle, true).await,
                    Ok(url) if probe(&url).await.is_ok() => Some(url),
                    _ => resume_local_host(&handle, false).await,
                };
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
    let builder = builder.on_menu_event(|app, event| match event.id().as_ref() {
        "connection" => {
            app.state::<Navigation>().cancel_startup();
            let _ = show_connection(app);
        }
        updates::ID => updates::select(app),
        _ => {}
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
        for locale in [
            "zh-CN", "en", "zh-TW", "ja", "ko", "de", "fr", "es", "ru", "ar",
        ] {
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
