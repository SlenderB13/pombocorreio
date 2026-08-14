#[cfg(target_os = "android")]
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
#[cfg(target_os = "android")]
use tauri_plugin_pombo_inbox::InboxExt;
use uuid::Uuid;

pub(crate) fn safe_destination(inbox: &Path, requested: &str) -> PathBuf {
    let filename = Path::new(requested)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("received-file");
    let candidate = inbox.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let stem = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = Path::new(filename)
        .extension()
        .and_then(|value| value.to_str());
    for number in 1..10_000 {
        let name = match extension {
            Some(extension) => format!("{stem} ({number}).{extension}"),
            None => format!("{stem} ({number})"),
        };
        let candidate = inbox.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }

    inbox.join(format!("{}-{filename}", Uuid::new_v4()))
}

#[cfg(target_os = "android")]
pub(crate) fn publish_received(app: &AppHandle, path: &Path, name: &str) -> Result<String, String> {
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
pub(crate) fn publish_received(
    _app: &AppHandle,
    path: &Path,
    _name: &str,
) -> Result<String, String> {
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, File},
        path::PathBuf,
    };

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
