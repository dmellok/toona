//! Profile generation checked against real files: three profiles Bambu Studio 2.9
//! wrote with "Create Custom Filament", two it wrote on import, and the system
//! presets they derive from (see `fixtures/` and PROFILE_SPEC.md).

use std::fs;
use std::path::{Path, PathBuf};

use toona_core::mapping::{base_candidates, choose_base, profile_alias, BaseSource};
use toona_core::profile::user_filament_id;
use toona_core::{convert_inherited, ConvertRequest};
use toona_core::{
    build_profile, Catalogue, Error, FilamentProfile, MaterialClass, PresetInfo, ProfileRequest, SearchFilter,
    Slicer, SlicerKind, SystemLibrary,
};
use serde_json::Value;

const USER_ID: &str = "10000001";
const PRINTER: &str = "Bambu Lab X2D 0.4 nozzle";

/// (filament name, system preset it was created from, settings edited by hand afterwards)
const STUDIO_CREATED: &[(&str, &str, &[&str])] = &[
    (
        "eSUN PLA Plus",
        "Bambu PLA Basic @BBL X2D 0.4 nozzle",
        &["textured_plate_temp", "textured_plate_temp_initial_layer"],
    ),
    (
        "eSUN PLA Matte",
        "Bambu PLA Basic @BBL X2D 0.4 nozzle",
        &["textured_plate_temp", "textured_plate_temp_initial_layer", "filament_max_volumetric_speed"],
    ),
    (
        "eSUN PETG Matte",
        "Bambu PETG Basic @BBL X2D 0.4 nozzle",
        &[
            "textured_plate_temp",
            "textured_plate_temp_initial_layer",
            "nozzle_temperature",
            "nozzle_temperature_initial_layer",
        ],
    ),
];

/// Studio's built-in defaults, which it writes out and the tool leaves to Studio.
const STUDIO_DEFAULT_KEYS: &[&str] = &[
    "close_optional_fan_first_x_layers",
    "compatible_printers_condition",
    "compatible_prints",
    "compatible_prints_condition",
    "default_filament_colour",
    "enable_overhang_bridge_fan",
    "enable_pressure_advance",
    "filament_change_length_nc",
    "filament_idle_travel_cooling_enabled",
    "filament_notes",
    "filament_prime_tower_speed_factor_curve",
    "filament_retract_length_toolchange",
    "filament_retract_restart_extra_toolchange",
    "first_x_layer_optional_fan_speed",
    "first_x_layer_part_fan_speed",
    "ironing_fan_speed",
    "min_temperature_when_ec",
    "optional_cooling_fan_speed",
    "optional_fan_full_speed_layer",
    "overhang_threshold_participating_cooling",
    "pre_start_fan_time",
    "pressure_advance",
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/BambuStudio")
}

fn slicer() -> Slicer {
    Slicer::at(SlicerKind::BambuStudio, fixtures()).unwrap()
}

fn library() -> SystemLibrary {
    slicer().system_library().unwrap()
}

fn created_path(filament: &str) -> PathBuf {
    fixtures().join(format!("user/{USER_ID}/filament/base/{filament} @{PRINTER}.json"))
}

fn created_profile(filament: &str) -> FilamentProfile {
    FilamentProfile::from_json(&fs::read_to_string(created_path(filament)).unwrap()).unwrap()
}

/// "12.0" and "12" are the same setting value.
fn same_setting(a: &Value, b: &Value) -> bool {
    let numbers = |v: &Value| -> Option<Vec<f64>> {
        v.as_array()?.iter().map(|item| item.as_str()?.parse().ok()).collect()
    };
    a == b || matches!((numbers(a), numbers(b)), (Some(x), Some(y)) if x == y)
}

/// A dataset in the Open Filament Database's published shape.
fn catalogue() -> Catalogue {
    let dataset = serde_json::json!({
        "version": "test",
        "brands": [
            { "id": "b-esun", "name": "eSUN", "slug": "esun" },
            { "id": "b-poly", "name": "Polymaker", "slug": "polymaker" }
        ],
        "materials": [{ "id": "m-resin", "material": "Standard Resin", "material_class": "SLA" }],
        "filaments": [
            { "id": "f-plus", "name": "PLA Plus", "slug": "pla_plus", "brand_id": "b-esun", "material": "PLA",
              "density": 1.23, "min_print_temperature": 205, "max_print_temperature": 225,
              "min_bed_temperature": 45, "max_bed_temperature": 60, "discontinued": false },
            { "id": "f-silk", "name": "eSilk PLA", "slug": "esilk_pla", "brand_id": "b-esun", "material": "PLA",
              "density": 1.2, "discontinued": false },
            { "id": "f-tpu", "name": "TPU 95A", "slug": "tpu_95a", "brand_id": "b-esun", "material": "TPU",
              "density": 1.21, "max_volumetric_speed": 2.5, "discontinued": false,
              "slicer_settings": { "generic": { "nozzle_temp": 235, "first_layer_nozzle_temp": 240, "bed_temp": 40 } } },
            { "id": "f-peek", "name": "PEEK", "slug": "peek", "brand_id": "b-esun", "material": "PEEK",
              "density": 1.3, "discontinued": false },
            { "id": "f-resin", "name": "Standard Resin", "slug": "standard_resin", "brand_id": "b-esun",
              "material": "Standard Resin", "material_id": "m-resin", "density": 1.1, "discontinued": false },
            { "id": "f-polylite", "name": "Polylite PLA", "slug": "polylite_pla", "brand_id": "b-poly",
              "material": "PLA", "density": 1.17, "discontinued": false },
            { "id": "f-285", "name": "PLA Thick", "slug": "pla_thick", "brand_id": "b-esun", "material": "PLA",
              "density": 1.24, "discontinued": false }
        ],
        "variants": [
            { "id": "v-red", "name": "Fire Red", "slug": "fire_red", "filament_id": "f-plus", "color_hex": "#d6001c" },
            { "id": "v-gold", "name": "Gold", "slug": "gold", "filament_id": "f-silk", "color_hex": "#C9A227",
              "traits": { "silk": true } },
            { "id": "v-thick", "name": "Black", "slug": "black", "filament_id": "f-285", "color_hex": "#000000" }
        ],
        "sizes": [
            { "variant_id": "v-red", "diameter": 1.75, "gtin": "6922572201234" },
            { "variant_id": "v-red", "diameter": 2.85, "gtin": "793897749366" },
            { "variant_id": "v-gold", "diameter": 1.75, "gtin": "6922572209999", "barcode_identifier": "921128098885" },
            { "variant_id": "v-thick", "diameter": 2.85, "gtin": "6922572209999" },
            { "variant_id": "v-thick", "diameter": 2.85 }
        ]
    });
    Catalogue::from_json(dataset.to_string().as_bytes()).unwrap()
}

fn request<'a>(catalogue: &'a Catalogue, key: &str) -> ProfileRequest<'a> {
    ProfileRequest {
        filament: catalogue.find(key).unwrap(),
        variant: None,
        printer: PRINTER,
        base: None,
        user_id: Some(USER_ID),
        existing: &[],
    }
}

#[test]
fn resolving_a_system_preset_reproduces_the_profile_studio_created_from_it() {
    let library = library();
    for (filament, base, edited_by_hand) in STUDIO_CREATED {
        let real = created_profile(filament);
        let resolved = library.resolve_filament(base).unwrap();

        for (key, value) in &resolved.settings {
            // Stored as typed fields on the profile rather than as settings.
            if ["compatible_printers", "filament_vendor", "filament_type"].contains(&key.as_str()) {
                continue;
            }
            let written = real.settings.get(key).unwrap_or_else(|| panic!("{filament}: Studio did not write {key}"));
            if !edited_by_hand.contains(&key.as_str()) {
                assert!(same_setting(value, written), "{filament}: {key} is {value}, Studio wrote {written}");
            }
        }

        let only_in_real: Vec<&str> =
            real.settings.keys().map(String::as_str).filter(|k| !resolved.settings.contains_key(*k)).collect();
        for key in &only_in_real {
            assert!(STUDIO_DEFAULT_KEYS.contains(key), "{filament}: {key} is missing from the resolved preset");
        }
        assert_eq!(resolved.first_string("filament_type"), Some(real.filament_type[0].as_str()));
    }
}

#[test]
fn filament_id_is_the_one_studio_generated() {
    for (filament, _, _) in STUDIO_CREATED {
        let real = created_profile(filament);
        assert_eq!(user_filament_id(filament, Some(USER_ID)), real.filament_id, "{filament}");
    }
    // Signed out, the account id is not part of the hash.
    assert_ne!(user_filament_id("eSUN PLA Plus", None), user_filament_id("eSUN PLA Plus", Some(USER_ID)));
}

#[test]
fn profiles_studio_created_satisfy_the_spec() {
    for (filament, _, _) in STUDIO_CREATED {
        let real = created_profile(filament);
        real.validate().unwrap();
        assert_eq!(real.alias(), *filament);
        assert_eq!(real.version, library().version, "profile version is the system bundle version");
    }
}

#[test]
fn serialization_is_byte_identical_to_studio() {
    for (filament, _, _) in STUDIO_CREATED {
        let on_disk = fs::read_to_string(created_path(filament)).unwrap();
        let profile = FilamentProfile::from_json(&on_disk).unwrap();
        assert_eq!(profile.to_json(), on_disk, "{filament}");
    }
}

#[test]
fn imported_profiles_do_not_satisfy_the_spec() {
    for name in ["X2D  eSUN PLA+ Filament", "X2D  eSUN PETG Filament"] {
        let path = fixtures().join(format!("user/{USER_ID}/filament/{name}.json"));
        let json: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

        // What makes them unusable for the AMS: they inherit and carry no id of their own.
        assert!(!json["inherits"].as_str().unwrap().is_empty());
        assert!(json.get("filament_id").is_none());
        assert!(json.get("compatible_printers").is_none());
        assert!(FilamentProfile::from_json(&json.to_string()).is_err(), "{name} lacks the required keys");

        let info = PresetInfo::parse(&fs::read_to_string(path.with_extension("info")).unwrap());
        assert!(!info.base_id.is_empty(), "an imported profile points at its parent's setting_id");
    }
}

#[test]
fn info_sidecar_round_trips_byte_for_byte() {
    let path = created_path("eSUN PLA Plus").with_extension("info");
    let on_disk = fs::read_to_string(path).unwrap();
    let info = PresetInfo::parse(&on_disk);

    assert_eq!(info.setting_id, "PFUSd140dc0c9eff57");
    assert!(info.base_id.is_empty() && info.sync_info.is_empty() && info.user_id.is_empty());
    assert!(info.is_cloud_synced());
    assert_eq!(info.to_string(), on_disk);

    let fresh = PresetInfo::new_unsynced();
    assert!(!fresh.is_cloud_synced() && fresh.updated_time > 0);
    assert_eq!(fresh.to_string().lines().count(), 5);
}

#[test]
fn slicer_folder_is_read_like_the_real_one() {
    let slicer = slicer();
    let conf = slicer.conf().unwrap();
    assert_eq!(conf.user_id(), Some(USER_ID));
    assert!(conf.uploads_new_presets());
    assert_eq!(slicer.user_folders().unwrap(), [USER_ID]);
    assert!(slicer.custom_filament_dir(&conf).ends_with(format!("user/{USER_ID}/filament/base")));

    let library = slicer.system_library().unwrap();
    assert_eq!(library.version, "2.9.0.83");
    let printers = slicer.printers(&conf, &library);
    let names: Vec<&str> = printers.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [PRINTER, "Bambu Lab X2D 0.2 nozzle", "Bambu Lab X2D 0.6 nozzle", "Bambu Lab X2D 0.8 nozzle"],
        "selected preset first"
    );
    assert!(printers[0].selected && printers[0].nozzle == "0.4");

    let filaments = slicer.user_filaments(&conf).unwrap();
    assert_eq!(filaments.iter().filter(|f| f.is_root).count(), 3);
    assert_eq!(filaments.iter().filter(|f| !f.is_root).count(), 2);
}

#[test]
fn generated_profile_has_the_shape_of_a_studio_created_one() {
    let library = library();
    let catalogue = catalogue();
    let real = created_profile("eSUN PLA Plus");
    let mut request = request(&catalogue, "esun/PLA/pla_plus");
    request.base = Some("Bambu PLA Basic");

    let generated = build_profile(&library, &request).unwrap();

    assert_eq!(generated.name, real.name);
    assert_eq!(generated.filament_id, real.filament_id);
    assert_eq!(generated.from, real.from);
    assert_eq!(generated.inherits, real.inherits);
    assert_eq!(generated.version, real.version);
    assert_eq!(generated.filament_settings_id, real.filament_settings_id);
    assert_eq!(generated.compatible_printers, real.compatible_printers);
    assert_eq!(generated.filament_vendor, real.filament_vendor);
    assert_eq!(generated.filament_type, real.filament_type);

    let from_database = [
        "filament_density",
        "nozzle_temperature_range_low",
        "nozzle_temperature_range_high",
        "textured_plate_temp",
        "textured_plate_temp_initial_layer",
    ];
    for (key, value) in &generated.settings {
        let written = real.settings.get(key).unwrap_or_else(|| panic!("Studio does not write {key}"));
        if !from_database.contains(&key.as_str()) {
            assert!(same_setting(value, written), "{key} is {value}, Studio wrote {written}");
        }
    }
    assert_eq!(generated.settings["filament_density"], serde_json::json!(["1.23"]));
    assert_eq!(generated.settings["filament_diameter"], serde_json::json!(["1.75"]));
    assert_eq!(generated.settings["nozzle_temperature_range_low"], serde_json::json!(["205"]));
    assert_eq!(generated.settings["nozzle_temperature_range_high"], serde_json::json!(["225"]));
    // Base values already inside the database's range stay as they are.
    assert_eq!(generated.settings["nozzle_temperature"], serde_json::json!(vec!["220"; 6]));
    assert_eq!(generated.settings["hot_plate_temp"], serde_json::json!(["55"]));

    // And it survives a round trip through the file format.
    assert_eq!(FilamentProfile::from_json(&generated.to_json()).unwrap(), generated);
}

#[test]
fn base_profile_follows_material_and_traits() {
    let library = library();
    let catalogue = catalogue();
    let base = |key: &str| choose_base(&library, catalogue.find(key).unwrap(), PRINTER, None);

    assert_eq!(base("esun/PLA/pla_plus").unwrap().preset, "Generic PLA @BBL X2D 0.4 nozzle");
    assert_eq!(base("esun/PLA/pla_plus").unwrap().source, BaseSource::Generic);
    assert_eq!(base("esun/PLA/esilk_pla").unwrap().preset, "Generic PLA Silk @BBL X2D 0.4 nozzle");
    assert_eq!(base("esun/TPU/tpu_95a").unwrap().preset, "Generic TPU @BBL X2D 0.4 nozzle");
    assert!(matches!(base("esun/PEEK/peek"), Err(Error::NoBaseProfile { .. })));
    assert!(base_candidates(catalogue.find("esun/PEEK/peek").unwrap()).is_empty());

    // Each nozzle preset gets the system preset made for it.
    let small_nozzle = choose_base(&library, catalogue.find("esun/PLA/pla_plus").unwrap(), "Bambu Lab X2D 0.2 nozzle", None);
    assert_eq!(small_nozzle.unwrap().preset, "Generic PLA @BBL X2D 0.2 nozzle");
}

#[test]
fn a_vendor_tuned_preset_for_the_exact_product_beats_the_generic_one() {
    let library = library();
    let catalogue = catalogue();
    let polylite = catalogue.find("polymaker/PLA/polylite_pla").unwrap();
    let x1c = "Bambu Lab X1 Carbon 0.4 nozzle";

    // Studio ships "PolyLite PLA" for the X1 Carbon; the catalogue spells it "Polylite PLA".
    let choice = choose_base(&library, polylite, x1c, None).unwrap();
    assert_eq!(choice.preset, "PolyLite PLA @BBL X1C");
    assert_eq!(choice.source, BaseSource::VendorTuned);

    // It ships none for the X2D, so that printer gets the generic preset.
    let choice = choose_base(&library, polylite, PRINTER, None).unwrap();
    assert_eq!(choice.preset, "Generic PLA @BBL X2D 0.4 nozzle");
    assert_eq!(choice.source, BaseSource::Generic);

    // The vendor's tuning reaches the generated profile.
    let mut request = request(&catalogue, "polymaker/PLA/polylite_pla");
    request.printer = x1c;
    let profile = build_profile(&library, &request).unwrap();
    let tuned = library.resolve_filament("PolyLite PLA @BBL X1C").unwrap();
    for key in ["filament_flow_ratio", "filament_max_volumetric_speed", "slow_down_layer_time"] {
        assert_eq!(profile.settings.get(key), tuned.settings.get(key), "{key}");
    }
    assert_eq!(profile.filament_vendor, ["Polymaker"]);
    assert_eq!(profile.compatible_printers, [x1c]);
}

#[test]
fn database_values_override_the_base_and_nothing_else() {
    let library = library();
    let catalogue = catalogue();

    let tpu = build_profile(&library, &request(&catalogue, "esun/TPU/tpu_95a")).unwrap();
    let base = library.resolve_filament("Generic TPU @BBL X2D 0.4 nozzle").unwrap();
    let changed: Vec<&str> = tpu
        .settings
        .iter()
        .filter(|(key, value)| base.settings.get(*key) != Some(value))
        .map(|(key, _)| key.as_str())
        .collect();
    for key in &changed {
        assert!(
            [
                "filament_density",
                "filament_diameter",
                "nozzle_temperature",
                "nozzle_temperature_initial_layer",
                "hot_plate_temp",
                "hot_plate_temp_initial_layer",
                "textured_plate_temp",
                "textured_plate_temp_initial_layer",
                "filament_max_volumetric_speed",
            ]
            .contains(key),
            "{key} changed although the database does not provide it"
        );
    }
    assert_eq!(tpu.filament_type, ["TPU"]);
    assert_eq!(tpu.filament_vendor, ["eSUN"]);
    assert_eq!(tpu.settings["filament_density"], serde_json::json!(["1.21"]));
    assert_eq!(tpu.settings["nozzle_temperature"], serde_json::json!(vec!["235"; 6]));
    assert_eq!(tpu.settings["nozzle_temperature_initial_layer"], serde_json::json!(vec!["240"; 6]));
    for speed in tpu.settings["filament_max_volumetric_speed"].as_array().unwrap() {
        assert!(speed.as_str().unwrap().parse::<f64>().unwrap() <= 2.5);
    }
}

#[test]
fn colour_variant_names_and_colours_the_profile() {
    let library = library();
    let catalogue = catalogue();
    let filament = catalogue.find("esun/PLA/pla_plus").unwrap();
    let mut request = request(&catalogue, "esun/PLA/pla_plus");
    request.variant = filament.variant("fire red");
    assert!(request.variant.is_some());

    let profile = build_profile(&library, &request).unwrap();

    assert_eq!(profile.name, "eSUN PLA Plus Fire Red @Bambu Lab X2D 0.4 nozzle");
    assert_eq!(profile.settings["default_filament_colour"], serde_json::json!(["#D6001C"]));
    assert_eq!(profile.filament_id, user_filament_id("eSUN PLA Plus Fire Red", Some(USER_ID)));
    assert_eq!(profile_alias(filament, None), "eSUN PLA Plus");
}

#[test]
fn a_filament_keeps_its_id_across_printers_and_never_takes_another_ones() {
    let slicer = slicer();
    let conf = slicer.conf().unwrap();
    let library = slicer.system_library().unwrap();
    let catalogue = catalogue();
    let existing = slicer.user_filaments(&conf).unwrap();

    // "eSUN PLA Plus" already exists for the 0.4 nozzle: the 0.2 profile shares its id.
    let mut second_nozzle = request(&catalogue, "esun/PLA/pla_plus");
    second_nozzle.printer = "Bambu Lab X2D 0.2 nozzle";
    second_nozzle.existing = &existing;
    let profile = build_profile(&library, &second_nozzle).unwrap();
    assert_eq!(profile.filament_id, "P6c4d05a");
    assert_eq!(profile.compatible_printers, ["Bambu Lab X2D 0.2 nozzle"]);

    // A different filament whose hash is already taken gets another id.
    let silk_id = user_filament_id("eSUN eSilk PLA", Some(USER_ID));
    let mut taken = existing.clone();
    taken[0].filament_id = Some(silk_id.clone());
    let mut silk = request(&catalogue, "esun/PLA/esilk_pla");
    silk.existing = &taken;
    let profile = build_profile(&library, &silk).unwrap();
    assert_ne!(profile.filament_id, silk_id);
    profile.validate().unwrap();
}

#[test]
fn unusable_requests_are_refused() {
    let library = library();
    let catalogue = catalogue();

    let thick = build_profile(&library, &request(&catalogue, "esun/PLA/pla_thick"));
    assert!(matches!(thick, Err(Error::UnsupportedDiameter(_))));

    let mut unknown_printer = request(&catalogue, "esun/PLA/pla_plus");
    unknown_printer.printer = "Bambu Lab X9 0.4 nozzle";
    assert!(matches!(build_profile(&library, &unknown_printer), Err(Error::PrinterNotFound(_))));

    // Resin is catalogued but has no slicer to go to yet.
    let resin = catalogue.find("esun/Standard Resin/standard_resin").unwrap();
    assert_eq!(resin.class, MaterialClass::Resin);
    assert_eq!(catalogue.find("esun/PLA/pla_plus").unwrap().class, MaterialClass::Filament);
    let only_resin = SearchFilter { class: Some(MaterialClass::Resin), ..SearchFilter::default() };
    assert_eq!(catalogue.search(&only_resin).len(), 1);
    let refused = build_profile(&library, &request(&catalogue, "esun/Standard Resin/standard_resin"));
    assert!(matches!(refused, Err(Error::UnsupportedMaterialClass { .. })));

    assert!(matches!(catalogue.find("pla"), Err(Error::Ambiguous { .. })));
    assert!(matches!(catalogue.find("nylon"), Err(Error::FilamentNotFound(_))));
}

#[test]
fn validation_reports_every_broken_rule() {
    let mut profile = created_profile("eSUN PLA Plus");
    profile.inherits = "Bambu PLA Basic @BBL X2D 0.4 nozzle".into();
    profile.filament_id = "GFA00".into();
    profile.compatible_printers.clear();
    profile.settings.insert("setting_id".into(), Value::String("GFSA00".into()));
    profile.settings.insert("nozzle_temperature".into(), serde_json::json!(["220"]));
    profile.settings.insert("filament_density".into(), serde_json::json!([1.24]));

    let Err(Error::InvalidProfile { problems, .. }) = profile.validate() else { panic!("profile should be invalid") };

    for expected in ["inherits", "filament_id", "compatible_printers", "setting_id", "nozzle_temperature has 1", "filament_density"] {
        assert!(problems.iter().any(|p| p.contains(expected)), "no problem mentions {expected}: {problems:?}");
    }
}

fn imported<'a>(filaments: &'a [toona_core::UserFilament], name: &str) -> &'a toona_core::UserFilament {
    filaments.iter().find(|f| f.name == name).unwrap()
}

#[test]
fn converting_an_imported_profile_gives_what_studio_makes_for_the_same_filament() {
    // The user imported "X2D  eSUN PLA+ Filament" and separately created "eSUN PLA Plus"
    // in Studio from the same base with the same edit. Converting the first must
    // therefore reproduce the second.
    let slicer = slicer();
    let conf = slicer.conf().unwrap();
    let library = slicer.system_library().unwrap();
    let printers = slicer.printers(&conf, &library);
    let existing = slicer.user_filaments(&conf).unwrap();
    let source = imported(&existing, "X2D  eSUN PLA+ Filament");
    assert!(!source.is_root);

    let converted = convert_inherited(
        &library,
        &ConvertRequest { source, alias: None, vendor: Some("eSUN"), printers: &printers, user_id: Some(USER_ID), existing: &existing },
    )
    .unwrap();

    assert_eq!(converted.len(), 1);
    let profile = &converted[0];
    let real = created_profile("eSUN PLA Plus");
    assert_eq!(profile.name, "X2D eSUN PLA+ Filament @Bambu Lab X2D 0.4 nozzle");
    assert!(profile.inherits.is_empty());
    assert_eq!(profile.filament_id, user_filament_id("X2D eSUN PLA+ Filament", Some(USER_ID)));
    assert_eq!(profile.compatible_printers, real.compatible_printers);
    assert_eq!(profile.filament_vendor, real.filament_vendor);
    assert_eq!(profile.filament_type, real.filament_type);
    for (key, value) in &profile.settings {
        let written = real.settings.get(key).unwrap_or_else(|| panic!("Studio does not write {key}"));
        assert!(same_setting(value, written), "{key} is {value}, Studio wrote {written}");
    }
    assert_eq!(profile.settings["textured_plate_temp"], serde_json::json!(["60"]), "the user's edit is kept");
}

#[test]
fn conversion_keeps_per_variant_edits_and_inherits_the_other_slots() {
    let slicer = slicer();
    let conf = slicer.conf().unwrap();
    let library = slicer.system_library().unwrap();
    let printers = slicer.printers(&conf, &library);
    let existing = slicer.user_filaments(&conf).unwrap();
    let request = |name: &str| {
        let source = imported(&existing, name);
        convert_inherited(
            &library,
            &ConvertRequest { source, alias: Some("eSUN PETG"), vendor: None, printers: &printers, user_id: Some(USER_ID), existing: &existing },
        )
    };

    // The import sets the first extruder variant to 255 and leaves the rest to the parent (250).
    let profile = &request("X2D  eSUN PETG Filament").unwrap()[0];
    assert_eq!(profile.settings["nozzle_temperature"], serde_json::json!(["255", "250", "250", "250", "250", "250"]));
    assert_eq!(profile.name, "eSUN PETG @Bambu Lab X2D 0.4 nozzle");
    assert_eq!(profile.filament_vendor, ["Bambu Lab"], "without a vendor the parent's is kept");
    assert_eq!(profile.filament_type, ["PETG"]);

    // A profile that is already custom has nothing to convert.
    let custom = imported(&existing, "eSUN PLA Plus @Bambu Lab X2D 0.4 nozzle");
    let again = convert_inherited(
        &library,
        &ConvertRequest { source: custom, alias: None, vendor: None, printers: &printers, user_id: Some(USER_ID), existing: &existing },
    );
    assert!(matches!(again, Err(Error::NotConvertible { .. })));

    // Nor can one whose printer is not set up.
    let none: Vec<toona_core::PrinterPreset> = Vec::new();
    let source = imported(&existing, "X2D  eSUN PETG Filament");
    let unset = convert_inherited(
        &library,
        &ConvertRequest { source, alias: None, vendor: None, printers: &none, user_id: Some(USER_ID), existing: &existing },
    );
    assert!(matches!(unset, Err(Error::NotConvertible { .. })));
}

#[test]
fn a_scanned_barcode_finds_the_exact_colour() {
    let catalogue = catalogue();

    let found = catalogue.find_barcode("6922572201234");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].filament.key, "esun/PLA/pla_plus");
    assert_eq!(found[0].variant.name, "Fire Red");

    // Scanners and people add spaces; a UPC may arrive with or without its leading zero.
    assert_eq!(catalogue.find_barcode(" 6922572-201234 ").len(), 1);
    assert_eq!(catalogue.find_barcode("0793897749366")[0].variant.name, "Fire Red");
    assert_eq!(catalogue.find_barcode("921128098885")[0].variant.name, "Gold");

    // The database lists some barcodes twice; both are returned for the user to choose.
    let ambiguous = catalogue.find_barcode("6922572209999");
    assert_eq!(ambiguous.len(), 2);

    assert!(catalogue.find_barcode("4006381333931").is_empty(), "unknown barcode");
    assert!(catalogue.find_barcode("pla plus").is_empty(), "not a barcode");
    assert!(catalogue.find_barcode("12345").is_empty(), "too short to be one");
}

#[test]
fn a_recipe_rebuilds_the_profile_from_its_parent_and_names_only_what_changed() {
    let library = library();
    let catalogue = catalogue();
    let request = request(&catalogue, "esun/PLA/pla_plus");

    let (profile, recipe) = toona_core::build_profile_with_recipe(&library, &request).unwrap();

    let mut parent = library.resolve_filament(&recipe.parent).unwrap().settings;
    for key in ["compatible_printers", "filament_vendor", "filament_type"] {
        parent.remove(key);
    }
    // The brand is a field of its own on a profile, not one of its settings.
    let mut rebuilt = recipe.apply(&parent);
    assert_eq!(rebuilt.remove("filament_vendor"), Some(serde_json::json!(profile.filament_vendor)));
    assert_eq!(rebuilt, profile.settings);

    assert!(!recipe.changes.is_empty());
    assert!(recipe.changes.len() < 20, "a catalogue profile changes a handful of settings, not {}", recipe.changes.len());
    for change in &recipe.changes {
        assert_ne!(change.from.as_ref(), Some(&change.to), "{} is listed but unchanged", change.key);
    }
    let density = recipe.changes.iter().find(|c| c.key == "filament_density").expect("density comes from the catalogue");
    assert_eq!(density.layer, toona_core::Layer::Manufacturer);
}

#[test]
fn a_converted_profile_records_the_users_own_changes_over_the_preset_it_inherits() {
    let slicer = slicer();
    let conf = slicer.conf().unwrap();
    let library = library();
    let existing = slicer.user_filaments(&conf).unwrap();
    let printers = slicer.printers(&conf, &library);
    let source = existing.iter().find(|f| !f.is_root).expect("the fixtures include an imported profile");

    let converted = toona_core::convert_inherited_with_recipes(
        &library,
        &ConvertRequest { source, alias: None, vendor: None, printers: &printers, user_id: Some(USER_ID), existing: &existing },
    )
    .unwrap();

    let (profile, recipe) = &converted[0];
    let raw: serde_json::Map<String, Value> = serde_json::from_str(&fs::read_to_string(&source.path).unwrap()).unwrap();
    assert_eq!(recipe.parent, raw["inherits"].as_str().unwrap());
    assert!(recipe.changes.iter().all(|c| c.layer == toona_core::Layer::You));
    let parent = library.resolve_filament(&recipe.parent).unwrap().settings;
    let mut rebuilt = recipe.apply(&parent);
    for key in ["compatible_printers", "filament_vendor", "filament_type"] {
        rebuilt.remove(key);
    }
    assert_eq!(rebuilt, profile.settings);
}
