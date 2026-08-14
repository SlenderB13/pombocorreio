use crate::discovery::DiscoveryService;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, WebviewWindow, Window, WindowEvent,
};

#[cfg(target_os = "linux")]
fn repair_wayland_titlebar(window: &WebviewWindow) {
    use gtk::prelude::*;

    let Ok(gtk_window) = window.gtk_window() else {
        return;
    };
    let Some(titlebar) = gtk_window.titlebar() else {
        return;
    };
    let Ok(event_box) = titlebar.downcast::<gtk::EventBox>() else {
        return;
    };

    // Tao 0.35 places this box above its HeaderBar on Wayland, so it
    // intercepts clicks intended for the native window controls.
    event_box.set_above_child(false);
}

#[cfg(not(target_os = "linux"))]
fn repair_wayland_titlebar(_window: &WebviewWindow) {}

fn keep_discovery_visible(app: &AppHandle) {
    if let Err(error) = app.state::<DiscoveryService>().reannounce() {
        eprintln!("could not re-announce Pombo Correio: {error}");
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        repair_wayland_titlebar(&window);
        let _ = window.set_focus();
    }
    keep_discovery_visible(app);
}

pub(crate) fn handle_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }

    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        if let Err(error) = window.hide() {
            eprintln!("could not hide Pombo Correio window: {error}");
        }
        keep_discovery_visible(window.app_handle());
    }
}

pub(crate) fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Send files…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let tray_icon = Image::from_bytes(include_bytes!("../icons/tray-mail.png"))?;

    TrayIconBuilder::new()
        .icon(tray_icon)
        .tooltip("Pombo Correio")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    if let Some(window) = app.get_webview_window("main") {
        repair_wayland_titlebar(&window);
    }
    Ok(())
}
