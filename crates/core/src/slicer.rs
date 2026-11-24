//! Locating a slicer's config folder and reading what the tool needs from it.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sysinfo::{ProcessesToUpdate, System};

use crate::error::{Error, Result};
use crate::profile::PresetInfo;
use crate::system::{preset_alias, SystemLibrary};

/// Folder used for user presets when no account is signed in.
const SIGNED_OUT_FOLDER: &str = "default";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlicerKind {
    BambuStudio,
    /// Same profile family as Bambu Studio. Not verified against a real install yet.
    OrcaSlicer,
}

impl SlicerKind {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::BambuStudio => "Bambu Studio",
            Self::OrcaSlicer => "OrcaSlicer",
        }
    }

    /// Folder name under the OS config directory, also the stem of the `.conf` file.
    pub fn folder_name(self) -> &'static str {
        match self {
            Self::BambuStudio => "BambuStudio",
            Self::OrcaSlicer => "OrcaSlicer",
        }
    }

    /// Stable identifier used in the install manifest.
    pub fn id(self) -> &'static str {
        match self {
            Self::BambuStudio => "bambu-studio",
            Self::OrcaSlicer => "orca-slicer",
        }
    }

    /// Executable names, lowercase, without extension.
    fn process_names(self) -> &'static [&'static str] {
        match self {
            Self::BambuStudio => &["bambustudio", "bambu-studio", "bambu studio"],
            Self::OrcaSlicer => &["orcaslicer", "orca-slicer", "orca slicer"],
        }
    }
}

/// What the tool reads from `<Slicer>.conf`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppConf {
    /// `app.preset_folder`: the signed-in account id, or `default`.
    pub preset_folder: String,
    /// `app.sync_user_preset`: user presets are uploaded to the vendor's cloud.
    pub sync_user_preset: bool,
    /// `presets.machine`: the machine preset currently selected.
    pub selected_machine: Option<String>,
    /// `models`: printer models added to the slicer, with their enabled nozzle sizes.
    pub models: Vec<ConfiguredModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredModel {
    pub model: String,
    pub nozzles: Vec<String>,
}

impl AppConf {
    /// The account id, or `None` when signed out.
    pub fn user_id(&self) -> Option<&str> {
        match self.preset_folder.as_str() {
            "" | SIGNED_OUT_FOLDER => None,
            id => Some(id),
        }
    }

    /// New user presets will be uploaded to the cloud at the slicer's next start.
    pub fn uploads_new_presets(&self) -> bool {
        self.sync_user_preset && self.user_id().is_some()
    }

    fn parse(text: &str) -> serde_json::Result<Self> {
        // The file is JSON, sometimes followed by a checksum comment line.
        let json = text.rfind('}').map_or(text, |end| &text[..=end]);
        let root: Value = serde_json::from_str(json)?;

        let app = &root["app"];
        let preset_folder = match &app["preset_folder"] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => String::new(),
        };
        let sync_user_preset = match &app["sync_user_preset"] {
            Value::Bool(b) => *b,
            Value::String(s) => s == "true",
            _ => false,
        };
        let selected_machine = root["presets"]["machine"].as_str().map(str::to_string);
        let models = root["models"]
            .as_array()
            .map(|models| {
                models
                    .iter()
                    .filter_map(|entry| {
                        Some(ConfiguredModel {
                            model: entry["model"].as_str()?.to_string(),
                            nozzles: entry["nozzle_diameter"]
                                .as_str()
                                .unwrap_or_default()
                                .split(';')
                                .filter(|n| !n.is_empty())
                                .map(str::to_string)
                                .collect(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self { preset_folder, sync_user_preset, selected_machine, models })
    }
}

/// A printer/nozzle preset a filament profile can target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterPreset {
    /// System machine preset name, the value for `compatible_printers`.
    pub name: String,
    pub model: String,
    pub nozzle: String,
    /// Currently selected in the slicer.
    pub selected: bool,
}

/// An existing user filament profile found in the slicer's folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFilament {
    pub name: String,
    pub filament_id: Option<String>,
    pub is_root: bool,
    pub path: PathBuf,
}

impl UserFilament {
    pub fn alias(&self) -> &str {
        preset_alias(&self.name)
    }
}

#[derive(Debug, Clone)]
pub struct Slicer {
    pub kind: SlicerKind,
    pub config_dir: PathBuf,
}

impl Slicer {
    /// The slicer's config folder in its standard location for this OS:
    /// `%AppData%\<Name>`, `~/Library/Application Support/<Name>` or `~/.config/<Name>`.
    pub fn detect(kind: SlicerKind) -> Result<Self> {
        let base = directories::BaseDirs::new()
            .ok_or_else(|| Error::Other("could not determine the home directory".into()))?;
        Self::at(kind, base.config_dir().join(kind.folder_name()))
    }

    /// A slicer config folder at an explicit path.
    pub fn at(kind: SlicerKind, config_dir: impl Into<PathBuf>) -> Result<Self> {
        let config_dir = config_dir.into();
        if !config_dir.join("system").is_dir() {
            return Err(Error::SlicerNotFound { slicer: kind.display_name(), path: config_dir });
        }
        Ok(Self { kind, config_dir })
    }

    pub fn conf_path(&self) -> PathBuf {
        self.config_dir.join(format!("{}.conf", self.kind.folder_name()))
    }

    pub fn conf(&self) -> Result<AppConf> {
        let path = self.conf_path();
        let text = fs::read_to_string(&path).map_err(Error::read(&path))?;
        AppConf::parse(&text).map_err(Error::json(&path))
    }

    /// `system/BBL`: the Bambu Lab system presets.
    pub fn system_vendor_dir(&self) -> PathBuf {
        self.config_dir.join("system").join("BBL")
    }

    pub fn system_library(&self) -> Result<SystemLibrary> {
        SystemLibrary::load(&self.system_vendor_dir())
    }

    /// `user/`: every account's presets.
    pub fn user_root(&self) -> PathBuf {
        self.config_dir.join("user")
    }

    /// Names of the folders under `user/`.
    pub fn user_folders(&self) -> Result<Vec<String>> {
        let root = self.user_root();
        let mut names = Vec::new();
        if root.is_dir() {
            for entry in fs::read_dir(&root).map_err(Error::read(&root))? {
                let entry = entry.map_err(Error::read(&root))?;
                if entry.path().is_dir() {
                    names.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    /// `user/<preset_folder>/filament/base`: where root filament profiles live.
    pub fn custom_filament_dir(&self, conf: &AppConf) -> PathBuf {
        let folder = if conf.preset_folder.is_empty() { SIGNED_OUT_FOLDER } else { &conf.preset_folder };
        self.user_root().join(folder).join("filament").join("base")
    }

    /// Printer/nozzle presets for the models added to the slicer.
    pub fn printers(&self, conf: &AppConf, library: &SystemLibrary) -> Vec<PrinterPreset> {
        let mut printers: Vec<PrinterPreset> = library
            .machines()
            .iter()
            .filter(|machine| {
                conf.models
                    .iter()
                    .any(|m| m.model == machine.model && m.nozzles.contains(&machine.nozzle))
            })
            .map(|machine| PrinterPreset {
                name: machine.name.clone(),
                model: machine.model.clone(),
                nozzle: machine.nozzle.clone(),
                selected: conf.selected_machine.as_deref() == Some(machine.name.as_str()),
            })
            .collect();
        printers.sort_by(|a, b| b.selected.cmp(&a.selected).then_with(|| a.name.cmp(&b.name)));
        printers
    }

    /// User filament profiles of the active account, both inherited and root ones.
    pub fn user_filaments(&self, conf: &AppConf) -> Result<Vec<UserFilament>> {
        let base_dir = self.custom_filament_dir(conf);
        let mut found = Vec::new();
        for dir in [base_dir.parent().map(Path::to_path_buf), Some(base_dir.clone())].into_iter().flatten() {
            if !dir.is_dir() {
                continue;
            }
            for entry in fs::read_dir(&dir).map_err(Error::read(&dir))? {
                let path = entry.map_err(Error::read(&dir))?.path();
                if path.extension().is_none_or(|ext| ext != "json") {
                    continue;
                }
                // Unreadable files are the slicer's business; skip rather than fail.
                let Ok(text) = fs::read_to_string(&path) else { continue };
                let Ok(json) = serde_json::from_str::<Value>(&text) else { continue };
                let Some(name) = json["name"].as_str() else { continue };
                found.push(UserFilament {
                    name: name.to_string(),
                    filament_id: json["filament_id"].as_str().map(str::to_string),
                    is_root: json["inherits"].as_str().is_none_or(str::is_empty),
                    path,
                });
            }
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }

    /// Reads the `.info` next to a profile, if there is one.
    pub fn preset_info(profile_path: &Path) -> Option<PresetInfo> {
        fs::read_to_string(profile_path.with_extension("info")).ok().map(|text| PresetInfo::parse(&text))
    }

    /// Whether the slicer's main process is running. Fails if processes cannot be
    /// listed, so that callers do not write on a guess.
    pub fn is_running(&self) -> Result<bool> {
        let mut system = System::new()
            .map_err(|e| Error::Other(format!("could not list running processes: {e}")))?;
        system.refresh_processes(ProcessesToUpdate::All, true);
        Ok(system.processes().values().any(|process| {
            let name = process.name().to_string_lossy().to_lowercase();
            let exe_stem = process
                .exe()
                .and_then(Path::file_stem)
                .map(|stem| stem.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let name = name.trim_end_matches(".exe");
            self.kind.process_names().iter().any(|known| name == *known || exe_stem == *known)
        }))
    }
}
