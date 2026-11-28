//! What the window can ask for. Everything here is plain Rust over `toona-core`,
//! so it can be exercised without a window; `lib.rs` only exposes it to the UI.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use toona_core::catalogue::barcode_digits;

use crate::community::{apply_shared, layer_named, shareable, Community, Link, SignIn, User, Version};
use toona_core::install::{self, installed_matching};
use toona_core::mapping::{choose_base, profile_alias};
use toona_core::system::preset_alias;
use toona_core::recipe::catalogue_layer;
use toona_core::{
    build_profile_with_recipe, convert_inherited_with_recipes, AppConf, AppDirs, Catalogue, CatalogueFilament,
    ConvertRequest, Error, FilamentProfile, InstallOptions, Layer, Manifest, MaterialClass,
    Family, PlannedProfile, PrinterPreset, ProfileRequest, Recipe, Refresh, Slicer, SlicerKind, SystemLibrary, Variant,
};

/// The slicers the window offers, in the order it prefers them when nothing was chosen.
const SLICERS: &[SlicerKind] = &[SlicerKind::BambuStudio, SlicerKind::PrusaSlicer];

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
    community: Community,
    kind: Mutex<SlicerKind>,
    /// The slicer's settings folder, when it is not to be looked for in its usual place.
    folder: Option<PathBuf>,
    catalogue: Mutex<Option<Arc<Catalogue>>>,
    /// System presets are hundreds of files; parsed once per slicer folder, and again
    /// when the slicer's presets have changed since.
    library: Mutex<Option<(PathBuf, Option<std::time::SystemTime>, Arc<SystemLibrary>)>>,
}

struct Studio {
    slicer: Slicer,
    conf: AppConf,
    library: Arc<SystemLibrary>,
}

impl Studio {
    fn printers(&self) -> Vec<PrinterPreset> {
        self.slicer.printers(&self.conf, &self.library)
    }

    /// The set-up printer presets a profile in the slicer's folder is for. A Bambu Studio
    /// profile names its one printer; a PrusaSlicer preset shows for every printer its
    /// condition holds for. `made_for` stands in when the file does not say.
    fn printers_of(&self, path: &std::path::Path, made_for: Option<&str>, printers: &[PrinterPreset]) -> Vec<String> {
        let written = FilamentProfile::read(path);
        let mut found: Vec<String> = match (&written, self.library.family) {
            (Some(written), Family::Bambu) => written.compatible_printers.first().cloned().into_iter().collect(),
            (Some(written), Family::Prusa) => printers
                .iter()
                .filter(|printer| self.library.shows_for(&written.settings, &printer.name))
                .map(|printer| printer.name.clone())
                .collect(),
            (None, _) => Vec::new(),
        };
        if found.is_empty() {
            found.extend(made_for.map(str::to_string));
        }
        found
    }
}

#[derive(Serialize)]
pub struct SlicerDto {
    pub id: &'static str,
    pub name: &'static str,
    /// Its settings folder is on this computer.
    pub found: bool,
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
    /// [`SlicerKind::id`] of the slicer being worked with.
    pub id: &'static str,
    /// Every slicer the window can work with.
    pub slicers: Vec<SlicerDto>,
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

/// One thing the manufacturer lists, next to what the profile would use.
#[derive(Serialize)]
pub struct SpecDto {
    /// `nozzle`, `bed`, `flow` or `density`.
    pub key: &'static str,
    /// What the manufacturer lists, if anything.
    pub spec: Option<String>,
    /// What the profile would print with.
    pub value: Option<String>,
    /// Whether the profile's value is inside what the manufacturer lists; unknown without both.
    pub ok: Option<bool>,
    /// For `flow`, in mm³/s: the manufacturer's limit, the profile's value and the base preset's.
    pub limit: Option<f64>,
    pub at: Option<f64>,
    pub base: Option<f64>,
}

/// Where printer-specific settings from the manufacturer came from.
#[derive(Serialize)]
pub struct TunedDto {
    pub brand: String,
    /// The page the values were read from.
    pub source: String,
    pub retrieved: String,
    /// How many settings it sets on the printer asked about.
    pub settings: usize,
    /// Printer presets of this model the values were published for.
    pub published: Vec<String>,
    /// The values were published for the printer asked about, not borrowed from another nozzle.
    pub own: bool,
    /// The values in use on the printer asked about come from this other preset.
    pub borrowed_from: Option<String>,
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
    /// The filament name the slicer shows when it is added without a colour.
    pub alias: String,
    /// The name the catalogue gives the colour on screen, before any renaming.
    pub default_alias: String,
    /// Already in the slicer without a colour, put there by Toona.
    pub added: bool,
    /// The preset a profile for `printer` would be built on.
    pub base: Option<BaseDto>,
    /// Every preset the slicer offers for that printer, for choosing another base.
    pub bases: Vec<String>,
    /// The printer presets the base in use exists for. With a base the user chose, a
    /// nozzle the slicer has no such preset for is left out.
    pub base_printers: Vec<String>,
    /// What adding it would change over the base preset, for the chosen colour.
    pub changes: Vec<ChangeDto>,
    /// The manufacturer's figures next to the profile's.
    pub spec: Vec<SpecDto>,
    /// Set when the manufacturer published settings for this very printer and nozzle,
    /// which the profile then uses.
    pub tuned: Option<TunedDto>,
    /// Why it cannot be added, if it cannot.
    pub blocked: Option<String>,
}

#[derive(Deserialize)]
pub struct InstallRequest {
    pub key: String,
    pub variant: Option<String>,
    pub printers: Vec<String>,
    pub base: Option<String>,
    /// A filament name to use instead of the catalogue's.
    #[serde(default)]
    pub alias: Option<String>,
    /// Use the manufacturer's settings for another nozzle where it published none for this one.
    #[serde(default)]
    pub borrow_tuned: bool,
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
    /// Who makes it: the catalogue's brand, or else the vendor written in the profile.
    pub brand: Option<String>,
    pub printer: String,
    pub hex: Option<String>,
    /// The material, such as `PETG`, as the profile names it.
    pub material: Option<String>,
    /// Nozzle and bed temperature in °C and the flow limit in mm³/s, as the profile sets them.
    pub nozzle_temp: Option<String>,
    pub bed_temp: Option<String>,
    pub max_flow: Option<String>,
    pub synced: bool,
    /// The files are still where Toona put them.
    pub present: bool,
    /// The preset it is built on, when known.
    pub parent: Option<String>,
    pub changes: Vec<ChangeDto>,
    /// It was made from, or already shared as, a version on the Toona server.
    pub linked: bool,
}

/// A custom filament the user made in the slicer themselves.
#[derive(Serialize)]
pub struct OwnDto {
    pub profile_name: String,
    pub alias: String,
    /// Who makes it: the linked product's brand, or else the vendor written in the profile.
    pub brand: Option<String>,
    pub printer: String,
    pub hex: Option<String>,
    /// The material, such as `PETG`, as the profile names it.
    pub material: Option<String>,
    /// Nozzle and bed temperature in °C and the flow limit in mm³/s, as the profile sets them.
    pub nozzle_temp: Option<String>,
    pub bed_temp: Option<String>,
    pub max_flow: Option<String>,
    pub synced: bool,
    /// The catalogue product the user said this is, if they have.
    pub catalogue_key: Option<String>,
    /// That product's name, for showing.
    pub product: Option<String>,
    /// The preset Toona compares it against, once it is linked to a product.
    pub parent: Option<String>,
    pub changes: Vec<ChangeDto>,
    /// Already shared as a version on the Toona server.
    pub linked: bool,
}

/// A profile the user imported, which the AMS does not offer.
#[derive(Serialize)]
pub struct ImportedDto {
    pub name: String,
    /// Why Toona cannot make an AMS copy of it, if it cannot.
    pub problem: Option<String>,
    /// The AMS copy already exists, so there is nothing left to fix.
    pub fixed: bool,
}

#[derive(Serialize)]
pub struct InStudioDto {
    pub added: Vec<AddedDto>,
    /// Custom filaments made in the slicer, which Toona lists and can share but never changes.
    pub own: Vec<OwnDto>,
    /// Imported profiles the AMS cannot use. Toona can turn most into ones it can.
    pub imported: Vec<ImportedDto>,
}

#[derive(Serialize)]
pub struct RemoveDto {
    pub removed: usize,
    pub backup: Option<String>,
    /// Profiles the cloud still holds; they come back unless deleted in the slicer.
    pub cloud_synced: Vec<String>,
}

/// How many backups the restore list offers.
const BACKUPS_LISTED: usize = 30;

/// A backup of the slicer's folder that can be restored.
#[derive(Serialize)]
pub struct BackupDto {
    pub path: String,
    /// UTC, RFC 3339.
    pub taken: String,
    /// What Toona was about to do when it took it; empty for older backups.
    pub note: String,
    pub bytes: u64,
}

#[derive(Serialize)]
pub struct BarcodeDto {
    pub key: String,
    pub variant: String,
}

impl Backend {
    pub fn new(dirs: AppDirs) -> Self {
        let community = Community::new(dirs.data_dir.clone());
        let backend =
            Self { dirs, community, kind: Mutex::new(SLICERS[0]), folder: None, catalogue: Mutex::new(None), library: Mutex::new(None) };
        // The slicer chosen last time; before any choice, the first one that is installed.
        let kind = backend
            .chosen_slicer()
            .or_else(|| SLICERS.iter().copied().find(|kind| Slicer::detect(*kind).is_ok()))
            .unwrap_or(SLICERS[0]);
        backend.use_slicer(kind);
        backend
    }

    /// Works with one slicer whatever was chosen before, and without remembering it; with
    /// `folder`, on a settings folder somewhere other than the usual place. For examples and tests.
    pub fn with_slicer(mut self, kind: SlicerKind, folder: Option<PathBuf>) -> Self {
        self.use_slicer(kind);
        self.folder = folder;
        self
    }

    /// The slicer being worked with.
    pub(crate) fn kind(&self) -> SlicerKind {
        *self.kind.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn use_slicer(&self, kind: SlicerKind) {
        *self.kind.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = kind;
    }

    /// Switches to another slicer and remembers the choice.
    pub fn set_slicer(&self, id: &str) -> UiResult<Status> {
        let kind = SLICERS
            .iter()
            .copied()
            .find(|kind| kind.id() == id)
            .ok_or_else(|| other(format!("Toona doesn't work with \"{id}\".")))?;
        self.remember_slicer(kind)?;
        self.use_slicer(kind);
        Ok(self.status())
    }

    pub fn standard() -> Result<Self, Error> {
        Ok(Self::new(AppDirs::standard()?))
    }

    fn studio(&self) -> Result<Studio, Error> {
        let slicer = match &self.folder {
            Some(folder) => Slicer::at(self.kind(), folder)?,
            None => Slicer::detect(self.kind())?,
        };
        let conf = slicer.conf()?;
        let stamp = slicer.library_stamp();
        let mut cached = self.library.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let library = match cached.as_ref() {
            Some((dir, loaded, library)) if *dir == slicer.config_dir && *loaded == stamp => library.clone(),
            _ => {
                let library = Arc::new(slicer.system_library()?);
                *cached = Some((slicer.config_dir.clone(), stamp, library.clone()));
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

    pub(crate) fn data_dir(&self) -> &std::path::Path {
        &self.dirs.data_dir
    }

    pub(crate) fn server(&self) -> &Community {
        &self.community
    }

    /// How many profiles Toona has put in the slicer.
    pub(crate) fn added_count(&self) -> usize {
        self.manifest().profiles.len()
    }

    /// Catalogue products added after `since` (a time as the manifest writes it; everything
    /// when `None`), each once per slicer, with the time the latest of them was added.
    /// Copies made of the user's own imported profiles are left out: their keys carry the
    /// user's own names for things.
    pub(crate) fn products_added_since(&self, since: Option<&str>) -> (Vec<(String, &'static str)>, Option<String>) {
        let mut products: Vec<(String, &'static str)> = Vec::new();
        let mut latest: Option<String> = None;
        for profile in self.manifest().profiles {
            if profile.catalogue_key.starts_with("converted/") || since.is_some_and(|since| profile.installed_at.as_str() <= since) {
                continue;
            }
            let Some(kind) = SlicerKind::from_id(&profile.slicer) else { continue };
            let product = (profile.catalogue_key, kind.display_name());
            if !products.contains(&product) {
                products.push(product);
            }
            if latest.as_deref().is_none_or(|latest| profile.installed_at.as_str() > latest) {
                latest = Some(profile.installed_at);
            }
        }
        products.sort();
        (products, latest)
    }

    fn manifest(&self) -> Manifest {
        Manifest::load(&self.dirs).unwrap_or_default()
    }

    pub fn status(&self) -> Status {
        let kind = self.kind();
        let slicer = kind.display_name();
        let id = kind.id();
        let slicers: Vec<SlicerDto> = SLICERS
            .iter()
            .map(|kind| SlicerDto { id: kind.id(), name: kind.display_name(), found: Slicer::detect(*kind).is_ok() })
            .collect();
        match self.studio() {
            Ok(studio) => {
                let running = studio.slicer.is_running();
                Status {
                    slicer,
                    id,
                    slicers,
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
                id,
                slicers,
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
        let slicer = self.kind().id();
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
                    added: manifest.profiles.iter().any(|p| p.slicer == slicer && p.catalogue_key == filament.key),
                }
            })
            .collect();
        Ok(CatalogueDto { version: catalogue.version.clone(), warning: catalogue.warning.clone(), items })
    }

    pub fn detail(
        &self,
        key: &str,
        printer: Option<&str>,
        base_override: Option<&str>,
        colour: Option<&str>,
        name: Option<&str>,
        borrow_tuned: bool,
    ) -> UiResult<DetailDto> {
        let catalogue = self.load_catalogue(Refresh::IfMissing)?;
        let filament = find_by_key(&catalogue, key)?;
        let variant = colour.and_then(|name| filament.variant(name));
        let manifest = self.manifest();
        let studio = self.studio();
        let set_up = studio.as_ref().map(Studio::printers).unwrap_or_default();
        let added_aliases: Vec<&str> = manifest
            .profiles
            .iter()
            .filter(|p| p.slicer == self.kind().id() && p.catalogue_key == filament.key)
            .filter(|p| {
                printer.is_none_or(|printer| match (&studio, p.profile_file()) {
                    // What the file says counts: a PrusaSlicer preset serves several printers.
                    (Ok(studio), Some(file)) => studio.printers_of(file, Some(&p.printer), &set_up).iter().any(|name| name == printer),
                    _ => p.printer == printer,
                })
            })
            .map(|p| p.alias.as_str())
            .collect();

        let mut base = None;
        let mut bases = Vec::new();
        let mut blocked = None;
        let mut changes = Vec::new();
        let mut base_printers = Vec::new();
        let mut built = None;
        if filament.class == MaterialClass::Resin {
            blocked = Some(format!("{} only takes filament profiles.", self.kind().display_name()));
        } else if let (Some(printer), Ok(studio)) = (printer, &studio) {
            bases = studio.library.filaments_for_printer(printer).into_iter().map(str::to_string).collect();
            base_printers = set_up
                .iter()
                .filter(|p| choose_base(&studio.library, filament, &p.name, base_override).is_ok())
                .map(|p| p.name.clone())
                .collect();
            match choose_base(&studio.library, filament, printer, base_override) {
                Ok(choice) => {
                    // Building it for real is the only complete check that it can be added.
                    let request = ProfileRequest {
                        filament,
                        variant,
                        printer,
                        base: base_override,
                        user_id: studio.conf.user_id(),
                        existing: &[],
                        alias: name,
                        borrow_tuned,
                    };
                    match build_profile_with_recipe(&studio.library, &request) {
                        Ok((profile, recipe)) => {
                            changes = describe(&recipe);
                            let base_flow = studio
                                .library
                                .resolve_filament(&choice.preset)
                                .ok()
                                .and_then(|base| first(base.settings.get(FLOW_KEY)?))
                                // PrusaSlicer writes "no limit" as 0, which is no figure to compare with.
                                .filter(|flow| *flow > 0.0);
                            built = Some((profile, base_flow));
                        }
                        Err(error) => blocked = Some(sentence(&error.to_string())),
                    }
                    base = Some(BaseDto {
                        label: preset_alias(&choice.preset).to_string(),
                        preset: choice.preset,
                        source: choice.source.label(),
                    });
                }
                Err(error) => blocked = Some(sentence(&error.to_string())),
            }
        }
        let published: Vec<String> = printer
            .filter(|_| built.is_some())
            .map(|printer| toona_core::tuned::published_for(&filament.key, printer).into_iter().map(str::to_string).collect())
            .unwrap_or_default();
        let own = printer.filter(|_| built.is_some()).and_then(|printer| toona_core::tuned::tuned_for(&filament.key, printer));
        let borrowed = printer
            .filter(|_| built.is_some() && borrow_tuned)
            .and_then(|printer| toona_core::tuned::borrowed_for(&filament.key, printer));
        let mut spec = spec_rows(filament, built.as_ref().map(|(profile, flow)| (profile, *flow)), own.map(|(_, entry)| entry));
        if own.is_some() || borrowed.is_some() {
            // The values are the manufacturer's own for this printer; where they differ
            // from the catalogue's general figures, that is not a fault in the profile.
            for row in spec.iter_mut().filter(|row| row.ok == Some(false)) {
                row.ok = None;
            }
        }
        // Any set that covers this product on this model, to say where the values come from.
        let source = own.or(borrowed).or_else(|| published.first().and_then(|name| toona_core::tuned::tuned_for(&filament.key, name)));
        let tuned = source.map(|(set, entry)| TunedDto {
            brand: set.brand.clone(),
            source: set.source.clone(),
            retrieved: set.retrieved.clone(),
            settings: if own.is_some() || borrowed.is_some() { entry.settings.len() } else { 0 },
            published,
            own: own.is_some(),
            borrowed_from: borrowed.map(|(_, entry)| entry.printer.clone()),
        });
        // A name the person chose replaces the catalogue's for the colour on screen.
        let custom = name.map(toona_core::profile::sanitize_alias).filter(|alias| !alias.is_empty());
        let plain = custom.clone().filter(|_| variant.is_none()).unwrap_or_else(|| profile_alias(filament, None));

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
                    let chosen = colour == Some(variant.name.as_str());
                    let alias = custom.clone().filter(|_| chosen).unwrap_or_else(|| profile_alias(filament, Some(variant)));
                    VariantDto {
                        name: variant.name.clone(),
                        hex: clean_hex(variant.color_hex.as_deref()),
                        added: added_aliases.contains(&alias.as_str()),
                        alias,
                    }
                })
                .collect(),
            added: added_aliases.contains(&plain.as_str()),
            default_alias: profile_alias(filament, variant),
            alias: plain,
            base,
            bases,
            base_printers,
            changes,
            spec,
            tuned,
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
                    alias: request.alias.as_deref(),
                    borrow_tuned: request.borrow_tuned,
                },
            )?;
            let base = choose_base(&studio.library, filament, printer, request.base.as_deref())?;
            let dto = PlannedDto {
                name: profile.name.clone(),
                filament_id: profile.filament_id.clone(),
                base: preset_alias(&base.preset).to_string(),
                source: base.source.label(),
            };
            let item = PlannedProfile { catalogue_key: filament.key.clone(), printer: printer.clone(), profile, recipe: Some(recipe) };
            // One PrusaSlicer preset serves every printer its base preset does.
            if item.add_to(&mut planned) {
                described.push(dto);
            }
        }
        let alias = planned.first().map(|p: &PlannedProfile| p.profile.alias().to_string()).unwrap_or_else(|| profile_alias(filament, variant));
        Ok((alias, planned, described))
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
        let links = self.community.links();
        let printers = studio.as_ref().map(Studio::printers).unwrap_or_default();
        let added = manifest
            .profiles
            .iter()
            .filter(|p| p.slicer == self.kind().id())
            .flat_map(|profile| {
                let json = profile.profile_file();
                // Listed once for each printer it serves: a PrusaSlicer preset may serve several.
                let serves = match (&studio, json) {
                    (Some(studio), Some(file)) => studio.printers_of(file, Some(&profile.printer), &printers),
                    _ => vec![profile.printer.clone()],
                };
                // Profiles added before recipes were recorded get theirs worked out from the file.
                let recipe = profile.recipe.clone().or_else(|| {
                    recover_recipe(studio.as_ref()?, catalogue.as_deref()?, &profile.catalogue_key, &profile.printer, json?)
                });
                let facts = json.map(|path| Facts::read(path)).unwrap_or_default();
                let catalogue = &catalogue;
                let links = &links;
                serves.into_iter().map(move |printer| AddedDto {
                    profile_name: profile.profile_name.clone(),
                    alias: profile.alias.clone(),
                    catalogue_key: profile.catalogue_key.clone(),
                    brand: catalogue
                        .as_deref()
                        .and_then(|c| c.filaments().iter().find(|f| f.key == profile.catalogue_key))
                        .map(|f| f.brand.clone())
                        .or(facts.vendor.clone()),
                    printer,
                    hex: facts.hex.clone(),
                    material: facts.material.clone(),
                    nozzle_temp: facts.nozzle_temp.clone(),
                    bed_temp: facts.bed_temp.clone(),
                    max_flow: facts.max_flow.clone(),
                    synced: json.and_then(|path| Slicer::preset_info(path)).is_some_and(|info| info.is_cloud_synced()),
                    present: profile.files.iter().all(|f| f.exists()),
                    parent: recipe.as_ref().map(|r| preset_alias(&r.parent).to_string()),
                    changes: recipe.as_ref().map(describe).unwrap_or_default(),
                    linked: links.contains_key(&profile.profile_name),
                })
            })
            .collect();
        // Without the slicer there is nothing of the user's own to list; the added list still stands.
        let filaments = match &studio {
            Some(studio) => studio.slicer.user_filaments(&studio.conf)?,
            None => Vec::new(),
        };
        let products = self.own_products();
        let own = filaments
            .iter()
            .filter(|f| f.is_root && !manifest.profiles.iter().any(|p| p.files.contains(&f.path)))
            .flat_map(|f| {
                // One that shows for no printer set up here has no place in the list.
                let serves = studio.as_ref().map(|studio| studio.printers_of(&f.path, None, &printers)).unwrap_or_default();
                let key = products.get(&f.name).cloned();
                let filament = key.as_ref().and_then(|key| catalogue.as_deref()?.filaments().iter().find(|c| &c.key == key));
                let recipe = serves
                    .first()
                    .and_then(|printer| recover_recipe(studio.as_ref()?, catalogue.as_deref()?, &filament?.key, printer, &f.path));
                let facts = Facts::read(&f.path);
                let links = &links;
                serves.into_iter().map(move |printer| OwnDto {
                    profile_name: f.name.clone(),
                    alias: f.alias().to_string(),
                    brand: filament.map(|c| c.brand.clone()).or(facts.vendor.clone()),
                    printer,
                    hex: facts.hex.clone(),
                    material: facts.material.clone(),
                    nozzle_temp: facts.nozzle_temp.clone(),
                    bed_temp: facts.bed_temp.clone(),
                    max_flow: facts.max_flow.clone(),
                    synced: Slicer::preset_info(&f.path).is_some_and(|info| info.is_cloud_synced()),
                    product: filament.map(|c| format!("{} {}", c.brand, c.name)),
                    catalogue_key: filament.map(|c| c.key.clone()),
                    parent: recipe.as_ref().map(|r| preset_alias(&r.parent).to_string()),
                    changes: recipe.as_ref().map(describe).unwrap_or_default(),
                    linked: links.contains_key(&f.name),
                })
            })
            .collect();
        // Only what can really be converted is offered for fixing; the rest says why not.
        let imported = filaments
            .iter()
            .filter(|f| !f.is_root)
            .map(|source| {
                let mut fixed = false;
                let problem = studio.as_ref().and_then(|studio| {
                    let request = ConvertRequest {
                        source,
                        alias: None,
                        vendor: None,
                        printers: &printers,
                        user_id: studio.conf.user_id(),
                        existing: &filaments,
                    };
                    match convert_inherited_with_recipes(&studio.library, &request) {
                        Ok(copies) => {
                            fixed = copies.iter().all(|(copy, _)| filaments.iter().any(|f| f.is_root && f.name == copy.name));
                            None
                        }
                        Err(Error::NotConvertible { reason, .. }) => Some(sentence(&reason)),
                        Err(error) => Some(sentence(&error.to_string())),
                    }
                });
                ImportedDto { name: source.name.clone(), problem, fixed }
            })
            .collect();
        Ok(InStudioDto { added, own, imported })
    }

    pub fn remove(&self, profile_name: &str) -> UiResult<RemoveDto> {
        let studio = self.studio()?;
        if installed_matching(&self.manifest(), &studio.slicer, profile_name).is_empty() {
            return Err(Error::NotInstalled(profile_name.to_string()).into());
        }
        let report = install::uninstall(&studio.slicer, &self.dirs, profile_name)?;
        self.community.unlink(profile_name);
        Ok(RemoveDto {
            removed: report.removed.len(),
            backup: report.backup.map(|b| b.display().to_string()),
            cloud_synced: report.cloud_synced,
        })
    }

    /// Removes profiles by their full names behind one backup: what Toona added, and
    /// custom filaments the user made in the slicer themselves.
    pub fn remove_many(&self, profile_names: &[String]) -> UiResult<RemoveDto> {
        let studio = self.studio()?;
        // A PrusaSlicer preset is listed once per printer it serves, so a name may come twice.
        let mut names: Vec<String> = Vec::new();
        for name in profile_names {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        let profile_names = &names[..];
        let report = install::remove_many(&studio.slicer, &studio.conf, &self.dirs, profile_names)?;
        for name in profile_names {
            self.community.unlink(name);
            // The note of which product it was goes with it; failing to forget it harms nothing.
            let _ = self.link_product(name, None);
        }
        Ok(RemoveDto {
            removed: report.removed.len(),
            backup: report.backup.map(|b| b.display().to_string()),
            cloud_synced: report.cloud_synced,
        })
    }

    /// The backups Toona made of the slicer's folder, newest first.
    pub fn backups(&self) -> UiResult<Vec<BackupDto>> {
        let studio = self.studio()?;
        Ok(install::backups(&studio.slicer, &self.dirs)?
            .into_iter()
            .take(BACKUPS_LISTED)
            .map(|backup| BackupDto { path: backup.path.display().to_string(), taken: backup.taken, note: backup.note, bytes: backup.bytes })
            .collect())
    }

    /// Puts the slicer's folder back as it was in a backup. How it is now is backed up first.
    pub fn restore_backup(&self, path: &str) -> UiResult<String> {
        let studio = self.studio()?;
        let safety = install::restore(&studio.slicer, &self.dirs, std::path::Path::new(path))?;
        Ok(safety.display().to_string())
    }

    /// Adds a filament Toona already added for one nozzle to more printer presets: the same
    /// product, colour, name and base, with what the user or the community changed carried over.
    pub fn add_nozzles(&self, profile_name: &str, printers: &[String], borrow_tuned: bool, allow_cloud_sync: bool) -> UiResult<InstallDto> {
        let studio = self.studio()?;
        let catalogue = self.load_catalogue(Refresh::IfMissing)?;
        let manifest = self.manifest();
        let source = manifest
            .profiles
            .iter()
            .find(|p| p.slicer == self.kind().id() && p.profile_name == profile_name)
            .ok_or_else(|| UiError::from(Error::NotInstalled(profile_name.to_string())))?;
        let filament = find_by_key(&catalogue, &source.catalogue_key)?;
        let json = source.profile_file();
        let recipe = source
            .recipe
            .clone()
            .or_else(|| recover_recipe(&studio, &catalogue, &source.catalogue_key, &source.printer, json?));
        // Which colour it is: the one whose usual name it has, or else the one of its colour.
        let hex = json.and_then(|path| Facts::read(path).hex);
        let variant = filament
            .variants
            .iter()
            .find(|v| profile_alias(filament, Some(v)) == source.alias)
            .or_else(|| filament.variants.iter().find(|v| hex.is_some() && clean_hex(v.color_hex.as_deref()) == hex));
        let base = recipe.as_ref().map(|r| preset_alias(&r.parent).to_string());

        let mut planned = Vec::new();
        let mut profiles = Vec::new();
        for printer in printers {
            let request = |base: Option<String>| InstallRequest {
                key: filament.key.clone(),
                variant: variant.map(|v| v.name.clone()),
                printers: vec![printer.clone()],
                base,
                alias: Some(source.alias.clone()),
                borrow_tuned,
                allow_cloud_sync,
                dry_run: false,
            };
            // The same base where this nozzle has it; otherwise the one Toona would pick.
            let (_, mut one, described) = match self.plan(&studio, &catalogue, &request(base.clone())) {
                Ok(done) => done,
                Err(_) if base.is_some() => self.plan(&studio, &catalogue, &request(None))?,
                Err(error) => return Err(error),
            };
            for item in &mut one {
                let Some(new) = item.recipe.as_mut() else { continue };
                let carried = recipe.iter().flat_map(|r| &r.changes).filter(|c| matches!(c.layer, Layer::Community | Layer::You));
                for change in carried {
                    // Only settings this nozzle's profile has, shaped the way it holds them.
                    let Some(current) = item.profile.settings.get(&change.key).cloned() else { continue };
                    let value = single(&change.to).map_or_else(|| change.to.clone(), |text| filled(&current, text));
                    let from = new.changes.iter().find(|c| c.key == change.key).map_or(Some(current), |c| c.from.clone());
                    new.changes.retain(|c| c.key != change.key);
                    new.changes.push(toona_core::Change { key: change.key.clone(), layer: change.layer, from, to: value.clone() });
                    item.profile.settings.insert(change.key.clone(), value);
                }
                new.changes.sort_by(|a, b| a.key.cmp(&b.key));
            }
            planned.extend(one);
            profiles.extend(described);
        }
        let options = InstallOptions { confirm_cloud_sync: allow_cloud_sync, dry_run: false };
        let report = install::install(&studio.slicer, &studio.conf, &self.dirs, &planned, options)?;
        Ok(InstallDto {
            alias: source.alias.clone(),
            dry_run: report.dry_run,
            profiles,
            files: report.files.iter().map(|f| f.display().to_string()).collect(),
            backup: report.backup.map(|b| b.display().to_string()),
            will_cloud_sync: report.will_cloud_sync,
        })
    }

    /// Turns imported profiles into custom filaments the AMS can use. The originals are kept.
    pub fn fix_imported(&self, names: &[String], allow_cloud_sync: bool, dry_run: bool) -> UiResult<InstallDto> {
        let studio = self.studio()?;
        let existing = studio.slicer.user_filaments(&studio.conf)?;
        let printers = studio.printers();
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
                    printer: profile.compatible_printers.first().cloned().unwrap_or_default(),
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

// ---------- accounts and shared profiles ----------

#[derive(Serialize)]
pub struct AccountDto {
    /// The Toona server answers. When it does not, the window leaves the community parts out.
    pub available: bool,
    pub user: Option<User>,
}

/// A profile version someone shared, as the window shows it.
#[derive(Serialize)]
pub struct SharedDto {
    pub id: u64,
    pub profile_id: u64,
    pub parent_version: Option<u64>,
    pub author: String,
    pub note: String,
    pub created: String,
    pub worked: u64,
    pub problems: u64,
    pub badges: Vec<String>,
    /// The signed-in user's own answer: `worked` or `problems`.
    pub mine: Option<String>,
    /// The signed-in user shared this one.
    pub own: bool,
    pub built_on: String,
    pub changes: Vec<ChangeDto>,
    /// Why it cannot be added here, if it cannot.
    pub unusable: Option<String>,
    /// A profile in the slicer was made from this version.
    pub added: bool,
}

impl Backend {
    pub fn account(&self) -> AccountDto {
        AccountDto { available: self.community.available(), user: self.community.user() }
    }

    pub fn sign_in_start(&self) -> UiResult<SignIn> {
        self.community.sign_in_start()
    }

    pub fn sign_in_poll(&self, device_code: &str) -> UiResult<Option<User>> {
        self.community.sign_in_poll(device_code)
    }

    pub fn sign_out(&self) {
        self.community.sign_out()
    }

    fn shared_dto(&self, studio: Option<&Studio>, version: Version) -> SharedDto {
        let manifest = self.manifest();
        let added = self.community.links().iter().any(|(name, link)| {
            link.version_id == version.id && manifest.profiles.iter().any(|p| &p.profile_name == name)
        });
        let unusable = studio
            .filter(|studio| !studio.library.has_filament(&version.parent_preset))
            .map(|_| format!("Built on a preset your {} doesn't have.", self.kind().display_name()));
        SharedDto {
            id: version.id,
            profile_id: version.profile_id,
            parent_version: version.parent_version,
            own: self.community.user().is_some_and(|user| user.id == version.author.id),
            author: version.author.handle,
            note: version.note,
            created: version.created,
            worked: version.worked,
            problems: version.problems,
            badges: version.badges,
            mine: version.mine,
            built_on: preset_alias(&version.parent_preset).to_string(),
            changes: version
                .changes
                .iter()
                .filter(|change| shareable(&change.key))
                .map(|change| ChangeDto {
                    key: change.key.clone(),
                    layer: layer_named(&change.layer),
                    from: change.from.as_ref().filter(|value| !value.is_null()).map(show_value),
                    to: show_value(&change.to),
                })
                .collect(),
            unusable,
            added,
        }
    }

    /// Profiles people shared for a filament on one printer model and nozzle, best first.
    pub fn community(&self, key: &str, model: &str, nozzle: &str) -> UiResult<Vec<SharedDto>> {
        let versions = self.community.versions(key, model, nozzle, self.kind().id())?;
        let studio = self.studio().ok();
        Ok(versions.into_iter().map(|version| self.shared_dto(studio.as_ref(), version)).collect())
    }

    /// Adds a filament using a shared version's settings. The profile is built from the
    /// catalogue as usual, on the preset the version names, and the shared changes are
    /// laid over it; see [`apply_shared`] for what is and is not taken.
    pub fn install_community(&self, request: &InstallRequest, version_id: u64) -> UiResult<InstallDto> {
        let version = self.community.version(version_id)?;
        if version.product_key != request.key {
            return Err(other("That shared profile is for a different filament."));
        }
        let studio = self.studio()?;
        let catalogue = self.load_catalogue(Refresh::IfMissing)?;
        let base_settings = studio
            .library
            .resolve_filament(&version.parent_preset)
            .map_err(|_| other(format!("That profile is built on a preset your {} doesn't have.", self.kind().display_name())))?
            .settings;
        // A shared version is for one printer model and nozzle; only that preset gets it.
        let printers: Vec<String> = studio
            .slicer
            .printers(&studio.conf, &studio.library)
            .into_iter()
            .filter(|p| p.model == version.printer && p.nozzle == version.nozzle)
            .map(|p| p.name)
            .collect();
        if printers.is_empty() {
            return Err(other(format!("That profile is for the {} with a {} mm nozzle.", version.printer, version.nozzle)));
        }
        let wanted = InstallRequest {
            key: request.key.clone(),
            variant: request.variant.clone(),
            printers,
            base: Some(version.parent_preset.clone()),
            alias: request.alias.clone(),
            // A shared profile brings its own values; the manufacturer's are not laid over them.
            borrow_tuned: false,
            allow_cloud_sync: request.allow_cloud_sync,
            dry_run: request.dry_run,
        };
        let (alias, mut planned, profiles) = self.plan(&studio, &catalogue, &wanted)?;
        for plan in &mut planned {
            let recipe = plan.recipe.get_or_insert_with(|| Recipe { parent: version.parent_preset.clone(), changes: Vec::new() });
            apply_shared(&mut plan.profile, recipe, &base_settings, &version.changes);
        }
        let options = InstallOptions { confirm_cloud_sync: wanted.allow_cloud_sync, dry_run: wanted.dry_run };
        let report = install::install(&studio.slicer, &studio.conf, &self.dirs, &planned, options)?;
        if !report.dry_run {
            for plan in &planned {
                self.community.link(&plan.profile.name, Link { version_id: version.id, profile_id: version.profile_id });
            }
        }
        Ok(InstallDto {
            alias,
            dry_run: report.dry_run,
            profiles,
            files: report.files.iter().map(|f| f.display().to_string()).collect(),
            backup: report.backup.map(|b| b.display().to_string()),
            will_cloud_sync: report.will_cloud_sync,
        })
    }

    /// Shares the settings of a profile Toona added. Colour and G-code are left out. If the
    /// profile was made from someone's shared version, this is shared as an edit of it.
    pub fn share(&self, profile_name: &str, note: &str) -> UiResult<SharedDto> {
        let studio = self.studio()?;
        let catalogue = self.load_catalogue(Refresh::IfMissing)?;
        let Source { catalogue_key, printer: printer_preset, recipe, .. } = self.source(&studio, &catalogue, profile_name)?;
        let filament = find_by_key(&catalogue, &catalogue_key)?;
        let recipe =
            recipe.ok_or_else(|| other("Toona has no record of what this profile changes, so it can't be shared."))?;
        let changes: Vec<_> = recipe
            .changes
            .iter()
            .filter(|change| change.layer != Layer::Colour && shareable(&change.key))
            .map(|change| serde_json::json!({ "key": change.key, "layer": change.layer, "from": change.from, "to": change.to }))
            .collect();
        if changes.is_empty() {
            return Err(other("This profile is the same as its base preset, so there is nothing to share."));
        }
        let printer = studio
            .slicer
            .printers(&studio.conf, &studio.library)
            .into_iter()
            .find(|p| p.name == printer_preset)
            .ok_or_else(|| other(format!("The printer \"{printer_preset}\" is no longer set up in {}.", self.kind().display_name())))?;
        let body = serde_json::json!({
            "product": {
                "key": filament.key,
                "brand": filament.brand,
                "name": filament.name,
                "material": filament.material,
                "class": if filament.class == MaterialClass::Filament { "filament" } else { "resin" },
            },
            "printer": printer.model,
            "nozzle": printer.nozzle,
            "slicer": self.kind().id(),
            "parent_preset": recipe.parent,
            "changes": changes,
            "note": note.trim(),
        });
        let version = self.community.publish(self.community.links().get(profile_name).copied(), body)?;
        self.community.link(profile_name, Link { version_id: version.id, profile_id: version.profile_id });
        Ok(self.shared_dto(Some(&studio), version))
    }

    /// Records that the user printed with a shared version and how it went.
    pub fn confirm(&self, version_id: u64, outcome: &str, model: &str, nozzle: &str) -> UiResult<SharedDto> {
        let version = self.community.confirm(version_id, outcome, model, nozzle, "")?;
        Ok(self.shared_dto(self.studio().ok().as_ref(), version))
    }
}

// ---- one profile in detail: its layers, editing it, and its family of shared versions ----

/// A profile in the slicer that Toona knows the catalogue product of.
struct Source {
    catalogue_key: String,
    /// The printer preset it is for.
    printer: String,
    json: PathBuf,
    recipe: Option<Recipe>,
    /// Toona wrote it, so Toona may change it.
    added: bool,
}

/// Settings shown on a profile's page and offered for editing: key, group, the range a
/// value must be in, the size of one step, and the unit shown after the value.
const SETTINGS: &[(&str, &str, f64, f64, f64, &str)] = &[
    ("nozzle_temperature", "Temperature", 150.0, 350.0, 5.0, "°"),
    ("nozzle_temperature_initial_layer", "Temperature", 150.0, 350.0, 5.0, "°"),
    ("nozzle_temperature_range_low", "Temperature", 150.0, 350.0, 5.0, "°"),
    ("nozzle_temperature_range_high", "Temperature", 150.0, 350.0, 5.0, "°"),
    ("hot_plate_temp", "Temperature", 0.0, 130.0, 5.0, "°"),
    ("hot_plate_temp_initial_layer", "Temperature", 0.0, 130.0, 5.0, "°"),
    ("textured_plate_temp", "Temperature", 0.0, 130.0, 5.0, "°"),
    ("textured_plate_temp_initial_layer", "Temperature", 0.0, 130.0, 5.0, "°"),
    ("cool_plate_temp", "Temperature", 0.0, 130.0, 5.0, "°"),
    ("cool_plate_temp_initial_layer", "Temperature", 0.0, 130.0, 5.0, "°"),
    ("eng_plate_temp", "Temperature", 0.0, 130.0, 5.0, "°"),
    ("eng_plate_temp_initial_layer", "Temperature", 0.0, 130.0, 5.0, "°"),
    ("filament_flow_ratio", "Flow", 0.5, 1.5, 0.01, ""),
    ("filament_max_volumetric_speed", "Flow", 0.5, 60.0, 1.0, " mm³/s"),
    ("pressure_advance", "Flow", 0.0, 1.0, 0.002, ""),
    ("filament_retraction_length", "Flow", 0.0, 10.0, 0.1, " mm"),
    ("fan_max_speed", "Cooling", 0.0, 100.0, 10.0, "%"),
    ("fan_min_speed", "Cooling", 0.0, 100.0, 10.0, "%"),
    ("filament_density", "Material", 0.5, 20.0, 0.01, " g/cm³"),
];

/// [`SETTINGS`] for PrusaSlicer, which names its settings differently and has one bed.
const PRUSA_SETTINGS: &[(&str, &str, f64, f64, f64, &str)] = &[
    ("temperature", "Temperature", 150.0, 350.0, 5.0, "°"),
    ("first_layer_temperature", "Temperature", 150.0, 350.0, 5.0, "°"),
    ("bed_temperature", "Temperature", 0.0, 130.0, 5.0, "°"),
    ("first_layer_bed_temperature", "Temperature", 0.0, 130.0, 5.0, "°"),
    ("extrusion_multiplier", "Flow", 0.5, 1.5, 0.01, ""),
    ("filament_max_volumetric_speed", "Flow", 0.0, 60.0, 1.0, " mm³/s"),
    ("max_fan_speed", "Cooling", 0.0, 100.0, 10.0, "%"),
    ("min_fan_speed", "Cooling", 0.0, 100.0, 10.0, "%"),
    ("bridge_fan_speed", "Cooling", 0.0, 100.0, 10.0, "%"),
    ("filament_density", "Material", 0.0, 20.0, 0.01, " g/cm³"),
];

fn settings_of(family: Family) -> &'static [(&'static str, &'static str, f64, f64, f64, &'static str)] {
    match family {
        Family::Bambu => SETTINGS,
        Family::Prusa => PRUSA_SETTINGS,
    }
}

/// What each family calls the settings the window shows as plain facts.
struct Keys {
    nozzle: &'static str,
    /// The beds to read, the usual one first.
    beds: &'static [&'static str],
    colour: &'static str,
}

fn keys_of(family: Family) -> Keys {
    match family {
        // The smooth plate most printers ship with, or else the textured one.
        Family::Bambu => Keys { nozzle: "nozzle_temperature", beds: &["hot_plate_temp", "textured_plate_temp"], colour: "default_filament_colour" },
        Family::Prusa => Keys { nozzle: "temperature", beds: &["bed_temperature"], colour: "filament_colour" },
    }
}

/// The single value of a setting, when it has one: `["220", "220"]` and `"220"` are `220`.
fn single(value: &serde_json::Value) -> Option<&str> {
    match value {
        serde_json::Value::String(text) => Some(text),
        serde_json::Value::Array(items) => {
            let first = items.first()?.as_str()?;
            items.iter().all(|item| item.as_str() == Some(first)).then_some(first)
        }
        _ => None,
    }
}

/// `text` in every slot of `like`.
fn filled(like: &serde_json::Value, text: &str) -> serde_json::Value {
    match like {
        serde_json::Value::Array(items) => serde_json::json!(vec![text; items.len()]),
        _ => serde_json::json!(text),
    }
}

#[derive(Serialize)]
pub struct SettingDto {
    pub key: String,
    pub group: &'static str,
    /// The base preset's value.
    pub base: Option<String>,
    pub value: String,
    /// Who changed it over the base preset; `None` when it is the preset's own value.
    pub layer: Option<Layer>,
    pub editable: bool,
    pub min: f64,
    pub max: f64,
    /// How much one press of the stepper changes it.
    pub step: f64,
    /// Shown after the value, such as `°`.
    pub unit: &'static str,
}

#[derive(Serialize)]
pub struct ProfileDto {
    pub profile_name: String,
    pub alias: String,
    pub printer: String,
    pub hex: Option<String>,
    pub built_on: String,
    /// The catalogue product's brand and name.
    pub brand: Option<String>,
    pub product: Option<String>,
    /// Toona added it, so its settings can be edited here.
    pub editable: bool,
    /// The shared version it was made from or shared as, if any.
    pub version_id: Option<u64>,
    /// The most the manufacturer says it can be pushed through a nozzle, in mm³/s.
    pub max_flow: Option<f64>,
    pub settings: Vec<SettingDto>,
}

#[derive(Deserialize)]
pub struct EditDto {
    pub key: String,
    pub value: String,
}

impl Backend {
    fn own_products_path(&self) -> PathBuf {
        self.dirs.data_dir.join("own.json")
    }

    /// Which catalogue product the user said each of their own filaments is.
    fn own_products(&self) -> std::collections::BTreeMap<String, String> {
        std::fs::read_to_string(self.own_products_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Says which catalogue product one of the user's own filaments is, or with `None` forgets it.
    /// This is Toona's own note; the filament itself is not touched.
    pub fn link_product(&self, profile_name: &str, key: Option<&str>) -> UiResult<()> {
        let mut products = self.own_products();
        match key {
            Some(key) => {
                find_by_key(&*self.load_catalogue(Refresh::IfMissing)?, key)?;
                products.insert(profile_name.to_string(), key.to_string());
            }
            None => {
                products.remove(profile_name);
            }
        }
        std::fs::create_dir_all(&self.dirs.data_dir)
            .and_then(|_| std::fs::write(self.own_products_path(), serde_json::to_string(&products).unwrap_or_default()))
            .map_err(|error| other(format!("Could not save that: {error}")))
    }

    fn source(&self, studio: &Studio, catalogue: &Catalogue, profile_name: &str) -> UiResult<Source> {
        let manifest = self.manifest();
        if let Some(profile) =
            manifest.profiles.iter().find(|p| p.profile_name == profile_name && p.slicer == self.kind().id())
        {
            let json = profile
                .profile_file()
                .filter(|file| file.exists())
                .cloned()
                .ok_or_else(|| other(format!("The files for \"{profile_name}\" are missing.")))?;
            let recipe = profile
                .recipe
                .clone()
                .or_else(|| recover_recipe(studio, catalogue, &profile.catalogue_key, &profile.printer, &json));
            return Ok(Source { catalogue_key: profile.catalogue_key.clone(), printer: profile.printer.clone(), json, recipe, added: true });
        }
        let not_known = || UiError::from(Error::NotInstalled(profile_name.to_string()));
        let own = studio
            .slicer
            .user_filaments(&studio.conf)?
            .into_iter()
            .find(|f| f.is_root && f.name == profile_name)
            .ok_or_else(not_known)?;
        let catalogue_key = self
            .own_products()
            .remove(profile_name)
            .ok_or_else(|| other("Say which product this filament is first, so Toona knows what to compare it with."))?;
        let printer = studio.printers_of(&own.path, None, &studio.printers()).into_iter().next().ok_or_else(not_known)?;
        let recipe = recover_recipe(studio, catalogue, &catalogue_key, &printer, &own.path);
        Ok(Source { catalogue_key, printer, json: own.path, recipe, added: false })
    }

    /// One profile's settings with where each value comes from.
    pub fn profile(&self, profile_name: &str) -> UiResult<ProfileDto> {
        let studio = self.studio()?;
        let catalogue = self.load_catalogue(Refresh::IfMissing)?;
        let source = self.source(&studio, &catalogue, profile_name)?;
        let filament = catalogue.filaments().iter().find(|f| f.key == source.catalogue_key);
        let recipe = source.recipe.ok_or_else(|| other("Toona can't tell what this profile is built on."))?;
        let written = FilamentProfile::read(&source.json).ok_or_else(|| other("That profile's file could not be read."))?;
        let base = studio.library.resolve_filament(&recipe.parent).map(|resolved| resolved.settings).unwrap_or_default();

        let mut settings = Vec::new();
        let table = settings_of(studio.library.family);
        let known = |key: &str| table.iter().find(|(name, ..)| *name == key);
        let mut keys: Vec<&str> = table.iter().map(|(key, ..)| *key).filter(|key| written.settings.contains_key(*key)).collect();
        // Anything else the profile changes is shown too, but not offered for editing.
        keys.extend(recipe.changes.iter().map(|c| c.key.as_str()).filter(|key| known(key).is_none() && !key.contains("gcode")));
        for key in keys {
            let change = recipe.changes.iter().find(|c| c.key == key);
            let Some(value) = written.settings.get(key).or(change.map(|c| &c.to)) else { continue };
            let (group, min, max, step, unit) =
                known(key).map_or(("Other", 0.0, 0.0, 0.0, ""), |(_, group, min, max, step, unit)| (*group, *min, *max, *step, *unit));
            settings.push(SettingDto {
                key: key.to_string(),
                group,
                base: base.get(key).or(change.and_then(|c| c.from.as_ref())).map(show_value),
                value: show_value(value),
                layer: change.map(|c| c.layer),
                editable: source.added && known(key).is_some() && single(value).is_some_and(|text| text.parse::<f64>().is_ok()),
                min,
                max,
                step,
                unit,
            });
        }
        Ok(ProfileDto {
            profile_name: profile_name.to_string(),
            alias: preset_alias(profile_name).to_string(),
            printer: source.printer,
            hex: Facts::read(&source.json).hex,
            built_on: preset_alias(&recipe.parent).to_string(),
            brand: filament.map(|f| f.brand.clone()),
            product: filament.map(|f| f.name.clone()),
            editable: source.added,
            version_id: self.community.links().get(profile_name).map(|link| link.version_id),
            max_flow: filament.and_then(|f| f.max_volumetric_speed).filter(|v| *v > 0.0),
            settings,
        })
    }

    /// Changes settings of a profile Toona added. Only the settings in [`SETTINGS`], only
    /// to numbers in their range, and never on a profile the user made themselves.
    pub fn edit(&self, profile_name: &str, edits: &[EditDto], allow_cloud_sync: bool, dry_run: bool) -> UiResult<InstallDto> {
        let studio = self.studio()?;
        let catalogue = self.load_catalogue(Refresh::IfMissing)?;
        let source = self.source(&studio, &catalogue, profile_name)?;
        if !source.added {
            return Err(other(format!("Toona only edits filaments it added. Edit this one in {}.", self.kind().display_name())));
        }
        let old = source.recipe.ok_or_else(|| other("Toona can't tell what this profile is built on."))?;
        let mut profile = FilamentProfile::read(&source.json).ok_or_else(|| other("That profile's file could not be read."))?;

        for edit in edits {
            let (_, _, min, max, ..) = settings_of(profile.family)
                .iter()
                .find(|(key, ..)| *key == edit.key)
                .ok_or_else(|| other(format!("\"{}\" can't be edited here.", edit.key)))?;
            let text = edit.value.trim();
            let number: f64 = text.parse().map_err(|_| other(format!("\"{text}\" is not a number.")))?;
            if !number.is_finite() || number < *min || number > *max {
                return Err(other(format!("{} must be between {min} and {max}.", edit.key)));
            }
            let current = profile
                .settings
                .get(&edit.key)
                .filter(|value| single(value).is_some())
                .ok_or_else(|| other(format!("\"{}\" has different values per nozzle; edit it in {}.", edit.key, self.kind().display_name())))?;
            let value = filled(current, text);
            profile.settings.insert(edit.key.clone(), value);
        }

        let base = studio.library.base_settings(&old.parent)?;
        let edited: Vec<&str> = edits.iter().map(|edit| edit.key.as_str()).collect();
        let mut recipe = Recipe::between(&old.parent, &base, &profile.settings, |key| {
            if edited.contains(&key) {
                Layer::You
            } else {
                old.changes.iter().find(|c| c.key == key).map_or_else(|| catalogue_layer(key), |c| c.layer)
            }
        });
        // Changes to fields the profile stores outside its settings (the brand) carry over.
        recipe.changes.extend(old.changes.iter().filter(|c| !profile.settings.contains_key(&c.key) && !base.contains_key(&c.key)).cloned());
        recipe.changes.sort_by(|a, b| a.key.cmp(&b.key));

        let options = InstallOptions { confirm_cloud_sync: allow_cloud_sync, dry_run };
        let report = install::update(&studio.slicer, &studio.conf, &self.dirs, &profile, Some(recipe), options)?;
        Ok(InstallDto {
            alias: profile.alias().to_string(),
            dry_run: report.dry_run,
            profiles: Vec::new(),
            files: report.files.iter().map(|f| f.display().to_string()).collect(),
            backup: report.backup.map(|b| b.display().to_string()),
            will_cloud_sync: report.will_cloud_sync,
        })
    }

    /// Every shared version in the same profile as `version_id`, oldest first.
    pub fn family(&self, version_id: u64) -> UiResult<Vec<SharedDto>> {
        let versions = self.community.family(version_id)?;
        let studio = self.studio().ok();
        Ok(versions.into_iter().map(|version| self.shared_dto(studio.as_ref(), version)).collect())
    }
}

const FLOW_KEY: &str = "filament_max_volumetric_speed";

/// The highest number in a setting: a per-nozzle list such as `["21", "25"]` is `25`.
fn highest(value: &serde_json::Value) -> Option<f64> {
    let numbers = |text: &str| text.parse::<f64>().ok();
    match value {
        serde_json::Value::String(text) => numbers(text),
        serde_json::Value::Array(items) => {
            items.iter().filter_map(|item| numbers(item.as_str()?)).fold(None, |best, n| Some(best.map_or(n, |b: f64| b.max(n))))
        }
        _ => None,
    }
}

/// The first number of a setting: the standard nozzle's.
fn first(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::String(text) => text.parse().ok(),
        serde_json::Value::Array(items) => items.first()?.as_str()?.parse().ok(),
        _ => None,
    }
}

/// An error message as a sentence of its own: capitalised, with a full stop.
fn sentence(text: &str) -> String {
    let mut chars = text.trim().chars();
    let Some(first) = chars.next() else { return String::new() };
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// The manufacturer's figures for a filament next to what the built profile would use.
fn spec_rows(
    filament: &CatalogueFilament,
    built: Option<(&FilamentProfile, Option<f64>)>,
    tuned: Option<&toona_core::tuned::TunedEntry>,
) -> Vec<SpecDto> {
    // What the manufacturer published for this very printer says more than the catalogue's general figure.
    let published = |key: &str| tuned.and_then(|entry| entry.settings.get(key)?.parse::<f64>().ok());
    let setting = |key: &str| built.and_then(|(profile, _)| highest(profile.settings.get(key)?));
    let within = |value: Option<f64>, low: Option<f64>, high: Option<f64>| Some(value? >= low? && value? <= high?);
    let span = |low: Option<f64>, high: Option<f64>| Some(format!("{}–{} °C", low?, high?));
    let degrees = |value: Option<f64>| value.map(|v| format!("{v} °C"));

    let keys = keys_of(built.map_or(Family::Bambu, |(profile, _)| profile.family));
    let nozzle = setting(keys.nozzle);
    // A plate the preset does not allow reads 0; the first one that is allowed counts.
    let bed = keys.beds.iter().find_map(|key| setting(key).filter(|t| *t > 0.0)).or(keys.beds.last().and_then(|key| setting(key)));
    // The first value is the standard nozzle's; a printer with a second kind of nozzle lists more.
    let flow = built.and_then(|(profile, _)| first(profile.settings.get(FLOW_KEY)?));
    let density = setting("filament_density");
    // The flow the manufacturer tuned for this printer is its figure when the catalogue has none.
    let limit = published(FLOW_KEY).or(filament.max_volumetric_speed.filter(|v| *v > 0.0));
    let listed_density = published("filament_density").or(filament.density.filter(|d| *d > 0.0));
    let low = published("nozzle_temperature_range_low").or(filament.min_print_temperature);
    let high = published("nozzle_temperature_range_high").or(filament.max_print_temperature);
    vec![
        SpecDto {
            key: "nozzle",
            spec: span(low, high),
            value: degrees(nozzle),
            ok: within(nozzle, low, high),
            limit: None,
            at: None,
            base: None,
        },
        SpecDto {
            key: "bed",
            spec: span(filament.min_bed_temperature, filament.max_bed_temperature),
            value: degrees(bed),
            ok: within(bed, filament.min_bed_temperature, filament.max_bed_temperature),
            limit: None,
            at: None,
            base: None,
        },
        SpecDto {
            key: "flow",
            spec: limit.map(|v| format!("{v} mm³/s")),
            value: flow.map(|v| format!("{v} mm³/s")),
            ok: flow.zip(limit).map(|(flow, limit)| flow <= limit),
            limit,
            at: flow,
            base: built.and_then(|(_, base)| base),
        },
        SpecDto {
            key: "density",
            spec: listed_density.map(|v| format!("{v} g/cm³")),
            value: density.map(|v| format!("{v} g/cm³")),
            ok: density.zip(listed_density).map(|(a, b)| (a - b).abs() < 0.005),
            limit: None,
            at: None,
            base: None,
        },
    ]
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

/// What a profile file says about its filament, for listing it.
#[derive(Default)]
struct Facts {
    hex: Option<String>,
    /// Left out when it is the slicer's placeholder for "nobody in particular".
    vendor: Option<String>,
    material: Option<String>,
    nozzle_temp: Option<String>,
    bed_temp: Option<String>,
    max_flow: Option<String>,
}

impl Facts {
    fn read(path: &std::path::Path) -> Self {
        let Some(profile) = FilamentProfile::read(path) else { return Self::default() };
        let keys = keys_of(profile.family);
        // The first value: on a printer with two nozzles, the main one's.
        let text = |key: &str| {
            let value = profile.settings.get(key)?;
            let text = value.as_str().or_else(|| value.get(0)?.as_str())?.trim().trim_matches('"');
            (!text.is_empty()).then(|| text.to_string())
        };
        let field = |values: &[String]| values.first().map(|value| value.trim().trim_matches('"').to_string()).filter(|value| !value.is_empty());
        Self {
            hex: clean_hex(text(keys.colour).as_deref()),
            vendor: field(&profile.filament_vendor).filter(|vendor| !vendor.eq_ignore_ascii_case("generic")),
            material: field(&profile.filament_type),
            nozzle_temp: text(keys.nozzle),
            bed_temp: keys.beds.iter().find_map(|key| text(key)),
            // PrusaSlicer writes "no limit" as 0.
            max_flow: text(FLOW_KEY).filter(|flow| flow.parse::<f64>().map_or(true, |flow| flow > 0.0)),
        }
    }
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
    catalogue_key: &str,
    printer: &str,
    json: &std::path::Path,
) -> Option<Recipe> {
    let filament = catalogue.filaments().iter().find(|f| f.key == catalogue_key)?;
    let written = FilamentProfile::read(json)?;
    // A PrusaSlicer preset says which system preset it was made from; a Bambu Studio one does not.
    let parent = if written.family == Family::Prusa && studio.library.has_filament(&written.inherits) {
        written.inherits.clone()
    } else {
        choose_base(&studio.library, filament, printer, None).ok()?.preset
    };
    let base_settings = studio.library.base_settings(&parent).ok()?;
    Some(Recipe::between(&parent, &base_settings, &written.settings, catalogue_layer))
}
