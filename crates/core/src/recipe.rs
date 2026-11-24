//! A profile recorded as its parent plus what changed, instead of as a flat file.
//! This is what lets the tool say where each value came from, and later share,
//! compare and rebuild profiles. The flat profile the slicer needs is derived from it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who a change comes from. Later layers override earlier ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// The product's data from its manufacturer, via the catalogue.
    Manufacturer,
    /// One colour of the product.
    Colour,
    /// Settings another person shared.
    Community,
    /// The user's own settings.
    You,
}

/// One setting that differs from the parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub key: String,
    pub layer: Layer,
    /// The parent's value; `None` if the parent does not set this key.
    pub from: Option<Value>,
    pub to: Value,
}

/// A profile as "this parent, with these changes".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    /// The system preset the profile is built on.
    pub parent: String,
    /// Sorted by key.
    pub changes: Vec<Change>,
}

impl Recipe {
    /// Records how `settings` differ from `parent_settings`. Keys the parent sets
    /// but `settings` lacks are not changes: they are fields a root profile stores
    /// elsewhere or never writes.
    pub fn between(
        parent: &str,
        parent_settings: &BTreeMap<String, Value>,
        settings: &BTreeMap<String, Value>,
        layer_of: impl Fn(&str) -> Layer,
    ) -> Self {
        let changes = settings
            .iter()
            .filter(|(key, value)| parent_settings.get(*key) != Some(*value))
            .map(|(key, value)| Change {
                key: key.clone(),
                layer: layer_of(key),
                from: parent_settings.get(key).cloned(),
                to: value.clone(),
            })
            .collect();
        Self { parent: parent.to_string(), changes }
    }

    /// The parent's settings with the changes applied.
    pub fn apply(&self, parent_settings: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
        let mut settings = parent_settings.clone();
        for change in &self.changes {
            settings.insert(change.key.clone(), change.to.clone());
        }
        settings
    }
}

/// The layer a catalogue-driven change belongs to.
pub fn catalogue_layer(key: &str) -> Layer {
    if key == "default_filament_colour" {
        Layer::Colour
    } else {
        Layer::Manufacturer
    }
}
