//! The Open Filament Database (https://openfilamentdatabase.org, MIT), downloaded
//! from its daily GitHub release and cached locally.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const DATASET_URL: &str =
    "https://github.com/OpenFilamentCollective/open-filament-database/releases/latest/download/all.json.gz";
const CACHE_FILE: &str = "open-filament-database.json";
const USER_AGENT: &str = concat!("toona/", env!("CARGO_PKG_VERSION"));
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// When to download the dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// Use the cache; download only if there is none.
    IfMissing,
    /// Download if the cache is older than this; fall back to the cache if that fails.
    IfOlderThan(Duration),
    /// Always download.
    Force,
}

// The dataset as published. Only the fields the tool uses are declared.

/// How a material is printed. The database defines both; today it only lists filament.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MaterialClass {
    /// Filament, for FDM printers.
    #[default]
    #[serde(rename = "FFF")]
    Filament,
    /// Resin, for SLA printers. No supported slicer takes resin profiles yet.
    #[serde(rename = "SLA")]
    Resin,
}

impl MaterialClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::Filament => "filament",
            Self::Resin => "resin",
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawDataset {
    version: String,
    brands: Vec<RawBrand>,
    #[serde(default)]
    materials: Vec<RawMaterial>,
    filaments: Vec<RawFilament>,
    variants: Vec<RawVariant>,
    sizes: Vec<RawSize>,
}

#[derive(Debug, Deserialize)]
struct RawBrand {
    id: String,
    name: String,
    slug: String,
}

#[derive(Debug, Deserialize)]
struct RawMaterial {
    id: String,
    #[serde(default)]
    material_class: MaterialClass,
}

#[derive(Debug, Deserialize)]
struct RawFilament {
    material_id: Option<String>,
    id: String,
    name: String,
    slug: String,
    brand_id: String,
    material: String,
    density: Option<f64>,
    min_print_temperature: Option<f64>,
    max_print_temperature: Option<f64>,
    min_bed_temperature: Option<f64>,
    max_bed_temperature: Option<f64>,
    max_volumetric_speed: Option<f64>,
    #[serde(default)]
    discontinued: bool,
    #[serde(default)]
    slicer_settings: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawVariant {
    id: String,
    name: String,
    slug: String,
    filament_id: String,
    color_hex: Option<String>,
    #[serde(default)]
    color_hexes: Vec<String>,
    #[serde(default)]
    traits: HashMap<String, bool>,
}

#[derive(Debug, Deserialize)]
struct RawSize {
    variant_id: String,
    diameter: Option<f64>,
    /// Retail barcode. Published as a string, but tolerate a bare number.
    gtin: Option<serde_json::Value>,
    barcode_identifier: Option<serde_json::Value>,
}

/// One colour of a filament.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Variant {
    pub name: String,
    pub slug: String,
    /// `#RRGGBB`, when the database has one.
    pub color_hex: Option<String>,
    /// Every colour of a filament that has more than one, in the database's order, each
    /// `#RRGGBB`. Empty for a filament of one colour.
    pub color_hexes: Vec<String>,
    /// The colours run into each other along the filament, rather than lying side by side in it.
    pub gradual: bool,
    /// Retail barcodes (GTIN/EAN/UPC digits) of the spools sold in this colour.
    pub barcodes: Vec<String>,
}

/// One colour of one filament, found by its barcode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarcodeMatch<'a> {
    pub filament: &'a CatalogueFilament,
    pub variant: &'a Variant,
}

/// Recommended temperatures some entries carry beyond the min/max range.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct RecommendedTemps {
    pub nozzle: Option<f64>,
    pub first_layer_nozzle: Option<f64>,
    pub bed: Option<f64>,
    pub first_layer_bed: Option<f64>,
}

/// A filament product line of one brand, with its colours.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogueFilament {
    /// `<brand slug>/<material>/<filament slug>`, unique in the dataset.
    pub key: String,
    pub brand: String,
    pub name: String,
    pub material: String,
    /// Filament or resin.
    pub class: MaterialClass,
    pub density: Option<f64>,
    pub min_print_temperature: Option<f64>,
    pub max_print_temperature: Option<f64>,
    pub min_bed_temperature: Option<f64>,
    pub max_bed_temperature: Option<f64>,
    pub max_volumetric_speed: Option<f64>,
    pub recommended: RecommendedTemps,
    pub discontinued: bool,
    /// Diameters the filament is sold in, in mm, ascending.
    pub diameters: Vec<f64>,
    /// Names of ready-made profiles for this product in a slicer's own library,
    /// keyed by slicer (`bambustudio`, `orcaslicer`, …).
    pub slicer_profiles: BTreeMap<String, String>,
    /// Traits set on any of its colours (`silk`, `matte`, `contains_carbon_fiber`, …).
    pub traits: BTreeSet<String>,
    pub variants: Vec<Variant>,
}

impl CatalogueFilament {
    pub fn variant(&self, query: &str) -> Option<&Variant> {
        let wanted = query.trim().to_lowercase();
        self.variants
            .iter()
            .find(|v| v.slug.to_lowercase() == wanted || v.name.to_lowercase() == wanted)
    }

    /// Lowercase text the search matches against.
    fn haystack(&self) -> String {
        format!("{} {} {} {}", self.brand, self.name, self.material, self.key).to_lowercase()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilter<'a> {
    /// Words that must all appear in the brand, name, material or key.
    pub query: &'a str,
    /// Part of the brand name, e.g. `esun` for "eSUN 3D".
    pub brand: Option<&'a str>,
    /// Exact material, e.g. `PETG`.
    pub material: Option<&'a str>,
    /// Only filament, or only resin.
    pub class: Option<MaterialClass>,
}

#[derive(Debug)]
pub struct Catalogue {
    pub version: String,
    /// Set when a refresh failed and the cached copy was used instead.
    pub warning: Option<String>,
    filaments: Vec<CatalogueFilament>,
}

impl Catalogue {
    /// Loads the dataset from `cache_dir`, downloading it according to `refresh`.
    pub fn load(cache_dir: &Path, refresh: Refresh) -> Result<Self> {
        let cache_path = cache_dir.join(CACHE_FILE);
        let wants_download = match refresh {
            Refresh::Force => true,
            Refresh::IfMissing => !cache_path.exists(),
            Refresh::IfOlderThan(max_age) => cache_age(&cache_path).is_none_or(|age| age > max_age),
        };

        let mut warning = None;
        if wants_download {
            match download(&cache_path) {
                Ok(()) => {}
                // A stale cache is better than nothing unless a refresh was demanded.
                Err(error) if refresh != Refresh::Force && cache_path.exists() => {
                    warning = Some(format!("{error}; using the cached catalogue"));
                }
                Err(Error::Download(reason)) if !cache_path.exists() => {
                    return Err(Error::NoCatalogue(reason));
                }
                Err(error) => return Err(error),
            }
        }

        let bytes = fs::read(&cache_path).map_err(Error::read(&cache_path))?;
        let mut catalogue = Self::from_json(&bytes).map_err(Error::json(&cache_path))?;
        catalogue.warning = warning;
        Ok(catalogue)
    }

    /// Parses a dataset in the published `all.json` format.
    pub fn from_json(bytes: &[u8]) -> serde_json::Result<Self> {
        let raw: RawDataset = serde_json::from_slice(bytes)?;
        Ok(Self::from_raw(raw))
    }

    fn from_raw(raw: RawDataset) -> Self {
        let brands: HashMap<&str, &RawBrand> = raw.brands.iter().map(|b| (b.id.as_str(), b)).collect();

        let classes: HashMap<&str, MaterialClass> =
            raw.materials.iter().map(|m| (m.id.as_str(), m.material_class)).collect();

        let mut diameters_by_variant: HashMap<&str, Vec<f64>> = HashMap::new();
        for size in &raw.sizes {
            if let Some(diameter) = size.diameter {
                diameters_by_variant.entry(size.variant_id.as_str()).or_default().push(diameter);
            }
        }
        let mut barcodes_by_variant: HashMap<&str, Vec<String>> = HashMap::new();
        for size in &raw.sizes {
            for code in [&size.gtin, &size.barcode_identifier].into_iter().flatten() {
                let digits = match code {
                    serde_json::Value::String(text) => barcode_digits(text),
                    serde_json::Value::Number(number) => barcode_digits(&number.to_string()),
                    _ => None,
                };
                if let Some(digits) = digits {
                    let codes = barcodes_by_variant.entry(size.variant_id.as_str()).or_default();
                    if !codes.contains(&digits) {
                        codes.push(digits);
                    }
                }
            }
        }

        let mut variants_by_filament: HashMap<&str, Vec<&RawVariant>> = HashMap::new();
        for variant in &raw.variants {
            variants_by_filament.entry(variant.filament_id.as_str()).or_default().push(variant);
        }

        let mut filaments: Vec<CatalogueFilament> = raw
            .filaments
            .iter()
            .filter_map(|filament| {
                let brand = brands.get(filament.brand_id.as_str())?;
                let raw_variants = variants_by_filament.get(filament.id.as_str()).cloned().unwrap_or_default();

                let mut diameters: Vec<f64> = raw_variants
                    .iter()
                    .filter_map(|v| diameters_by_variant.get(v.id.as_str()))
                    .flatten()
                    .copied()
                    .collect();
                diameters.sort_by(f64::total_cmp);
                diameters.dedup();

                let traits = raw_variants
                    .iter()
                    .flat_map(|v| v.traits.iter().filter(|(_, on)| **on).map(|(name, _)| name.clone()))
                    .collect();

                let mut variants: Vec<Variant> = raw_variants
                    .iter()
                    .map(|v| Variant {
                        name: v.name.clone(),
                        slug: v.slug.clone(),
                        color_hex: v.color_hex.as_deref().and_then(normalize_hex),
                        color_hexes: {
                            let mut all: Vec<String> = v.color_hexes.iter().filter_map(|hex| normalize_hex(hex)).collect();
                            all.dedup();
                            if all.len() > 1 { all } else { Vec::new() }
                        },
                        gradual: v.traits.get("gradual_color_change").copied().unwrap_or(false),
                        barcodes: barcodes_by_variant.get(v.id.as_str()).cloned().unwrap_or_default(),
                    })
                    .collect();
                variants.sort_by(|a, b| a.name.cmp(&b.name));

                let generic = filament.slicer_settings.get("generic");
                let temp = |key: &str| generic.and_then(|g| g.get(key)).and_then(serde_json::Value::as_f64);

                Some(CatalogueFilament {
                    key: format!("{}/{}/{}", brand.slug, filament.material, filament.slug),
                    brand: brand.name.clone(),
                    name: filament.name.clone(),
                    material: filament.material.clone(),
                    class: filament
                        .material_id
                        .as_deref()
                        .and_then(|id| classes.get(id).copied())
                        .unwrap_or_default(),
                    density: filament.density,
                    min_print_temperature: filament.min_print_temperature,
                    max_print_temperature: filament.max_print_temperature,
                    min_bed_temperature: filament.min_bed_temperature,
                    max_bed_temperature: filament.max_bed_temperature,
                    max_volumetric_speed: filament.max_volumetric_speed,
                    recommended: RecommendedTemps {
                        nozzle: temp("nozzle_temp"),
                        first_layer_nozzle: temp("first_layer_nozzle_temp"),
                        bed: temp("bed_temp"),
                        first_layer_bed: temp("first_layer_bed_temp"),
                    },
                    discontinued: filament.discontinued,
                    slicer_profiles: filament
                        .slicer_settings
                        .iter()
                        .filter_map(|(slicer, settings)| {
                            let name = settings.get("profile_name")?.as_str()?;
                            Some((slicer.clone(), name.to_string()))
                        })
                        .collect(),
                    diameters,
                    traits,
                    variants,
                })
            })
            .collect();
        filaments.sort_by(|a, b| a.key.cmp(&b.key));

        Self { version: raw.version, warning: None, filaments }
    }

    pub fn filaments(&self) -> &[CatalogueFilament] {
        &self.filaments
    }

    pub fn len(&self) -> usize {
        self.filaments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.filaments.is_empty()
    }

    pub fn brands(&self) -> Vec<&str> {
        let set: BTreeSet<&str> = self.filaments.iter().map(|f| f.brand.as_str()).collect();
        set.into_iter().collect()
    }

    pub fn materials(&self) -> Vec<&str> {
        let set: BTreeSet<&str> = self.filaments.iter().map(|f| f.material.as_str()).collect();
        set.into_iter().collect()
    }

    pub fn search(&self, filter: &SearchFilter) -> Vec<&CatalogueFilament> {
        let words: Vec<String> = filter.query.split_whitespace().map(str::to_lowercase).collect();
        let brand = filter.brand.map(str::to_lowercase);
        let material = filter.material.map(str::to_lowercase);
        self.filaments
            .iter()
            .filter(|f| brand.as_ref().is_none_or(|b| f.brand.to_lowercase().contains(b.as_str())))
            .filter(|f| material.as_ref().is_none_or(|m| f.material.to_lowercase() == *m))
            .filter(|f| filter.class.is_none_or(|class| f.class == class))
            .filter(|f| {
                let haystack = f.haystack();
                words.iter().all(|word| haystack.contains(word))
            })
            .collect()
    }

    /// Every colour whose spool carries this barcode. Usually one; the database lists
    /// a few barcodes on more than one entry. Empty if `code` is not a barcode.
    pub fn find_barcode(&self, code: &str) -> Vec<BarcodeMatch<'_>> {
        let Some(wanted) = barcode_digits(code) else { return Vec::new() };
        // A 12-digit UPC is the same code as the 13-digit EAN with a leading zero.
        let wanted = wanted.trim_start_matches('0');
        self.filaments
            .iter()
            .flat_map(|filament| filament.variants.iter().map(move |variant| BarcodeMatch { filament, variant }))
            .filter(|m| m.variant.barcodes.iter().any(|code| code.trim_start_matches('0') == wanted))
            .collect()
    }

    /// Finds one filament by its key, or by a search query that matches exactly one.
    pub fn find(&self, query: &str) -> Result<&CatalogueFilament> {
        let wanted = query.trim().to_lowercase();
        if let Some(exact) = self.filaments.iter().find(|f| f.key.to_lowercase() == wanted) {
            return Ok(exact);
        }
        let matches = self.search(&SearchFilter { query, ..SearchFilter::default() });
        match matches.as_slice() {
            [] => Err(Error::FilamentNotFound(query.to_string())),
            [only] => Ok(only),
            many => Err(Error::Ambiguous {
                query: query.to_string(),
                candidates: many.iter().take(8).map(|f| f.key.clone()).collect(),
            }),
        }
    }
}

fn cache_age(path: &Path) -> Option<Duration> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

fn download(cache_path: &Path) -> Result<()> {
    let failed = |error: &dyn std::fmt::Display| Error::Download(error.to_string());

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| failed(&e))?;
    let response = client
        .get(DATASET_URL)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|e| failed(&e))?;
    let compressed = response.bytes().map_err(|e| failed(&e))?;

    let mut json = Vec::new();
    flate2::read::GzDecoder::new(compressed.as_ref())
        .read_to_end(&mut json)
        .map_err(|e| failed(&e))?;
    // Reject a broken download before it replaces a good cache.
    Catalogue::from_json(&json).map_err(|e| failed(&e))?;

    let dir = cache_path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(Error::write(dir))?;
    let partial: PathBuf = cache_path.with_extension("partial");
    fs::write(&partial, &json).map_err(Error::write(&partial))?;
    fs::rename(&partial, cache_path).map_err(Error::write(cache_path))
}

/// The digits of a retail barcode as scanned or typed (spaces and dashes allowed),
/// or `None` if the text is not one. GTINs are 8, 12, 13 or 14 digits.
pub fn barcode_digits(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|c| c.is_ascii_digit() || c == ' ' || c == '-') {
        return None;
    }
    let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
    (8..=14).contains(&digits.len()).then_some(digits)
}

/// `#rrggbb`, `RRGGBB` or `#RRGGBBAA` -> `#RRGGBB`.
fn normalize_hex(raw: &str) -> Option<String> {
    let digits = raw.trim().trim_start_matches('#');
    if digits.len() < 6 || !digits.is_char_boundary(6) || !digits[..6].chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("#{}", digits[..6].to_uppercase()))
}
