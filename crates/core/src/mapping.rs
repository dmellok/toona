//! Turning a catalogue filament into a profile: pick the closest Bambu system
//! preset for the printer, flatten it, and override what the database provides.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::recipe::{catalogue_layer, Change, Layer, Recipe};
use crate::catalogue::{CatalogueFilament, MaterialClass, Variant};
use crate::error::{Error, Result};
use crate::profile::{hashed_filament_id, sanitize_alias, FilamentProfile};
use crate::slicer::{Family, UserFilament};
use crate::system::{preset_alias, SystemLibrary};

/// Diameter Bambu printers use, in mm.
const SUPPORTED_DIAMETER: f64 = 1.75;
/// Database temperatures outside these ranges are treated as data errors and ignored.
const PLAUSIBLE_NOZZLE: std::ops::RangeInclusive<f64> = 150.0..=500.0;
const PLAUSIBLE_BED: std::ops::RangeInclusive<f64> = 0.0..=150.0;
/// Plates most third-party filament is printed on. The other plate types keep the base values.
const BED_KEYS: &[&str] = &[
    "hot_plate_temp",
    "hot_plate_temp_initial_layer",
    "textured_plate_temp",
    "textured_plate_temp_initial_layer",
];

pub struct ProfileRequest<'a> {
    pub filament: &'a CatalogueFilament,
    /// A colour of the filament. Adds its name to the profile name and sets the colour.
    pub variant: Option<&'a Variant>,
    /// System machine preset name, e.g. `Bambu Lab X2D 0.4 nozzle`.
    pub printer: &'a str,
    /// Base system preset (full name or the part before `@`) instead of the automatic choice.
    pub base: Option<&'a str>,
    /// Signed-in account id; part of the filament id hash.
    pub user_id: Option<&'a str>,
    /// User filament profiles already in the slicer, for id reuse and collision checks.
    pub existing: &'a [UserFilament],
    /// A filament name to use instead of `<brand> <name>[ <colour>]`.
    pub alias: Option<&'a str>,
    /// Where the manufacturer published settings for another nozzle of this printer
    /// but not this one, use those (all but the ones that depend on nozzle size).
    pub borrow_tuned: bool,
}

/// Display name of the profile's filament: `<brand> <name>[ <colour>]`.
pub fn profile_alias(filament: &CatalogueFilament, variant: Option<&Variant>) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if !filament.name.to_lowercase().starts_with(&filament.brand.to_lowercase()) {
        parts.push(&filament.brand);
    }
    parts.push(&filament.name);
    if let Some(variant) = variant {
        parts.push(&variant.name);
    }
    sanitize_alias(&parts.join(" "))
}

/// PrusaSlicer's presets to try for a filament, closest first. It ships generic presets
/// for only a few materials; for the others, Prusa's own filament of that material is the
/// nearest starting point there is.
fn prusa_base_candidates(filament: &CatalogueFilament) -> Vec<&'static str> {
    let name = filament.name.to_lowercase();
    let has_trait = |t: &str| filament.traits.contains(t);
    let named = |words: &[&str]| words.iter().any(|w| name.contains(w));
    let carbon = has_trait("contains_carbon_fiber") || named(&["carbon", "-cf", " cf"]);
    let silk = has_trait("silk") || named(&["silk"]);

    match filament.material.to_uppercase().as_str() {
        "PLA" if silk => vec!["Generic PLA Silk", "Generic PLA"],
        "PLA" | "PHA" => vec!["Generic PLA"],
        "PETG" | "PET" | "CPE" | "PCTG" => vec!["Generic PETG"],
        "ABS" => vec!["Generic ABS"],
        "ASA" => vec!["Generic ASA", "Prusament ASA", "Generic ABS"],
        "TPU" | "TPE" | "TPC" | "PEBA" => vec!["Generic FLEX"],
        "PC" if carbon => vec!["Prusament PC Blend Carbon Fiber", "Prusament PC Blend"],
        "PC" => vec!["Generic PC", "Prusament PC Blend"],
        "PA" | "PA6" | "PA66" | "PA11" | "PA12" if carbon => vec!["Prusament PA11 Carbon Fiber"],
        "PVA" | "BVOH" => vec!["Generic PVA"],
        "PVB" => vec!["Prusament PVB"],
        "HIPS" => vec!["Generic HIPS"],
        _ => Vec::new(),
    }
}

/// System preset aliases to try for a filament, closest first.
pub fn base_candidates(filament: &CatalogueFilament) -> Vec<&'static str> {
    let name = filament.name.to_lowercase();
    let has_trait = |t: &str| filament.traits.contains(t);
    let named = |words: &[&str]| words.iter().any(|w| name.contains(w));

    let carbon = has_trait("contains_carbon_fiber") || named(&["carbon", "-cf", " cf"]);
    let glass = has_trait("contains_glass_fiber") || named(&["glass", "-gf", " gf"]);
    let silk = has_trait("silk") || named(&["silk"]);
    let fast = has_trait("high_flow") || named(&["high speed", "high-speed", "hyper", "rapid"]);

    match filament.material.to_uppercase().as_str() {
        "PLA" if carbon => vec!["Generic PLA-CF", "Generic PLA"],
        "PLA" if silk => vec!["Generic PLA Silk", "Generic PLA"],
        "PLA" if fast => vec!["Generic PLA High Speed", "Generic PLA"],
        "PLA" => vec!["Generic PLA"],
        "PETG" | "PET" | "CPE" if carbon => vec!["Generic PETG-CF", "Generic PETG"],
        "PETG" | "PET" | "CPE" if fast => vec!["Generic PETG HF", "Generic PETG"],
        "PETG" | "PET" | "CPE" => vec!["Generic PETG"],
        "PCTG" => vec!["Generic PCTG", "Generic PETG"],
        "ABS" => vec!["Generic ABS"],
        "ASA" => vec!["Generic ASA"],
        "TPU" | "TPE" | "TPC" | "PEBA" => vec!["Generic TPU"],
        "PA" | "PA6" | "PA66" | "PA11" | "PA12" if carbon || glass => vec!["Generic PA-CF", "Generic PA"],
        "PA" | "PA6" | "PA66" | "PA11" | "PA12" => vec!["Generic PA"],
        "PC" => vec!["Generic PC"],
        "PP" if carbon => vec!["Generic PP-CF", "Generic PP"],
        "PP" if glass => vec!["Generic PP-GF", "Generic PP"],
        "PP" => vec!["Generic PP"],
        "PPA" if glass => vec!["Generic PPA-GF", "Generic PPA-CF"],
        "PPA" => vec!["Generic PPA-CF"],
        "PVA" => vec!["Generic PVA"],
        "BVOH" => vec!["Generic BVOH", "Generic PVA"],
        "HIPS" => vec!["Generic HIPS"],
        "PHA" => vec!["Generic PHA", "Generic PLA"],
        "EVA" => vec!["Generic EVA"],
        "PE" => vec!["Generic PE"],
        _ => Vec::new(),
    }
}

/// Where a profile's settings come from, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum BaseSource {
    /// Chosen by the user.
    Explicit,
    /// A preset the slicer ships for this exact product, tuned by the vendor.
    VendorTuned,
    /// The slicer's generic preset for the material: a starting point only.
    Generic,
}

impl BaseSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Explicit => "chosen base",
            Self::VendorTuned => "vendor-tuned preset",
            Self::Generic => "generic starting point",
        }
    }
}

/// The system preset a profile is built on, and what kind of source it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseChoice {
    pub preset: String,
    pub source: BaseSource,
}

/// Slicer keys under which the catalogue links products to a family's presets.
fn linked_slicers(family: Family) -> &'static [&'static str] {
    match family {
        Family::Bambu => &["bambustudio", "orcaslicer"],
        Family::Prusa => &["prusaslicer"],
    }
}

/// Names a slicer may ship a vendor-tuned preset for this product under.
fn vendor_preset_names(filament: &CatalogueFilament, family: Family) -> Vec<String> {
    let mut names: Vec<String> = linked_slicers(family)
        .iter()
        .filter_map(|slicer| filament.slicer_profiles.get(*slicer).cloned())
        .collect();
    names.push(format!("{} {}", filament.brand, filament.name));
    names.push(filament.name.clone());
    names
}

/// The system preset to build on for `filament` on `printer`: a preset for the
/// exact product if the slicer ships one for that printer, otherwise the generic
/// preset for the material.
pub fn choose_base(
    library: &SystemLibrary,
    filament: &CatalogueFilament,
    printer: &str,
    explicit: Option<&str>,
) -> Result<BaseChoice> {
    let for_printer = library.filaments_for_printer(printer);
    if let Some(wanted) = explicit {
        // A base is chosen while looking at one nozzle, so it may arrive as that nozzle's
        // preset name. The same filament under another nozzle is the same choice.
        let alias = preset_alias(wanted);
        return for_printer
            .iter()
            .find(|name| **name == wanted)
            .or_else(|| for_printer.iter().find(|name| preset_alias(name) == alias))
            .map(|name| BaseChoice { preset: name.to_string(), source: BaseSource::Explicit })
            .ok_or_else(|| Error::BaseNotForPrinter { base: alias.to_string(), printer: printer.to_string() });
    }

    let vendor_tuned = vendor_preset_names(filament, library.family).into_iter().find_map(|wanted| {
        for_printer
            .iter()
            .find(|name| {
                let alias = preset_alias(name);
                alias.eq_ignore_ascii_case(&wanted) && !alias.starts_with("Generic ")
            })
            .map(|name| name.to_string())
    });
    if let Some(preset) = vendor_tuned {
        return Ok(BaseChoice { preset, source: BaseSource::VendorTuned });
    }

    let candidates = match library.family {
        Family::Bambu => base_candidates(filament),
        Family::Prusa => prusa_base_candidates(filament),
    };
    candidates
        .into_iter()
        .find_map(|alias| library.filament_by_alias(alias, printer))
        .map(|preset| BaseChoice { preset: preset.to_string(), source: BaseSource::Generic })
        .ok_or_else(|| Error::NoBaseProfile {
            material: filament.material.clone(),
            printer: printer.to_string(),
        })
}

/// Builds the profile for one filament on one printer preset. The result is validated.
pub fn build_profile(library: &SystemLibrary, request: &ProfileRequest) -> Result<FilamentProfile> {
    build_profile_with_recipe(library, request).map(|(profile, _)| profile)
}

/// [`build_profile`], plus the profile as its base preset and what was changed over it.
pub fn build_profile_with_recipe(
    library: &SystemLibrary,
    request: &ProfileRequest,
) -> Result<(FilamentProfile, Recipe)> {
    let filament = request.filament;
    if library.machine(request.printer).is_none() {
        return Err(Error::PrinterNotFound(request.printer.to_string()));
    }
    // These are FDM slicers' filament presets. A resin slicer would get its own builder.
    if filament.class != MaterialClass::Filament {
        return Err(Error::UnsupportedMaterialClass {
            product: format!("{} {}", filament.brand, filament.name),
            class: filament.class.label(),
            slicer: library.family.slicer_name(),
        });
    }
    if library.family == Family::Prusa {
        return build_prusa_profile(library, request);
    }
    if !filament.diameters.is_empty() && !filament.diameters.iter().any(|d| is_supported_diameter(*d)) {
        return Err(Error::UnsupportedDiameter(format!("{} {}", filament.brand, filament.name)));
    }

    let base_name = choose_base(library, filament, request.printer, request.base)?.preset;
    let base = library.resolve_filament(&base_name)?;
    let filament_type = base
        .first_string("filament_type")
        .ok_or_else(|| Error::Other(format!("{base_name} has no filament_type")))?
        .to_string();

    let mut settings = base.settings;
    let base_vendor = settings.get("filament_vendor").cloned();
    for key in ["compatible_printers", "filament_vendor", "filament_type"] {
        settings.remove(key);
    }
    let base_settings = settings.clone();
    apply_database_values(&mut settings, filament, request.variant);
    // Settings the manufacturer published for this very printer and nozzle say more
    // than the catalogue's ranges, so they go on last.
    if let Some((_, entry)) = crate::tuned::tuned_for(&filament.key, request.printer) {
        crate::tuned::apply(&mut settings, entry);
    } else if request.borrow_tuned {
        if let Some((_, entry)) = crate::tuned::borrowed_for(&filament.key, request.printer) {
            crate::tuned::apply_borrowed(&mut settings, entry);
        }
    }

    let mut recipe = Recipe::between(&base_name, &base_settings, &settings, catalogue_layer);
    // The brand is a field of its own in a profile, so it is compared separately.
    let vendor = Value::Array(vec![Value::String(filament.brand.clone())]);
    if base_vendor.as_ref() != Some(&vendor) {
        recipe.changes.push(Change {
            key: "filament_vendor".into(),
            layer: Layer::Manufacturer,
            from: base_vendor,
            to: vendor,
        });
        recipe.changes.sort_by(|a, b| a.key.cmp(&b.key));
    }

    let alias = request
        .alias
        .map(crate::profile::sanitize_alias)
        .filter(|alias| !alias.is_empty())
        .unwrap_or_else(|| profile_alias(filament, request.variant));
    let name = format!("{alias} @{}", request.printer);
    let profile = FilamentProfile {
        family: Family::Bambu,
        filament_id: filament_id_for(&alias, request.user_id, request.existing),
        filament_settings_id: vec![name.clone()],
        name,
        from: "User".to_string(),
        inherits: String::new(),
        version: library.version_for(&base_name),
        compatible_printers: vec![request.printer.to_string()],
        filament_vendor: vec![filament.brand.clone()],
        filament_type: vec![filament_type],
        settings,
    };
    profile.validate()?;
    Ok((profile, recipe))
}

/// The PrusaSlicer preset for a filament on a printer: the system preset that shows for
/// that printer, flattened, with what the catalogue knows laid over it. It keeps the
/// base's own printer condition, so it shows for exactly the printers its base shows for,
/// and is named after the base's `@` suffix the way PrusaSlicer's own presets are.
fn build_prusa_profile(library: &SystemLibrary, request: &ProfileRequest) -> Result<(FilamentProfile, Recipe)> {
    let filament = request.filament;
    let product = || format!("{} {}", filament.brand, filament.name);
    let base_name = choose_base(library, filament, request.printer, request.base)?.preset;
    let base = library.resolve_filament(&base_name)?;
    let filament_type = base
        .first_string("filament_type")
        .ok_or_else(|| Error::Other(format!("{base_name} has no filament_type")))?
        .to_string();
    let base_vendor = base.first_string("filament_vendor").map(str::to_string);
    let base_settings = library.base_settings(&base_name)?;
    let mut settings = base_settings.clone();

    // The printer decides the diameter, and the base preset is made for that printer.
    let diameter = number(&settings, "filament_diameter").unwrap_or(SUPPORTED_DIAMETER);
    if !filament.diameters.is_empty() && !filament.diameters.iter().any(|d| (d - diameter).abs() < 0.01) {
        return Err(Error::UnsupportedDiameter(product()));
    }
    apply_prusa_values(&mut settings, filament, request.variant);

    let mut recipe = Recipe::between(&base_name, &base_settings, &settings, catalogue_layer);
    if base_vendor.as_deref() != Some(filament.brand.as_str()) {
        recipe.changes.push(Change {
            key: "filament_vendor".into(),
            layer: Layer::Manufacturer,
            from: base_vendor.map(Value::String),
            to: Value::String(filament.brand.clone()),
        });
        recipe.changes.sort_by(|a, b| a.key.cmp(&b.key));
    }

    let alias = request
        .alias
        .map(crate::profile::sanitize_prusa_name)
        .filter(|alias| !alias.is_empty())
        .unwrap_or_else(|| crate::profile::sanitize_prusa_name(&profile_alias(filament, request.variant)));
    // "Generic PLA @MK4S HF0.4" gives "<filament> @MK4S HF0.4"; a template's mark is not a printer.
    let suffix = base_name
        .split_once('@')
        .map(|(_, suffix)| suffix.trim())
        .filter(|suffix| !suffix.is_empty() && !base_name.ends_with(crate::prusa::TEMPLATE_SUFFIX));
    let name = match suffix {
        Some(suffix) => format!("{alias} @{suffix}"),
        None => alias.clone(),
    };
    if library.has_filament(&name) {
        return Err(Error::Other(format!(
            "PrusaSlicer already has a preset of its own called \"{name}\". Pick a colour, or give yours another name."
        )));
    }
    let profile = FilamentProfile {
        family: Family::Prusa,
        name,
        from: "User".to_string(),
        inherits: base_name,
        version: String::new(),
        filament_id: String::new(),
        filament_settings_id: Vec::new(),
        compatible_printers: Vec::new(),
        filament_vendor: vec![filament.brand.clone()],
        filament_type: vec![filament_type],
        settings,
    };
    profile.validate()?;
    Ok((profile, recipe))
}

/// What the catalogue knows, in PrusaSlicer's settings: one value each, no temperature range.
fn apply_prusa_values(settings: &mut BTreeMap<String, Value>, filament: &CatalogueFilament, variant: Option<&Variant>) {
    let mut set = |key: &str, value: String| {
        settings.insert(key.to_string(), Value::String(value));
    };
    if let Some(density) = filament.density.filter(|d| *d > 0.0) {
        set("filament_density", format_number(density));
    }
    if let Some(colour) = variant.and_then(|v| v.color_hex.as_deref()) {
        let digits = colour.trim().trim_start_matches('#');
        if digits.len() >= 6 && digits.chars().all(|c| c.is_ascii_hexdigit()) {
            set("filament_colour", format!("#{}", digits[..6].to_ascii_uppercase()));
        }
    }

    let recommended = filament.recommended;
    let nozzle_range = plausible_range(filament.min_print_temperature, filament.max_print_temperature, &PLAUSIBLE_NOZZLE);
    let nozzle = recommended.nozzle.filter(|t| PLAUSIBLE_NOZZLE.contains(t));
    let first_nozzle = recommended.first_layer_nozzle.filter(|t| PLAUSIBLE_NOZZLE.contains(t)).or(nozzle);
    let bed_range = plausible_range(filament.min_bed_temperature, filament.max_bed_temperature, &PLAUSIBLE_BED);
    let bed = recommended.bed.filter(|t| PLAUSIBLE_BED.contains(t));
    let first_bed = recommended.first_layer_bed.filter(|t| PLAUSIBLE_BED.contains(t)).or(bed);
    for (key, target, range) in [
        ("temperature", nozzle, nozzle_range),
        ("first_layer_temperature", first_nozzle, nozzle_range),
        ("bed_temperature", bed, bed_range),
        ("first_layer_bed_temperature", first_bed, bed_range),
    ] {
        let Some(current) = number(settings, key) else { continue };
        let wanted = match (target, range) {
            (Some(target), _) => target,
            (None, Some((low, high))) => current.clamp(low, high),
            (None, None) => current,
        };
        // PrusaSlicer keeps temperatures as whole degrees.
        settings.insert(key.to_string(), Value::String(format_number(wanted.round())));
    }

    if let Some(limit) = filament.max_volumetric_speed.filter(|v| *v > 0.0) {
        // 0 is PrusaSlicer's "no limit".
        let wanted = match number(settings, "filament_max_volumetric_speed") {
            Some(current) if current > 0.0 => current.min(limit),
            _ => limit,
        };
        settings.insert("filament_max_volumetric_speed".to_string(), Value::String(format_number(wanted)));
    }
}

fn number(settings: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    settings.get(key)?.as_str()?.trim().parse().ok()
}

/// Studio's rule: a filament name keeps one id across printers; a new name gets a hash.
pub(crate) fn filament_id_for(alias: &str, user_id: Option<&str>, existing: &[UserFilament]) -> String {
    let same_filament = existing
        .iter()
        .filter(|f| f.is_root && f.alias() == alias)
        .find_map(|f| f.filament_id.clone());
    if let Some(id) = same_filament {
        return id;
    }
    let taken = |id: &str| existing.iter().any(|f| f.filament_id.as_deref() == Some(id));
    (0..)
        .map(|salt| hashed_filament_id(alias, user_id, salt))
        .find(|id| !taken(id))
        .expect("an unused id exists")
}

fn apply_database_values(
    settings: &mut BTreeMap<String, Value>,
    filament: &CatalogueFilament,
    variant: Option<&Variant>,
) {
    let single = |value: String| Value::Array(vec![Value::String(value)]);

    if let Some(density) = filament.density.filter(|d| *d > 0.0) {
        settings.insert("filament_density".into(), single(format_number(density)));
    }
    if filament.diameters.iter().any(|d| is_supported_diameter(*d)) {
        settings.insert("filament_diameter".into(), single(format_number(SUPPORTED_DIAMETER)));
    }
    if let Some(colour) = variant.and_then(|v| v.color_hex.clone()) {
        settings.insert("default_filament_colour".into(), single(colour));
    }

    // Nozzle: the database range becomes the allowed range; print temperatures follow
    // the recommended value when there is one, otherwise the base value pulled into range.
    let nozzle_range = plausible_range(filament.min_print_temperature, filament.max_print_temperature, &PLAUSIBLE_NOZZLE);
    let recommended = filament.recommended;
    let nozzle = recommended.nozzle.filter(|t| PLAUSIBLE_NOZZLE.contains(t));
    let first_nozzle = recommended.first_layer_nozzle.filter(|t| PLAUSIBLE_NOZZLE.contains(t)).or(nozzle);
    if let Some((low, high)) = nozzle_range {
        settings.insert("nozzle_temperature_range_low".into(), single(format_number(low)));
        settings.insert("nozzle_temperature_range_high".into(), single(format_number(high)));
    }
    adjust_temperatures(settings, "nozzle_temperature", nozzle, nozzle_range);
    adjust_temperatures(settings, "nozzle_temperature_initial_layer", first_nozzle, nozzle_range);

    let bed_range = plausible_range(filament.min_bed_temperature, filament.max_bed_temperature, &PLAUSIBLE_BED);
    let bed = recommended.bed.filter(|t| PLAUSIBLE_BED.contains(t));
    let first_bed = recommended.first_layer_bed.filter(|t| PLAUSIBLE_BED.contains(t)).or(bed);
    for key in BED_KEYS {
        // "0" marks a plate the base preset does not allow for this material.
        let supported = first_value(settings, key).is_some_and(|t| t > 0.0);
        if supported {
            let target = if key.ends_with("_initial_layer") { first_bed } else { bed };
            adjust_temperatures(settings, key, target, bed_range);
        }
    }

    if let Some(limit) = filament.max_volumetric_speed.filter(|v| *v > 0.0) {
        map_numbers(settings, "filament_max_volumetric_speed", |speed| speed.min(limit));
    }
}

/// Sets every value of `key` to `target`, or without a target clamps them into `range`.
fn adjust_temperatures(
    settings: &mut BTreeMap<String, Value>,
    key: &str,
    target: Option<f64>,
    range: Option<(f64, f64)>,
) {
    map_numbers(settings, key, |current| match (target, range) {
        (Some(target), _) => target,
        (None, Some((low, high))) => current.clamp(low, high),
        (None, None) => current,
    });
}

/// Applies `f` to each numeric entry of an array option, leaving `nil` entries alone.
fn map_numbers(settings: &mut BTreeMap<String, Value>, key: &str, f: impl Fn(f64) -> f64) {
    let Some(Value::Array(items)) = settings.get_mut(key) else { return };
    for item in items {
        if let Some(number) = item.as_str().and_then(|s| s.parse::<f64>().ok()) {
            *item = Value::String(format_number(f(number)));
        }
    }
}

fn first_value(settings: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    settings.get(key)?.as_array()?.first()?.as_str()?.parse().ok()
}

fn plausible_range(
    low: Option<f64>,
    high: Option<f64>,
    plausible: &std::ops::RangeInclusive<f64>,
) -> Option<(f64, f64)> {
    match (low, high) {
        (Some(low), Some(high)) if low <= high && plausible.contains(&low) && plausible.contains(&high) => {
            Some((low, high))
        }
        _ => None,
    }
}

fn is_supported_diameter(diameter: f64) -> bool {
    (diameter - SUPPORTED_DIAMETER).abs() < 0.01
}

/// `220.0` -> `220`, `1.24` -> `1.24`: the way Studio writes numbers.
fn format_number(value: f64) -> String {
    format!("{value}")
}
