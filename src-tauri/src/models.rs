use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Peer {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) trusted: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileMeta {
    pub(crate) name: String,
    pub(crate) size: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextMeta {
    pub(crate) preview: String,
    pub(crate) size: usize,
}

#[derive(Clone, Deserialize)]
pub(crate) struct SelectedFile {
    pub(crate) path: String,
    pub(crate) name: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Offer {
    pub(crate) id: String,
    pub(crate) sender_id: String,
    pub(crate) sender_name: String,
    #[serde(default)]
    pub(crate) files: Vec<FileMeta>,
    #[serde(default)]
    pub(crate) text: Option<TextMeta>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSnapshot {
    pub(crate) device_id: String,
    pub(crate) device_name: String,
    pub(crate) inbox: String,
    pub(crate) peers: Vec<Peer>,
    pub(crate) incoming: Vec<Offer>,
}
