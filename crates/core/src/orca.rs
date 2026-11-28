//! OrcaSlicer's system presets.
//!
//! Up to 2.4 OrcaSlicer kept them the way Bambu Studio does: a folder of JSON files for
//! each vendor in `system/`. From 2.5 each vendor is one packed file, `system/<vendor>.opc`.
//! Both are read here into the same shape, a JSON object per preset as the files had it.
//!
//! The packed format is OrcaSlicer's `PresetCacheFormat.cpp`: a 20 byte header (magic,
//! format version, body size, CRC-32 of the body) and a body of little-endian values, with
//! every length a 64 bit count. The body names its settings once, in a dictionary, and
//! each preset then refers to them by number.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::system::{json_files, normalize_version, read_object, MachinePreset};

/// The vendor whose filaments are offered to every printer and built on by other vendors.
const LIBRARY_VENDOR: &str = "OrcaFilamentLibrary";
/// Tried first for the version a profile is stamped with when its base preset names none.
const MAIN_VENDOR: &str = "BBL";

const MAGIC: u32 = 0x4F52_435A; // "ORCZ"
const FORMAT_VERSION: u32 = 2;
const HEADER_LEN: usize = 20;

/// Set on the type of a setting that holds a list.
const LIST: u16 = 0x4000;
const ENUM: u16 = 9;

/// One vendor's presets, each as the JSON object its file held.
struct Vendor {
    name: String,
    filaments: Vec<Map<String, Value>>,
    machines: Vec<Map<String, Value>>,
}

pub(crate) struct Loaded {
    pub version: String,
    /// Presets that can be chosen are under their own name. The ones others are built on
    /// are under [`scoped`] names, since two vendors may each have one of the same name.
    /// `inherits` and `include` already hold the name their preset is stored under.
    pub filaments: HashMap<String, Map<String, Value>>,
    pub machines: Vec<MachinePreset>,
}

fn scoped(name: &str, vendor: &str) -> String {
    format!("{name}\u{1}{vendor}")
}

/// Reads every vendor in a `system` folder.
pub(crate) fn load(system_dir: &Path) -> Result<Loaded> {
    let mut vendors = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(system_dir)
        .map_err(Error::read(system_dir))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for path in &entries {
        let Some(name) = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()) else { continue };
        if path.extension().is_some_and(|ext| ext == "opc") {
            vendors.push(read_cache(path, &name)?);
        } else if path.is_dir() && path.with_extension("json").is_file() && !path.with_extension("opc").is_file() {
            vendors.push(read_folder(path, &name)?);
        }
    }
    if vendors.is_empty() {
        return Err(Error::Other(format!("no system presets in {}", system_dir.display())));
    }
    // The library last: where two vendors have a filament of one name, a printer's own vendor wins.
    vendors.sort_by_key(|vendor| vendor.name == LIBRARY_VENDOR);

    let stored_as = |vendor: &Vendor| -> HashMap<String, String> {
        vendor
            .filaments
            .iter()
            .filter_map(|preset| {
                let name = preset.get("name")?.as_str()?;
                let key = if selectable(preset) { name.to_string() } else { scoped(name, &vendor.name) };
                Some((name.to_string(), key))
            })
            .collect()
    };
    let library = vendors.iter().find(|vendor| vendor.name == LIBRARY_VENDOR).map(stored_as).unwrap_or_default();

    let mut version = None;
    let mut filaments = HashMap::new();
    let mut machines = Vec::new();
    for vendor in vendors {
        let own = stored_as(&vendor);
        let stamp = vendor.filaments.iter().chain(&vendor.machines).find_map(|preset| preset.get("version")?.as_str());
        if let Some(stamp) = stamp {
            if vendor.name == MAIN_VENDOR || version.is_none() {
                version = Some(normalize_version(stamp));
            }
        }
        for mut preset in vendor.filaments {
            let Some(key) = preset.get("name").and_then(Value::as_str).and_then(|name| own.get(name)).cloned() else {
                continue;
            };
            // A parent is the vendor's own preset of that name, or else the library's.
            let stored = |name: &str| own.get(name).or_else(|| library.get(name)).cloned().unwrap_or_else(|| name.to_string());
            if let Some(parent) = preset.get("inherits").and_then(Value::as_str).filter(|parent| !parent.is_empty()) {
                let parent = stored(parent);
                preset.insert("inherits".into(), Value::String(parent));
            }
            let includes: Vec<Value> = match preset.get("include") {
                Some(Value::String(one)) => vec![Value::String(stored(one))],
                Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).map(|name| Value::String(stored(name))).collect(),
                _ => Vec::new(),
            };
            if !includes.is_empty() {
                preset.insert("include".into(), Value::Array(includes));
            }
            filaments.entry(key).or_insert(preset);
        }
        for preset in &vendor.machines {
            let text = |key: &str| preset.get(key).and_then(Value::as_str);
            if !selectable(preset) {
                continue;
            }
            if let (Some(name), Some(model), Some(nozzle)) = (text("name"), text("printer_model"), text("printer_variant")) {
                machines.push(MachinePreset {
                    name: name.to_string(),
                    model: model.to_string(),
                    nozzle: nozzle.to_string(),
                    model_id: model.to_string(),
                    user: false,
                });
            }
        }
    }
    machines.sort_by(|a, b| a.name.cmp(&b.name));
    machines.dedup_by(|a, b| a.name == b.name);

    let version = version.ok_or_else(|| Error::Other(format!("the presets in {} have no version", system_dir.display())))?;
    Ok(Loaded { version, filaments, machines })
}

fn selectable(preset: &Map<String, Value>) -> bool {
    preset.get("instantiation").and_then(Value::as_str) == Some("true")
}

/// A vendor kept the old way: `<vendor>.json` beside a folder of JSON presets.
fn read_folder(dir: &Path, name: &str) -> Result<Vendor> {
    let index = read_object(&dir.with_extension("json"))?;
    let version = index.get("version").and_then(Value::as_str).unwrap_or_default().to_string();
    let read = |folder: &str, kind: &str| -> Result<Vec<Map<String, Value>>> {
        let folder = dir.join(folder);
        let mut presets = Vec::new();
        if !folder.is_dir() {
            return Ok(presets);
        }
        for path in json_files(&folder)? {
            let mut preset = read_object(&path)?;
            // The folders also hold lookup tables, which have no name or are of no type.
            let named = preset.get("name").is_some_and(Value::is_string);
            if !named || preset.get("type").and_then(Value::as_str).is_some_and(|found| found != kind) {
                continue;
            }
            if !version.is_empty() {
                preset.entry("version").or_insert_with(|| Value::String(version.clone()));
            }
            presets.push(preset);
        }
        Ok(presets)
    };
    Ok(Vendor { name: name.to_string(), filaments: read("filament", "filament")?, machines: read("machine", "machine")? })
}

/// A vendor packed into one `.opc` file.
fn read_cache(path: &Path, name: &str) -> Result<Vendor> {
    let unreadable = |why: &str| {
        Error::Other(format!(
            "OrcaSlicer's presets in {} can't be read ({why}). A newer Toona may be needed for this OrcaSlicer.",
            path.display()
        ))
    };
    let bytes = fs::read(path).map_err(Error::read(path))?;
    if bytes.len() <= HEADER_LEN {
        return Err(unreadable("the file is too short"));
    }
    let (header, body) = bytes.split_at(HEADER_LEN);
    let mut header = Reader { bytes: header, at: 0 };
    let truncated = |_| unreadable("the file is cut short");
    let (magic, format, size, crc) =
        (header.u32().map_err(truncated)?, header.u32().map_err(truncated)?, header.u64().map_err(truncated)?, header.u32().map_err(truncated)?);
    if magic != MAGIC {
        return Err(unreadable("it is not a preset file"));
    }
    if format != FORMAT_VERSION {
        return Err(unreadable(&format!("format {format}, where {FORMAT_VERSION} is known")));
    }
    let mut check = flate2::Crc::new();
    check.update(body);
    if size != body.len() as u64 || check.sum() != crc {
        return Err(unreadable("the file is damaged"));
    }
    parse_body(body, name).map_err(|Truncated| unreadable("its contents are not laid out as expected"))
}

/// Reading ran past the end of the file, or met something that cannot be there.
struct Truncated;

type Read<T> = std::result::Result<T, Truncated>;

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Read<&'a [u8]> {
        let end = self.at.checked_add(len).filter(|end| *end <= self.bytes.len()).ok_or(Truncated)?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn array<const N: usize>(&mut self) -> Read<[u8; N]> {
        self.take(N)?.try_into().map_err(|_| Truncated)
    }

    fn u8(&mut self) -> Read<u8> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Read<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Read<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn i32(&mut self) -> Read<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Read<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn f64(&mut self) -> Read<f64> {
        Ok(f64::from_le_bytes(self.array()?))
    }

    /// A count of things that each take at least `each` bytes, so a wild one is caught
    /// before anything is set aside for it.
    fn count(&mut self, each: usize) -> Read<usize> {
        let count = usize::try_from(self.u64()?).map_err(|_| Truncated)?;
        match count.checked_mul(each.max(1)) {
            Some(needed) if needed <= self.bytes.len() - self.at => Ok(count),
            _ => Err(Truncated),
        }
    }

    fn string(&mut self) -> Read<String> {
        let len = self.count(1)?;
        Ok(String::from_utf8_lossy(self.take(len)?).into_owned())
    }

    fn list<T>(&mut self, each: usize, mut one: impl FnMut(&mut Self) -> Read<T>) -> Read<Vec<T>> {
        let count = self.count(each)?;
        (0..count).map(|_| one(self)).collect()
    }

    fn strings(&mut self) -> Read<Vec<String>> {
        self.list(8, Self::string)
    }

    fn skip_strings(&mut self, count: usize) -> Read<()> {
        (0..count).try_for_each(|_| self.string().map(drop))
    }
}

fn parse_body(body: &[u8], name: &str) -> Read<Vendor> {
    let mut reader = Reader { bytes: body, at: 0 };
    if reader.u32()? != FORMAT_VERSION {
        return Err(Truncated);
    }
    let _vendor = reader.string()?;
    let version = reader.string()?;

    let keys = reader.strings()?;
    let types = reader.list(2, Reader::u16)?;
    let enums = reader.strings()?;
    if keys.len() != types.len() {
        return Err(Truncated);
    }
    let dictionary = Dictionary { keys, types, enums };

    // The vendors' own details: names, printer models, artwork. Nothing here is needed.
    for _ in 0..reader.count(8)? {
        reader.skip_strings(6)?; // the map's key, then name, id, version and two addresses
        for _ in 0..reader.count(8)? {
            reader.skip_strings(3)?; // id, name, model id
            reader.u8()?; // technology
            reader.skip_strings(1)?; // family
            for _ in 0..3 {
                reader.strings()?; // nozzle sizes, default materials, unsupported plates
            }
            reader.skip_strings(9)?; // bed model, textures and the like
        }
        reader.strings()?; // default filaments
        reader.strings()?; // default resin materials
    }

    let _processes = presets(&mut reader, &dictionary, &version, false)?;
    let filaments = presets(&mut reader, &dictionary, &version, true)?;
    let machines = presets(&mut reader, &dictionary, &version, true)?;
    reader.u64()?; // how many mistakes OrcaSlicer counted when it packed the file
    if reader.at != body.len() {
        return Err(Truncated);
    }
    Ok(Vendor { name: name.to_string(), filaments, machines })
}

struct Dictionary {
    keys: Vec<String>,
    types: Vec<u16>,
    /// Names of the choices that settings of a fixed set of choices take. The first is empty.
    enums: Vec<String>,
}

/// One list of presets. With `keep` false they are read past and none is returned.
fn presets(reader: &mut Reader, dictionary: &Dictionary, version: &str, keep: bool) -> Read<Vec<Map<String, Value>>> {
    let count = reader.u32()?;
    let mut found = Vec::new();
    for _ in 0..count {
        let mut preset = Map::new();
        let name = reader.string()?;
        let _path = reader.string()?;
        for _ in 0..reader.u32()? {
            let index = usize::from(reader.u16()?);
            let (Some(key), Some(kind)) = (dictionary.keys.get(index), dictionary.types.get(index)) else {
                return Err(Truncated);
            };
            let value = setting(reader, *kind, &dictionary.enums)?;
            if keep {
                preset.insert(key.clone(), value);
            }
        }
        let inherits = reader.string()?;
        let includes = reader.strings()?;
        let _description = reader.string()?;
        let instantiation = reader.string()?;
        let setting_id = reader.string()?;
        let filament_id = reader.string()?;
        let _renamed_from = reader.strings()?;
        if !keep {
            continue;
        }
        preset.insert("name".into(), name.into());
        preset.insert("version".into(), version.into());
        preset.insert("instantiation".into(), instantiation.into());
        for (key, value) in [("inherits", inherits), ("setting_id", setting_id), ("filament_id", filament_id)] {
            if !value.is_empty() {
                preset.insert(key.into(), value.into());
            }
        }
        if !includes.is_empty() {
            preset.insert("include".into(), includes.into());
        }
        found.push(preset);
    }
    Ok(found)
}

/// One setting's value, as the text a JSON preset would hold for it.
fn setting(reader: &mut Reader, kind: u16, enums: &[String]) -> Read<Value> {
    let (base, is_list) = (kind & !LIST, kind & LIST != 0);
    if base == ENUM {
        // Choices are counted in 32 bits, and a single one is still written as a list.
        let count = reader.u32()?;
        let mut chosen = Vec::new();
        for _ in 0..count {
            let index = usize::from(reader.u16()?);
            // The first name stands for a choice that had none, whose number follows.
            let text = if index == 0 { number(reader.i32()?) } else { enums.get(index).ok_or(Truncated)?.clone() };
            chosen.push(Value::String(text));
        }
        return match (is_list, chosen.len()) {
            (true, _) => Ok(Value::Array(chosen)),
            (false, 1) => Ok(chosen.remove(0)),
            (false, _) => Err(Truncated),
        };
    }
    if is_list {
        let smallest = match base {
            1 | 4 | 3 | 10 | 11 => 8,
            2 => 4,
            5 => 9,
            6 => 16,
            _ => 1,
        };
        return Ok(Value::Array(reader.list(smallest, |reader| single(reader, base).map(Value::String))?));
    }
    single(reader, base).map(Value::String)
}

fn single(reader: &mut Reader, base: u16) -> Read<String> {
    let point = |reader: &mut Reader| -> Read<String> { Ok(format!("{}x{}", decimal(reader.f64()?), decimal(reader.f64()?))) };
    Ok(match base {
        1 => decimal(reader.f64()?),
        2 => number(reader.i32()?),
        3 => reader.string()?,
        4 => format!("{}%", decimal(reader.f64()?)),
        5 => {
            let value = decimal(reader.f64()?);
            if reader.u8()? != 0 { format!("{value}%") } else { value }
        }
        6 => point(reader)?,
        7 => format!("{},{},{}", decimal(reader.f64()?), decimal(reader.f64()?), decimal(reader.f64()?)),
        8 => match reader.u8()? {
            0 => "0".to_string(),
            1 => "1".to_string(),
            _ => "nil".to_string(),
        },
        // A list of lists: each inner one is a single piece of text in a JSON preset.
        10 => reader.list(16, point)?.join(","),
        11 => reader.list(4, |reader| reader.i32().map(number))?.join(","),
        _ => return Err(Truncated),
    })
}

/// A whole number, where the largest one stands for "not set".
fn number(value: i32) -> String {
    if value == i32::MAX { "nil".to_string() } else { value.to_string() }
}

/// A number with a fraction, where "not a number" stands for "not set".
fn decimal(value: f64) -> String {
    if value.is_nan() { "nil".to_string() } else { value.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Writer(Vec<u8>);

    impl Writer {
        fn u16(&mut self, value: u16) -> &mut Self {
            self.0.extend(value.to_le_bytes());
            self
        }
        fn u32(&mut self, value: u32) -> &mut Self {
            self.0.extend(value.to_le_bytes());
            self
        }
        fn u64(&mut self, value: u64) -> &mut Self {
            self.0.extend(value.to_le_bytes());
            self
        }
        fn text(&mut self, value: &str) -> &mut Self {
            self.u64(value.len() as u64);
            self.0.extend(value.as_bytes());
            self
        }
        fn texts(&mut self, values: &[&str]) -> &mut Self {
            self.u64(values.len() as u64);
            values.iter().for_each(|value| {
                self.text(value);
            });
            self
        }
    }

    /// A packed file for one vendor with one filament and one printer, laid out as OrcaSlicer does.
    fn packed(vendor: &str) -> Vec<u8> {
        let mut body = Writer::default();
        body.u32(FORMAT_VERSION).text(vendor).text("02.05.00.03");
        body.texts(&["nozzle_temperature", "filament_type", "filament_z_hop_types", "printer_model", "printer_variant", "compatible_printers"]);
        body.u64(6);
        for kind in [LIST | 2, LIST | 3, LIST | ENUM, 3, 3, LIST | 3] {
            body.u16(kind);
        }
        body.texts(&["", "Spiral Lift"]);
        // One vendor, with one printer model.
        body.u64(1).text(vendor).text("A vendor").text(vendor).text("2.5.3").text("").text("");
        body.u64(1).text("model").text("Test Printer").text("TP1");
        body.0.push(0);
        body.text("").texts(&["0.4"]).texts(&[]).texts(&[]);
        for _ in 0..9 {
            body.text("");
        }
        body.texts(&[]).texts(&[]);
        // Processes: one, to be read past.
        body.u32(1).text("0.20mm Standard").text("process/a.json").u32(0);
        body.text("").texts(&[]).text("").text("true").text("").text("").texts(&[]);
        // Filaments: a base and the one that can be chosen.
        body.u32(2);
        body.text("fdm_filament_pla").text("filament/fdm_filament_pla.json").u32(2);
        body.u16(0).u64(1).u32(220);
        body.u16(1).texts(&["PLA"]);
        body.text("").texts(&[]).text("").text("false").text("").text("GFL99").texts(&[]);
        body.text("Test PLA @Test Printer 0.4 nozzle").text("filament/b.json").u32(2);
        body.u16(2).u32(2).u16(1).u16(0).u32(7);
        body.u16(5).texts(&["Test Printer 0.4 nozzle"]);
        body.text("fdm_filament_pla").texts(&[]).text("").text("true").text("GFSL99").text("").texts(&[]);
        // Printers.
        body.u32(1).text("Test Printer 0.4 nozzle").text("machine/c.json").u32(2);
        body.u16(3).text("Test Printer");
        body.u16(4).text("0.4");
        body.text("").texts(&[]).text("").text("true").text("").text("").texts(&[]);
        body.u64(0);

        let mut check = flate2::Crc::new();
        check.update(&body.0);
        let mut file = Writer::default();
        file.u32(MAGIC).u32(FORMAT_VERSION).u64(body.0.len() as u64).u32(check.sum());
        file.0.extend(body.0);
        file.0
    }

    #[test]
    fn a_packed_vendor_reads_as_its_json_presets_would() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Test.opc"), packed("Test")).unwrap();
        let loaded = load(dir.path()).unwrap();

        assert_eq!(loaded.version, "2.5.0.3");
        assert_eq!(loaded.machines.len(), 1);
        assert_eq!((loaded.machines[0].model.as_str(), loaded.machines[0].nozzle.as_str()), ("Test Printer", "0.4"));

        let chosen = &loaded.filaments["Test PLA @Test Printer 0.4 nozzle"];
        assert_eq!(chosen["inherits"], "fdm_filament_pla\u{1}Test");
        assert_eq!(chosen["filament_z_hop_types"], serde_json::json!(["Spiral Lift", "7"]));
        assert_eq!(chosen["compatible_printers"], serde_json::json!(["Test Printer 0.4 nozzle"]));
        let base = &loaded.filaments["fdm_filament_pla\u{1}Test"];
        assert_eq!(base["nozzle_temperature"], serde_json::json!(["220"]));
        assert_eq!(base["filament_id"], "GFL99");
    }

    #[test]
    fn a_damaged_or_newer_packed_file_is_refused_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test.opc");

        let mut damaged = packed("Test");
        let last = damaged.len() - 1;
        damaged[last] ^= 1;
        fs::write(&path, damaged).unwrap();
        assert!(load(dir.path()).err().unwrap().to_string().contains("damaged"));

        let mut newer = packed("Test");
        newer[4] = 3;
        fs::write(&path, newer).unwrap();
        assert!(load(dir.path()).err().unwrap().to_string().contains("format 3"));
    }

    #[test]
    fn numbers_read_as_a_preset_file_writes_them() {
        assert_eq!(decimal(21.0), "21");
        assert_eq!(decimal(0.98), "0.98");
        assert_eq!(decimal(f64::NAN), "nil");
        assert_eq!(number(i32::MAX), "nil");
    }
}
