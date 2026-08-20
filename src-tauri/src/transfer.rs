use crate::{
    models::{FileMeta, Offer, SelectedFile, TextMeta},
    state::CoreState,
};
use std::{fs::File, thread, time::Duration};
use tauri::AppHandle;
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
use uuid::Uuid;

pub(crate) const MAX_TEXT_BYTES: usize = 64 * 1024;

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

#[tauri::command]
pub(crate) async fn send_files(
    peer_ids: Vec<String>,
    files: Vec<SelectedFile>,
    state: tauri::State<'_, CoreState>,
    app: AppHandle,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || send_files_blocking(peer_ids, files, state, app))
        .await
        .map_err(|error| error.to_string())?
}

fn send_files_blocking(
    peer_ids: Vec<String>,
    files: Vec<SelectedFile>,
    state: CoreState,
    app: AppHandle,
) -> Result<(), String> {
    let (sender_id, sender_name, peers) = {
        let inner = state.0.lock().map_err(|error| error.to_string())?;
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

    let metadata = collect_metadata(&app, &files)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;

    for peer in peers {
        let offer = Offer {
            id: Uuid::new_v4().to_string(),
            sender_id: sender_id.clone(),
            sender_name: sender_name.clone(),
            files: metadata.clone(),
            text: None,
        };
        let base = format!("http://{}:{}", peer.address, peer.port);
        client
            .post(format!("{base}/v1/offers"))
            .json(&offer)
            .send()
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;

        wait_for_acceptance(&client, &base, &offer.id, &peer.name)?;
        upload_files(&client, &base, &offer.id, &files, &app)?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn send_text(
    peer_ids: Vec<String>,
    text: String,
    state: tauri::State<'_, CoreState>,
) -> Result<(), String> {
    if text.is_empty() {
        return Err("enter some text first".into());
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(format!("text is larger than {} KiB", MAX_TEXT_BYTES / 1024));
    }

    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || send_text_blocking(peer_ids, text, state))
        .await
        .map_err(|error| error.to_string())?
}

fn send_text_blocking(peer_ids: Vec<String>, text: String, state: CoreState) -> Result<(), String> {
    let (sender_id, sender_name, peers) = {
        let inner = state.0.lock().map_err(|error| error.to_string())?;
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

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let preview = text.chars().take(120).collect::<String>();

    for peer in peers {
        let offer = Offer {
            id: Uuid::new_v4().to_string(),
            sender_id: sender_id.clone(),
            sender_name: sender_name.clone(),
            files: Vec::new(),
            text: Some(TextMeta {
                preview: preview.clone(),
                size: text.len(),
            }),
        };
        let base = format!("http://{}:{}", peer.address, peer.port);
        client
            .post(format!("{base}/v1/offers"))
            .json(&offer)
            .send()
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;

        wait_for_acceptance(&client, &base, &offer.id, &peer.name)?;
        client
            .post(format!("{base}/v1/text/{}", offer.id))
            .header("content-type", "text/plain; charset=utf-8")
            .body(text.clone())
            .send()
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn collect_metadata(app: &AppHandle, files: &[SelectedFile]) -> Result<Vec<FileMeta>, String> {
    files
        .iter()
        .map(|selected| {
            let file = open_selected(app, selected)?;
            let metadata = file
                .metadata()
                .map_err(|error| format!("{}: {error}", selected.name))?;
            if !metadata.is_file() {
                return Err(format!("{} is not a regular file", selected.name));
            }
            Ok(FileMeta {
                name: selected.name.clone(),
                size: metadata.len(),
            })
        })
        .collect()
}

fn wait_for_acceptance(
    client: &reqwest::blocking::Client,
    base: &str,
    offer_id: &str,
    peer_name: &str,
) -> Result<(), String> {
    for _ in 0..120 {
        thread::sleep(Duration::from_millis(500));
        let status: serde_json::Value = client
            .get(format!("{base}/v1/offers/{offer_id}"))
            .send()
            .map_err(|error| error.to_string())?
            .json()
            .map_err(|error| error.to_string())?;
        match status["status"].as_str() {
            Some("accepted") => return Ok(()),
            Some("declined") => return Err(format!("{peer_name} declined the transfer")),
            _ => {}
        }
    }
    Err(format!("{peer_name} did not answer"))
}

fn upload_files(
    client: &reqwest::blocking::Client,
    base: &str,
    offer_id: &str,
    files: &[SelectedFile],
    app: &AppHandle,
) -> Result<(), String> {
    for (index, selected) in files.iter().enumerate() {
        let file = open_selected(app, selected)?;
        client
            .post(format!("{base}/v1/files/{offer_id}/{index}"))
            .body(reqwest::blocking::Body::new(file))
            .send()
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
