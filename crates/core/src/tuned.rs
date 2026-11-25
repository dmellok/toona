//! Settings a manufacturer publishes for one of its filaments on one printer and nozzle.
//! Only numbers are kept, in `data/tuned/`, each file naming where they were read from.
//! They are applied over the base preset as the manufacturer's own values.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::Value;

/// One manufacturer's published settings.
#[derive(Debug, Deserialize)]
pub struct TunedSet {
    pub brand: String,
    /// The page the values were read from.
    pub source: String,
    /// The day they were read, `YYYY-MM-DD`.
    pub retrieved: String,
    entries: Vec<TunedEntry>,
}

#[derive(Debug, Deserialize)]
pub struct TunedEntry {
    /// Catalogue key of the product.
    pub product: String,
    /// The printer preset the values are for, e.g. `Bambu Lab X2D 0.4 nozzle`.
    pub printer: String,
    pub settings: BTreeMap<String, String>,
}

const FILES: &[&str] = &[include_str!("../data/tuned/siraya_tech.json")];

fn sets() -> &'static [TunedSet] {
    static SETS: OnceLock<Vec<TunedSet>> = OnceLock::new();
    SETS.get_or_init(|| {
        FILES.iter().map(|text| serde_json::from_str(text).expect("data/tuned files are valid")).collect()
    })
}

/// The manufacturer's settings for a product on a printer preset, if it published any.
pub fn tuned_for(product: &str, printer: &str) -> Option<(&'static TunedSet, &'static TunedEntry)> {
    sets().iter().find_map(|set| {
        set.entries.iter().find(|entry| entry.product == product && entry.printer == printer).map(|entry| (set, entry))
    })
}

/// Settings that depend on the nozzle's size, so they are not carried to another nozzle.
const NOZZLE_BOUND: &[&str] = &["filament_max_volumetric_speed"];

/// The part of a preset name before the nozzle size: `Bambu Lab X2D 0.4 nozzle` gives `Bambu Lab X2D`.
fn model(printer: &str) -> &str {
    let Some(rest) = printer.strip_suffix(" nozzle") else { return printer };
    rest.rsplit_once(' ').map_or(printer, |(model, _)| model)
}

/// Every printer preset of the same model the manufacturer published this product for.
pub fn published_for(product: &str, printer: &str) -> Vec<&'static str> {
    let wanted = model(printer);
    sets()
        .iter()
        .flat_map(|set| &set.entries)
        .filter(|entry| entry.product == product && model(&entry.printer) == wanted)
        .map(|entry| entry.printer.as_str())
        .collect()
}

/// For a nozzle the manufacturer published nothing for: its settings for another nozzle
/// of the same printer, the 0.4 mm one if there is one.
pub fn borrowed_for(product: &str, printer: &str) -> Option<(&'static TunedSet, &'static TunedEntry)> {
    if tuned_for(product, printer).is_some() {
        return None;
    }
    let published = published_for(product, printer);
    let from = published.iter().find(|name| name.ends_with(" 0.4 nozzle")).or(published.first())?;
    tuned_for(product, from)
}

/// As [`apply`], for a nozzle other than the one the values were published for: the
/// settings that depend on nozzle size keep the base preset's values.
pub fn apply_borrowed(settings: &mut BTreeMap<String, Value>, entry: &TunedEntry) {
    apply_where(settings, entry, |key| !NOZZLE_BOUND.contains(&key));
}

/// Lays the tuned values over `settings`. Only settings the base preset already has
/// are touched. A manufacturer tunes the printer's standard nozzle: the first slot of
/// a setting takes the value, and so do the other slots where the base preset made no
/// difference between nozzle variants. Where it did, those keep the base's values.
pub fn apply(settings: &mut BTreeMap<String, Value>, entry: &TunedEntry) {
    apply_where(settings, entry, |_| true);
}

fn apply_where(settings: &mut BTreeMap<String, Value>, entry: &TunedEntry, wanted: impl Fn(&str) -> bool) {
    for (key, value) in entry.settings.iter().filter(|(key, _)| wanted(key)) {
        let Some(Value::Array(items)) = settings.get_mut(key) else { continue };
        let Some(first) = items.first().cloned() else { continue };
        if !first.is_string() {
            continue;
        }
        for item in items.iter_mut().filter(|item| **item == first) {
            *item = Value::String(value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_data_file_loads_and_holds_only_plain_numbers() {
        for set in sets() {
            assert!(set.source.starts_with("https://") && !set.entries.is_empty(), "{}", set.brand);
            for entry in &set.entries {
                for (key, value) in &entry.settings {
                    assert!(!key.contains("gcode"), "{key}");
                    assert!(value.parse::<f64>().is_ok(), "{} {key} = {value}", entry.product);
                }
            }
        }
    }

    #[test]
    fn tuned_values_replace_the_standard_nozzle_and_leave_other_variants() {
        let (_, entry) = tuned_for("siraya_tech/TPU/flex_tpu_85a", "Bambu Lab X2D 0.4 nozzle").unwrap();
        let mut settings = BTreeMap::from([
            ("nozzle_temperature".to_string(), serde_json::json!(["220", "220"])),
            ("filament_max_volumetric_speed".to_string(), serde_json::json!(["3.2", "5"])),
        ]);
        apply(&mut settings, entry);
        assert_eq!(settings["nozzle_temperature"], serde_json::json!(["240", "240"]));
        assert_eq!(settings["filament_max_volumetric_speed"], serde_json::json!(["9", "5"]));
        // A setting the base preset lacks is not added.
        assert!(!settings.contains_key("filament_flow_ratio"));
        assert!(tuned_for("siraya_tech/TPU/flex_tpu_85a", "Some Other Printer 0.4 nozzle").is_none());
    }

    #[test]
    fn another_nozzle_can_borrow_everything_but_the_flow_limit() {
        let product = "siraya_tech/TPU/flex_tpu_85a";
        assert!(published_for(product, "Bambu Lab X2D 0.8 nozzle").contains(&"Bambu Lab X2D 0.4 nozzle"));
        assert!(borrowed_for(product, "Bambu Lab X2D 0.4 nozzle").is_none(), "it has its own");
        assert!(borrowed_for(product, "Some Other Printer 0.4 nozzle").is_none());
        let Some((_, entry)) = borrowed_for(product, "Bambu Lab X2D 0.25 nozzle") else { panic!("nothing to borrow") };
        let mut settings = BTreeMap::from([
            ("nozzle_temperature".to_string(), serde_json::json!(["220"])),
            ("filament_max_volumetric_speed".to_string(), serde_json::json!(["2"])),
        ]);
        apply_borrowed(&mut settings, entry);
        assert_eq!(settings["nozzle_temperature"], serde_json::json!(["240"]));
        assert_eq!(settings["filament_max_volumetric_speed"], serde_json::json!(["2"]));
    }
}
