//! The Toona desktop app. The window is in `../src`; everything it can ask for is in `api`.

pub mod api;
pub mod community;

use std::sync::Arc;

use api::{Backend, UiError};
use tauri::State;

type UiResult<T> = Result<T, UiError>;
type Shared<'a> = State<'a, Arc<Backend>>;

/// Runs backend work on a thread that is allowed to block. Commands themselves run on
/// the async runtime, where the blocking HTTP client (catalogue download, Toona server)
/// panics, and where zipping a backup would stall other commands.
async fn blocking<T: Send + 'static>(
    backend: &Arc<Backend>,
    work: impl FnOnce(&Backend) -> UiResult<T> + Send + 'static,
) -> UiResult<T> {
    let backend = backend.clone();
    tauri::async_runtime::spawn_blocking(move || work(&backend))
        .await
        .unwrap_or_else(|error| Err(UiError { kind: "other", message: format!("Toona hit an internal error: {error}") }))
}

#[tauri::command]
async fn status(backend: Shared<'_>) -> UiResult<api::Status> {
    blocking(&backend, |b| Ok(b.status())).await
}

#[tauri::command]
async fn catalogue(backend: Shared<'_>, refresh: bool) -> UiResult<api::CatalogueDto> {
    blocking(&backend, move |b| b.catalogue(refresh)).await
}

#[tauri::command]
async fn detail(
    backend: Shared<'_>,
    key: String,
    printer: Option<String>,
    base: Option<String>,
    colour: Option<String>,
) -> UiResult<api::DetailDto> {
    blocking(&backend, move |b| b.detail(&key, printer.as_deref(), base.as_deref(), colour.as_deref())).await
}

#[tauri::command]
async fn install(backend: Shared<'_>, request: api::InstallRequest) -> UiResult<api::InstallDto> {
    blocking(&backend, move |b| b.install(&request)).await
}

#[tauri::command]
async fn export_bundle(backend: Shared<'_>, request: api::InstallRequest) -> UiResult<Vec<String>> {
    blocking(&backend, move |b| b.export(&request)).await
}

#[tauri::command]
async fn in_studio(backend: Shared<'_>) -> UiResult<api::InStudioDto> {
    blocking(&backend, |b| b.in_studio()).await
}

#[tauri::command]
async fn remove(backend: Shared<'_>, profile_name: String) -> UiResult<api::RemoveDto> {
    blocking(&backend, move |b| b.remove(&profile_name)).await
}

#[tauri::command]
async fn fix_imported(
    backend: Shared<'_>,
    names: Vec<String>,
    allow_cloud_sync: bool,
    dry_run: bool,
) -> UiResult<api::InstallDto> {
    blocking(&backend, move |b| b.fix_imported(&names, allow_cloud_sync, dry_run)).await
}

#[tauri::command]
async fn barcode(backend: Shared<'_>, code: String) -> UiResult<Vec<api::BarcodeDto>> {
    blocking(&backend, move |b| b.barcode(&code)).await
}

#[tauri::command]
async fn account(backend: Shared<'_>) -> UiResult<api::AccountDto> {
    blocking(&backend, |b| Ok(b.account())).await
}

#[tauri::command]
async fn sign_in_start(backend: Shared<'_>) -> UiResult<community::SignIn> {
    blocking(&backend, |b| b.sign_in_start()).await
}

#[tauri::command]
async fn sign_in_poll(backend: Shared<'_>, device_code: String) -> UiResult<Option<community::User>> {
    blocking(&backend, move |b| b.sign_in_poll(&device_code)).await
}

#[tauri::command]
async fn sign_out(backend: Shared<'_>) -> UiResult<()> {
    blocking(&backend, |b| {
        b.sign_out();
        Ok(())
    })
    .await
}

/// Opens GitHub's page for typing in the sign-in code. No other address is opened.
#[tauri::command]
async fn open_sign_in_page(app: tauri::AppHandle) -> UiResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url("https://github.com/login/device", None::<&str>)
        .map_err(|error| UiError { kind: "other", message: error.to_string() })
}

#[tauri::command]
async fn community(backend: Shared<'_>, key: String, model: String, nozzle: String) -> UiResult<Vec<api::SharedDto>> {
    blocking(&backend, move |b| b.community(&key, &model, &nozzle)).await
}

#[tauri::command]
async fn install_community(
    backend: Shared<'_>,
    request: api::InstallRequest,
    version_id: u64,
) -> UiResult<api::InstallDto> {
    blocking(&backend, move |b| b.install_community(&request, version_id)).await
}

#[tauri::command]
async fn share(backend: Shared<'_>, profile_name: String, note: String) -> UiResult<api::SharedDto> {
    blocking(&backend, move |b| b.share(&profile_name, &note)).await
}

#[tauri::command]
async fn confirm(
    backend: Shared<'_>,
    version_id: u64,
    outcome: String,
    model: String,
    nozzle: String,
) -> UiResult<api::SharedDto> {
    blocking(&backend, move |b| b.confirm(version_id, &outcome, &model, &nozzle)).await
}

#[tauri::command]
async fn link_product(backend: Shared<'_>, profile_name: String, key: Option<String>) -> UiResult<()> {
    blocking(&backend, move |b| b.link_product(&profile_name, key.as_deref())).await
}

#[tauri::command]
async fn profile(backend: Shared<'_>, profile_name: String) -> UiResult<api::ProfileDto> {
    blocking(&backend, move |b| b.profile(&profile_name)).await
}

#[tauri::command]
async fn edit(
    backend: Shared<'_>,
    profile_name: String,
    edits: Vec<api::EditDto>,
    allow_cloud_sync: bool,
    dry_run: bool,
) -> UiResult<api::InstallDto> {
    blocking(&backend, move |b| b.edit(&profile_name, &edits, allow_cloud_sync, dry_run)).await
}

#[tauri::command]
async fn family(backend: Shared<'_>, version_id: u64) -> UiResult<Vec<api::SharedDto>> {
    blocking(&backend, move |b| b.family(version_id)).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let backend = Arc::new(Backend::standard().expect("could not determine the home directory"));
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
            barcode,
            account,
            sign_in_start,
            sign_in_poll,
            sign_out,
            open_sign_in_page,
            community,
            install_community,
            share,
            confirm,
            link_product,
            profile,
            edit,
            family
        ])
        .run(tauri::generate_context!())
        .expect("error while running Toona");
}
