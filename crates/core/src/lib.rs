//! Core of Toona: browse the Open Filament Database and install its
//! filaments into Bambu Studio as custom filament profiles.
//!
//! All logic lives here so the CLI and the desktop app share it. The profile format
//! is specified in `PROFILE_SPEC.md` at the repository root.

pub mod catalogue;
pub mod convert;
pub mod error;
pub mod install;
pub mod mapping;
pub mod profile;
pub mod recipe;
pub mod slicer;
pub mod system;

pub use catalogue::{BarcodeMatch, Catalogue, CatalogueFilament, MaterialClass, Refresh, SearchFilter, Variant};
pub use convert::{convert_inherited, convert_inherited_with_recipes, ConvertRequest};
pub use error::{Error, Result};
pub use install::{AppDirs, InstallOptions, InstallReport, InstalledProfile, Manifest, PlannedProfile, UninstallReport};
pub use mapping::{build_profile, build_profile_with_recipe, ProfileRequest};
pub use profile::{FilamentProfile, PresetInfo};
pub use recipe::{Change, Layer, Recipe};
pub use slicer::{AppConf, PrinterPreset, Slicer, SlicerKind, UserFilament};
pub use system::SystemLibrary;
