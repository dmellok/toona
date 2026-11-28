//! PrusaSlicer support checked against a small settings folder shaped like a real one
//! (`fixtures/PrusaSlicer`): Prusa's bundle, the templates bundle, a printer preset and a
//! filament preset the user made. The values in it were written for these tests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use toona_core::catalogue::RecommendedTemps;
use toona_core::mapping::{choose_base, BaseSource};
use toona_core::{
    build_profile_with_recipe, CatalogueFilament, Error, Family, FilamentProfile, Layer, MaterialClass, ProfileRequest,
    Slicer, SlicerKind, SystemLibrary, Variant,
};

const MK3S: &str = "Original Prusa i3 MK3S & MK3S+";
const MK3S_06: &str = "Original Prusa i3 MK3S & MK3S+ 0.6 nozzle";
const MK4S_HF: &str = "Original Prusa MK4S HF0.4 nozzle";
const VORON: &str = "My Voron";

fn slicer() -> Slicer {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/PrusaSlicer");
    Slicer::at(SlicerKind::PrusaSlicer, folder).unwrap()
}

fn library() -> SystemLibrary {
    slicer().system_library().unwrap()
}

fn filament(brand: &str, name: &str, material: &str) -> CatalogueFilament {
    CatalogueFilament {
        key: format!("{}/{material}/{}", brand.to_lowercase(), name.to_lowercase().replace(' ', "_")),
        brand: brand.to_string(),
        name: name.to_string(),
        material: material.to_string(),
        class: MaterialClass::Filament,
        density: Some(1.23),
        min_print_temperature: Some(215.0),
        max_print_temperature: Some(235.0),
        min_bed_temperature: Some(45.0),
        max_bed_temperature: Some(55.0),
        max_volumetric_speed: Some(18.0),
        recommended: RecommendedTemps::default(),
        discontinued: false,
        diameters: vec![1.75, 2.85],
        slicer_profiles: BTreeMap::new(),
        traits: BTreeSet::new(),
        variants: vec![Variant { name: "Orange".into(), slug: "orange".into(), color_hex: Some("ff6720".into()), barcodes: Vec::new() }],
    }
}

fn request<'a>(filament: &'a CatalogueFilament, printer: &'a str) -> ProfileRequest<'a> {
    ProfileRequest {
        filament,
        variant: filament.variants.first(),
        printer,
        base: None,
        user_id: None,
        existing: &[],
        alias: None,
        borrow_tuned: false,
    }
}

#[test]
fn the_settings_folder_is_read_like_the_real_one() {
    let slicer = slicer();
    let conf = slicer.conf().unwrap();
    let library = library();
    assert_eq!(library.family, Family::Prusa);
    assert_eq!(library.version, "2.5.16");
    assert!(!conf.uploads_new_presets(), "PrusaSlicer has no cloud for presets");

    // The models added in the slicer with the nozzles chosen for them, and the user's own
    // printer preset. Not the 0.8 mm MK3S, the plain MK4S nozzle or the resin printer.
    let printers = slicer.printers(&conf, &library);
    let listed: Vec<(&str, &str, &str)> = printers.iter().map(|p| (p.name.as_str(), p.model.as_str(), p.nozzle.as_str())).collect();
    assert_eq!(
        listed,
        [
            (MK3S, "Original Prusa i3 MK3S & MK3S+", "0.4"),
            (VORON, "My Voron", "0.5"),
            (MK4S_HF, "Original Prusa MK4S", "HF0.4"),
            (MK3S_06, "Original Prusa i3 MK3S & MK3S+", "0.6"),
        ]
    );
    assert!(printers[0].selected && !printers[1].selected);

    let own = slicer.user_filaments(&conf).unwrap();
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].name, "My Own PLA");
    assert!(own[0].is_root && own[0].filament_id.is_none());
    assert_eq!(slicer.custom_filament_dir(&conf), slicer.config_dir.join("filament"));
}

#[test]
fn each_printer_is_offered_the_presets_prusaslicer_would_show_for_it() {
    let library = library();
    // Its own vendor's presets first, the generic templates after them.
    assert_eq!(
        library.filaments_for_printer(MK3S),
        ["Generic PETG", "Generic PLA", "Prusament PLA", "Generic ABS @Template", "Generic PLA @Template"]
    );
    // A preset that lists printers by name shows for those and no others.
    assert!(library.filaments_for_printer(MK3S_06).contains(&"Only For One Printer"));
    // The MK4S is kept away from the older presets by a keyword in its notes, and its
    // high-flow nozzle from the standard nozzle's.
    assert_eq!(
        library.filaments_for_printer(MK4S_HF),
        ["Generic PLA @MK4S HF0.4", "Generic ABS @Template", "Generic PLA @Template"]
    );
    // A printer of the user's own gets whatever does not ask for a Prusa.
    let voron = library.filaments_for_printer(VORON);
    assert!(voron.contains(&"Generic PLA") && voron.contains(&"Generic PLA @Template") && !voron.contains(&"Generic PLA @MK4S HF0.4"));
    // Building blocks are never offered.
    assert!(library.filaments_for_printer(MK3S).iter().all(|name| !name.starts_with('*')));
}

#[test]
fn a_preset_is_flattened_through_all_its_parents_in_order() {
    let library = library();
    let resolved = library.resolve_filament("Generic PLA @MK4S HF0.4").unwrap();
    let value = |key: &str| resolved.settings[key].as_str().unwrap();
    // *PLAHF* is the last parent, so its values stand over *PLAPG*'s and Generic PLA's.
    assert_eq!(value("filament_max_volumetric_speed"), "24");
    assert!(value("compatible_printers_condition").contains("nozzle_high_flow[0]") && !value("compatible_printers_condition").contains("! nozzle_high_flow"));
    assert!(value("start_filament_gcode").starts_with("\"M572"));
    assert_eq!(value("temperature"), "210");
    assert_eq!(value("filament_density"), "1.24");
    // Two bundles have a `*common*` and a "Generic PLA"; each preset finds its own bundle's.
    let template = library.resolve_filament("Generic PLA @Template").unwrap();
    assert_eq!(template.settings["temperature"], "205");
    assert!(!template.settings.contains_key("min_print_speed"));
}

#[test]
fn a_catalogue_filament_becomes_a_preset_prusaslicer_can_load() {
    let library = library();
    let esun = filament("eSUN", "PLA Plus", "PLA");
    let (profile, recipe) = build_profile_with_recipe(&library, &request(&esun, MK3S)).unwrap();

    assert_eq!(profile.family, Family::Prusa);
    assert_eq!(profile.name, "eSUN PLA Plus Orange");
    assert_eq!(profile.alias(), "eSUN PLA Plus Orange");
    assert_eq!(profile.inherits, "Generic PLA");
    assert_eq!(profile.extension(), "ini");
    assert!(profile.filament_id.is_empty());

    let value = |key: &str| profile.settings[key].as_str().unwrap();
    assert_eq!(value("filament_colour"), "#FF6720");
    assert_eq!(value("filament_density"), "1.23");
    // The base's 210 °C is below what the manufacturer allows, so it is pulled up; its
    // 60 °C bed is above, so it is pulled down. The first layer's 215 °C is already fine.
    assert_eq!(value("temperature"), "215");
    assert_eq!(value("first_layer_temperature"), "215");
    assert_eq!(value("bed_temperature"), "55");
    // The base's own flow limit is lower than the filament's, so it stays.
    assert_eq!(value("filament_max_volumetric_speed"), "15");
    // It shows for the printers its base shows for: the condition is the base's, untouched.
    assert_eq!(profile.settings["compatible_printers_condition"], library.resolve_filament("Generic PLA").unwrap().settings["compatible_printers_condition"]);
    assert!(library.shows_for(&profile.settings, MK3S) && library.shows_for(&profile.settings, MK3S_06));
    assert!(!library.shows_for(&profile.settings, MK4S_HF));

    let changed: Vec<(&str, Layer)> = recipe.changes.iter().map(|c| (c.key.as_str(), c.layer)).collect();
    assert_eq!(
        changed,
        [
            ("bed_temperature", Layer::Manufacturer),
            ("filament_colour", Layer::Colour),
            ("filament_density", Layer::Manufacturer),
            ("filament_vendor", Layer::Manufacturer),
            ("first_layer_bed_temperature", Layer::Manufacturer),
            ("temperature", Layer::Manufacturer),
        ]
    );
    assert_eq!(recipe.parent, "Generic PLA");
    // The recipe rebuilds the profile from its base; the brand is a field of its own.
    let mut rebuilt = recipe.apply(&library.base_settings("Generic PLA").unwrap());
    assert_eq!(rebuilt.remove("filament_vendor"), Some("eSUN".into()));
    assert_eq!(rebuilt, profile.settings);

    let text = profile.to_ini();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("# generated by Toona "));
    let mut sorted = lines[1..].to_vec();
    sorted.sort_unstable_by_key(|line| line.split(" = ").next().unwrap_or_default().to_string());
    assert_eq!(lines[1..], sorted[..], "settings are written in order of their names");
    for line in ["filament_vendor = eSUN", "filament_type = PLA", "inherits = Generic PLA", "compatible_printers = ", "filament_notes = \"\""] {
        assert!(lines.contains(&line), "{line}");
    }
    // Reading the file back gives the same profile.
    assert_eq!(FilamentProfile::from_ini(&profile.name, &text), profile);
}

#[test]
fn the_name_follows_the_base_presets_printer_mark() {
    let library = library();
    let esun = filament("eSUN", "PLA Plus", "PLA");
    let (hf, _) = build_profile_with_recipe(&library, &request(&esun, MK4S_HF)).unwrap();
    assert_eq!(hf.name, "eSUN PLA Plus Orange @MK4S HF0.4");
    assert_eq!(hf.alias(), "eSUN PLA Plus Orange");
    assert_eq!(hf.inherits, "Generic PLA @MK4S HF0.4");
    // The filament's limit is lower than the high-flow preset's 24, so it is used.
    assert_eq!(hf.settings["filament_max_volumetric_speed"], "18");

    // A template is nobody's printer, so its mark is left off; and where the base has no
    // flow limit (0), the manufacturer's is set.
    let abs = filament("eSUN", "ABS Plus", "ABS");
    let mut wanted = request(&abs, VORON);
    wanted.variant = None;
    let (template, _) = build_profile_with_recipe(&library, &wanted).unwrap();
    assert_eq!(template.name, "eSUN ABS Plus");
    assert_eq!(template.inherits, "Generic ABS @Template");
    assert_eq!(template.settings["filament_max_volumetric_speed"], "18");

    // A name of the user's own, cleaned of what PrusaSlicer does not allow in one.
    let mut named = request(&esun, MK3S);
    named.alias = Some("Orange [spool 2]: garage");
    assert_eq!(build_profile_with_recipe(&library, &named).unwrap().0.name, "Orange (spool 2)- garage");
}

#[test]
fn the_base_is_the_vendors_own_preset_when_the_slicer_ships_one() {
    let library = library();
    let mut prusament = filament("Prusa Polymers", "Prusament PLA", "PLA");
    prusament.slicer_profiles.insert("prusaslicer".into(), "Prusament PLA".into());
    let base = choose_base(&library, &prusament, MK3S, None).unwrap();
    assert_eq!((base.preset.as_str(), base.source), ("Prusament PLA", BaseSource::VendorTuned));
    // A link the catalogue has for another slicer's library says nothing about this one.
    let mut other = filament("eSUN", "PLA Plus", "PLA");
    other.slicer_profiles.insert("bambustudio".into(), "Prusament PLA".into());
    assert_eq!(choose_base(&library, &other, MK3S, None).unwrap().source, BaseSource::Generic);
    // A base picked by hand, by either name.
    assert_eq!(choose_base(&library, &other, MK3S, Some("Generic PETG")).unwrap().source, BaseSource::Explicit);
}

#[test]
fn what_cannot_be_made_is_refused_with_a_reason() {
    let library = library();
    // PrusaSlicer's own name for one of its presets is taken.
    let mut prusament = filament("Prusament", "PLA", "PLA");
    prusament.variants.clear();
    let mut plain = request(&prusament, MK3S);
    plain.variant = None;
    let error = build_profile_with_recipe(&library, &plain).unwrap_err();
    assert!(error.to_string().contains("already has a preset of its own called \"Prusament PLA\""), "{error}");

    let mut thick = filament("eSUN", "PLA Thick", "PLA");
    thick.diameters = vec![2.85];
    assert!(matches!(build_profile_with_recipe(&library, &request(&thick, MK3S)), Err(Error::UnsupportedDiameter(_))));

    let peek = filament("eSUN", "PEEK", "PEEK");
    assert!(matches!(build_profile_with_recipe(&library, &request(&peek, MK3S)), Err(Error::NoBaseProfile { .. })));
    assert!(matches!(build_profile_with_recipe(&library, &request(&peek, "No Such Printer")), Err(Error::PrinterNotFound(_))));

    let mut resin = filament("eSUN", "Standard Resin", "Resin");
    resin.class = MaterialClass::Resin;
    let error = build_profile_with_recipe(&library, &request(&resin, MK3S)).unwrap_err();
    assert!(error.to_string().contains("PrusaSlicer only takes filament profiles"), "{error}");
}

#[test]
fn a_preset_the_user_made_is_read_with_what_it_was_made_from() {
    let slicer = slicer();
    let own = &slicer.user_filaments(&slicer.conf().unwrap()).unwrap()[0];
    let profile = FilamentProfile::read(&own.path).unwrap();
    assert_eq!(profile.family, Family::Prusa);
    assert_eq!(profile.name, "My Own PLA");
    assert_eq!(profile.inherits, "Generic PLA");
    assert_eq!(profile.filament_vendor, ["Polymaker"]);
    assert_eq!(profile.filament_type, ["PLA"]);
    assert_eq!(profile.settings["temperature"], "212");
    let library = library();
    assert!(library.shows_for(&profile.settings, MK3S) && !library.shows_for(&profile.settings, MK4S_HF));
}

/// Every preset of the PrusaSlicer installed on this computer, if there is one: each
/// condition must be understood and each preset must flatten. Run with `--ignored`.
#[test]
#[ignore = "needs PrusaSlicer's settings on this computer"]
fn the_installed_prusaslicers_own_presets_are_all_understood() {
    let slicer = Slicer::detect(SlicerKind::PrusaSlicer).unwrap();
    let library = slicer.system_library().unwrap();
    let mut seen = 0;
    for machine in library.machines() {
        for name in library.filaments_for_printer(&machine.name) {
            let resolved = library.resolve_filament(name).unwrap();
            assert!(resolved.first_string("filament_type").is_some(), "{name}");
            seen += 1;
        }
    }
    assert!(seen > 0);
    let text = std::fs::read_to_string(slicer.config_dir.join("vendor/PrusaResearch.ini")).unwrap();
    let conditions: BTreeSet<&str> = text.lines().filter_map(|line| line.strip_prefix("compatible_printers_condition = ")).collect();
    for condition in conditions.iter().filter(|condition| !condition.is_empty()) {
        for machine in library.machines().iter().take(40) {
            assert!(toona_core::prusa::understood(condition, &library, &machine.name), "{condition}");
        }
    }
}
