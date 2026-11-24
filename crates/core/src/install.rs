//! Writing profiles into the slicer's user folder, tracking them in a manifest,
//! removing them again, and exporting an importable bundle instead.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{Error, Result};
use crate::profile::{FilamentProfile, PresetInfo};
use crate::recipe::Recipe;
use crate::slicer::{AppConf, Slicer};

const MANIFEST_FILE: &str = "manifest.json";
const MANIFEST_SCHEMA: u32 = 1;
/// Zip entry Studio reads the bundle layout from (PROFILE_SPEC.md §8).
const BUNDLE_STRUCTURE_FILE: &str = "bundle_structure.json";
const BUNDLE_EXTENSION: &str = "bbsflmt";

/// Where the tool keeps its own files.
#[derive(Debug, Clone)]
pub struct AppDirs {
    /// Manifest and backups.
    pub data_dir: PathBuf,
    /// Downloaded catalogue.
    pub cache_dir: PathBuf,
}

impl AppDirs {
    /// The per-user locations for this OS.
    pub fn standard() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("", "", "toona")
            .ok_or_else(|| Error::Other("could not determine the home directory".into()))?;
        Ok(Self { data_dir: dirs.data_dir().to_path_buf(), cache_dir: dirs.cache_dir().to_path_buf() })
    }

    /// Everything under one folder.
    pub fn under(root: &Path) -> Self {
        Self { data_dir: root.join("data"), cache_dir: root.join("cache") }
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.data_dir.join(MANIFEST_FILE)
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }
}

/// One profile this tool wrote into a slicer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledProfile {
    /// Catalogue key of the filament it was generated from.
    pub catalogue_key: String,
    /// Filament name shown in the slicer.
    pub alias: String,
    pub profile_name: String,
    pub filament_id: String,
    pub printer: String,
    /// [`crate::SlicerKind::id`] of the slicer it was written to.
    pub slicer: String,
    /// Every file created for it.
    pub files: Vec<PathBuf>,
    /// UTC, RFC 3339.
    pub installed_at: String,
    /// The profile as its parent preset plus changes. Missing for profiles installed
    /// before this was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<Recipe>,
}

/// The record of everything the tool has installed. Uninstall removes only what is listed here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub profiles: Vec<InstalledProfile>,
}

impl Manifest {
    pub fn load(dirs: &AppDirs) -> Result<Self> {
        let path = dirs.manifest_path();
        if !path.exists() {
            return Ok(Self { schema: MANIFEST_SCHEMA, profiles: Vec::new() });
        }
        let text = fs::read_to_string(&path).map_err(Error::read(&path))?;
        serde_json::from_str(&text).map_err(Error::json(&path))
    }

    fn save(&self, dirs: &AppDirs) -> Result<()> {
        let text = serde_json::to_string_pretty(self).expect("manifest serializes to JSON");
        write_atomically(&dirs.manifest_path(), text.as_bytes())
    }

    fn owns(&self, path: &Path) -> bool {
        self.profiles.iter().any(|p| p.files.iter().any(|f| f == path))
    }
}

/// A profile ready to install, with the catalogue entry it came from.
#[derive(Debug, Clone)]
pub struct PlannedProfile {
    pub catalogue_key: String,
    pub profile: FilamentProfile,
    /// Recorded in the manifest with the profile.
    pub recipe: Option<Recipe>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InstallOptions {
    /// The user has agreed that the slicer will upload the new profiles to its cloud.
    pub confirm_cloud_sync: bool,
    /// Check everything and report what would be written, without writing.
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct InstallReport {
    pub dry_run: bool,
    /// Files written, or that would be written.
    pub files: Vec<PathBuf>,
    pub backup: Option<PathBuf>,
    /// The slicer will upload these profiles to its cloud at its next start.
    pub will_cloud_sync: bool,
}

#[derive(Debug, Clone)]
pub struct UninstallReport {
    pub removed: Vec<PathBuf>,
    /// Listed in the manifest but already gone.
    pub missing: Vec<PathBuf>,
    pub backup: Option<PathBuf>,
    /// Profiles the cloud holds a copy of. The slicer will download them again
    /// unless they are also deleted there.
    pub cloud_synced: Vec<String>,
}

/// Writes profiles into the slicer's user folder.
///
/// Refuses while the slicer is running, never overwrites a file it did not create,
/// and backs up the whole user folder first.
pub fn install(
    slicer: &Slicer,
    conf: &AppConf,
    dirs: &AppDirs,
    planned: &[PlannedProfile],
    options: InstallOptions,
) -> Result<InstallReport> {
    install_checked(slicer, conf, dirs, planned, options, slicer.is_running()?)
}

fn install_checked(
    slicer: &Slicer,
    conf: &AppConf,
    dirs: &AppDirs,
    planned: &[PlannedProfile],
    options: InstallOptions,
    slicer_running: bool,
) -> Result<InstallReport> {
    let mut manifest = Manifest::load(dirs)?;
    let target_dir = slicer.custom_filament_dir(conf);
    let existing = slicer.user_filaments(conf)?;

    let mut targets = Vec::new();
    for item in planned {
        item.profile.validate()?;
        let json_path = target_dir.join(format!("{}.json", item.profile.file_stem()));
        let info_path = json_path.with_extension("info");
        // Any preset of that name the tool did not write belongs to the user.
        let clash = existing
            .iter()
            .find(|f| f.name == item.profile.name && !manifest.owns(&f.path))
            .map(|f| f.path.clone())
            .or_else(|| [&json_path, &info_path].into_iter().find(|p| p.exists() && !manifest.owns(p)).cloned());
        if let Some(path) = clash {
            return Err(Error::ForeignFile { path });
        }
        targets.push((item, json_path, info_path));
    }

    let will_cloud_sync = conf.uploads_new_presets();
    let files: Vec<PathBuf> = targets.iter().flat_map(|(_, json, info)| [json.clone(), info.clone()]).collect();
    if options.dry_run {
        return Ok(InstallReport { dry_run: true, files, backup: None, will_cloud_sync });
    }
    if will_cloud_sync && !options.confirm_cloud_sync {
        return Err(Error::CloudSyncNotConfirmed { user_id: conf.preset_folder.clone() });
    }
    if slicer_running {
        return Err(Error::SlicerRunning(slicer.kind.display_name()));
    }

    let backup = backup_user_folder(slicer, dirs)?;
    fs::create_dir_all(&target_dir).map_err(Error::write(&target_dir))?;
    let installed_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for (item, json_path, info_path) in &targets {
        write_atomically(json_path, item.profile.to_json().as_bytes())?;
        write_atomically(info_path, PresetInfo::new_unsynced().to_string().as_bytes())?;

        manifest.profiles.retain(|p| !(p.slicer == slicer.kind.id() && p.profile_name == item.profile.name));
        manifest.profiles.push(InstalledProfile {
            catalogue_key: item.catalogue_key.clone(),
            alias: item.profile.alias().to_string(),
            profile_name: item.profile.name.clone(),
            filament_id: item.profile.filament_id.clone(),
            printer: item.profile.compatible_printers[0].clone(),
            slicer: slicer.kind.id().to_string(),
            files: vec![json_path.clone(), info_path.clone()],
            installed_at: installed_at.clone(),
            recipe: item.recipe.clone(),
        });
        // Saved after each profile so a failure part-way leaves no untracked file.
        manifest.save(dirs)?;
    }

    Ok(InstallReport { dry_run: false, files, backup: Some(backup), will_cloud_sync })
}

/// Profiles in the manifest for this slicer that `query` names: a catalogue key,
/// a filament name, or a full profile name.
pub fn installed_matching<'a>(manifest: &'a Manifest, slicer: &Slicer, query: &str) -> Vec<&'a InstalledProfile> {
    let wanted = query.trim().to_lowercase();
    manifest
        .profiles
        .iter()
        .filter(|p| p.slicer == slicer.kind.id())
        .filter(|p| {
            [&p.catalogue_key, &p.alias, &p.profile_name, &p.filament_id]
                .iter()
                .any(|field| field.to_lowercase() == wanted)
        })
        .collect()
}

/// Removes the files of every installed profile that `query` names, and nothing else.
pub fn uninstall(slicer: &Slicer, dirs: &AppDirs, query: &str) -> Result<UninstallReport> {
    uninstall_checked(slicer, dirs, query, slicer.is_running()?)
}

fn uninstall_checked(slicer: &Slicer, dirs: &AppDirs, query: &str, slicer_running: bool) -> Result<UninstallReport> {
    let mut manifest = Manifest::load(dirs)?;
    let doomed: Vec<InstalledProfile> =
        installed_matching(&manifest, slicer, query).into_iter().cloned().collect();
    if doomed.is_empty() {
        return Err(Error::NotInstalled(query.to_string()));
    }
    if slicer_running {
        return Err(Error::SlicerRunning(slicer.kind.display_name()));
    }

    let backup = backup_user_folder(slicer, dirs)?;
    let user_root = slicer.user_root();
    let mut report = UninstallReport { removed: Vec::new(), missing: Vec::new(), backup: Some(backup), cloud_synced: Vec::new() };

    for profile in &doomed {
        let synced = profile
            .files
            .iter()
            .filter(|f| f.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|f| Slicer::preset_info(f))
            .any(|info| info.is_cloud_synced());
        if synced {
            report.cloud_synced.push(profile.profile_name.clone());
        }
        for file in &profile.files {
            // The manifest is a plain file; never let it point the tool outside the user folder.
            if !file.starts_with(&user_root) {
                return Err(Error::Other(format!(
                    "manifest lists {} which is outside {}; not removing it",
                    file.display(),
                    user_root.display()
                )));
            }
            if file.exists() {
                fs::remove_file(file).map_err(Error::write(file))?;
                report.removed.push(file.clone());
            } else {
                report.missing.push(file.clone());
            }
        }
        manifest.profiles.retain(|p| p != profile);
        manifest.save(dirs)?;
    }
    Ok(report)
}

/// Rewrites the settings of a profile the tool installed, keeping its name, its files
/// and its cloud identity. Only profiles in the manifest can be updated.
pub fn update(
    slicer: &Slicer,
    conf: &AppConf,
    dirs: &AppDirs,
    profile: &FilamentProfile,
    recipe: Option<Recipe>,
    options: InstallOptions,
) -> Result<InstallReport> {
    update_checked(slicer, conf, dirs, profile, recipe, options, slicer.is_running()?)
}

fn update_checked(
    slicer: &Slicer,
    conf: &AppConf,
    dirs: &AppDirs,
    profile: &FilamentProfile,
    recipe: Option<Recipe>,
    options: InstallOptions,
    slicer_running: bool,
) -> Result<InstallReport> {
    let mut manifest = Manifest::load(dirs)?;
    let not_installed = || Error::NotInstalled(profile.name.clone());
    let index = manifest
        .profiles
        .iter()
        .position(|p| p.slicer == slicer.kind.id() && p.profile_name == profile.name)
        .ok_or_else(not_installed)?;
    profile.validate()?;
    let json_path = manifest.profiles[index]
        .files
        .iter()
        .find(|f| f.extension().is_some_and(|ext| ext == "json") && f.exists())
        .cloned()
        .ok_or_else(not_installed)?;
    let info_path = json_path.with_extension("info");
    let mut info = Slicer::preset_info(&json_path).unwrap_or_else(PresetInfo::new_unsynced);

    let will_cloud_sync = conf.uploads_new_presets();
    let files = vec![json_path.clone(), info_path.clone()];
    if options.dry_run {
        return Ok(InstallReport { dry_run: true, files, backup: None, will_cloud_sync });
    }
    if will_cloud_sync && !options.confirm_cloud_sync {
        return Err(Error::CloudSyncNotConfirmed { user_id: conf.preset_folder.clone() });
    }
    if slicer_running {
        return Err(Error::SlicerRunning(slicer.kind.display_name()));
    }

    let backup = backup_user_folder(slicer, dirs)?;
    // A newer time marks the file as changed. For a profile the cloud already holds,
    // `update` is the slicer's own state for "changed here, upload it" (PROFILE_SPEC.md §4).
    info.updated_time = PresetInfo::new_unsynced().updated_time.max(info.updated_time + 1);
    if info.is_cloud_synced() {
        info.sync_info = "update".to_string();
    }
    write_atomically(&json_path, profile.to_json().as_bytes())?;
    write_atomically(&info_path, info.to_string().as_bytes())?;
    manifest.profiles[index].recipe = recipe;
    manifest.save(dirs)?;
    Ok(InstallReport { dry_run: false, files, backup: Some(backup), will_cloud_sync })
}

/// Zips the slicer's whole `user` folder into the tool's backup folder.
pub fn backup_user_folder(slicer: &Slicer, dirs: &AppDirs) -> Result<PathBuf> {
    let failed = |error: &dyn std::fmt::Display| Error::Backup(error.to_string());
    let user_root = slicer.user_root();
    let backups = dirs.backups_dir();
    fs::create_dir_all(&backups).map_err(Error::write(&backups))?;

    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let path = backups.join(format!("{}-user-{stamp}.zip", slicer.kind.id()));
    let file = fs::File::create(&path).map_err(Error::write(&path))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut pending = vec![user_root.clone()];
    while let Some(dir) = pending.pop() {
        let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
            .map_err(Error::read(&dir))?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .collect();
        entries.sort();
        for entry in entries {
            let relative = entry.strip_prefix(&user_root).map_err(|e| failed(&e))?;
            let name = relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            if entry.is_dir() {
                zip.add_directory(format!("{name}/"), options).map_err(|e| failed(&e))?;
                pending.push(entry);
            } else {
                zip.start_file(name, options).map_err(|e| failed(&e))?;
                let bytes = fs::read(&entry).map_err(Error::read(&entry))?;
                zip.write_all(&bytes).map_err(|e| failed(&e))?;
            }
        }
    }
    zip.finish().map_err(|e| failed(&e))?;
    Ok(path)
}

/// Writes one `<filament name>.bbsflmt` per filament into `out_dir`, for
/// File > Import > Import Configs. Touches nothing in the slicer's folder.
pub fn export_bundles(profiles: &[FilamentProfile], out_dir: &Path, user_id: Option<&str>) -> Result<Vec<PathBuf>> {
    let failed = |error: &dyn std::fmt::Display| Error::Other(format!("bundle export failed: {error}"));
    for profile in profiles {
        profile.validate()?;
    }
    fs::create_dir_all(out_dir).map_err(Error::write(out_dir))?;

    let mut by_filament: BTreeMap<&str, Vec<&FilamentProfile>> = BTreeMap::new();
    for profile in profiles {
        by_filament.entry(profile.alias()).or_default().push(profile);
    }

    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut written = Vec::new();
    for (alias, group) in by_filament {
        let path = out_dir.join(format!("{alias}.{BUNDLE_EXTENSION}"));
        let file = fs::File::create(&path).map_err(Error::write(&path))?;
        let mut zip = zip::ZipWriter::new(file);

        let mut by_vendor: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for profile in group {
            let vendor = profile.filament_vendor[0].as_str();
            let entry = format!("{vendor}/{}.json", profile.file_stem());
            zip.start_file(entry.as_str(), options).map_err(|e| failed(&e))?;
            zip.write_all(profile.to_json().as_bytes()).map_err(|e| failed(&e))?;
            by_vendor.entry(vendor).or_default().push(entry);
        }

        let structure = json!({
            "bundle_id": format!("{}_{alias}_{stamp}", user_id.unwrap_or("offline")),
            "bundle_type": "filament config bundle",
            "filament_name": alias,
            "filament_vendor": by_vendor
                .iter()
                .map(|(vendor, paths)| json!({ "vendor": vendor, "filament_path": paths }))
                .collect::<Vec<_>>(),
            "version": "",
        });
        zip.start_file(BUNDLE_STRUCTURE_FILE, options).map_err(|e| failed(&e))?;
        let text = serde_json::to_string_pretty(&structure).expect("bundle structure serializes");
        zip.write_all(text.as_bytes()).map_err(|e| failed(&e))?;
        zip.finish().map_err(|e| failed(&e))?;
        written.push(path);
    }
    Ok(written)
}

/// Writes through a temporary file so the slicer never sees a half-written profile.
/// Studio deletes profiles it cannot parse, so the temporary name must not end in `.json`.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(Error::write(dir))?;
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, bytes).map_err(Error::write(&temporary))?;
    fs::rename(&temporary, path).map_err(Error::write(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slicer::SlicerKind;

    fn fixture_slicer(temp: &Path) -> Slicer {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/BambuStudio");
        let target = temp.join("BambuStudio");
        copy_dir(&source, &target);
        Slicer::at(SlicerKind::BambuStudio, target).unwrap()
    }

    fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    /// A real Studio-made profile under a new name, standing in for a generated one.
    fn planned(slicer: &Slicer, conf: &AppConf, alias: &str) -> PlannedProfile {
        let source = slicer.custom_filament_dir(conf).join("eSUN PLA Plus @Bambu Lab X2D 0.4 nozzle.json");
        let mut profile = FilamentProfile::from_json(&fs::read_to_string(source).unwrap()).unwrap();
        profile.name = format!("{alias} @Bambu Lab X2D 0.4 nozzle");
        profile.filament_settings_id = vec![profile.name.clone()];
        profile.filament_id = crate::profile::user_filament_id(alias, conf.user_id());
        PlannedProfile { catalogue_key: "test/PLA/test".into(), profile, recipe: None }
    }

    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files = Vec::new();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(current) = pending.pop() {
            for entry in fs::read_dir(current).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    files.push((path.clone(), fs::read(path).unwrap()));
                }
            }
        }
        files.sort();
        files
    }

    const CONFIRMED: InstallOptions = InstallOptions { confirm_cloud_sync: true, dry_run: false };

    #[test]
    fn install_then_uninstall_restores_the_user_folder_exactly() {
        let temp = tempfile::tempdir().unwrap();
        let slicer = fixture_slicer(temp.path());
        let dirs = AppDirs::under(&temp.path().join("app"));
        let conf = slicer.conf().unwrap();
        let before = snapshot(&slicer.user_root());

        let item = planned(&slicer, &conf, "Test PLA");
        let report = install_checked(&slicer, &conf, &dirs, &[item.clone()], CONFIRMED, false).unwrap();

        let json_path = slicer.custom_filament_dir(&conf).join("Test PLA @Bambu Lab X2D 0.4 nozzle.json");
        assert_eq!(report.files, vec![json_path.clone(), json_path.with_extension("info")]);
        assert_eq!(fs::read_to_string(&json_path).unwrap(), item.profile.to_json());
        let info = Slicer::preset_info(&json_path).unwrap();
        assert!(!info.is_cloud_synced() && info.base_id.is_empty() && info.updated_time > 0);
        assert!(report.backup.as_ref().unwrap().exists());
        assert_eq!(Manifest::load(&dirs).unwrap().profiles.len(), 1);
        assert_eq!(snapshot(&slicer.user_root()).len(), before.len() + 2);

        let removed = uninstall_checked(&slicer, &dirs, "Test PLA", false).unwrap();
        assert_eq!(removed.removed.len(), 2);
        assert!(removed.cloud_synced.is_empty());
        assert_eq!(snapshot(&slicer.user_root()), before);
        assert!(Manifest::load(&dirs).unwrap().profiles.is_empty());
    }

    #[test]
    fn update_rewrites_settings_and_keeps_the_cloud_identity() {
        let temp = tempfile::tempdir().unwrap();
        let slicer = fixture_slicer(temp.path());
        let dirs = AppDirs::under(&temp.path().join("app"));
        let conf = slicer.conf().unwrap();
        let item = planned(&slicer, &conf, "Test PLA");
        install_checked(&slicer, &conf, &dirs, &[item.clone()], CONFIRMED, false).unwrap();
        let json_path = slicer.custom_filament_dir(&conf).join("Test PLA @Bambu Lab X2D 0.4 nozzle.json");
        // Stand in for the slicer having synced it since.
        let synced = PresetInfo { setting_id: "PFUS0123456789abcd".into(), updated_time: 100, ..PresetInfo::default() };
        fs::write(json_path.with_extension("info"), synced.to_string()).unwrap();

        let mut edited = item.profile.clone();
        let count = edited.settings["nozzle_temperature"].as_array().unwrap().len();
        edited.settings.insert("nozzle_temperature".into(), serde_json::json!(vec!["215"; count]));
        let recipe = Recipe { parent: "Base".into(), changes: Vec::new() };

        let unchanged = fs::read(&json_path).unwrap();
        let dry = update_checked(&slicer, &conf, &dirs, &edited, None, InstallOptions { dry_run: true, ..CONFIRMED }, false).unwrap();
        assert!(dry.dry_run && dry.backup.is_none());
        assert!(matches!(
            update_checked(&slicer, &conf, &dirs, &edited, None, CONFIRMED, true),
            Err(Error::SlicerRunning(_))
        ));
        assert!(matches!(
            update_checked(&slicer, &conf, &dirs, &edited, None, InstallOptions::default(), false),
            Err(Error::CloudSyncNotConfirmed { .. })
        ));
        assert_eq!(fs::read(&json_path).unwrap(), unchanged, "refusals and dry runs write nothing");

        let report = update_checked(&slicer, &conf, &dirs, &edited, Some(recipe.clone()), CONFIRMED, false).unwrap();
        assert!(report.backup.unwrap().exists());
        assert_eq!(fs::read_to_string(&json_path).unwrap(), edited.to_json());
        let info = Slicer::preset_info(&json_path).unwrap();
        assert_eq!(info.setting_id, "PFUS0123456789abcd");
        assert_eq!(info.sync_info, "update");
        assert!(info.updated_time > 100);
        let manifest = Manifest::load(&dirs).unwrap();
        assert_eq!(manifest.profiles.len(), 1);
        assert_eq!(manifest.profiles[0].recipe, Some(recipe));

        let mut stranger = edited.clone();
        stranger.name = "Not Mine @Bambu Lab X2D 0.4 nozzle".into();
        stranger.filament_settings_id = vec![stranger.name.clone()];
        assert!(matches!(
            update_checked(&slicer, &conf, &dirs, &stranger, None, CONFIRMED, false),
            Err(Error::NotInstalled(_))
        ));
    }

    #[test]
    fn refuses_while_the_slicer_is_running_and_writes_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let slicer = fixture_slicer(temp.path());
        let dirs = AppDirs::under(&temp.path().join("app"));
        let conf = slicer.conf().unwrap();
        let before = snapshot(&slicer.user_root());

        let item = planned(&slicer, &conf, "Test PLA");
        let result = install_checked(&slicer, &conf, &dirs, &[item], CONFIRMED, true);

        assert!(matches!(result, Err(Error::SlicerRunning(_))));
        assert_eq!(snapshot(&slicer.user_root()), before);
        assert!(!dirs.data_dir.exists(), "no backup or manifest is created on refusal");
    }

    #[test]
    fn requires_cloud_sync_confirmation_when_the_slicer_syncs_presets() {
        let temp = tempfile::tempdir().unwrap();
        let slicer = fixture_slicer(temp.path());
        let dirs = AppDirs::under(&temp.path().join("app"));
        let mut conf = slicer.conf().unwrap();
        assert!(conf.uploads_new_presets(), "fixture conf mirrors the real one: sync on, signed in");
        let item = planned(&slicer, &conf, "Test PLA");

        let unconfirmed = install_checked(&slicer, &conf, &dirs, &[item.clone()], InstallOptions::default(), false);
        assert!(matches!(unconfirmed, Err(Error::CloudSyncNotConfirmed { .. })));

        conf.sync_user_preset = false;
        let local_only = install_checked(&slicer, &conf, &dirs, &[item], InstallOptions::default(), false).unwrap();
        assert!(!local_only.will_cloud_sync);
    }

    #[test]
    fn never_overwrites_or_removes_profiles_it_did_not_create() {
        let temp = tempfile::tempdir().unwrap();
        let slicer = fixture_slicer(temp.path());
        let dirs = AppDirs::under(&temp.path().join("app"));
        let conf = slicer.conf().unwrap();

        // Same name as the profile Studio created.
        let item = planned(&slicer, &conf, "eSUN PLA Plus");
        let result = install_checked(&slicer, &conf, &dirs, &[item], CONFIRMED, false);
        assert!(matches!(result, Err(Error::ForeignFile { .. })));

        let result = uninstall_checked(&slicer, &dirs, "eSUN PLA Plus", false);
        assert!(matches!(result, Err(Error::NotInstalled(_))));
    }

    #[test]
    fn dry_run_reports_files_without_writing() {
        let temp = tempfile::tempdir().unwrap();
        let slicer = fixture_slicer(temp.path());
        let dirs = AppDirs::under(&temp.path().join("app"));
        let conf = slicer.conf().unwrap();
        let before = snapshot(&slicer.user_root());

        let item = planned(&slicer, &conf, "Test PLA");
        let options = InstallOptions { dry_run: true, confirm_cloud_sync: false };
        // Running slicer and unconfirmed sync do not matter: nothing is written.
        let report = install_checked(&slicer, &conf, &dirs, &[item], options, true).unwrap();

        assert!(report.dry_run && report.will_cloud_sync && report.files.len() == 2);
        assert_eq!(snapshot(&slicer.user_root()), before);
    }

    #[test]
    fn uninstall_reports_profiles_the_cloud_still_holds() {
        let temp = tempfile::tempdir().unwrap();
        let slicer = fixture_slicer(temp.path());
        let dirs = AppDirs::under(&temp.path().join("app"));
        let conf = slicer.conf().unwrap();
        let item = planned(&slicer, &conf, "Test PLA");
        let report = install_checked(&slicer, &conf, &dirs, &[item], CONFIRMED, false).unwrap();

        // What Studio does after uploading: writes the cloud's id into the .info.
        let synced = PresetInfo { setting_id: "PFUS0123456789abcd".into(), ..PresetInfo::new_unsynced() };
        fs::write(&report.files[1], synced.to_string()).unwrap();

        let removed = uninstall_checked(&slicer, &dirs, "test/PLA/test", false).unwrap();
        assert_eq!(removed.cloud_synced, vec!["Test PLA @Bambu Lab X2D 0.4 nozzle".to_string()]);
    }

    #[test]
    fn backup_contains_every_file_of_the_user_folder() {
        let temp = tempfile::tempdir().unwrap();
        let slicer = fixture_slicer(temp.path());
        let dirs = AppDirs::under(&temp.path().join("app"));

        let backup = backup_user_folder(&slicer, &dirs).unwrap();

        let mut archive = zip::ZipArchive::new(fs::File::open(backup).unwrap()).unwrap();
        let user_root = slicer.user_root();
        for (path, bytes) in snapshot(&user_root) {
            let name = path.strip_prefix(&user_root).unwrap().to_string_lossy().replace('\\', "/");
            let mut stored = Vec::new();
            std::io::Read::read_to_end(&mut archive.by_name(&name).unwrap(), &mut stored).unwrap();
            assert_eq!(stored, bytes, "{name}");
        }
    }

    #[test]
    fn bundle_has_the_layout_studio_exports() {
        let temp = tempfile::tempdir().unwrap();
        let slicer = fixture_slicer(temp.path());
        let conf = slicer.conf().unwrap();
        let item = planned(&slicer, &conf, "Test PLA");

        let written = export_bundles(&[item.profile.clone()], &temp.path().join("out"), conf.user_id()).unwrap();

        assert_eq!(written, vec![temp.path().join("out/Test PLA.bbsflmt")]);
        let mut archive = zip::ZipArchive::new(fs::File::open(&written[0]).unwrap()).unwrap();
        let mut names: Vec<String> = archive.file_names().map(|name| name.unwrap().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["bundle_structure.json", "eSUN/Test PLA @Bambu Lab X2D 0.4 nozzle.json"]);

        let mut text = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("bundle_structure.json").unwrap(), &mut text).unwrap();
        let structure: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(structure["bundle_type"], "filament config bundle");
        assert_eq!(structure["filament_name"], "Test PLA");
        assert_eq!(structure["filament_vendor"][0]["vendor"], "eSUN");
        assert_eq!(
            structure["filament_vendor"][0]["filament_path"][0],
            "eSUN/Test PLA @Bambu Lab X2D 0.4 nozzle.json"
        );
    }
}
