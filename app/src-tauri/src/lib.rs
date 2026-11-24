//! The Toona desktop app. The window is in `../src`; everything it can ask for is in `api`.

pub mod api;

use api::{Backend, UiError};
use tauri::State;

type UiResult<T> = Result<T, UiError>;

// Commands are marked `async` so they run off the main thread: the first catalogue
// load downloads a file, and installing zips a backup.

#[tauri::command(async)]
fn status(backend: State<'_, Backend>) -> api::Status {
    backend.status()
}

#[tauri::command(async)]
fn catalogue(backend: State<'_, Backend>, refresh: bool) -> UiResult<api::CatalogueDto> {
    backend.catalogue(refresh)
}

#[tauri::command(async)]
fn detail(
    backend: State<'_, Backend>,
    key: String,
    printer: Option<String>,
    base: Option<String>,
) -> UiResult<api::DetailDto> {
    backend.detail(&key, printer.as_deref(), base.as_deref())
}

#[tauri::command(async)]
fn install(backend: State<'_, Backend>, request: api::InstallRequest) -> UiResult<api::InstallDto> {
    backend.install(&request)
}

#[tauri::command(async)]
fn export_bundle(backend: State<'_, Backend>, request: api::InstallRequest) -> UiResult<Vec<String>> {
    backend.export(&request)
}

#[tauri::command(async)]
fn in_studio(backend: State<'_, Backend>) -> UiResult<api::InStudioDto> {
    backend.in_studio()
}

#[tauri::command(async)]
fn remove(backend: State<'_, Backend>, profile_name: String) -> UiResult<api::RemoveDto> {
    backend.remove(&profile_name)
}

#[tauri::command(async)]
fn fix_imported(
    backend: State<'_, Backend>,
    names: Vec<String>,
    allow_cloud_sync: bool,
    dry_run: bool,
) -> UiResult<api::InstallDto> {
    backend.fix_imported(&names, allow_cloud_sync, dry_run)
}

#[tauri::command(async)]
fn barcode(backend: State<'_, Backend>, code: String) -> UiResult<Vec<api::BarcodeDto>> {
    backend.barcode(&code)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let backend = Backend::standard().expect("could not determine the home directory");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(backend)
        .invoke_handler(tauri::generate_handler![
            status,
            catalogue,
            detail,
            install,
            export_bundle,
            in_studio,
            remove,
            fix_imported,
            barcode
        ])
        .run(tauri::generate_context!())
        .expect("error while running Toona");
}
