//! The slicer's bundled system presets (`system/BBL`), and the resolution of a
//! system filament preset into a flat config, as described in PROFILE_SPEC.md §3.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::orca;
use crate::prusa;
use crate::slicer::Family;

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
    /// The model as the slicer shows it.
    pub model: String,
    pub nozzle: String,
    /// The model as the slicer's own settings file names it. The same as `model` in Bambu Studio.
    pub model_id: String,
    /// A printer preset the user made, not one the slicer ships.
    pub user: bool,
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
    pub family: Family,
    filaments: HashMap<String, Map<String, Value>>,
    machines: Vec<MachinePreset>,
    /// PrusaSlicer only: the printer presets, for working out which filaments show for which.
    printer_presets: HashMap<String, Map<String, Value>>,
    /// The filament presets that show for each printer asked about so far, where that takes
    /// working out: PrusaSlicer and OrcaSlicer.
    shown: Mutex<HashMap<String, Arc<Vec<String>>>>,
    /// OrcaSlicer: a filament that names no printers is offered to all of them.
    open_to_all: bool,
    /// OrcaSlicer: for each printer the user made from a system one, that system printer.
    /// It is offered the filaments made for its parent as well as its own.
    parents: HashMap<String, String>,
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
                    model_id: model.to_string(),
                    user: false,
                });
            }
        }
        machines.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Self { version, filaments, machines, ..Self::default() })
    }

    /// Loads every vendor in OrcaSlicer's `system` folder, and the printers the user made
    /// themselves from `user_machine_dir` (`user/<account>/machine`) when it is given.
    pub fn load_orca(system_dir: &Path, user_machine_dir: Option<&Path>) -> Result<Self> {
        let loaded = orca::load(system_dir)?;
        let mut machines = loaded.machines;
        let mut parents = HashMap::new();
        for printer in user_machine_dir.map(|dir| orca::user_printers(dir, &machines)).unwrap_or_default() {
            if let Some(parent) = printer.parent {
                parents.insert(printer.machine.name.clone(), parent);
            }
            machines.push(printer.machine);
        }
        machines.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { version: loaded.version, filaments: loaded.filaments, machines, open_to_all: true, parents, ..Self::default() })
    }

    /// The version a profile built on `filament` is stamped with: that of the vendor
    /// bundle the preset came from.
    pub fn version_for(&self, filament: &str) -> String {
        self.filaments
            .get(filament)
            .and_then(|preset| preset.get("version")?.as_str())
            .map_or_else(|| self.version.clone(), normalize_version)
    }

    /// Loads PrusaSlicer's vendor bundles and the user's printer presets from its settings folder.
    pub fn load_prusa(config_dir: &Path) -> Result<Self> {
        let loaded = prusa::load(config_dir)?;
        let machines = loaded
            .printers
            .into_iter()
            .map(|printer| MachinePreset {
                name: printer.name,
                model: printer.model,
                nozzle: printer.nozzle,
                model_id: printer.model_id,
                user: printer.user,
            })
            .collect();
        Ok(Self {
            version: loaded.version,
            family: Family::Prusa,
            filaments: loaded.filaments,
            machines,
            printer_presets: loaded.printer_presets,
            ..Self::default()
        })
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

    pub(crate) fn printer_view<'a>(&'a self, printer: &'a str) -> prusa::PrinterView<'a> {
        prusa::PrinterView { presets: &self.printer_presets, name: printer }
    }

    /// Whether a filament with these settings shows for `printer` in PrusaSlicer.
    pub fn shows_for(&self, settings: &BTreeMap<String, Value>, printer: &str) -> bool {
        let text = |key: &str| settings.get(key).and_then(Value::as_str).unwrap_or_default();
        let view = prusa::PrinterView { presets: &self.printer_presets, name: printer };
        prusa::compatible(text("compatible_printers"), text("compatible_printers_condition"), &view)
    }

    /// PrusaSlicer's system filament presets that show for `printer`: its own vendor's
    /// first, then the generic templates, each sorted by name.
    fn shown_for_printer(&self, printer: &str) -> Arc<Vec<String>> {
        let mut shown = self.shown.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(known) = shown.get(printer) {
            return known.clone();
        }
        let view = prusa::PrinterView { presets: &self.printer_presets, name: printer };
        let value = |name: &str, key: &str| prusa::lookup(&self.filaments, name, key).unwrap_or_default();
        // Thousands of presets share a few hundred conditions.
        let mut answers: HashMap<(&str, &str), bool> = HashMap::new();
        let mut names: Vec<String> = self
            .filaments
            .keys()
            .filter(|name| !name.starts_with('*'))
            .filter(|name| {
                let asked = (value(name, "compatible_printers"), value(name, "compatible_printers_condition"));
                *answers.entry(asked).or_insert_with(|| prusa::compatible(asked.0, asked.1, &view))
            })
            .cloned()
            .collect();
        names.sort_by(|a, b| a.ends_with(prusa::TEMPLATE_SUFFIX).cmp(&b.ends_with(prusa::TEMPLATE_SUFFIX)).then_with(|| a.cmp(b)));
        let names = Arc::new(names);
        shown.insert(printer.to_string(), names.clone());
        names
    }

    /// OrcaSlicer's system filament presets that show for `printer`: the ones made for it
    /// first, then the ones offered to every printer, each sorted by name.
    fn open_for_printer(&self, printer: &str) -> Arc<Vec<String>> {
        let mut shown = self.shown.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(known) = shown.get(printer) {
            return known.clone();
        }
        let parent = self.parents.get(printer).map(String::as_str);
        let mut names: Vec<(bool, &String)> = self
            .filaments
            .iter()
            .filter(|(_, preset)| preset.get("instantiation").and_then(Value::as_str) == Some("true"))
            .filter_map(|(name, _)| {
                let listed: Vec<&str> = match self.inherited(name, "compatible_printers", 0) {
                    Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).filter(|p| !p.is_empty()).collect(),
                    Some(Value::String(one)) if !one.is_empty() => vec![one.as_str()],
                    _ => Vec::new(),
                };
                // As OrcaSlicer has it: for every printer, for this one, or for the one it was made from.
                let named = listed.contains(&printer) || parent.is_some_and(|parent| listed.contains(&parent));
                (listed.is_empty() || named).then_some((listed.is_empty(), name))
            })
            .collect();
        names.sort();
        let names = Arc::new(names.into_iter().map(|(_, name)| name.clone()).collect::<Vec<_>>());
        shown.insert(printer.to_string(), names.clone());
        names
    }

    /// A setting as a preset has it or takes it from the presets it is built on.
    fn inherited(&self, name: &str, key: &str, depth: usize) -> Option<&Value> {
        let preset = self.filaments.get(name)?;
        if let Some(own) = preset.get(key) {
            return Some(own);
        }
        if depth > MAX_CHAIN_DEPTH {
            return None;
        }
        let includes = match preset.get("include") {
            Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
            Some(Value::String(one)) => vec![one.as_str()],
            _ => Vec::new(),
        };
        includes
            .into_iter()
            .rev()
            .chain(preset.get("inherits").and_then(Value::as_str).filter(|parent| !parent.is_empty()))
            .find_map(|parent| self.inherited(parent, key, depth + 1))
    }

    /// A system filament preset's settings as a profile built on it starts out: resolved
    /// through its whole chain, without the fields a profile keeps apart from its settings.
    pub fn base_settings(&self, name: &str) -> Result<BTreeMap<String, Value>> {
        let mut settings = self.resolve_filament(name)?.settings;
        for key in self.family.field_keys() {
            settings.remove(*key);
        }
        Ok(settings)
    }

    /// Selectable system filament presets that list `printer` as compatible, sorted.
    pub fn filaments_for_printer(&self, printer: &str) -> Vec<&str> {
        if self.family == Family::Prusa {
            let shown = self.shown_for_printer(printer);
            return shown.iter().filter_map(|name| self.filaments.get_key_value(name).map(|(key, _)| key.as_str())).collect();
        }
        if self.open_to_all {
            let shown = self.open_for_printer(printer);
            return shown.iter().filter_map(|name| self.filaments.get_key_value(name).map(|(key, _)| key.as_str())).collect();
        }
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

        let (mut settings, mut filament_id) = match preset.get("inherits") {
            Some(Value::String(parent)) if !parent.is_empty() => self.resolve_chain(parent, depth + 1)?,
            // PrusaSlicer: several parents, each laid over the ones before it.
            Some(Value::Array(parents)) => {
                let mut settings = BTreeMap::new();
                for parent in parents.iter().filter_map(Value::as_str) {
                    settings.extend(self.resolve_chain(parent, depth + 1)?.0);
                }
                (settings, None)
            }
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

pub(crate) fn read_object(path: &Path) -> Result<Map<String, Value>> {
    let text = fs::read_to_string(path).map_err(Error::read(path))?;
    serde_json::from_str(&text).map_err(Error::json(path))
}

pub(crate) fn json_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
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
