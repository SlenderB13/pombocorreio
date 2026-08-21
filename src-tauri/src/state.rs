use crate::{
    models::{AppSnapshot, Offer, Peer},
    settings::{persist, Settings},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter};

pub(crate) struct PendingOffer {
    pub(crate) offer: Offer,
    pub(crate) accepted: Option<bool>,
}

pub(crate) struct Inner {
    pub(crate) settings: Settings,
    pub(crate) settings_path: PathBuf,
    pub(crate) peers: HashMap<String, Peer>,
    pub(crate) offers: HashMap<String, PendingOffer>,
}

#[derive(Clone)]
pub(crate) struct CoreState(pub(crate) Arc<Mutex<Inner>>);

pub(crate) fn emit_change(app: &AppHandle) {
    let _ = app.emit("state-changed", ());
}

#[tauri::command]
pub(crate) fn snapshot(state: tauri::State<CoreState>) -> AppSnapshot {
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
        auto_open_links: inner.settings.auto_open_links,
    }
}

#[tauri::command]
pub(crate) fn set_auto_open_links(
    enabled: bool,
    state: tauri::State<CoreState>,
    app: AppHandle,
) -> Result<(), String> {
    let mut inner = state.0.lock().map_err(|error| error.to_string())?;
    inner.settings.auto_open_links = enabled;
    persist(&inner)?;
    drop(inner);
    emit_change(&app);
    Ok(())
}

#[tauri::command]
pub(crate) fn answer_offer(
    offer_id: String,
    accept: bool,
    trust: bool,
    state: tauri::State<CoreState>,
    app: AppHandle,
) -> Result<(), String> {
    let mut inner = state.0.lock().map_err(|error| error.to_string())?;
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
