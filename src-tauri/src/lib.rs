pub mod browser_integration;
mod commands;
mod downloader;
mod errors;
mod extractors;
mod filesystem;
mod models;
mod process;
mod setup;
mod startup;

use browser_integration::ipc::BrowserIntegration;
use commands::discover_dependencies;
use downloader::DownloadManager;
use serde::Serialize;
use startup::{first_url_from_args, wants_tray};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};

/// A URL (and whether it was a Quick Download) Pully was launched with —
/// either a direct argument, resolved from a `.url` file, or a launch
/// `pully-native-host` triggered on the browser's behalf — that the
/// frontend hasn't picked up yet. Cleared once read via `take_pending_url`,
/// so a later plain relaunch doesn't re-open it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingLaunch {
    pub url: String,
    pub quick: bool,
}

pub struct PendingUrl(pub Mutex<Option<PendingLaunch>>);

/// Mirrors the user's "Run in system tray" setting (see `App.tsx` /
/// Settings). While `true`, closing the window hides it instead of quitting
/// — Pully keeps running so browser-triggered Quick Downloads and passive
/// tab detection keep working. Quitting for real happens from the tray
/// icon's menu. Defaults to `true`: that's the whole point of tray mode
/// being available at all.
pub struct TraySettings {
    pub run_in_tray: AtomicBool,
}

impl Default for TraySettings {
    fn default() -> Self {
        Self {
            run_in_tray: AtomicBool::new(true),
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // The app (and its frontend) is already running by the time this
            // fires, so an event reaches it directly — unlike the very first
            // launch, which is handled through `PendingUrl` in `setup` below
            // because the frontend hasn't mounted its listener yet. A relaunch
            // via a desktop shortcut / `.url` file always means "show me the
            // app", regardless of tray mode.
            if let Some(url) = first_url_from_args(&argv) {
                let _ = app.emit("external-url", url);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let launch_args: Vec<String> = std::env::args().collect();
            let pending = first_url_from_args(&launch_args).map(|url| PendingLaunch {
                url,
                quick: wants_tray(&launch_args),
            });
            let start_hidden = pending.as_ref().is_some_and(|p| p.quick);
            app.manage(PendingUrl(Mutex::new(pending)));
            app.manage(TraySettings::default());

            let dependencies = discover_dependencies(app.handle());
            let yt_dlp = dependencies
                .yt_dlp
                .clone()
                .unwrap_or_else(|| std::path::PathBuf::from("yt-dlp"));
            app.manage(DownloadManager::new(
                yt_dlp,
                dependencies.spotiflac.clone(),
                dependencies.ffmpeg.clone(),
                dependencies.deno.clone(),
            ));
            let native_host = dependencies.native_host.clone();
            app.manage(Mutex::new(dependencies));

            let browser_integration = BrowserIntegration::default();
            if let Some(token) = browser_integration::pipe_auth::issue_token() {
                let _ = browser_integration.expected_token.set(token);
            } else {
                eprintln!(
                    "[browser-integration] could not write the pipe auth token; \
                     browser-triggered actions will be rejected until Pully restarts \
                     with a writable local app data directory."
                );
            }
            app.manage(browser_integration);
            browser_integration::ipc::start(app.handle().clone());

            if let Some(window) = app.get_webview_window("main") {
                // A Quick Download launched us while nobody asked to see the
                // window — keep it out of the way in the tray instead of
                // flashing it open just to hide it again a moment later.
                if start_hidden {
                    let _ = window.hide();
                }
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        let run_in_tray = app_handle
                            .state::<TraySettings>()
                            .run_in_tray
                            .load(Ordering::Relaxed);
                        if run_in_tray {
                            api.prevent_close();
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.hide();
                            }
                        }
                    }
                });
            }

            let show_item = MenuItem::with_id(app, "show", "Show Pully", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit Pully", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let mut tray_builder = TrayIconBuilder::new().tooltip("Pully");
            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            tray_builder
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Registry helper processes and disk writes must not delay the
            // event loop. All command and browser state is ready first.
            tauri::async_runtime::spawn_blocking(move || {
                browser_integration::launcher::record_app_path();
                if let Some(host) = native_host {
                    browser_integration::native_messaging::register(&host);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::analyze_url,
            commands::dependency_info,
            commands::start_download,
            commands::cancel_download,
            commands::remove_download,
            commands::retry_download,
            commands::set_concurrency,
            commands::open_download,
            commands::take_pending_url,
            commands::browser_tabs,
            commands::browser_connection_status,
            commands::set_browser_integration_enabled,
            commands::push_theme,
            commands::set_run_in_tray,
            commands::install_dependencies,
            commands::install_spotiflac
        ])
        .run(tauri::generate_context!())
        .expect("error while running Pully");
}
