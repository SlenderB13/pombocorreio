use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tauri::{plugin::{Builder, PluginApi, TauriPlugin}, AppHandle, Manager, Runtime};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Plugin(#[from] tauri::plugin::mobile::PluginInvokeError),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishArgs<'a> {
    source_path: &'a str,
    display_name: &'a str,
    mime_type: &'a str,
}

#[derive(Deserialize)]
struct PublishResponse {
    uri: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedFile {
    pub path: String,
    pub name: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedContent {
    pub files: Vec<SharedFile>,
    pub text: Option<String>,
}

pub struct Inbox<R: Runtime>(tauri::plugin::PluginHandle<R>);

impl<R: Runtime> Inbox<R> {
    pub fn publish(&self, source_path: &str, display_name: &str, mime_type: &str) -> Result<String> {
        let response = self.0.run_mobile_plugin::<PublishResponse>("publish", PublishArgs {
            source_path,
            display_name,
            mime_type,
        })?;
        Ok(response.uri)
    }

    pub fn take_shared_content(&self) -> Result<SharedContent> {
        Ok(self
            .0
            .run_mobile_plugin::<SharedContent>("takeSharedContent", ())?)
    }

    pub fn clear_shared_content(&self) -> Result<()> {
        self.0
            .run_mobile_plugin::<()>("clearSharedContent", ())?;
        Ok(())
    }
}

pub trait InboxExt<R: Runtime> {
    fn inbox(&self) -> &Inbox<R>;
}

impl<R: Runtime, T: Manager<R>> InboxExt<R> for T {
    fn inbox(&self) -> &Inbox<R> {
        self.state::<Inbox<R>>().inner()
    }
}

fn init_android<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> tauri::Result<Inbox<R>> {
    let handle = api.register_android_plugin("dev.pombocorreio.inbox", "InboxPlugin")?;
    Ok(Inbox(handle))
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("pombo-inbox")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            app.manage(init_android(app, api)?);
            Ok(())
        })
        .build()
}
