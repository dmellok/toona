//! OrcaSlicer's own presets, where it is installed on this computer.

use std::path::PathBuf;

use toona_core::SystemLibrary;

/// Every vendor OrcaSlicer ships, and every filament each printer is offered, can be read
/// and worked out in full.
#[test]
#[ignore = "needs OrcaSlicer on this computer"]
fn the_installed_orcaslicers_own_presets_are_all_understood() {
    let shipped = PathBuf::from("/Applications/OrcaSlicer.app/Contents/Resources/profiles");
    let library = SystemLibrary::load_orca(&shipped).expect("every shipped vendor reads");
    assert!(library.machines().len() > 500, "only {} printers", library.machines().len());

    let mut resolved = 0;
    for machine in library.machines() {
        let offered = library.filaments_for_printer(&machine.name);
        assert!(!offered.is_empty(), "{} is offered no filaments", machine.name);
        for name in offered {
            let filament = library.resolve_filament(name).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(filament.first_string("filament_type").is_some(), "{name} has no material");
            resolved += 1;
        }
    }
    println!("{} printers, {resolved} filament presets worked out", library.machines().len());
}
