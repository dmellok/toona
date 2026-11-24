//! What the window can ask for. Everything here is plain Rust over `toona-core`,
//! so it can be exercised without a window; `lib.rs` only exposes it to the UI.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use toona_core::catalogue::barcode_digits;
use toona_core::install::{self, installed_matching};
use toona_core::mapping::{choose_base, profile_alias};
use toona_core::system::preset_alias;
use toona_core::recipe::catalogue_layer;
use toona_core::{
    build_profile_with_recipe, convert_inherited_with_recipes, AppConf, AppDirs, Catalogue, CatalogueFilament,
    ConvertRequest, Error, FilamentProfile, InstallOptions, InstalledProfile, Layer, Manifest, MaterialClass,
    PlannedProfile, ProfileRequest, Recipe, Refresh, Slicer, SlicerKind, SystemLibrary, Variant,
};

/// The catalogue is published daily; a cached copy is refreshed after this long.
const CATALOGUE_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Colour stripes shown on a catalogue card.
const CARD_STRIPES: usize = 10;

/// An error the window can react to: `kind` picks the dialog, `message` is for people.
#[derive(Debug, Serialize)]
pub struct UiError {
    pub kind: &'static str,
    pub message: String,
}

impl From<Error> for UiError {
    fn from(error: Error) -> Self {
        let kind = match &error {
            Error::SlicerRunning(_) => "running",
            Error::CloudSyncNotConfirmed { .. } => "cloud_sync",
            Error::SlicerNotFound { .. } => "no_slicer",
            Error::NoCatalogue(_) | Error::Download(_) => "offline",
            _ => "other",
        };
        Self { kind, message: error.to_string() }
    }
}

type UiResult<T> = Result<T, UiError>;

fn other(message: impl Into<String>) -> UiError {
    UiError { kind: "other", message: message.into() }
}

pub struct Backend {
    dirs: AppDirs,
    kind: SlicerKind,
    catalogue: Mutex<Option<Arc<Catalogue>>>,
    /// System presets are a few hundred files; parsed once per slicer folder.
    library: Mutex<Option<(PathBuf, Arc<SystemLibrary>)>>,
}

struct Studio {
    slicer: Slicer,
    conf: AppConf,
    library: Arc<SystemLibrary>,
}

#[derive(Serialize)]
pub struct PrinterDto {
    /// The preset name profiles are made for.
    pub name: String,
    pub model: String,
    pub nozzle: String,
    pub selected: bool,
}

#[derive(Serialize)]
pub struct Status {
    pub slicer: &'static str,
    pub found: bool,
    pub running: bool,
    /// New profiles will be uploaded to the slicer's cloud.
    pub cloud_sync: bool,
    pub printers: Vec<PrinterDto>,
    /// Why the slicer cannot be used, if it cannot.
    pub problem: Option<String>,
}

#[derive(Serialize)]
pub struct CardDto {
    pub key: String,
    pub brand: String,
    pub name: String,
    pub material: String,
    pub resin: bool,
    pub colours: usize,
    pub stripes: Vec<String>,
    pub discontinued: bool,
    pub added: bool,
}

#[derive(Serialize)]
pub struct CatalogueDto {
    pub version: String,
    pub warning: Option<String>,
    pub items: Vec<CardDto>,
}

#[derive(Serialize)]
pub struct VariantDto {
    pub name: String,
    /// The filament name the slicer shows for this colour.
    pub alias: String,
    pub hex: Option<String>,
    /// Already in the slicer, put there by Toona.
    pub added: bool,
}

#[derive(Serialize)]
pub struct BaseDto {
    pub preset: String,
    /// Shown to people: the preset name without the printer part.
    pub label: String,
    pub source: &'static str,
}

/// One setting a profile changes over the preset it is built on.
#[derive(Serialize)]
pub struct ChangeDto {
    pub key: String,
    pub layer: Layer,
    /// The base preset's value, if it has one.
    pub from: Option<String>,
    pub to: String,
}

#[derive(Serialize)]
pub struct DetailDto {
    pub key: String,
    pub brand: String,
    pub name: String,
    pub material: String,
    pub nozzle: Option<[f64; 2]>,
    pub bed: Option<[f64; 2]>,
    pub density: Option<f64>,
    pub traits: Vec<String>,
    pub discontinued: bool,
    pub variants: Vec<VariantDto>,
    /// The preset a profile for `printer` would be built on.
    pub base: Option<BaseDto>,
    /// Every preset the slicer offers for that printer, for choosing another base.
    pub bases: Vec<String>,
    /// What adding it would change over the base preset, for the first colour.
    pub changes: Vec<ChangeDto>,
    /// Why it cannot be added, if it cannot.
    pub blocked: Option<String>,
}

#[derive(Deserialize)]
pub struct InstallRequest {
    pub key: String,
    pub variant: Option<String>,
    pub printers: Vec<String>,
    pub base: Option<String>,
    #[serde(default)]
    pub allow_cloud_sync: bool,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize)]
pub struct PlannedDto {
    pub name: String,
    pub filament_id: String,
    pub base: String,
    pub source: &'static str,
}

#[derive(Serialize)]
pub struct InstallDto {
    pub alias: String,
    pub dry_run: bool,
    pub profiles: Vec<PlannedDto>,
    pub files: Vec<String>,
    pub backup: Option<String>,
    pub will_cloud_sync: bool,
}

#[derive(Serialize)]
pub struct AddedDto {
    pub profile_name: String,
    pub alias: String,
    pub catalogue_key: String,
    pub printer: String,
    pub hex: Option<String>,
    pub synced: bool,
    /// The files are still where Toona put them.
    pub present: bool,
    /// The preset it is built on, when known.
    pub parent: Option<String>,
    pub changes: Vec<ChangeDto>,
}

#[derive(Serialize)]
pub struct InStudioDto {
    pub added: Vec<AddedDto>,
    /// Imported profiles the AMS cannot use, which Toona can turn into ones it can.
    pub imported: Vec<String>,
}

#[derive(Serialize)]
pub struct RemoveDto {
    pub removed: usize,
    pub backup: Option<String>,
    /// Profiles the cloud still holds; they come back unless deleted in the slicer.
    pub cloud_synced: Vec<String>,
}

#[derive(Serialize)]
pub struct BarcodeDto {
    pub key: String,
    pub variant: String,
}

impl Backend {
    pub fn new(dirs: AppDirs) -> Self {
        Self { dirs, kind: SlicerKind::BambuStudio, catalogue: Mutex::new(None), library: Mutex::new(None) }
    }

    pub fn standard() -> Result<Self, Error> {
        Ok(Self::new(AppDirs::standard()?))
    }

    fn studio(&self) -> Result<Studio, Error> {
        let slicer = Slicer::detect(self.kind)?;
        let conf = slicer.conf()?;
        let mut cached = self.library.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let library = match cached.as_ref() {
            Some((dir, library)) if *dir == slicer.config_dir => library.clone(),
            _ => {
                let library = Arc::new(slicer.system_library()?);
                *cached = Some((slicer.config_dir.clone(), library.clone()));
                library
            }
        };
        Ok(Studio { slicer, conf, library })
    }

    fn load_catalogue(&self, refresh: Refresh) -> Result<Arc<Catalogue>, Error> {
        let mut cached = self.catalogue.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if refresh != Refresh::Force {
            if let Some(catalogue) = cached.as_ref() {
                return Ok(catalogue.clone());
            }
        }
        let catalogue = Arc::new(Catalogue::load(&self.dirs.cache_dir, refresh)?);
        *cached = Some(catalogue.clone());
        Ok(catalogue)
    }

    fn manifest(&self) -> Manifest {
        Manifest::load(&self.dirs).unwrap_or_default()
    }

    pub fn status(&self) -> Status {
        let slicer = self.kind.display_name();
        match self.studio() {
            Ok(studio) => {
                let running = studio.slicer.is_running();
                Status {
                    slicer,
                    found: true,
                    // If the check itself fails, assume it is open: writing is the risky direction.
                    running: running.as_ref().copied().unwrap_or(true),
                    cloud_sync: studio.conf.uploads_new_presets(),
                    printers: studio
                        .slicer
                        .printers(&studio.conf, &studio.library)
                        .into_iter()
                        .map(|p| PrinterDto { name: p.name, model: p.model, nozzle: p.nozzle, selected: p.selected })
                        .collect(),
                    problem: running.err().map(|error| error.to_string()),
                }
            }
            Err(error) => Status {
                slicer,
                found: false,
                running: false,
                cloud_sync: false,
                printers: Vec::new(),
                problem: Some(error.to_string()),
            },
        }
    }

    pub fn catalogue(&self, force_refresh: bool) -> UiResult<CatalogueDto> {
        let refresh = if force_refresh { Refresh::Force } else { Refresh::IfOlderThan(CATALOGUE_MAX_AGE) };
        let catalogue = self.load_catalogue(refresh)?;
        let manifest = self.manifest();
        let items = catalogue
            .filaments()
            .iter()
            .map(|filament| {
                let mut stripes: Vec<String> = Vec::new();
                for hex in filament.variants.iter().filter_map(|v| clean_hex(v.color_hex.as_deref())) {
                    if !stripes.contains(&hex) {
                        stripes.push(hex);
                    }
                    if stripes.len() == CARD_STRIPES {
                        break;
                    }
                }
                CardDto {
                    key: filament.key.clone(),
                    brand: filament.brand.clone(),
                    name: filament.name.clone(),
                    material: filament.material.clone(),
                    resin: filament.class == MaterialClass::Resin,
                    colours: filament.variants.len(),
                    stripes,
                    discontinued: filament.discontinued,
                    added: manifest.profiles.iter().any(|p| p.catalogue_key == filament.key),
                }
            })
            .collect();
        Ok(CatalogueDto { version: catalogue.version.clone(), warning: catalogue.warning.clone(), items })
    }

    pub fn detail(&self, key: &str, printer: Option<&str>, base_override: Option<&str>) -> UiResult<DetailDto> {
        let catalogue = self.load_catalogue(Refresh::IfMissing)?;
        let filament = find_by_key(&catalogue, key)?;
        let manifest = self.manifest();
        let added_aliases: Vec<&str> = manifest
            .profiles
            .iter()
            .filter(|p| p.catalogue_key == filament.key)
            .filter(|p| printer.is_none_or(|printer| p.printer == printer))
            .map(|p| p.alias.as_str())
            .collect();

        let mut base = None;
        let mut bases = Vec::new();
        let mut blocked = None;
        let mut changes = Vec::new();
        if filament.class == MaterialClass::Resin {
            blocked = Some(format!("{} only takes filament profiles.", self.kind.display_name()));
        } else if let (Some(printer), Ok(studio)) = (printer, self.studio()) {
            bases = studio.library.filaments_for_printer(printer).into_iter().map(str::to_string).collect();
            match choose_base(&studio.library, filament, printer, base_override) {
                Ok(choice) => {
                    // Building it for real is the only complete check that it can be added.
                    let request = ProfileRequest {
                        filament,
                        variant: filament.variants.first(),
                        printer,
                        base: base_override,
                        user_id: studio.conf.user_id(),
                        existing: &[],
                    };
                    match build_profile_with_recipe(&studio.library, &request) {
                        Ok((_, recipe)) => changes = describe(&recipe),
                        Err(error) => blocked = Some(error.to_string()),
                    }
                    base = Some(BaseDto {
                        label: preset_alias(&choice.preset).to_string(),
                        preset: choice.preset,
                        source: choice.source.label(),
                    });
                }
                Err(error) => blocked = Some(error.to_string()),
            }
        }

        let range = |low: Option<f64>, high: Option<f64>| low.zip(high).map(|(low, high)| [low, high]);
        Ok(DetailDto {
            key: filament.key.clone(),
            brand: filament.brand.clone(),
            name: filament.name.clone(),
            material: filament.material.clone(),
            nozzle: range(filament.min_print_temperature, filament.max_print_temperature),
            bed: range(filament.min_bed_temperature, filament.max_bed_temperature),
            density: filament.density,
            traits: filament.traits.iter().cloned().collect(),
            discontinued: filament.discontinued,
            variants: filament
                .variants
                .iter()
                .map(|variant| {
                    let alias = profile_alias(filament, Some(variant));
                    VariantDto {
                        name: variant.name.clone(),
                        hex: clean_hex(variant.color_hex.as_deref()),
                        added: added_aliases.contains(&alias.as_str()),
                        alias,
                    }
                })
                .collect(),
            base,
            bases,
            changes,
            blocked,
        })
    }

    /// Builds the profiles a request asks for, one per printer.
    fn plan(
        &self,
        studio: &Studio,
        catalogue: &Catalogue,
        request: &InstallRequest,
    ) -> UiResult<(String, Vec<PlannedProfile>, Vec<PlannedDto>)> {
        let filament = find_by_key(catalogue, &request.key)?;
        let variant: Option<&Variant> = match &request.variant {
            Some(name) => Some(
                filament
                    .variant(name)
                    .ok_or_else(|| other(format!("{} {} has no colour \"{name}\".", filament.brand, filament.name)))?,
            ),
            None => None,
        };
        if request.printers.is_empty() {
            return Err(Error::NoPrinters.into());
        }
        let existing = studio.slicer.user_filaments(&studio.conf)?;
        let mut planned = Vec::new();
        let mut described = Vec::new();
        for printer in &request.printers {
            let (profile, recipe) = build_profile_with_recipe(
                &studio.library,
                &ProfileRequest {
                    filament,
                    variant,
                    printer,
                    base: request.base.as_deref(),
                    user_id: studio.conf.user_id(),
                    existing: &existing,
                },
            )?;
            let base = choose_base(&studio.library, filament, printer, request.base.as_deref())?;
            described.push(PlannedDto {
                name: profile.name.clone(),
                filament_id: profile.filament_id.clone(),
                base: preset_alias(&base.preset).to_string(),
                source: base.source.label(),
            });
            planned.push(PlannedProfile { catalogue_key: filament.key.clone(), profile, recipe: Some(recipe) });
        }
        Ok((profile_alias(filament, variant), planned, described))
    }

    /// Writes a filament's profiles into the slicer, or with `dry_run` reports what it would write.
    pub fn install(&self, request: &InstallRequest) -> UiResult<InstallDto> {
        let studio = self.studio()?;
        let catalogue = self.load_catalogue(Refresh::IfMissing)?;
        let (alias, planned, profiles) = self.plan(&studio, &catalogue, request)?;
        let options = InstallOptions { confirm_cloud_sync: request.allow_cloud_sync, dry_run: request.dry_run };
        let report = install::install(&studio.slicer, &studio.conf, &self.dirs, &planned, options)?;
        Ok(InstallDto {
            alias,
            dry_run: report.dry_run,
            profiles,
            files: report.files.iter().map(|f| f.display().to_string()).collect(),
            backup: report.backup.map(|b| b.display().to_string()),
            will_cloud_sync: report.will_cloud_sync,
        })
    }

    /// Writes importable bundles into the Downloads folder instead of the slicer's folder.
    pub fn export(&self, request: &InstallRequest) -> UiResult<Vec<String>> {
        let studio = self.studio()?;
        let catalogue = self.load_catalogue(Refresh::IfMissing)?;
        let (_, planned, _) = self.plan(&studio, &catalogue, request)?;
        let downloads = directories::UserDirs::new()
            .and_then(|dirs| dirs.download_dir().map(|dir| dir.to_path_buf()))
            .ok_or_else(|| other("Could not find your Downloads folder."))?;
        let profiles: Vec<_> = planned.into_iter().map(|p| p.profile).collect();
        let written = install::export_bundles(&profiles, &downloads.join("Toona"), studio.conf.user_id())?;
        Ok(written.iter().map(|path| path.display().to_string()).collect())
    }

    pub fn in_studio(&self) -> UiResult<InStudioDto> {
        let manifest = self.manifest();
        let studio = self.studio().ok();
        let catalogue = self.load_catalogue(Refresh::IfMissing).ok();
        let added = manifest
            .profiles
            .iter()
            .filter(|p| p.slicer == self.kind.id())
            .map(|profile| {
                let json = profile.files.iter().find(|f| f.extension().is_some_and(|ext| ext == "json"));
                // Profiles added before recipes were recorded get theirs worked out from the file.
                let recipe = profile.recipe.clone().or_else(|| {
                    recover_recipe(studio.as_ref()?, catalogue.as_deref()?, profile, json?)
                });
                AddedDto {
                    profile_name: profile.profile_name.clone(),
                    alias: profile.alias.clone(),
                    catalogue_key: profile.catalogue_key.clone(),
                    printer: profile.printer.clone(),
                    hex: json.and_then(|path| profile_colour(path)),
                    synced: json.and_then(|path| Slicer::preset_info(path)).is_some_and(|info| info.is_cloud_synced()),
                    present: profile.files.iter().all(|f| f.exists()),
                    parent: recipe.as_ref().map(|r| preset_alias(&r.parent).to_string()),
                    changes: recipe.as_ref().map(describe).unwrap_or_default(),
                }
            })
            .collect();
        // Without the slicer there is nothing imported to list; the added list still stands.
        let imported = match &studio {
            Some(studio) => studio
                .slicer
                .user_filaments(&studio.conf)?
                .into_iter()
                .filter(|f| !f.is_root)
                .map(|f| f.name)
                .collect(),
            None => Vec::new(),
        };
        Ok(InStudioDto { added, imported })
    }

    pub fn remove(&self, profile_name: &str) -> UiResult<RemoveDto> {
        let studio = self.studio()?;
        if installed_matching(&self.manifest(), &studio.slicer, profile_name).is_empty() {
            return Err(Error::NotInstalled(profile_name.to_string()).into());
        }
        let report = install::uninstall(&studio.slicer, &self.dirs, profile_name)?;
        Ok(RemoveDto {
            removed: report.removed.len(),
            backup: report.backup.map(|b| b.display().to_string()),
            cloud_synced: report.cloud_synced,
        })
    }

    /// Turns imported profiles into custom filaments the AMS can use. The originals are kept.
    pub fn fix_imported(&self, names: &[String], allow_cloud_sync: bool, dry_run: bool) -> UiResult<InstallDto> {
        let studio = self.studio()?;
        let existing = studio.slicer.user_filaments(&studio.conf)?;
        let printers = studio.slicer.printers(&studio.conf, &studio.library);
        let mut planned = Vec::new();
        let mut described = Vec::new();
        for name in names {
            let source = existing
                .iter()
                .find(|f| !f.is_root && f.name == *name)
                .ok_or_else(|| other(format!("No imported profile is called \"{name}\".")))?;
            let request = ConvertRequest {
                source,
                alias: None,
                vendor: None,
                printers: &printers,
                user_id: studio.conf.user_id(),
                existing: &existing,
            };
            for (profile, recipe) in convert_inherited_with_recipes(&studio.library, &request)? {
                described.push(PlannedDto {
                    name: profile.name.clone(),
                    filament_id: profile.filament_id.clone(),
                    base: source.name.clone(),
                    source: "imported profile",
                });
                planned.push(PlannedProfile {
                    catalogue_key: format!("converted/{}", source.name),
                    profile,
                    recipe: Some(recipe),
                });
            }
        }
        let options = InstallOptions { confirm_cloud_sync: allow_cloud_sync, dry_run };
        let report = install::install(&studio.slicer, &studio.conf, &self.dirs, &planned, options)?;
        Ok(InstallDto {
            alias: names.join(", "),
            dry_run: report.dry_run,
            profiles: described,
            files: report.files.iter().map(|f| f.display().to_string()).collect(),
            backup: report.backup.map(|b| b.display().to_string()),
            will_cloud_sync: report.will_cloud_sync,
        })
    }

    /// What a scanned or typed barcode identifies. Empty if the text is not a barcode.
    pub fn barcode(&self, code: &str) -> UiResult<Vec<BarcodeDto>> {
        if barcode_digits(code).is_none() {
            return Ok(Vec::new());
        }
        let catalogue = self.load_catalogue(Refresh::IfMissing)?;
        Ok(catalogue
            .find_barcode(code)
            .into_iter()
            .map(|found| BarcodeDto { key: found.filament.key.clone(), variant: found.variant.name.clone() })
            .collect())
    }
}

fn find_by_key<'a>(catalogue: &'a Catalogue, key: &str) -> UiResult<&'a CatalogueFilament> {
    catalogue
        .filaments()
        .iter()
        .find(|filament| filament.key == key)
        .ok_or_else(|| Error::FilamentNotFound(key.to_string()).into())
}

/// A colour the window can use directly: `#RRGGBB`, or nothing.
fn clean_hex(raw: Option<&str>) -> Option<String> {
    let digits = raw?.trim().trim_start_matches('#');
    let digits = digits.get(..6)?;
    digits.chars().all(|c| c.is_ascii_hexdigit()).then(|| format!("#{}", digits.to_ascii_uppercase()))
}

/// The colour a profile file gives its filament.
fn profile_colour(path: &std::path::Path) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    clean_hex(json.get("default_filament_colour")?.get(0)?.as_str())
}

/// A setting's value as people read it: `["220", "220"]` is `220`.
fn show_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(items) => {
            let mut parts: Vec<String> = Vec::new();
            for item in items {
                let text = item.as_str().map(str::to_string).unwrap_or_else(|| item.to_string());
                if !parts.contains(&text) {
                    parts.push(text);
                }
            }
            parts.join(", ")
        }
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn describe(recipe: &Recipe) -> Vec<ChangeDto> {
    recipe
        .changes
        .iter()
        .map(|change| ChangeDto {
            key: change.key.clone(),
            layer: change.layer,
            from: change.from.as_ref().map(show_value),
            to: show_value(&change.to),
        })
        .collect()
}

/// Works out the recipe of a catalogue profile from its file: the base preset the
/// tool would choose for it today, and how the file differs from that preset.
fn recover_recipe(
    studio: &Studio,
    catalogue: &Catalogue,
    profile: &InstalledProfile,
    json: &std::path::Path,
) -> Option<Recipe> {
    let filament = catalogue.filaments().iter().find(|f| f.key == profile.catalogue_key)?;
    let written = FilamentProfile::from_json(&std::fs::read_to_string(json).ok()?).ok()?;
    let base = choose_base(&studio.library, filament, &profile.printer, None).ok()?;
    let mut base_settings = studio.library.resolve_filament(&base.preset).ok()?.settings;
    for key in ["compatible_printers", "filament_vendor", "filament_type"] {
        base_settings.remove(key);
    }
    Some(Recipe::between(&base.preset, &base_settings, &written.settings, catalogue_layer))
}
