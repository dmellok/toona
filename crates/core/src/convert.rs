//! Turning a user profile that inherits from a system preset (what Import Configs
//! produces) into a root profile the AMS can use, keeping the user's settings.
//! See PROFILE_SPEC.md §1 for why inherited profiles are not offered for AMS slots.

use std::fs;

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::mapping::filament_id_for;
use crate::profile::{sanitize_alias, FilamentProfile};
use crate::recipe::{Layer, Recipe};
use crate::slicer::{PrinterPreset, UserFilament};
use crate::system::{is_preset_meta_key, SystemLibrary};

/// Marks a slot of a per-variant setting that keeps the parent's value.
const INHERIT_SLOT: &str = "nil";

pub struct ConvertRequest<'a> {
    /// The inherited profile to convert. It is read, never modified.
    pub source: &'a UserFilament,
    /// Filament name for the new profile. Default: the source's name.
    pub alias: Option<&'a str>,
    /// Brand to record. Default: the vendor of the preset it inherits from.
    pub vendor: Option<&'a str>,
    /// Printer presets set up in the slicer.
    pub printers: &'a [PrinterPreset],
    pub user_id: Option<&'a str>,
    pub existing: &'a [UserFilament],
}

/// Builds one root profile per printer the source profile applies to.
/// The results are validated and carry the source's settings on top of its parent's.
pub fn convert_inherited(library: &SystemLibrary, request: &ConvertRequest) -> Result<Vec<FilamentProfile>> {
    Ok(convert_inherited_with_recipes(library, request)?.into_iter().map(|(profile, _)| profile).collect())
}

/// [`convert_inherited`], plus each profile as the system preset it inherits from and
/// the user's own changes over it.
pub fn convert_inherited_with_recipes(
    library: &SystemLibrary,
    request: &ConvertRequest,
) -> Result<Vec<(FilamentProfile, Recipe)>> {
    let source = request.source;
    let refuse = |reason: String| Error::NotConvertible { name: source.name.clone(), reason };

    let text = fs::read_to_string(&source.path).map_err(Error::read(&source.path))?;
    let json: Map<String, Value> = serde_json::from_str(&text).map_err(Error::json(&source.path))?;
    let parent_name = json.get("inherits").and_then(Value::as_str).unwrap_or_default();
    if parent_name.is_empty() {
        return Err(refuse("it is already a custom filament".into()));
    }
    if !library.has_filament(parent_name) {
        return Err(refuse(format!(
            "it inherits from \"{parent_name}\", which is not one of the slicer's system presets"
        )));
    }

    let parent = library.resolve_filament(parent_name)?;
    let mut settings = parent.settings;
    let parent_settings = settings.clone();
    for (key, value) in &json {
        if is_preset_meta_key(key) {
            continue;
        }
        let merged = match (settings.get(key), value) {
            (Some(Value::Array(inherited)), Value::Array(own)) => Value::Array(merge_slots(inherited, own)),
            _ => value.clone(),
        };
        settings.insert(key.clone(), merged);
    }

    let take_strings = |settings: &mut std::collections::BTreeMap<String, Value>, key: &str| -> Vec<String> {
        match settings.remove(key) {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            _ => Vec::new(),
        }
    };
    let compatible = take_strings(&mut settings, "compatible_printers");
    let parent_vendor = take_strings(&mut settings, "filament_vendor");
    let filament_type = take_strings(&mut settings, "filament_type");
    if filament_type.is_empty() {
        return Err(refuse(format!("\"{parent_name}\" has no filament type")));
    }

    let targets: Vec<&str> = compatible
        .iter()
        .map(String::as_str)
        .filter(|printer| request.printers.iter().any(|p| p.name == *printer))
        .collect();
    if targets.is_empty() {
        return Err(refuse(format!(
            "it is made for {}, which is not set up in the slicer",
            if compatible.is_empty() { "no printer".to_string() } else { compatible.join(", ") }
        )));
    }

    // What the user changed over the preset they imported onto. The printer and brand
    // lists were taken out above; they are the parent's own, not the user's.
    let recipe = Recipe::between(parent_name, &parent_settings, &settings, |_| Layer::You);

    let alias = sanitize_alias(request.alias.unwrap_or(&source.name));
    let vendor = match request.vendor {
        Some(vendor) => vendor.to_string(),
        None => parent_vendor.into_iter().next().unwrap_or_else(|| "Generic".to_string()),
    };
    let filament_id = filament_id_for(&alias, request.user_id, request.existing);

    targets
        .into_iter()
        .map(|printer| {
            let name = format!("{alias} @{printer}");
            let profile = FilamentProfile {
                filament_settings_id: vec![name.clone()],
                name,
                from: "User".to_string(),
                inherits: String::new(),
                version: library.version.clone(),
                filament_id: filament_id.clone(),
                compatible_printers: vec![printer.to_string()],
                filament_vendor: vec![vendor.clone()],
                filament_type: filament_type.clone(),
                settings: settings.clone(),
            };
            profile.validate()?;
            Ok((profile, recipe.clone()))
        })
        .collect()
}

/// A child's per-variant array over its parent's: `nil` slots keep the parent's value.
/// Arrays of different lengths are not slot-aligned, so the child's replaces the parent's
/// unless it is nothing but `nil`.
fn merge_slots(inherited: &[Value], own: &[Value]) -> Vec<Value> {
    let is_inherit = |value: &Value| value.as_str() == Some(INHERIT_SLOT);
    if own.len() == inherited.len() {
        own.iter()
            .zip(inherited)
            .map(|(mine, theirs)| if is_inherit(mine) { theirs.clone() } else { mine.clone() })
            .collect()
    } else if own.iter().all(is_inherit) {
        inherited.to_vec()
    } else {
        own.to_vec()
    }
}
