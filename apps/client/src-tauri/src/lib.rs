use std::{
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{Manager, Url, WebviewUrl, WebviewWindowBuilder};
mod bootstrap;
mod copilot;
mod updates;

static WORKBENCH_URL: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("http://127.0.0.1:17860/phone").expect("valid fixed workbench URL")
});

#[derive(Default)]
struct Startup(AtomicBool);

fn is_setup_url(url: &Url) -> bool {
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

impl Startup {
    fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

fn is_workbench_origin(url: &Url) -> bool {
    url.scheme() == WORKBENCH_URL.scheme()
        && url.host() == WORKBENCH_URL.host()
        && url.port() == WORKBENCH_URL.port()
        && url.username().is_empty()
        && url.password().is_none()
}

async fn probe_workbench(url: Url) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(6))
        .build()
    else {
        return false;
    };
    client.head(url).send().await.is_ok_and(|response| {
        response.status().is_success()
            && response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.starts_with("text/html"))
    })
}

fn show_setup(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("setup") {
        return window.set_focus().map_err(|e| e.to_string());
    }
    WebviewWindowBuilder::new(app, "setup", WebviewUrl::App("index.html".into()))
        .title("")
        .inner_size(680., 600.)
        .resizable(false)
        .on_navigation(is_setup_url)
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

fn workbench_url(locale: Option<&str>) -> Url {
    let mut url = WORKBENCH_URL.clone();
    if let Some(locale) = locale {
        url.query_pairs_mut().append_pair("language", locale);
    }
    url
}

fn show_workbench(app: &tauri::AppHandle, locale: Option<&str>) -> Result<(), String> {
    let url = workbench_url(locale);
    let label = "workbench";
    if let Some(window) = app.get_webview_window(label) {
        window
            .navigate(url)
            .and_then(|_| window.set_focus())
            .map_err(|e| e.to_string())?;
    } else {
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
            .title("AImanity")
            .inner_size(width, height)
            .min_inner_size(700., 600.)
            .on_navigation(is_workbench_origin)
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .build()
            .map_err(|e| e.to_string())?;
    }
    if let Some(window) = app.get_webview_window("setup") {
        let _ = window.close();
    }
    Ok(())
}

fn require_setup_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|e| e.to_string())?;
    if window.label() != "setup" || !is_setup_url(&url) {
        return Err("Native configuration is available only in the setup window".into());
    }
    Ok(())
}

fn require_workbench_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|e| e.to_string())?;
    if window.label() == "workbench" && is_workbench_origin(&url) && url.path() == "/phone" {
        Ok(())
    } else {
        Err("This command is available only in the local workbench".into())
    }
}

#[tauri::command]
async fn check_client_update(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Option<updates::UpdateDetails>, String> {
    require_workbench_window(&window)?;
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
    require_workbench_window(&window)?;
    let version = semver::Version::parse(&version).map_err(|error| error.to_string())?;
    updates::apply(&app, Some(version))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn deployment_status(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<bootstrap::Snapshot, String> {
    require_setup_window(&window)?;
    bootstrap::snapshot(&app)
}
#[tauri::command]
fn start_deployment(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    request: bootstrap::Request,
) -> Result<(), String> {
    require_setup_window(&window)?;
    app.state::<Startup>().cancel();
    bootstrap::start(app, request)
}
#[tauri::command]
fn cancel_deployment(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    require_setup_window(&window)?;
    app.state::<Startup>().cancel();
    bootstrap::cancel(&app);
    Ok(())
}
#[tauri::command]
fn accept_deployment_license(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    require_setup_window(&window)?;
    bootstrap::accept_license(&app)
}

#[tauri::command]
async fn open_workbench(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    locale: Option<String>,
) -> Result<(), String> {
    require_setup_window(&window)?;
    let locale = checked_locale(locale)?;
    app.state::<Startup>().cancel();
    if !bootstrap::ready(&app) || !probe_workbench(workbench_url(None)).await {
        return Err("workbench_unavailable".into());
    }
    show_workbench(&app, locale.as_deref())
}

async fn resume_local_host(app: &tauri::AppHandle, upgrade: bool) -> bool {
    if app.state::<Startup>().cancelled()
        || !bootstrap::resume_installed(app, upgrade).unwrap_or(false)
    {
        return false;
    }
    let setup = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = show_setup(&setup);
    });
    let waiting = app.clone();
    tauri::async_runtime::spawn_blocking(move || bootstrap::wait_until_idle(&waiting))
        .await
        .is_ok()
        && bootstrap::ready(app)
        && probe_workbench(workbench_url(None)).await
}

pub fn run() {
    {
        let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
        if let Some(code) = updates::run(&arguments) {
            std::process::exit(code);
        }
        if let [command, folder] = arguments.as_slice()
            && command == "--verify-bundled-host"
        {
            if let Err(error) = bootstrap::verify_bundled_host(std::path::Path::new(folder)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
            std::process::exit(0);
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
        .manage(Startup::default())
        .manage(bootstrap::Bootstrap::default())
        .manage(copilot::Voice::default())
        .invoke_handler(tauri::generate_handler![
            open_workbench,
            deployment_status,
            start_deployment,
            cancel_deployment,
            accept_deployment_license,
            check_client_update,
            install_client_update,
            copilot::native_voice
        ])
        .setup(|app| {
            updates::cleanup_current(app.handle());
            {
                use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
                let setup = MenuItem::with_id(app, "setup", "Setup…", true, Some("CmdOrCtrl+,"))?;
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
                    "AImanity",
                    true,
                    &[
                        &setup,
                        &update,
                        &PredefinedMenuItem::separator(app)?,
                        &PredefinedMenuItem::quit(app, Some("Quit AImanity"))?,
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
                #[cfg(any(target_os = "macos", windows))]
                let upgrade = bootstrap::upgrade_needed(&handle).unwrap_or(false);
                #[cfg(not(any(target_os = "macos", windows)))]
                let upgrade = false;
                let reachable = !upgrade
                    && bootstrap::installed(&handle).unwrap_or(false)
                    && probe_workbench(workbench_url(None)).await
                    || resume_local_host(&handle, upgrade).await;
                let ui = handle.clone();
                // Opening Setup while probing takes precedence over auto-open.
                let _ = handle.run_on_main_thread(move || {
                    if ui.state::<Startup>().cancelled() {
                        return;
                    }
                    // A normal restart must preserve the language chosen later in the workbench.
                    if !reachable || show_workbench(&ui, None).is_err() {
                        let _ = show_setup(&ui);
                    }
                });
            });
            Ok(())
        });
    let builder = builder.on_menu_event(|app, event| match event.id().as_ref() {
        "setup" => {
            app.state::<Startup>().cancel();
            let _ = show_setup(app);
        }
        updates::ID => updates::select(app),
        _ => {}
    });
    builder
        .build(tauri::generate_context!())
        .expect("Unable to launch AImanity client")
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
    fn language_handoff_is_bounded_and_preserves_local_workbench() {
        for locale in [
            "zh-CN", "en", "zh-TW", "ja", "ko", "de", "fr", "es", "ru", "ar",
        ] {
            assert_eq!(
                checked_locale(Some(locale.into())).unwrap().as_deref(),
                Some(locale)
            );
            let url = workbench_url(Some(locale));
            assert!(is_workbench_origin(&url));
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
        assert!(workbench_url(None).query().is_none());
    }
    #[test]
    fn native_configuration_rejects_lookalike_or_non_asset_urls() {
        for address in [
            "tauri://localhost",
            "tauri://localhost/index.html",
            "http://tauri.localhost/",
        ] {
            assert!(is_setup_url(&Url::parse(address).unwrap()));
        }
        for address in [
            "tauri://attacker/",
            "about:blank",
            "https://tauri.localhost.evil.example/",
            "http://tauri.localhost:17860/",
            "file://tauri.localhost/",
            "tauri://localhost/other.html",
            "tauri://localhost/?language=en",
            "http://user@tauri.localhost/",
        ] {
            assert!(!is_setup_url(&Url::parse(address).unwrap()), "{address}");
        }
    }

    #[test]
    fn workbench_navigation_keeps_the_fixed_local_origin() {
        assert!(is_workbench_origin(&workbench_url(None)));
        assert!(is_workbench_origin(
            &workbench_url(None).join("/login").unwrap()
        ));
        for address in [
            "http://10.1.2.3:17860/phone",
            "http://127.0.0.1:17861/phone",
            "https://127.0.0.1:17860/phone",
            "http://user@127.0.0.1:17860/phone",
        ] {
            assert!(!is_workbench_origin(&Url::parse(address).unwrap()));
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
            let url = Url::parse(&format!("http://{address}/phone")).unwrap();
            assert_eq!(
                tauri::async_runtime::block_on(probe_workbench(url)),
                expected
            );
            server.join().unwrap();
        }
    }
}
