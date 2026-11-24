//! The slicer's bundled system presets (`system/BBL`), and the resolution of a
//! system filament preset into a flat config, as described in PROFILE_SPEC.md §3.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{Error, Result};

/// Filament options that hold one value per extruder variant. Checked against the
/// profiles Bambu Studio 2.9 writes: `filament_flush_temp_fast`,
/// `filament_retract_length_toolchange` and `filament_retract_restart_extra_toolchange`
/// are in Studio's own list on `master` but are written with a single value, so
/// they are not here.
pub const VARIANT_KEYS: &[&str] = &[
    "filament_flow_ratio",
    "filament_max_volumetric_speed",
    "filament_ramming_volumetric_speed",
    "filament_pre_cooling_temperature",
    "filament_ramming_travel_time",
    "filament_ramming_volumetric_speed_nc",
    "filament_pre_cooling_temperature_nc",
    "filament_ramming_travel_time_nc",
    "filament_extruder_variant",
    "filament_retraction_length",
    "filament_retract_length_nc",
    "filament_z_hop",
    "filament_z_hop_types",
    "filament_retract_restart_extra",
    "filament_retraction_speed",
    "filament_deretraction_speed",
    "filament_retraction_minimum_travel",
    "filament_retract_when_changing_layer",
    "filament_wipe",
    "filament_wipe_distance",
    "filament_retract_before_wipe",
    "filament_long_retractions_when_cut",
    "filament_retraction_distances_when_cut",
    "long_retractions_when_ec",
    "retraction_distances_when_ec",
    "nozzle_temperature_initial_layer",
    "nozzle_temperature",
    "filament_flush_volumetric_speed",
    "filament_flush_temp",
    "filament_enable_overhang_speed",
    "filament_bridge_speed",
    "filament_overhang_1_4_speed",
    "filament_overhang_2_4_speed",
    "filament_overhang_3_4_speed",
    "filament_overhang_4_4_speed",
    "filament_overhang_totally_speed",
    "override_process_overhang_speed",
    "filament_adaptive_volumetric_speed",
    "volumetric_speed_coefficients",
    "filament_cooling_before_tower",
    "filament_preheat_temperature_delta",
    "slow_down_min_speed",
];

/// Keys that describe a preset rather than configure the filament.
const PRESET_META_KEYS: &[&str] = &[
    "name",
    "inherits",
    "from",
    "version",
    "setting_id",
    "filament_id",
    "instantiation",
    "type",
    "description",
    "include",
    "filament_settings_id",
];

pub(crate) fn is_preset_meta_key(key: &str) -> bool {
    PRESET_META_KEYS.contains(&key) || key == "base_id"
}

/// Present in system presets but never written by Studio into a user profile.
const DROPPED_KEYS: &[&str] = &[
    "filament_contact_safe",
    "filament_emission_safe",
    "filament_ingredients_safe",
    "filament_long_retractions_when_ec",
    "filament_retraction_distances_when_ec",
];

const VARIANT_COUNT_KEY: &str = "filament_extruder_variant";
const MAX_CHAIN_DEPTH: usize = 32;

/// A system machine preset: one printer model with one nozzle size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachinePreset {
    pub name: String,
    pub model: String,
    pub nozzle: String,
}

/// A system filament preset resolved through its whole inherits/include chain.
#[derive(Debug, Clone)]
pub struct ResolvedFilament {
    pub name: String,
    pub settings: BTreeMap<String, Value>,
    pub filament_id: Option<String>,
}

impl ResolvedFilament {
    pub fn first_string(&self, key: &str) -> Option<&str> {
        match self.settings.get(key)? {
            Value::String(s) => Some(s),
            Value::Array(items) => items.first()?.as_str(),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct SystemLibrary {
    /// Bundle version as Studio writes it into user profiles, e.g. `2.9.0.83`.
    pub version: String,
    filaments: HashMap<String, Map<String, Value>>,
    machines: Vec<MachinePreset>,
}

impl SystemLibrary {
    /// Loads `<vendor_dir>.json` and the `filament/` and `machine/` folders in `vendor_dir`
    /// (`system/BBL`).
    pub fn load(vendor_dir: &Path) -> Result<Self> {
        let index_path = vendor_dir.with_extension("json");
        let index = read_object(&index_path)?;
        let version = index
            .get("version")
            .and_then(Value::as_str)
            .map(normalize_version)
            .ok_or_else(|| Error::Other(format!("{} has no version", index_path.display())))?;

        let mut filaments = HashMap::new();
        for path in json_files(&vendor_dir.join("filament"))? {
            let preset = read_object(&path)?;
            // The folder also holds lookup tables (colour codes, id maps) with no name.
            if let Some(name) = preset.get("name").and_then(Value::as_str) {
                filaments.insert(name.to_string(), preset);
            }
        }

        let mut machines = Vec::new();
        for path in json_files(&vendor_dir.join("machine"))? {
            let preset = read_object(&path)?;
            let text = |key: &str| preset.get(key).and_then(Value::as_str);
            if text("type") != Some("machine") || text("instantiation") != Some("true") {
                continue;
            }
            if let (Some(name), Some(model), Some(nozzle)) =
                (text("name"), text("printer_model"), text("printer_variant"))
            {
                machines.push(MachinePreset {
                    name: name.to_string(),
                    model: model.to_string(),
                    nozzle: nozzle.to_string(),
                });
            }
        }
        machines.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Self { version, filaments, machines })
    }

    pub fn machines(&self) -> &[MachinePreset] {
        &self.machines
    }

    pub fn machine(&self, name: &str) -> Option<&MachinePreset> {
        self.machines.iter().find(|m| m.name == name)
    }

    pub fn has_filament(&self, name: &str) -> bool {
        self.filaments.contains_key(name)
    }

    /// Selectable system filament presets that list `printer` as compatible, sorted.
    pub fn filaments_for_printer(&self, printer: &str) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .filaments
            .iter()
            .filter(|(_, preset)| preset.get("instantiation").and_then(Value::as_str) == Some("true"))
            .filter(|(_, preset)| {
                preset
                    .get("compatible_printers")
                    .and_then(Value::as_array)
                    .is_some_and(|list| list.iter().any(|p| p.as_str() == Some(printer)))
            })
            .map(|(name, _)| name.as_str())
            .collect();
        names.sort_unstable();
        names
    }

    /// The system preset for `printer` whose alias (the name up to `@`) is `alias`.
    pub fn filament_by_alias(&self, alias: &str, printer: &str) -> Option<&str> {
        self.filaments_for_printer(printer)
            .into_iter()
            .find(|name| preset_alias(name) == alias)
    }

    /// Resolves a system filament preset into the flat settings Studio would write
    /// for a root user profile cloned from it.
    pub fn resolve_filament(&self, name: &str) -> Result<ResolvedFilament> {
        let (mut settings, filament_id) = self.resolve_chain(name, 0)?;

        let variants = settings
            .get(VARIANT_COUNT_KEY)
            .and_then(Value::as_array)
            .map_or(1, Vec::len);
        for key in VARIANT_KEYS {
            if let Some(Value::Array(items)) = settings.get_mut(*key) {
                if let Some(last) = items.last().cloned() {
                    items.resize(variants, last);
                }
            }
        }
        for key in DROPPED_KEYS {
            settings.remove(*key);
        }

        Ok(ResolvedFilament { name: name.to_string(), settings, filament_id })
    }

    fn resolve_chain(
        &self,
        name: &str,
        depth: usize,
    ) -> Result<(BTreeMap<String, Value>, Option<String>)> {
        if depth > MAX_CHAIN_DEPTH {
            return Err(Error::PresetCycle(name.to_string()));
        }
        let preset = self
            .filaments
            .get(name)
            .ok_or_else(|| Error::PresetNotFound(name.to_string()))?;

        let (mut settings, mut filament_id) = match preset.get("inherits").and_then(Value::as_str) {
            Some(parent) if !parent.is_empty() => self.resolve_chain(parent, depth + 1)?,
            _ => (BTreeMap::new(), None),
        };

        // Included templates go over the parent and under the preset's own values.
        let includes: Vec<&str> = match preset.get("include") {
            Some(Value::String(one)) => vec![one.as_str()],
            Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        for include in includes {
            let template = self
                .filaments
                .get(include)
                .ok_or_else(|| Error::PresetNotFound(include.to_string()))?;
            apply_settings(&mut settings, template);
        }
        apply_settings(&mut settings, preset);

        if let Some(id) = preset.get("filament_id").and_then(Value::as_str) {
            filament_id = Some(id.to_string());
        }
        Ok((settings, filament_id))
    }
}

/// The part of a preset name before `@`, which Studio shows as the filament's name.
pub fn preset_alias(name: &str) -> &str {
    name.split('@').next().unwrap_or(name).trim_end()
}

/// `02.09.00.83` -> `2.9.0.83`, the form Studio writes into profiles.
pub fn normalize_version(raw: &str) -> String {
    raw.split('.')
        .map(|part| part.trim_start_matches('0'))
        .map(|part| if part.is_empty() { "0" } else { part })
        .collect::<Vec<_>>()
        .join(".")
}

fn apply_settings(target: &mut BTreeMap<String, Value>, preset: &Map<String, Value>) {
    for (key, value) in preset {
        if !PRESET_META_KEYS.contains(&key.as_str()) {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn read_object(path: &Path) -> Result<Map<String, Value>> {
    let text = fs::read_to_string(path).map_err(Error::read(path))?;
    serde_json::from_str(&text).map_err(Error::json(path))
}

fn json_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).map_err(Error::read(&current))? {
            let path = entry.map_err(Error::read(&current))?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "json") {
                files.push(path);
            }
        }
    }
    Ok(files)
}
