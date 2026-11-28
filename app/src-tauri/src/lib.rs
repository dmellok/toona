//! The Toona desktop app. The window is in `../src`; everything it can ask for is in `api`.

pub mod api;
pub mod community;
pub mod prefs;

use std::sync::Arc;

use api::{Backend, UiError};
use serde::Serialize;
use tauri::{Emitter, State};
use tauri_plugin_updater::UpdaterExt;

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
async fn set_slicer(backend: Shared<'_>, id: String) -> UiResult<api::Status> {
    blocking(&backend, move |b| b.set_slicer(&id)).await
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
    name: Option<String>,
    borrow_tuned: bool,
) -> UiResult<api::DetailDto> {
    blocking(&backend, move |b| b.detail(&key, printer.as_deref(), base.as_deref(), colour.as_deref(), name.as_deref(), borrow_tuned)).await
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
async fn remove_many(backend: Shared<'_>, profile_names: Vec<String>) -> UiResult<api::RemoveDto> {
    blocking(&backend, move |b| b.remove_many(&profile_names)).await
}

#[tauri::command]
async fn backups(backend: Shared<'_>) -> UiResult<Vec<api::BackupDto>> {
    blocking(&backend, |b| b.backups()).await
}

#[tauri::command]
async fn restore_backup(backend: Shared<'_>, path: String) -> UiResult<String> {
    blocking(&backend, move |b| b.restore_backup(&path)).await
}

#[tauri::command]
async fn add_nozzles(
    backend: Shared<'_>,
    profile_name: String,
    printers: Vec<String>,
    borrow_tuned: bool,
    allow_cloud_sync: bool,
) -> UiResult<api::InstallDto> {
    blocking(&backend, move |b| b.add_nozzles(&profile_name, &printers, borrow_tuned, allow_cloud_sync)).await
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
async fn standard(backend: Shared<'_>, key: String, model: String, nozzle: String) -> UiResult<community::Standard> {
    blocking(&backend, move |b| b.standard_score(&key, &model, &nozzle)).await
}

#[tauri::command]
async fn confirm_standard(
    backend: Shared<'_>,
    key: String,
    outcome: String,
    model: String,
    nozzle: String,
) -> UiResult<community::Standard> {
    blocking(&backend, move |b| b.confirm_standard(&key, &outcome, &model, &nozzle)).await
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

#[tauri::command]
async fn prefs(backend: Shared<'_>) -> UiResult<prefs::PrefsDto> {
    blocking(&backend, |b| Ok(b.prefs())).await
}

#[tauri::command]
async fn set_prefs(backend: Shared<'_>, updates: bool, usage: bool) -> UiResult<prefs::PrefsDto> {
    blocking(&backend, move |b| b.set_prefs(updates, usage)).await
}

/// Sends the usage hello if the person opted in and none was sent today.
#[tauri::command]
async fn send_usage(backend: Shared<'_>) -> UiResult<bool> {
    blocking(&backend, |b| b.send_usage()).await
}

#[derive(Serialize)]
struct UpdateDto {
    version: String,
    notes: String,
}

fn update_error(error: tauri_plugin_updater::Error) -> UiError {
    use tauri_plugin_updater::Error;
    match error {
        Error::Reqwest(_) | Error::Network(_) => UiError { kind: "offline", message: "Could not reach the Toona server.".into() },
        other => UiError { kind: "other", message: format!("The update could not be used: {other}") },
    }
}

/// Asks the Toona server whether there is a newer version. The request carries this
/// version and the kind of computer, nothing else.
async fn newer(app: &tauri::AppHandle, backend: &Backend) -> UiResult<Option<tauri_plugin_updater::Update>> {
    let endpoint = format!("{}/app/update/{{{{target}}}}/{{{{arch}}}}/{{{{current_version}}}}", backend.server().api());
    let endpoint = endpoint.parse().map_err(|_| UiError { kind: "other", message: "The update address is not valid.".into() })?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .and_then(|builder| builder.timeout(std::time::Duration::from_secs(15)).build())
        .map_err(update_error)?;
    updater.check().await.map_err(update_error)
}

/// `automatic` is the check made when the window opens, which only runs if the person allowed it.
#[tauri::command]
async fn check_update(app: tauri::AppHandle, backend: Shared<'_>, automatic: bool) -> UiResult<Option<UpdateDto>> {
    if automatic && !blocking(&backend, |b| Ok(b.checks_for_updates())).await? {
        return Ok(None);
    }
    Ok(newer(&app, &backend)
        .await?
        .map(|update| UpdateDto { version: update.version.clone(), notes: update.body.clone().unwrap_or_default() }))
}

/// Downloads the newer version, checks its signature against the key built into this
/// app, replaces the app and starts it again.
#[tauri::command]
async fn install_update(app: tauri::AppHandle, backend: Shared<'_>) -> UiResult<()> {
    let Some(update) = newer(&app, &backend).await? else {
        return Err(UiError { kind: "other", message: "There is no newer version any more.".into() });
    };
    let mut done: u64 = 0;
    let progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                done += chunk as u64;
                let _ = progress.emit("update-progress", (done, total));
            },
            || {},
        )
        .await
        .map_err(update_error)?;
    app.restart()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let backend = Arc::new(Backend::standard().expect("could not determine the home directory"));
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(backend)
        .invoke_handler(tauri::generate_handler![
            status,
            set_slicer,
            catalogue,
            detail,
            install,
            export_bundle,
            in_studio,
            remove,
            remove_many,
            backups,
            restore_backup,
            add_nozzles,
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
            standard,
            confirm_standard,
            link_product,
            profile,
            edit,
            family,
            prefs,
            set_prefs,
            send_usage,
            check_update,
            install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running Toona");
}

#[cfg(test)]
mod tests {
    /// The shape the Toona server answers an update check with must be one the updater reads.
    #[test]
    fn the_servers_update_answer_is_understood() {
        let answer = serde_json::json!({
            "version": "0.3.1",
            "notes": "Faster start.",
            "pub_date": "2026-11-25T01:02:03.456Z",
            "url": "https://example.com/Toona.app.tar.gz",
            "signature": "c2lnbmF0dXJl",
        });
        let release: tauri_plugin_updater::RemoteRelease = serde_json::from_value(answer).expect("readable");
        assert_eq!(release.version.to_string(), "0.3.1");
        assert_eq!(release.download_url("darwin-aarch64").unwrap().as_str(), "https://example.com/Toona.app.tar.gz");
        assert!(release.pub_date.is_some());
    }
}
