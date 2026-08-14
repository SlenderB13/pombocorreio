use crate::state::Inner;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File},
    path::PathBuf,
};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub(crate) struct Settings {
    pub(crate) device_id: String,
    pub(crate) device_name: String,
    pub(crate) inbox: PathBuf,
    #[serde(default)]
    pub(crate) trusted_devices: HashSet<String>,
}

pub(crate) fn load_settings(root: PathBuf, downloads: Option<PathBuf>) -> Result<Settings, String> {
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let path = root.join("settings.json");
    if path.exists() {
        return serde_json::from_reader(File::open(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string());
    }

    let inbox = downloads
        .map(|path| path.join("Pombo Correio"))
        .unwrap_or_else(|| root.join("Inbox"));
    fs::create_dir_all(&inbox).map_err(|error| error.to_string())?;

    Ok(Settings {
        device_id: Uuid::new_v4().to_string(),
        device_name: hostname::get()
            .ok()
            .and_then(|value| value.into_string().ok())
            .unwrap_or_else(|| "My device".into()),
        inbox,
        trusted_devices: HashSet::new(),
    })
}

pub(crate) fn persist(inner: &Inner) -> Result<(), String> {
    let file = File::create(&inner.settings_path).map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(file, &inner.settings).map_err(|error| error.to_string())
}
