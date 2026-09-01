pub mod models;

/// Initialize COM in Multi-Threaded Apartment mode on Windows worker threads.
/// Tauri's main thread uses STA (required for WebView2/DragDrop), so any spawned
/// background threads that touch COM APIs (e.g., Actix, Tokio, networking)
/// must explicitly init COM as MTA to avoid OLE_E_WRONGCOMPOBJ / RPC_E_CHANGED_MODE
/// errors during startup and teardown.
#[cfg(target_os = "windows")]
fn init_com_on_worker_thread() {
    extern "system" {
        fn CoInitializeEx(reserved: *const std::ffi::c_void, coinit: u32) -> i32;
    }
    const COINIT_MULTITHREADED: u32 = 0x0;
    // HRESULT codes
    const S_OK: i32 = 0;
    const S_FALSE: i32 = 1;
    const RPC_E_CHANGED_MODE: i32 = -2147417850; // 0x80010106

    let hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED) };
    match hr {
        S_OK | S_FALSE => {
            log::info!("COM MTA initialized on worker thread (hr=0x{:x})", hr as u32);
        }
        RPC_E_CHANGED_MODE => {
            // Thread was already initialized with a different apartment model.
            // This is non-fatal; the existing mode will be used.
            log::warn!(
                "COM already initialized in a different mode on this worker thread (hr=0x{:x})",
                hr as u32
            );
        }
        _ => {
            log::error!(
                "Failed to initialize COM on worker thread (hr=0x{:x})",
                hr as u32
            );
        }
    }
}

pub mod commands;
pub mod bandwidth;
pub mod transfer_policy;
pub mod transfer_retry;
pub mod split_manifest;
pub mod transfer_log;
pub mod failure_classifier;
pub mod split_upload_resume;

use tauri::{Manager, Emitter};


use tokio::sync::Mutex;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use commands::TelegramState;
use commands::streaming::StreamConfig;
use rand::Rng;


pub mod server;
pub mod api_routes;
pub mod db;
pub mod share_routes;
pub mod mp4_utils;


/// Single source of truth for the Actix streaming server port.
/// Referenced in lib.rs (server startup) and exposed to the frontend
/// via cmd_get_stream_info so no component ever hardcodes the port.
pub const STREAM_PORT: u16 = 14201;

/// Generate a random 32-character hex token for streaming server auth
fn generate_stream_token() -> String {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..16).map(|_| rng.random()).collect();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Holds the Actix-web server stop handle so we can shut it down
/// from the RunEvent::Exit handler for graceful Ctrl+C termination.
pub struct ActixServerHandle(pub Arc<std::sync::Mutex<Option<actix_web::dev::ServerHandle>>>);

/// Tracks whether the API server is currently running (for the frontend status dot)
pub struct ApiServerRunning(pub Arc<std::sync::atomic::AtomicBool>);

/// Holds the API server stop handle separately so we can restart it independently
pub struct ApiServerHandle(pub Arc<std::sync::Mutex<Option<actix_web::dev::ServerHandle>>>);

/// Restart (or stop) the API server based on current settings.
/// Called from Tauri commands when the user changes API settings.
pub fn restart_api_server(app: &tauri::AppHandle) {
    // Stop existing API server if running
    let api_handle_arc = app.state::<ApiServerHandle>().0.clone();
    let old_handle = api_handle_arc.lock().ok().and_then(|mut g| g.take());
    if let Some(handle) = old_handle {
        log::info!("Stopping existing API server...");
        drop(handle.stop(true));
        // Give it a moment to release the port
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    let settings = commands::api_settings::load_settings(app);
    let running_flag = app.state::<ApiServerRunning>().0.clone();

    if !settings.enabled {
        running_flag.store(false, std::sync::atomic::Ordering::Relaxed);
        log::info!("API server disabled");
        return;
    }

    // Need TelegramState to share with the API server
    let tg_state = Arc::new(app.state::<TelegramState>().inner().clone());
    let bw_manager = app.state::<Arc<bandwidth::BandwidthManager>>().inner().clone();
    let net_config = app.state::<Arc<transfer_policy::TransferPolicy>>().inner().clone();
    let db_pool = app.state::<db::DbConnection>().inner().clone();
    let api_port = settings.port;
    let key_hash = settings.key_hash.clone();
    let handle_for_thread = api_handle_arc.clone();

    // Resolve cache dirs before the thread spawn since app is a reference
    let preview_dir = app.path().app_cache_dir().unwrap_or_default().join("previews");
    let thumbnail_dir = app.path().app_data_dir().unwrap_or_default().join("thumbnails");

    std::thread::spawn(move || {
        #[cfg(target_os = "windows")]
        init_com_on_worker_thread();
        let sys = actix_rt::System::new();
        sys.block_on(async move {
            let api_state_data = actix_web::web::Data::new(tg_state);
            let api_state = actix_web::web::Data::new(api_routes::ApiState {
                key_hash,
            });
            let cache_dirs = actix_web::web::Data::new(api_routes::CacheDirs {
                thumbnail_dir,
                preview_dir,
            });
            let api_bw = actix_web::web::Data::new(bw_manager);
            let api_net = actix_web::web::Data::new(net_config);
            let api_db = actix_web::web::Data::new(db_pool);

            log::info!("Starting REST API server on port {}", api_port);

            match actix_web::HttpServer::new(move || {
                let cors = actix_cors::Cors::default()
                    .allowed_origin_fn(|origin, _req_head| {
                        let origin_bytes = origin.as_bytes();
                        origin_bytes.starts_with(b"tauri://")
                            || origin_bytes.starts_with(b"http://tauri.localhost")
                            || origin_bytes.starts_with(b"https://tauri.localhost")
                            || origin_bytes.starts_with(b"http://localhost")
                            || origin_bytes.starts_with(b"http://127.0.0.1")
                            || origin_bytes.starts_with(b"https://asset.localhost")
                            || origin_bytes.starts_with(b"http://asset.localhost")
                            || origin_bytes == b"null"
                    })
                    .allow_any_method()
                    .allow_any_header();

                actix_web::App::new()
                    .wrap(cors)
                    .app_data(api_state_data.clone())
                    .app_data(api_state.clone())
                    .app_data(cache_dirs.clone())
                    .app_data(api_bw.clone())
                    .app_data(api_net.clone())
                    .app_data(api_db.clone())
                    .configure(api_routes::configure_api)
            })
            .bind(("127.0.0.1", api_port)) {
                Ok(bound) => {
                    let server = bound.run();
                    *handle_for_thread.lock().unwrap() = Some(server.handle());
                    running_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    log::info!("REST API server started on http://127.0.0.1:{}", api_port);
                    server.await.ok();
                }
                Err(e) => {
                    running_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                    log::error!("Failed to start API server on port {}: {}", api_port, e);
                }
            }
        });
    });
}

#[tauri::command]
fn cmd_open_file_externally(path: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app_handle
        .opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Gather system diagnostics and environment info for debugging.
/// Returns a formatted string suitable for copying to clipboard.
#[tauri::command]
fn cmd_get_system_diagnostics(
    app: tauri::AppHandle,
) -> Result<String, String> {
    let mut lines: Vec<String> = Vec::new();

    lines.push("=== TeleStash Diagnostics ===".into());
    lines.push(format!("Package: {}", env!("CARGO_PKG_NAME")));
    lines.push(format!("Version: {}", env!("CARGO_PKG_VERSION")));

    // OS info
    lines.push(format!("OS: {} {}", std::env::consts::OS, std::env::consts::ARCH));

    lines.push("Package Type: Windows installer".to_string());

    // App data dir
    if let Ok(dir) = app.path().app_data_dir() {
        lines.push(format!("App Data: {}", dir.display()));
    }

    lines.push("==================================".into());

    Ok(lines.join("\n"))
}

pub fn run() {
    env_logger::init();

    // libsql (grammers-session's storage) asserts in its global init that
    // SQLITE_CONFIG_SERIALIZED can still be set; sqlite3_config returns
    // SQLITE_MISUSE once any SQLite in this process has been initialized,
    // and db.rs opens its databases during setup — so libsql must go first
    // or the panic kills the session task and session restore hangs forever.
    {
        let warm_path = std::env::temp_dir().join("telestash-libsql-warmup.session");
        let _ = tauri::async_runtime::block_on(async {
            let _ = grammers_session::storages::SqliteSession::open(&warm_path).await;
        });
        let _ = std::fs::remove_file(&warm_path);
        log::info!("libsql threading config warmed up before other SQLite users");
    }

    let stream_token = generate_stream_token();

    // Shared handle for stopping the Actix streaming server during shutdown
    let server_handle: Arc<std::sync::Mutex<Option<actix_web::dev::ServerHandle>>> =
        Arc::new(std::sync::Mutex::new(None));
    let server_handle_for_setup = server_handle.clone();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init());

    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    let builder = builder.plugin(tauri_plugin_window_state::Builder::default().build());

    let app = builder
        .setup(move |app| {
            app.manage(TelegramState {
                client: Arc::new(Mutex::new(None)),
                login_token: Arc::new(Mutex::new(None)),
                password_token: Arc::new(Mutex::new(None)),
                api_id: Arc::new(Mutex::new(None)),
                runner_shutdown: Arc::new(std::sync::Mutex::new(None)),
                runner_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
                peer_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
                cancelled_transfers: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
                paused_transfers: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
                pause_notifiers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            });
            app.manage(Arc::new(bandwidth::BandwidthManager::new(app.handle())));
            app.manage(StreamConfig { token: stream_token.clone(), port: STREAM_PORT });
            app.manage(ActixServerHandle(server_handle_for_setup.clone()));
            app.manage(ApiServerHandle(Arc::new(std::sync::Mutex::new(None))));
            app.manage(ApiServerRunning(Arc::new(std::sync::atomic::AtomicBool::new(false))));

            let net_config = Arc::new(transfer_policy::TransferPolicy::new());
            app.manage(net_config.clone());
            app.manage(commands::english_cc::EnglishCcManager::new());
            
            // Initialize SQLite Database
            let db_pool = db::init_db(app.handle()).map_err(|e| {
                log::error!("Failed to initialize SQLite database: {}", e);
                e
            })?;
            app.manage(db_pool.clone());
            
            let state = Arc::new(app.state::<TelegramState>().inner().clone());
            let token_for_server = stream_token.clone();
            let handle_for_thread = server_handle_for_setup.clone();
            let db_pool_for_server = db_pool.clone();
            let app_handle_for_server = app.handle().clone();
            std::thread::spawn(move || {
                init_com_on_worker_thread();
                let sys = actix_rt::System::new();
                sys.block_on(async move {
                    match server::start_server(state, STREAM_PORT, token_for_server, db_pool_for_server, app_handle_for_server).await {
                        Ok(server) => {
                            if let Ok(mut handle) = handle_for_thread.lock() {
                                *handle = Some(server.handle());
                            }
                            server.await.ok();
                        }
                        Err(e) => log::error!("Streaming server failed: {}", e),
                    }
                });
            });

            // Start API server if enabled in settings
            restart_api_server(app.handle());

            // Windows System Tray Integration
            let show_item = tauri::menu::MenuItemBuilder::with_id("show", "Open TeleStash").build(app);
            let updates_item = tauri::menu::MenuItemBuilder::with_id("check_updates", "Check for Updates").build(app);
            let settings_item = tauri::menu::MenuItemBuilder::with_id("settings", "Settings").build(app);
            let quit_item = tauri::menu::MenuItemBuilder::with_id("quit", "Exit").build(app);
            let sep1 = tauri::menu::PredefinedMenuItem::separator(app);
            let sep2 = tauri::menu::PredefinedMenuItem::separator(app);

            if let (Ok(show), Ok(updates), Ok(settings), Ok(quit), Ok(s1), Ok(s2)) = (
                show_item, updates_item, settings_item, quit_item, sep1, sep2
            ) {
                if let Ok(tray_menu) = tauri::menu::MenuBuilder::new(app).items(&[
                    &show,
                    &s1,
                    &updates,
                    &settings,
                    &s2,
                    &quit,
                ]).build() {
                    let icon = app.default_window_icon().cloned();
                    if let Some(icon) = icon {
                        let _ = tauri::tray::TrayIconBuilder::new()
                            .icon(icon)
                            .menu(&tray_menu)
                            .on_menu_event(|app, event| match event.id.as_ref() {
                                "show" => {
                                    if let Some(window) = app.get_webview_window("main") {
                                        let _ = window.show();
                                        let _ = window.unminimize();
                                        let _ = window.set_focus();
                                    }
                                }
                                "check_updates" => {
                                    if let Some(window) = app.get_webview_window("main") {
                                        let _ = window.show();
                                        let _ = window.unminimize();
                                        let _ = window.set_focus();
                                    }
                                    let _ = app.emit("tray-check-updates", ());
                                }
                                "settings" => {
                                    if let Some(window) = app.get_webview_window("main") {
                                        let _ = window.show();
                                        let _ = window.unminimize();
                                        let _ = window.set_focus();
                                    }
                                    let _ = app.emit("tray-open-settings", ());
                                }
                                "quit" => {
                                    app.exit(0);
                                }
                                _ => {}
                            })
                            .on_tray_icon_event(|tray, event| {
                                if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. } = event {
                                    let app = tray.app_handle();
                                    if let Some(window) = app.get_webview_window("main") {
                                        let _ = window.show();
                                        let _ = window.unminimize();
                                        let _ = window.set_focus();
                                    }
                                }
                            })
                            .build(app);
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::cmd_auth_request_code,
            commands::cmd_auth_sign_in,
            commands::cmd_auth_check_password,
            commands::cmd_get_files,
            commands::cmd_upload_file,
            commands::initiate_upload,
            cmd_open_file_externally,
            commands::settings::cmd_set_autostart,
            commands::cmd_connect,
            commands::cmd_log,
            commands::cmd_delete_file,
            commands::cmd_download_file,
            commands::cmd_move_files,
            commands::cmd_create_folder,
            commands::cmd_delete_folder,
            commands::cmd_rename_folder,
            commands::cmd_rename_file,
            commands::cmd_get_bandwidth,
            commands::cmd_delete_preview_for_message,
            commands::cmd_get_preview,
            commands::cmd_clean_preview_cache,
            commands::cmd_logout,
            commands::cmd_generate_english_cc,
            commands::cmd_get_english_cc_status,
            commands::cmd_cancel_english_cc,
            commands::cmd_scan_folders,
            commands::cmd_search_global,
            commands::cmd_check_connection,
            commands::cmd_is_network_available,
            commands::cmd_clean_cache,
            commands::cmd_get_thumbnail,
            commands::cmd_get_stream_info,
            commands::cmd_play_in_mpv,
            commands::cmd_cancel_transfer,
            commands::cmd_pause_transfer,
            commands::cmd_resume_transfer,
            commands::cmd_auth_qr_login,
            commands::cmd_auth_qr_poll,
            commands::cmd_get_api_settings,
            commands::cmd_update_api_settings,
            commands::cmd_regenerate_api_key,
            commands::cmd_delete_image_thumbnail,
            commands::cmd_delete_temp_zip,
            commands::cmd_create_share,
            commands::cmd_list_shares,
            commands::cmd_revoke_share,
            commands::cmd_toggle_folder_visibility,
            commands::cmd_export_folder_invite,
            cmd_get_system_diagnostics,
            commands::cmd_get_video_metadata,
            commands::cmd_list_archive_contents,
            commands::cmd_extract_archive_entry,
            commands::cmd_get_enriched_folders,
            commands::cmd_update_folder_order,
            commands::cmd_create_group,
            commands::cmd_update_group,
            commands::cmd_delete_group,
            commands::cmd_assign_folder_to_group,
            commands::cmd_update_group_order,
            commands::cmd_get_groups,
            commands::cmd_get_video_subtitles,
            commands::cmd_attach_video_subtitles,
            commands::cmd_delete_video_subtitle,
            commands::cmd_list_directory_files,
            transfer_log::cmd_get_transfer_logs,
            transfer_log::cmd_clear_transfer_logs,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::WindowEvent { event: tauri::WindowEvent::CloseRequested { api, .. }, .. } = &event {
            api.prevent_close();
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.hide();
            }
        }
        if let tauri::RunEvent::Exit = event {
            log::info!("Application exiting — shutting down background services...");

            // 1. Shutdown the grammers network runner
            let shutdown_arc = app_handle.state::<TelegramState>().runner_shutdown.clone();
            let runner_tx = shutdown_arc.lock().ok().and_then(|mut g| g.take());
            if let Some(tx) = runner_tx {
                log::info!("Signaling network runner shutdown...");
                let _ = tx.send(());
            }

            // 2. Stop the Actix streaming server (graceful)
            let server_arc = app_handle.state::<ActixServerHandle>().0.clone();
            let server_handle = server_arc.lock().ok().and_then(|mut g| g.take());
            if let Some(handle) = server_handle {
                log::info!("Stopping Actix streaming server...");
                drop(handle.stop(true));
            }

            // 3. Stop the API server (graceful)
            let api_arc = app_handle.state::<ApiServerHandle>().0.clone();
            let api_handle = api_arc.lock().ok().and_then(|mut g| g.take());
            if let Some(handle) = api_handle {
                log::info!("Stopping API server...");
                drop(handle.stop(true));
            }
        }
    });
}

