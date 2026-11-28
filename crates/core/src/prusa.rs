//! PrusaSlicer's side of things: its INI files, the vendor bundles its system presets
//! come in, and the conditions that tie a filament preset to printers.
//!
//! PrusaSlicer keeps a filament as one flat `.ini` in `filament/`, named after the
//! preset. Unlike Bambu Studio it has no file per printer: a preset carries a
//! `compatible_printers_condition`, an expression over the printer's settings, and shows
//! for every printer the expression holds for.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use regex::Regex;
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::slicer::{AppConf, ConfiguredModel};

/// The bundle of Prusa's own printers. Loaded first, so its preset names win.
const HOME_VENDOR: &str = "PrusaResearch";
/// What PrusaSlicer appends to the names of presets from a templates bundle.
pub const TEMPLATE_SUFFIX: &str = " @Template";
/// Separates a preset's name from its bundle in the key of a preset only that bundle can see.
const SCOPE: char = '\u{1}';

/// One line of an INI file.
enum Line<'a> {
    Section(&'a str),
    Pair(&'a str, &'a str),
}

/// The sections and `key = value` lines of an INI file, in order. Values are kept exactly
/// as written (quotes and escapes included), so they can be written back unchanged.
fn lines(text: &str) -> impl Iterator<Item = Line<'_>> {
    text.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            return None;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            return Some(Line::Section(section.trim()));
        }
        let (key, value) = line.split_once('=')?;
        Some(Line::Pair(key.trim(), value.trim()))
    })
}

/// A flat preset file: every `key = value` line before any section.
pub fn parse_flat(text: &str) -> Map<String, Value> {
    let mut found = Map::new();
    for line in lines(text) {
        match line {
            Line::Section(_) => break,
            Line::Pair(key, value) => {
                found.insert(key.to_string(), Value::String(value.to_string()));
            }
        }
    }
    found
}

/// The values of a list setting such as `compatible_printers`: `"A B";C` is `A B` and `C`.
pub fn split_list(raw: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut any = false;
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            '\\' if quoted => current.extend(chars.next()),
            ';' if !quoted => {
                if any || !current.trim().is_empty() {
                    values.push(current.trim().to_string());
                }
                current.clear();
                any = false;
            }
            c => current.push(c),
        }
    }
    if any || !current.trim().is_empty() {
        values.push(current.trim().to_string());
    }
    values
}

/// `PrusaSlicer.ini`: the selected printer, and the printer models added from each vendor.
pub(crate) fn parse_app_conf(text: &str) -> AppConf {
    let mut conf = AppConf::default();
    let mut section = "";
    for line in lines(text) {
        match line {
            Line::Section(name) => section = name,
            Line::Pair("printer", value) if section == "presets" && !value.is_empty() => {
                conf.selected_machine = Some(value.to_string());
            }
            Line::Pair(key, value) if section.starts_with("vendor:") => {
                if let Some(model) = key.strip_prefix("model:") {
                    conf.models.push(ConfiguredModel { model: model.to_string(), nozzles: split_list(value) });
                }
            }
            Line::Pair(..) => {}
        }
    }
    conf
}

/// A printer preset as the library lists it.
#[derive(Debug, Clone)]
pub(crate) struct Printer {
    pub name: String,
    /// The model's id in its bundle, such as `MK3S`. Empty for a preset the user made.
    pub model_id: String,
    /// The model as PrusaSlicer shows it.
    pub model: String,
    /// The nozzle variant, such as `0.4` or `HF0.4`.
    pub nozzle: String,
    /// The user's own preset, from `printer/`.
    pub user: bool,
}

/// Everything read from the vendor bundles and the user's printer presets.
#[derive(Debug, Default)]
pub(crate) struct Loaded {
    pub version: String,
    /// Filament presets by name. `inherits` holds a list of the keys of its parents.
    pub filaments: HashMap<String, Map<String, Value>>,
    pub printers: Vec<Printer>,
    /// Printer presets by name, shaped like `filaments`.
    pub printer_presets: HashMap<String, Map<String, Value>>,
}

/// Reads `vendor/*.ini` and `printer/*.ini` under PrusaSlicer's settings folder.
pub(crate) fn load(config_dir: &Path) -> Result<Loaded> {
    let mut loaded = Loaded::default();
    let vendor_dir = config_dir.join("vendor");
    let mut bundles: Vec<std::path::PathBuf> = ini_files(&vendor_dir)?;
    // Prusa's own bundle first: where two bundles name a preset alike, the first one stands.
    bundles.sort_by_key(|path| (path.file_stem().is_none_or(|stem| stem != HOME_VENDOR), path.clone()));
    for path in bundles {
        let text = fs::read_to_string(&path).map_err(Error::read(&path))?;
        let vendor = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
        add_bundle(&mut loaded, &vendor, &text);
    }

    for path in ini_files(&config_dir.join("printer"))? {
        // An unreadable preset is the slicer's business; skip rather than fail.
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Some(name) = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()) else { continue };
        let mut preset = parse_flat(&text);
        // A saved preset is complete in itself; the preset it was made from is only a note.
        preset.remove("inherits");
        if text_of(&preset, "printer_technology").is_some_and(|technology| technology != "FFF") {
            continue;
        }
        let nozzle = text_of(&preset, "printer_variant")
            .filter(|variant| !variant.is_empty())
            .or_else(|| text_of(&preset, "nozzle_diameter").and_then(|all| all.split(',').next()))
            .unwrap_or("0.4")
            .trim()
            .to_string();
        loaded.printers.retain(|printer| printer.name != name);
        loaded.printers.push(Printer { name: name.clone(), model_id: String::new(), model: name.clone(), nozzle, user: true });
        loaded.printer_presets.insert(name, preset);
    }
    loaded.printers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(loaded)
}

fn ini_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir).map_err(Error::read(dir))? {
            let path = entry.map_err(Error::read(dir))?.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "ini") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn text_of<'a>(preset: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    preset.get(key).and_then(Value::as_str)
}

/// Whether a preset name is one of a bundle's building blocks (`*common*`), which
/// presets inherit from and nobody selects.
fn is_abstract(name: &str) -> bool {
    name.starts_with('*')
}

fn add_bundle(loaded: &mut Loaded, vendor: &str, text: &str) {
    let mut vendor_section = Map::new();
    let mut models: Vec<(String, Map<String, Value>)> = Vec::new();
    let mut filaments: Vec<(String, Map<String, Value>)> = Vec::new();
    let mut printers: Vec<(String, Map<String, Value>)> = Vec::new();
    // Where the lines being read go: the list, and the index in it.
    let mut target: Option<(u8, usize)> = None;
    for line in lines(text) {
        match line {
            Line::Section(section) => {
                let (kind, name) = section.split_once(':').unwrap_or((section, ""));
                let name = name.trim().to_string();
                target = match kind.trim() {
                    "vendor" => Some((0, 0)),
                    "printer_model" => {
                        models.push((name, Map::new()));
                        Some((1, models.len() - 1))
                    }
                    "filament" => {
                        filaments.push((name, Map::new()));
                        Some((2, filaments.len() - 1))
                    }
                    "printer" => {
                        printers.push((name, Map::new()));
                        Some((3, printers.len() - 1))
                    }
                    _ => None,
                };
            }
            Line::Pair(key, value) => {
                let value = Value::String(value.to_string());
                match target {
                    Some((0, _)) => drop(vendor_section.insert(key.to_string(), value)),
                    Some((1, index)) => drop(models[index].1.insert(key.to_string(), value)),
                    Some((2, index)) => drop(filaments[index].1.insert(key.to_string(), value)),
                    Some((3, index)) => drop(printers[index].1.insert(key.to_string(), value)),
                    _ => {}
                }
            }
        }
    }

    let templates = text_of(&vendor_section, "templates_profile") == Some("1");
    if loaded.version.is_empty() {
        loaded.version = text_of(&vendor_section, "config_version").unwrap_or_default().to_string();
    }
    // A preset anyone can select goes by its name; a building block only by name and bundle.
    let key = |name: &str| {
        if is_abstract(name) {
            format!("{name}{SCOPE}{vendor}")
        } else if templates {
            format!("{name}{TEMPLATE_SUFFIX}")
        } else {
            name.to_string()
        }
    };
    let keyed = |mut preset: Map<String, Value>| {
        let parents: Vec<Value> = text_of(&preset, "inherits")
            .map(|parents| split_list(parents).iter().map(|parent| Value::String(key(parent))).collect())
            .unwrap_or_default();
        preset.insert("inherits".to_string(), Value::Array(parents));
        preset
    };

    for (name, preset) in filaments {
        loaded.filaments.entry(key(&name)).or_insert_with(|| keyed(preset));
    }

    let model_names: HashMap<&str, String> = models
        .iter()
        .map(|(id, model)| (id.as_str(), text_of(model, "name").unwrap_or(id).replace("&&", "&")))
        .collect();
    let mut added = Vec::new();
    for (name, preset) in printers {
        let key = key(&name);
        if loaded.printer_presets.contains_key(&key) {
            continue;
        }
        loaded.printer_presets.insert(key.clone(), keyed(preset));
        if !is_abstract(&name) {
            added.push(key);
        }
    }
    for name in added {
        let find = |key: &str| lookup(&loaded.printer_presets, &name, key).map(str::to_string);
        if find("printer_technology").is_some_and(|technology| technology != "FFF") {
            continue;
        }
        let model_id = find("printer_model").unwrap_or_default();
        let Some(nozzle) = find("printer_variant").filter(|variant| !variant.is_empty()) else { continue };
        let model = model_names.get(model_id.as_str()).cloned().unwrap_or_else(|| model_id.clone());
        loaded.printers.push(Printer { name, model_id, model, nozzle, user: false });
    }
}

/// A preset's value for `key`: its own, or the one it inherits. Later parents override earlier ones.
pub(crate) fn lookup<'a>(presets: &'a HashMap<String, Map<String, Value>>, name: &str, key: &str) -> Option<&'a str> {
    fn walk<'a>(presets: &'a HashMap<String, Map<String, Value>>, name: &str, key: &str, depth: usize) -> Option<&'a str> {
        if depth > 32 {
            return None;
        }
        let preset = presets.get(name)?;
        if let Some(own) = preset.get(key).and_then(Value::as_str) {
            return Some(own);
        }
        let parents = preset.get("inherits")?.as_array()?;
        parents.iter().rev().filter_map(Value::as_str).find_map(|parent| walk(presets, parent, key, depth + 1))
    }
    walk(presets, name, key, 0)
}

/// What a condition can ask of a printer: its settings by name.
pub(crate) struct PrinterView<'a> {
    pub presets: &'a HashMap<String, Map<String, Value>>,
    pub name: &'a str,
}

impl PrinterView<'_> {
    fn setting(&self, key: &str) -> Option<String> {
        if key == "num_extruders" {
            let nozzles = self.setting("nozzle_diameter")?;
            return Some(nozzles.split(',').count().to_string());
        }
        lookup(self.presets, self.name, key).map(str::to_string).or_else(|| {
            // PrusaSlicer's own defaults for the settings conditions ask about.
            let default = match key {
                "printer_notes" | "printer_model" | "printer_variant" | "printer_vendor" => "",
                "nozzle_diameter" => "0.4",
                "single_extruder_multi_material" | "nozzle_high_flow" => "0",
                _ => return None,
            };
            Some(default.to_string())
        })
    }
}

/// Whether a filament preset shows for a printer, by PrusaSlicer's rule: a list of printer
/// names decides if there is one; otherwise the condition does; with neither it shows for all.
pub(crate) fn compatible(list: &str, condition: &str, printer: &PrinterView) -> bool {
    let names = split_list(list);
    if !names.is_empty() {
        return names.iter().any(|name| name == printer.name);
    }
    if condition.trim().is_empty() {
        return true;
    }
    // PrusaSlicer treats a condition it cannot work out as "compatible with everything".
    evaluate(condition, printer).unwrap_or(true)
}

/// Whether a condition can be worked out for a printer at all, rather than falling back
/// to "compatible with everything". For checking the tool against a slicer's own presets.
pub fn understood(condition: &str, library: &crate::system::SystemLibrary, printer: &str) -> bool {
    evaluate(condition, &library.printer_view(printer)).is_some()
}

/// A value inside a condition: its text, and the number it is when it is one.
#[derive(Debug, Clone, PartialEq)]
struct Operand {
    text: String,
    number: Option<f64>,
}

impl Operand {
    fn truth(value: bool) -> Self {
        Self { text: if value { "1" } else { "0" }.to_string(), number: Some(if value { 1.0 } else { 0.0 }) }
    }

    fn read(text: &str) -> Self {
        Self { number: text.trim().parse().ok(), text: text.trim().to_string() }
    }

    fn is_true(&self) -> bool {
        match self.number {
            Some(number) => number != 0.0,
            None => self.text == "true",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Number(String),
    Text(String),
    Pattern(String),
    Symbol(&'static str),
}

fn tokens(source: &str) -> Option<Vec<Token>> {
    const SYMBOLS: &[&str] = &["&&", "||", "==", "!=", "<>", "<=", ">=", "=~", "!~", "(", ")", "[", "]", "!", "<", ">"];
    let chars: Vec<char> = source.chars().collect();
    let mut found = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c.is_whitespace() {
            at += 1;
        } else if c == '"' || (c == '/' && matches!(found.last(), Some(Token::Symbol("=~" | "!~")))) {
            let mut text = String::new();
            at += 1;
            loop {
                match chars.get(at)? {
                    '\\' if chars.get(at + 1) == Some(&c) => {
                        text.push(c);
                        at += 2;
                    }
                    end if *end == c => break,
                    other => {
                        text.push(*other);
                        at += 1;
                    }
                }
            }
            at += 1;
            found.push(if c == '"' { Token::Text(text) } else { Token::Pattern(text) });
        } else if c.is_ascii_digit() || (c == '.' && chars.get(at + 1).is_some_and(char::is_ascii_digit)) {
            let start = at;
            while at < chars.len() && (chars[at].is_ascii_digit() || chars[at] == '.') {
                at += 1;
            }
            found.push(Token::Number(chars[start..at].iter().collect()));
        } else if c.is_alphabetic() || c == '_' {
            let start = at;
            while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_') {
                at += 1;
            }
            found.push(Token::Word(chars[start..at].iter().collect()));
        } else {
            let rest: String = chars[at..chars.len().min(at + 2)].iter().collect();
            let symbol = SYMBOLS.iter().find(|symbol| rest.starts_with(**symbol))?;
            found.push(Token::Symbol(symbol));
            at += symbol.len();
        }
    }
    Some(found)
}

/// Works out a condition for a printer. `None` when it cannot be read or asks for a
/// setting nobody knows.
pub(crate) fn evaluate(condition: &str, printer: &PrinterView) -> Option<bool> {
    let tokens = tokens(condition)?;
    let mut parser = Parser { tokens: &tokens, at: 0, printer };
    let value = parser.either()?;
    (parser.at == tokens.len()).then(|| value.is_true())
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    printer: &'a PrinterView<'a>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn word(&self, wanted: &str) -> bool {
        matches!(self.peek(), Some(Token::Word(word)) if word == wanted)
    }

    fn symbol(&self, wanted: &str) -> bool {
        matches!(self.peek(), Some(Token::Symbol(symbol)) if *symbol == wanted)
    }

    fn either(&mut self) -> Option<Operand> {
        let mut left = self.both()?;
        while self.word("or") || self.symbol("||") {
            self.at += 1;
            let right = self.both()?;
            left = Operand::truth(left.is_true() || right.is_true());
        }
        Some(left)
    }

    fn both(&mut self) -> Option<Operand> {
        let mut left = self.negated()?;
        while self.word("and") || self.symbol("&&") {
            self.at += 1;
            let right = self.negated()?;
            left = Operand::truth(left.is_true() && right.is_true());
        }
        Some(left)
    }

    fn negated(&mut self) -> Option<Operand> {
        if self.word("not") || self.symbol("!") {
            self.at += 1;
            let value = self.negated()?;
            return Some(Operand::truth(!value.is_true()));
        }
        self.compared()
    }

    fn compared(&mut self) -> Option<Operand> {
        let left = self.operand()?;
        let Some(Token::Symbol(symbol)) = self.peek().cloned() else { return Some(left) };
        if !matches!(symbol, "==" | "!=" | "<>" | "<" | ">" | "<=" | ">=" | "=~" | "!~") {
            return Some(left);
        }
        self.at += 1;
        if matches!(symbol, "=~" | "!~") {
            let Some(Token::Pattern(pattern)) = self.peek().cloned() else { return None };
            self.at += 1;
            let matched = matches_whole(&pattern, &left.text)?;
            return Some(Operand::truth(matched == (symbol == "=~")));
        }
        let right = self.operand()?;
        let result = match (symbol, left.number, right.number) {
            ("==", Some(a), Some(b)) => a == b,
            ("!=" | "<>", Some(a), Some(b)) => a != b,
            ("<", Some(a), Some(b)) => a < b,
            (">", Some(a), Some(b)) => a > b,
            ("<=", Some(a), Some(b)) => a <= b,
            (">=", Some(a), Some(b)) => a >= b,
            ("==", ..) => left.text == right.text,
            ("!=" | "<>", ..) => left.text != right.text,
            _ => return None,
        };
        Some(Operand::truth(result))
    }

    fn operand(&mut self) -> Option<Operand> {
        match self.peek()?.clone() {
            Token::Symbol("(") => {
                self.at += 1;
                let value = self.either()?;
                self.symbol(")").then(|| self.at += 1)?;
                Some(value)
            }
            Token::Number(number) => {
                self.at += 1;
                Some(Operand::read(&number))
            }
            Token::Text(text) => {
                self.at += 1;
                // Text in quotes is text, even when it reads like a number.
                Some(Operand { text, number: None })
            }
            Token::Word(word) => {
                self.at += 1;
                match word.as_str() {
                    "true" => return Some(Operand::truth(true)),
                    "false" => return Some(Operand::truth(false)),
                    _ => {}
                }
                let raw = self.printer.setting(&word)?;
                if self.symbol("[") {
                    self.at += 1;
                    let index = self.either()?.number? as usize;
                    self.symbol("]").then(|| self.at += 1)?;
                    // A list with fewer values than asked for repeats its last one.
                    let values: Vec<&str> = raw.split(',').collect();
                    return Some(Operand::read(values.get(index).or(values.last())?));
                }
                Some(Operand::read(&raw))
            }
            _ => None,
        }
    }
}

/// PrusaSlicer matches a pattern against the whole text, not anywhere in it.
fn matches_whole(pattern: &str, text: &str) -> Option<bool> {
    static COMPILED: Mutex<Option<HashMap<String, Option<Regex>>>> = Mutex::new(None);
    let mut compiled = COMPILED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let regex = compiled
        .get_or_insert_with(HashMap::new)
        .entry(pattern.to_string())
        .or_insert_with(|| Regex::new(&format!("(?s)^(?:{pattern})$")).ok())
        .as_ref()?;
    Some(regex.is_match(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printer(pairs: &[(&str, &str)]) -> HashMap<String, Map<String, Value>> {
        let preset = pairs.iter().map(|(key, value)| (key.to_string(), Value::String(value.to_string()))).collect();
        HashMap::from([("Test".to_string(), preset)])
    }

    #[test]
    fn conditions_read_the_printer_the_way_prusaslicer_does() {
        let presets = printer(&[
            ("printer_model", "MK4S"),
            ("printer_notes", r"Keep these.\nPRINTER_VENDOR_PRUSA3D\nPRINTER_MODEL_MK4S\nPG"),
            ("nozzle_diameter", "0.4"),
            ("nozzle_high_flow", "1"),
        ]);
        let view = PrinterView { presets: &presets, name: "Test" };
        let holds = |condition: &str| evaluate(condition, &view);

        assert_eq!(holds(r"printer_notes=~/.*PRINTER_MODEL_MK4S.*/ and nozzle_diameter[0]==0.4"), Some(true));
        // A pattern has to match the whole value: MK4 is not MK4S.
        assert_eq!(holds("printer_model=~/(MK4|MK4IS)/"), Some(false));
        assert_eq!(holds("printer_model=~/(MK4S|MK4SMMU3)/and nozzle_high_flow[0]"), Some(true));
        assert_eq!(holds(r#"printer_model!="MK3.5" and ! single_extruder_multi_material"#), Some(true));
        assert_eq!(holds("! (printer_notes=~/.*PG.*/ and nozzle_diameter[0]>=0.6) && num_extruders==1"), Some(true));
        assert_eq!(holds("nozzle_diameter[0]!=0.8 and printer_notes!~/.*PG.*/"), Some(false));
        assert_eq!(holds("no_such_setting == 1"), None);
        assert_eq!(holds("nozzle_diameter[0] =="), None);

        assert!(compatible("", "", &view));
        assert!(compatible("", "no_such_setting == 1", &view), "an unreadable condition hides nothing");
        assert!(compatible("\"Other\";Test", "false", &view), "a list of names is all that counts when there is one");
        assert!(!compatible("Other", "true", &view));
    }

    #[test]
    fn lists_split_on_semicolons_outside_quotes() {
        assert_eq!(split_list(r#""MK3S & MK3S+";Plain; "a;b""#), ["MK3S & MK3S+", "Plain", "a;b"]);
        assert_eq!(split_list("0.4; 0.25;HF0.4"), ["0.4", "0.25", "HF0.4"]);
        assert!(split_list("").is_empty());
        assert_eq!(split_list(r#""""#), [""]);
    }

    #[test]
    fn the_app_settings_give_the_selected_printer_and_the_added_models() {
        let conf = parse_app_conf(
            "version = 2.9.7\n\n[presets]\nfilament = Prusament PLA\nprinter = Original Prusa i3 MK3S & MK3S+\n\n[vendor:PrusaResearch]\nmodel:MK3S = 0.4;0.6\nmodel:MK4S = HF0.4\n",
        );
        assert_eq!(conf.selected_machine.as_deref(), Some("Original Prusa i3 MK3S & MK3S+"));
        assert_eq!(conf.models.len(), 2);
        assert_eq!(conf.models[0].nozzles, ["0.4", "0.6"]);
        assert!(conf.user_id().is_none() && !conf.uploads_new_presets());
    }
}
