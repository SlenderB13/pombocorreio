use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
#[cfg(target_os = "android")]
use tauri_plugin_pombo_inbox::InboxExt;
use tiny_http::{Header, Method, Response, Server, StatusCode};
use uuid::Uuid;

const SERVICE_TYPE: &str = "_pombocorreio._tcp.local.";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Peer {
    id: String,
    name: String,
    address: String,
    port: u16,
    trusted: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileMeta {
    name: String,
    size: u64,
}

#[derive(Clone, Deserialize)]
struct SelectedFile {
    path: String,
    name: String,
}

fn open_selected(app: &AppHandle, selected: &SelectedFile) -> Result<File, String> {
    let path = selected
        .path
        .parse::<FilePath>()
        .map_err(|error| format!("{}: {error}", selected.name))?;
    let mut options = OpenOptions::new();
    options.read(true);
    app.fs()
        .open(path, options)
        .map_err(|error| format!("{}: {error}", selected.name))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Offer {
    id: String,
    sender_id: String,
    sender_name: String,
    files: Vec<FileMeta>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    device_id: String,
    device_name: String,
    inbox: String,
    peers: Vec<Peer>,
    incoming: Vec<Offer>,
}

#[derive(Serialize, Deserialize)]
struct Settings {
    device_id: String,
    device_name: String,
    inbox: PathBuf,
    #[serde(default)]
    trusted_devices: HashSet<String>,
}

struct PendingOffer {
    offer: Offer,
    accepted: Option<bool>,
}

struct Inner {
    settings: Settings,
    settings_path: PathBuf,
    peers: HashMap<String, Peer>,
    offers: HashMap<String, PendingOffer>,
}

#[derive(Clone)]
struct CoreState(Arc<Mutex<Inner>>);

fn emit_change(app: &AppHandle) {
    let _ = app.emit("state-changed", ());
}

fn write_json<T: Serialize>(status: u16, value: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_default();
    Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("content-type", "application/json").expect("valid header"))
}

fn safe_destination(inbox: &Path, requested: &str) -> PathBuf {
    let filename = Path::new(requested)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("received-file");
    let candidate = inbox.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("file");
    let ext = Path::new(filename).extension().and_then(|v| v.to_str());
    for n in 1..10_000 {
        let name = match ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = inbox.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    inbox.join(format!("{}-{filename}", Uuid::new_v4()))
}

#[cfg(target_os = "android")]
fn publish_received(app: &AppHandle, path: &Path, name: &str) -> Result<String, String> {
    let source = path
        .to_str()
        .ok_or("received file path is not valid UTF-8")?;
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    let uri = app
        .inbox()
        .publish(source, name, mime.essence_str())
        .map_err(|error| format!("Could not save to Android Downloads: {error}"))?;
    fs::remove_file(path).map_err(|error| format!("Could not remove temporary file: {error}"))?;
    Ok(uri)
}

#[cfg(not(target_os = "android"))]
fn publish_received(_app: &AppHandle, path: &Path, _name: &str) -> Result<String, String> {
    Ok(path.display().to_string())
}

fn start_http_server(
    listener: TcpListener,
    state: CoreState,
    app: AppHandle,
) -> Result<(), String> {
    let server = Server::from_listener(listener, None).map_err(|error| error.to_string())?;
    thread::spawn(move || {
        for mut request in server.incoming_requests() {
            let method = request.method().clone();
            let path = request
                .url()
                .trim_matches('/')
                .split('/')
                .collect::<Vec<_>>();
            if method == Method::Post && path.as_slice() == ["v1", "offers"] {
                let mut body = String::new();
                let result = request
                    .as_reader()
                    .read_to_string(&mut body)
                    .map_err(|e| e.to_string())
                    .and_then(|_| serde_json::from_str::<Offer>(&body).map_err(|e| e.to_string()));
                match result {
                    Ok(offer) => {
                        let trusted = {
                            let inner = state.0.lock().expect("state lock");
                            inner.settings.trusted_devices.contains(&offer.sender_id)
                        };
                        state.0.lock().expect("state lock").offers.insert(
                            offer.id.clone(),
                            PendingOffer {
                                offer,
                                accepted: if trusted { Some(true) } else { None },
                            },
                        );
                        emit_change(&app);
                        let _ = request.respond(write_json(202, &serde_json::json!({"status": if trusted { "accepted" } else { "pending" }})));
                    }
                    Err(error) => {
                        let _ =
                            request.respond(write_json(400, &serde_json::json!({"error": error})));
                    }
                }
            } else if method == Method::Get && path.len() == 3 && path[..2] == ["v1", "offers"] {
                let status = state
                    .0
                    .lock()
                    .expect("state lock")
                    .offers
                    .get(path[2])
                    .map(|pending| match pending.accepted {
                        Some(true) => "accepted",
                        Some(false) => "declined",
                        None => "pending",
                    })
                    .unwrap_or("missing");
                let _ = request.respond(write_json(200, &serde_json::json!({"status": status})));
            } else if method == Method::Post && path.len() == 4 && path[..2] == ["v1", "files"] {
                let transfer_id = path[2];
                let index = path[3].parse::<usize>();
                let target = index.ok().and_then(|index| {
                    let inner = state.0.lock().expect("state lock");
                    let pending = inner.offers.get(transfer_id)?;
                    if pending.accepted != Some(true) {
                        return None;
                    }
                    let file = pending.offer.files.get(index)?;
                    Some(safe_destination(&inner.settings.inbox, &file.name))
                });
                match target {
                    Some(target) => match File::create(&target)
                        .and_then(|mut file| std::io::copy(request.as_reader(), &mut file))
                    {
                        Ok(_) => {
                            let name = target
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("received-file")
                                .to_string();
                            match publish_received(&app, &target, &name) {
                                Ok(saved) => {
                                    emit_change(&app);
                                    let _ = request.respond(write_json(
                                        201,
                                        &serde_json::json!({"saved": saved}),
                                    ));
                                }
                                Err(error) => {
                                    let _ = fs::remove_file(&target);
                                    let _ = request.respond(write_json(
                                        500,
                                        &serde_json::json!({"error": error}),
                                    ));
                                }
                            }
                        }
                        Err(error) => {
                            let _ = request.respond(write_json(
                                500,
                                &serde_json::json!({"error": error.to_string()}),
                            ));
                        }
                    },
                    None => {
                        let _ = request.respond(write_json(
                            403,
                            &serde_json::json!({"error": "transfer was not accepted"}),
                        ));
                    }
                }
            } else {
                let _ =
                    request.respond(write_json(404, &serde_json::json!({"error": "not found"})));
            }
        }
    });
    Ok(())
}

fn start_discovery(port: u16, state: CoreState, app: AppHandle) -> Result<ServiceDaemon, String> {
    let mdns = ServiceDaemon::new().map_err(|error| error.to_string())?;
    let (id, name) = {
        let inner = state.0.lock().expect("state lock");
        (
            inner.settings.device_id.clone(),
            inner.settings.device_name.clone(),
        )
    };
    let properties = [("id", id.as_str()), ("name", name.as_str())];
    let service = ServiceInfo::new(
        SERVICE_TYPE,
        &id,
        &format!("{id}.local."),
        (),
        port,
        &properties[..],
    )
    .map_err(|error| error.to_string())?
    .enable_addr_auto();
    mdns.register(service).map_err(|error| error.to_string())?;
    let receiver = mdns
        .browse(SERVICE_TYPE)
        .map_err(|error| error.to_string())?;
    thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    let Some(peer_id) = info.get_property_val_str("id") else {
                        continue;
                    };
                    if peer_id == id {
                        continue;
                    }
                    let name = info
                        .get_property_val_str("name")
                        .unwrap_or("Nearby device")
                        .to_string();
                    let address = info
                        .get_addresses()
                        .iter()
                        .find(|ip| ip.is_ipv4())
                        .or_else(|| info.get_addresses().iter().next())
                        .map(|ip| ip.to_ip_addr().to_string());
                    if let Some(address) = address {
                        let mut inner = state.0.lock().expect("state lock");
                        let trusted = inner.settings.trusted_devices.contains(peer_id);
                        inner.peers.insert(
                            peer_id.to_string(),
                            Peer {
                                id: peer_id.to_string(),
                                name,
                                address,
                                port: info.get_port(),
                                trusted,
                            },
                        );
                        drop(inner);
                        emit_change(&app);
                    }
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    let mut inner = state.0.lock().expect("state lock");
                    inner.peers.retain(|_, peer| !fullname.contains(&peer.id));
                    drop(inner);
                    emit_change(&app);
                }
                _ => {}
            }
        }
    });
    Ok(mdns)
}

fn load_settings(root: PathBuf, downloads: Option<PathBuf>) -> Result<Settings, String> {
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let path = root.join("settings.json");
    if path.exists() {
        return serde_json::from_reader(File::open(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string());
    }
    let inbox = downloads
        .map(|path| path.join("Pombo Correio"))
        .unwrap_or_else(|| root.join("Inbox"));
    fs::create_dir_all(&inbox).map_err(|e| e.to_string())?;
    Ok(Settings {
        device_id: Uuid::new_v4().to_string(),
        device_name: hostname::get()
            .ok()
            .and_then(|v| v.into_string().ok())
            .unwrap_or_else(|| "My device".into()),
        inbox,
        trusted_devices: HashSet::new(),
    })
}

fn persist(inner: &Inner) -> Result<(), String> {
    let file = File::create(&inner.settings_path).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(file, &inner.settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn snapshot(state: tauri::State<CoreState>) -> AppSnapshot {
    let inner = state.0.lock().expect("state lock");
    AppSnapshot {
        device_id: inner.settings.device_id.clone(),
        device_name: inner.settings.device_name.clone(),
        inbox: inner.settings.inbox.display().to_string(),
        peers: inner.peers.values().cloned().collect(),
        incoming: inner
            .offers
            .values()
            .filter(|pending| pending.accepted.is_none())
            .map(|pending| pending.offer.clone())
            .collect(),
    }
}

#[tauri::command]
fn answer_offer(
    offer_id: String,
    accept: bool,
    trust: bool,
    state: tauri::State<CoreState>,
    app: AppHandle,
) -> Result<(), String> {
    let mut inner = state.0.lock().map_err(|e| e.to_string())?;
    let sender = inner
        .offers
        .get_mut(&offer_id)
        .ok_or("offer no longer exists")?;
    sender.accepted = Some(accept);
    let sender_id = sender.offer.sender_id.clone();
    if accept && trust {
        inner.settings.trusted_devices.insert(sender_id);
        persist(&inner)?;
    }
    drop(inner);
    emit_change(&app);
    Ok(())
}

#[tauri::command]
async fn send_files(
    peer_ids: Vec<String>,
    files: Vec<SelectedFile>,
    state: tauri::State<'_, CoreState>,
    app: AppHandle,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (sender_id, sender_name, peers) = {
            let inner = state.0.lock().map_err(|e| e.to_string())?;
            let peers = peer_ids
                .iter()
                .map(|id| {
                    inner
                        .peers
                        .get(id)
                        .cloned()
                        .ok_or_else(|| format!("device {id} is no longer available"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            (
                inner.settings.device_id.clone(),
                inner.settings.device_name.clone(),
                peers,
            )
        };
        let metadata = files
            .iter()
            .map(|selected| {
                let file = open_selected(&app, selected)?;
                let metadata = file
                    .metadata()
                    .map_err(|e| format!("{}: {e}", selected.name))?;
                if !metadata.is_file() {
                    return Err(format!("{} is not a regular file", selected.name));
                }
                Ok(FileMeta {
                    name: selected.name.clone(),
                    size: metadata.len(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        for peer in peers {
            let offer = Offer {
                id: Uuid::new_v4().to_string(),
                sender_id: sender_id.clone(),
                sender_name: sender_name.clone(),
                files: metadata.clone(),
            };
            let base = format!("http://{}:{}", peer.address, peer.port);
            client
                .post(format!("{base}/v1/offers"))
                .json(&offer)
                .send()
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let mut accepted = false;
            for _ in 0..120 {
                thread::sleep(Duration::from_millis(500));
                let status: serde_json::Value = client
                    .get(format!("{base}/v1/offers/{}", offer.id))
                    .send()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())?;
                match status["status"].as_str() {
                    Some("accepted") => {
                        accepted = true;
                        break;
                    }
                    Some("declined") => return Err(format!("{} declined the transfer", peer.name)),
                    _ => {}
                }
            }
            if !accepted {
                return Err(format!("{} did not answer", peer.name));
            }
            for (index, selected) in files.iter().enumerate() {
                let file = open_selected(&app, selected)?;
                client
                    .post(format!("{base}/v1/files/{}/{index}", offer.id))
                    .body(reqwest::blocking::Body::new(file))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(desktop)]
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::{
        image::Image,
        menu::{Menu, MenuItem},
        tray::TrayIconBuilder,
    };
    let open = MenuItem::with_id(app, "open", "Send files…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let tray_icon = Image::from_bytes(include_bytes!("../icons/tray-mail.png"))?;
    TrayIconBuilder::new()
        .icon(tray_icon)
        .tooltip("Pombo Correio")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    if let Some(window) = app.get_webview_window("main") {
        let hide_window = window.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = hide_window.hide();
            }
        });
    }
    Ok(())
}

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
            let settings_path = root.join("settings.json");
            let state = CoreState(Arc::new(Mutex::new(Inner {
                settings,
                settings_path,
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
            setup_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![snapshot, answer_offer, send_files])
        .run(tauri::generate_context!())
        .expect("error while running Pombo Correio");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destination_never_overwrites_an_existing_file() {
        let root = std::env::temp_dir().join(format!("pombocorreio-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        File::create(root.join("photo.jpg")).unwrap();
        assert_eq!(
            safe_destination(&root, "photo.jpg"),
            root.join("photo (1).jpg")
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn destination_strips_parent_directories() {
        let root = PathBuf::from("/tmp/inbox");
        assert_eq!(
            safe_destination(&root, "../../secret.txt"),
            root.join("secret.txt")
        );
    }
}
