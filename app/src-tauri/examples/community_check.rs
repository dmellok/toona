//! Walks the account and sharing calls against a local Toona API, using a copy of the
//! real install record in a temporary folder. Nothing is written to the slicer: the
//! install is a dry run. Start the API first (see api/README.md), then:
//!   TOONA_API=http://localhost:8788 cargo run -p toona-app --example community_check

use toona_app_lib::api::{Backend, EditDto, InstallRequest};
use toona_core::AppDirs;

fn main() {
    assert!(std::env::var("TOONA_API").is_ok(), "set TOONA_API to a local server");
    let real = AppDirs::standard().unwrap();
    let temp = std::env::temp_dir().join(format!("toona-community-check-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::copy(real.data_dir.join("manifest.json"), temp.join("manifest.json")).expect("no install record to share from");
    let dirs = AppDirs { data_dir: temp.clone(), cache_dir: real.cache_dir.clone() };
    let run = std::process::id();

    let as_user = |handle: &str| {
        std::env::set_var("TOONA_DEV_HANDLE", format!("{handle}-{run}"));
        let backend = Backend::new(AppDirs { data_dir: dirs.data_dir.clone(), cache_dir: dirs.cache_dir.clone() });
        let start = backend.sign_in_start().expect("sign in");
        assert!(start.done);
        backend
    };

    let author = as_user("author");
    let account = author.account();
    assert!(account.available, "the server did not answer");
    println!("signed in as @{}", account.user.unwrap().handle);

    let added = author.in_studio().unwrap().added;
    let row = added.first().expect("nothing added by Toona to share");
    let status = author.status();
    let printer = status.printers.iter().find(|p| p.name == row.printer).expect("printer of the added profile");

    let shared = author.share(&row.profile_name, "Shared from the check").expect("share");
    println!("shared v{} of {} with {} changes, built on {}", shared.id, row.alias, shared.changes.len(), shared.built_on);
    assert!(shared.own && shared.parent_version.is_none());
    assert!(shared.changes.iter().all(|c| c.key != "default_filament_colour" && !c.key.contains("gcode")));

    let again = author.share(&row.profile_name, "An edit").expect("share again");
    assert_eq!(again.parent_version, Some(shared.id), "a second share is a child of the first");

    assert!(author.confirm(shared.id, "worked", &printer.model, &printer.nozzle).is_err(), "own profile");

    let other = as_user("printer");
    let list = other.community(&row.catalogue_key, &printer.model, &printer.nozzle).expect("list");
    assert!(list.iter().any(|v| v.id == shared.id && !v.own && v.unusable.is_none()));
    let confirmed = other.confirm(shared.id, "worked", &printer.model, &printer.nozzle).expect("confirm");
    assert_eq!((confirmed.worked, confirmed.mine.as_deref()), (1, Some("worked")));
    println!("confirmed by a second user: {} worked", confirmed.worked);

    let plan = other
        .install_community(
            &InstallRequest {
                key: row.catalogue_key.clone(),
                variant: None,
                printers: vec![],
                base: None,
                allow_cloud_sync: false,
                dry_run: true,
            },
            again.id,
        )
        .expect("dry-run install of the shared version");
    assert!(plan.dry_run);
    println!("dry-run install would write {} profile(s): {}", plan.profiles.len(), plan.profiles[0].name);

    // A profile's page, and editing it (dry run: nothing is written).
    let page = author.profile(&row.profile_name).expect("profile page");
    assert!(page.editable && page.version_id == Some(again.id));
    let nozzle = page.settings.iter().find(|s| s.key == "nozzle_temperature").expect("nozzle setting");
    assert!(nozzle.editable);
    println!("profile page: {} settings, built on {}, nozzle {}", page.settings.len(), page.built_on, nozzle.value);
    let edit = |key: &str, value: &str| vec![EditDto { key: key.into(), value: value.into() }];
    let dry = author.edit(&row.profile_name, &edit("nozzle_temperature", "215"), false, true).expect("dry-run edit");
    assert!(dry.dry_run && dry.files.len() == 2);
    assert!(author.edit(&row.profile_name, &edit("nozzle_temperature", "900"), false, true).is_err(), "out of range");
    assert!(author.edit(&row.profile_name, &edit("nozzle_temperature", "hot"), false, true).is_err(), "not a number");
    assert!(author.edit(&row.profile_name, &edit("filament_start_gcode", "1"), false, true).is_err(), "not editable");

    let family = other.family(again.id).expect("family");
    assert_eq!(family.iter().map(|v| (v.id, v.parent_version)).collect::<Vec<_>>(), [(shared.id, None), (again.id, Some(shared.id))]);

    // The user's own custom filaments: listed, linkable to a product, then shareable, never editable.
    let own = author.in_studio().unwrap().own;
    println!("own filaments: {:?}", own.iter().map(|o| o.alias.as_str()).collect::<Vec<_>>());
    if let Some(mine) = own.first() {
        assert!(mine.catalogue_key.is_none() && mine.changes.is_empty());
        assert!(author.share(&mine.profile_name, "").is_err(), "needs a product first");
        author.link_product(&mine.profile_name, Some(&row.catalogue_key)).expect("link");
        let linked = author.in_studio().unwrap().own.into_iter().find(|o| o.profile_name == mine.profile_name).unwrap();
        println!("linked {} to {}: {} changes over {}", linked.alias, linked.product.as_deref().unwrap(), linked.changes.len(), linked.parent.as_deref().unwrap_or("?"));
        assert!(linked.catalogue_key.is_some());
        let page = author.profile(&mine.profile_name).expect("own profile page");
        assert!(!page.editable && page.settings.iter().all(|s| !s.editable));
        assert!(author.edit(&mine.profile_name, &edit("nozzle_temperature", "215"), false, true).is_err(), "own profiles are not edited");
        if !linked.changes.is_empty() {
            let shared_own = author.share(&mine.profile_name, "My own tuning").expect("share own");
            println!("shared own filament as v{}", shared_own.id);
        }
        author.link_product(&mine.profile_name, None).expect("unlink");
        assert!(author.in_studio().unwrap().own.iter().all(|o| o.catalogue_key.is_none()));
    }

    other.sign_out();
    assert!(other.account().user.is_none());
    std::fs::remove_dir_all(&temp).ok();
    println!("all community checks passed");
}
