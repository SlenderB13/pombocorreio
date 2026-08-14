mod discovery;
mod models;
mod server;
mod settings;
mod state;
mod storage;
mod transfer;

#[cfg(desktop)]
mod tray;

use discovery::start_discovery;
use server::start_http_server;
use settings::{load_settings, persist};
use state::{answer_offer, snapshot, CoreState, Inner};
use std::{
    collections::HashMap,
    net::TcpListener,
    sync::{Arc, Mutex},
};
use tauri::Manager;
use transfer::send_files;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init());

    #[cfg(target_os = "android")]
    let builder = builder.plugin(tauri_plugin_pombo_inbox::init());

    builder
        .setup(|app| {
            let root = app
                .path()
                .app_local_data_dir()
                .map_err(std::io::Error::other)?;
            let downloads = app.path().download_dir().ok();
            let settings = load_settings(root.clone(), downloads).map_err(std::io::Error::other)?;
            let state = CoreState(Arc::new(Mutex::new(Inner {
                settings,
                settings_path: root.join("settings.json"),
                peers: HashMap::new(),
                offers: HashMap::new(),
            })));

            {
                let inner = state.0.lock().expect("state lock");
                persist(&inner).map_err(std::io::Error::other)?;
            }

            let listener = TcpListener::bind("0.0.0.0:0")?;
            let port = listener.local_addr()?.port();
            start_http_server(listener, state.clone(), app.handle().clone())
                .map_err(std::io::Error::other)?;
            let mdns = start_discovery(port, state.clone(), app.handle().clone())
                .map_err(std::io::Error::other)?;

            app.manage(state);
            app.manage(mdns);

            #[cfg(desktop)]
            tray::setup_tray(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![snapshot, answer_offer, send_files])
        .run(tauri::generate_context!())
        .expect("error while running Pombo Correio");
}
