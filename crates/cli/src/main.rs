//! Command-line front end. All logic is in `toona-core`; this file only parses
//! arguments and prints.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use toona_core::install::{self, installed_matching};
use toona_core::catalogue::barcode_digits;
use toona_core::mapping::{choose_base, profile_alias};
use toona_core::{convert_inherited_with_recipes, ConvertRequest, UserFilament};
use toona_core::{
    build_profile_with_recipe, AppConf, AppDirs, Catalogue, CatalogueFilament, InstallOptions, Manifest, MaterialClass,
    PlannedProfile,
    PrinterPreset, ProfileRequest, Refresh, SearchFilter, Slicer, SlicerKind, SystemLibrary, Variant,
};

/// The catalogue is published daily; a cached copy is refreshed after this long.
const CATALOGUE_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Parser)]
#[command(name = "toona", version, about = "Add catalogue filaments to Bambu Studio as custom filament profiles")]
struct Cli {
    /// Slicer to work with
    #[arg(long, global = true, value_enum, default_value_t = SlicerArg::Bambu)]
    slicer: SlicerArg,

    /// Slicer config folder, instead of the standard location
    #[arg(long, global = true, value_name = "DIR")]
    config_dir: Option<PathBuf>,

    /// Folder for this tool's manifest, backups and catalogue cache
    #[arg(long, global = true, value_name = "DIR")]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum SlicerArg {
    Bambu,
    /// Experimental: not verified against a real install
    Orca,
    Prusa,
}

#[derive(Clone, Copy, ValueEnum)]
enum ClassArg {
    Filament,
    Resin,
}

impl From<ClassArg> for MaterialClass {
    fn from(arg: ClassArg) -> Self {
        match arg {
            ClassArg::Filament => Self::Filament,
            ClassArg::Resin => Self::Resin,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Search the filament catalogue
    Search {
        /// Words that must all match the brand, name or material, or a barcode
        query: Vec<String>,
        #[arg(long)]
        brand: Option<String>,
        #[arg(long)]
        material: Option<String>,
        /// Only filament or only resin
        #[arg(long, value_enum)]
        class: Option<ClassArg>,
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    /// Show one filament, its colours and the base profile it would use
    Show {
        /// Catalogue key (brand/material/filament), a barcode, or words that match exactly one filament
        filament: String,
    },
    /// Write a filament's profiles into the slicer's user folder
    Install(InstallArgs),
    /// Turn imported profiles that the AMS cannot use into custom filaments that it can
    Convert(ConvertArgs),
    /// Remove profiles this tool installed
    Uninstall {
        /// Catalogue key, filament name or profile name
        filament: String,
    },
    /// List the profiles this tool installed
    ListInstalled,
    /// Show the detected slicer folder, account and printer presets
    Printers,
    /// Download the latest catalogue
    Update,
}

#[derive(Args)]
struct InstallArgs {
    /// Catalogue key (brand/material/filament), a barcode, or words that match exactly one filament
    filament: String,

    /// Colour to name and tint the profile with (see `show`)
    #[arg(long, value_name = "COLOUR")]
    variant: Option<String>,

    /// Printer preset to install for; repeatable. Default: the one selected in the slicer
    #[arg(long, value_name = "PRESET")]
    printer: Vec<String>,

    /// Install for every nozzle size enabled in the slicer
    #[arg(long, conflicts_with = "printer")]
    all_printers: bool,

    /// System filament preset to build on, instead of the automatic choice
    #[arg(long, value_name = "PRESET")]
    base: Option<String>,

    /// Write importable .bbsflmt bundles into DIR instead of the slicer's folder
    #[arg(long, value_name = "DIR")]
    export: Option<PathBuf>,

    /// Agree that the slicer will upload the new profiles to its cloud at its next start
    #[arg(long)]
    allow_cloud_sync: bool,

    /// Show what would be written without writing
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args)]
struct ConvertArgs {
    /// Name of the imported profile. Without it, the convertible profiles are listed
    profile: Option<String>,

    /// Convert every imported profile
    #[arg(long, conflicts_with_all = ["profile", "name"])]
    all: bool,

    /// Filament name for the new profile. Default: the imported profile's name
    #[arg(long, value_name = "NAME")]
    name: Option<String>,

    /// Brand to record. Default: the brand of the preset the profile inherits from
    #[arg(long, value_name = "BRAND")]
    vendor: Option<String>,

    /// Write importable .bbsflmt bundles into DIR instead of the slicer's folder
    #[arg(long, value_name = "DIR")]
    export: Option<PathBuf>,

    /// Agree that the slicer will upload the new profiles to its cloud at its next start
    #[arg(long)]
    allow_cloud_sync: bool,

    /// Show what would be written without writing
    #[arg(long)]
    dry_run: bool,
}

struct Context_ {
    slicer: Slicer,
    conf: AppConf,
    library: SystemLibrary,
    dirs: AppDirs,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dirs = match &cli.data_dir {
        Some(root) => AppDirs::under(root),
        None => AppDirs::standard()?,
    };

    match &cli.command {
        Command::Search { query, brand, material, class, limit } => {
            let catalogue = load_catalogue(&dirs, Refresh::IfOlderThan(CATALOGUE_MAX_AGE))?;
            let filter = SearchFilter {
                query: &query.join(" "),
                brand: brand.as_deref(),
                material: material.as_deref(),
                class: class.map(MaterialClass::from),
            };
            if !search_barcode(&catalogue, filter.query) {
                search(&catalogue, &filter, *limit);
            }
        }
        Command::Update => {
            let catalogue = load_catalogue(&dirs, Refresh::Force)?;
            println!("Catalogue {}: {} filaments from {} brands.", catalogue.version, catalogue.len(), catalogue.brands().len());
        }
        Command::Show { filament } => {
            let catalogue = load_catalogue(&dirs, Refresh::IfOlderThan(CATALOGUE_MAX_AGE))?;
            // Works without a slicer; the base profile line needs one.
            let context = open(&cli, dirs.clone()).ok();
            let (found, scanned) = resolve_target(&catalogue, filament)?;
            show(found, context.as_ref());
            if let Some(variant) = scanned {
                println!("  scanned    {}", variant.name);
            }
        }
        Command::Install(args) => {
            let catalogue = load_catalogue(&dirs, Refresh::IfOlderThan(CATALOGUE_MAX_AGE))?;
            install_command(&open(&cli, dirs)?, &catalogue, args)?;
        }
        Command::Convert(args) => convert_command(&open(&cli, dirs)?, args)?,
        Command::Uninstall { filament } => uninstall_command(&open(&cli, dirs)?, filament)?,
        Command::ListInstalled => list_installed(&dirs)?,
        Command::Printers => printers_command(&open(&cli, dirs)?)?,
    }
    Ok(())
}

fn open(cli: &Cli, dirs: AppDirs) -> Result<Context_> {
    let kind = match cli.slicer {
        SlicerArg::Bambu => SlicerKind::BambuStudio,
        SlicerArg::Orca => SlicerKind::OrcaSlicer,
        SlicerArg::Prusa => SlicerKind::PrusaSlicer,
    };
    let slicer = match &cli.config_dir {
        Some(dir) => Slicer::at(kind, dir)?,
        None => Slicer::detect(kind)?,
    };
    let conf = slicer.conf()?;
    let library = slicer.system_library()?;
    Ok(Context_ { slicer, conf, library, dirs })
}

fn load_catalogue(dirs: &AppDirs, refresh: Refresh) -> Result<Catalogue> {
    let catalogue = Catalogue::load(&dirs.cache_dir, refresh)?;
    if let Some(warning) = &catalogue.warning {
        eprintln!("warning: {warning}");
    }
    Ok(catalogue)
}

/// Prints what a scanned or typed barcode identifies. Returns false if `query` is not a barcode.
fn search_barcode(catalogue: &Catalogue, query: &str) -> bool {
    if barcode_digits(query).is_none() {
        return false;
    }
    let matches = catalogue.find_barcode(query);
    if matches.is_empty() {
        println!("No filament in the catalogue has barcode {}.", query.trim());
    }
    for found in &matches {
        println!(
            "{}  {} {}, {}  [{}]",
            found.filament.key, found.filament.brand, found.filament.name, found.variant.name, found.filament.material
        );
    }
    true
}

/// The filament, and for a barcode also the colour, that `query` names.
fn resolve_target<'a>(catalogue: &'a Catalogue, query: &str) -> Result<(&'a CatalogueFilament, Option<&'a Variant>)> {
    if barcode_digits(query).is_none() {
        return Ok((catalogue.find(query)?, None));
    }
    let matches = catalogue.find_barcode(query);
    match matches.as_slice() {
        [] => bail!("no filament in the catalogue has barcode {}", query.trim()),
        [only] => Ok((only.filament, Some(only.variant))),
        many => bail!(
            "barcode {} is listed on more than one filament: {}. Install by key and --variant instead.",
            query.trim(),
            many.iter().map(|m| format!("{} ({})", m.filament.key, m.variant.name)).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn search(catalogue: &Catalogue, filter: &SearchFilter, limit: usize) {
    let matches = catalogue.search(filter);
    if matches.is_empty() {
        println!("No filaments match.");
        return;
    }
    let key_width = matches.iter().take(limit).map(|f| f.key.len()).max().unwrap_or(0);
    for filament in matches.iter().take(limit) {
        println!(
            "{:key_width$}  {} {}  [{}, {} {}{}]",
            filament.key,
            filament.brand,
            filament.name,
            filament.material,
            filament.variants.len(),
            if filament.variants.len() == 1 { "colour" } else { "colours" },
            if filament.discontinued { ", discontinued" } else { "" },
        );
    }
    if matches.len() > limit {
        println!("… {} more. Narrow the search or raise --limit.", matches.len() - limit);
    }
}

fn show(filament: &CatalogueFilament, context: Option<&Context_>) {
    let range = |low: Option<f64>, high: Option<f64>, unit: &str| match (low, high) {
        (Some(low), Some(high)) => format!("{low}–{high} {unit}"),
        _ => "not in the database".to_string(),
    };
    println!("{} {}", filament.brand, filament.name);
    println!("  key        {}", filament.key);
    println!("  material   {} ({})", filament.material, filament.class.label());
    println!("  density    {}", filament.density.map_or("not in the database".into(), |d| format!("{d} g/cm³")));
    println!(
        "  diameter   {}",
        if filament.diameters.is_empty() {
            "not in the database".to_string()
        } else {
            filament.diameters.iter().map(|d| format!("{d} mm")).collect::<Vec<_>>().join(", ")
        }
    );
    println!("  nozzle     {}", range(filament.min_print_temperature, filament.max_print_temperature, "°C"));
    println!("  bed        {}", range(filament.min_bed_temperature, filament.max_bed_temperature, "°C"));
    if !filament.traits.is_empty() {
        println!("  traits     {}", filament.traits.iter().cloned().collect::<Vec<_>>().join(", "));
    }
    if filament.discontinued {
        println!("  status     discontinued");
    }

    if let Some(context) = context {
        for printer in context.slicer.printers(&context.conf, &context.library).iter().filter(|p| p.selected) {
            match choose_base(&context.library, filament, &printer.name, None) {
                Ok(base) => println!("  base       {} ({})", base.preset, base.source.label()),
                Err(error) => println!("  base       none: {error}"),
            }
        }
    }

    println!("  colours    {}", filament.variants.len());
    for variant in &filament.variants {
        println!("    {}  {}", variant.color_hex.as_deref().unwrap_or("       "), variant.name);
    }
}

fn install_command(context: &Context_, catalogue: &Catalogue, args: &InstallArgs) -> Result<()> {
    let Context_ { slicer, conf, library, dirs } = context;
    let (filament, scanned) = resolve_target(catalogue, &args.filament)?;
    let variant = match &args.variant {
        Some(name) => Some(
            filament
                .variant(name)
                .with_context(|| format!("{} {} has no colour \"{name}\". Run `show` to list them.", filament.brand, filament.name))?,
        ),
        // A barcode already says which colour it is.
        None => scanned,
    };

    let printers = target_printers(context, args)?;
    let existing = slicer.user_filaments(conf)?;
    let mut planned = Vec::new();
    for printer in &printers {
        let (profile, recipe) = build_profile_with_recipe(
            library,
            &ProfileRequest {
                filament,
                variant,
                printer: &printer.name,
                base: args.base.as_deref(),
                user_id: conf.user_id(),
                existing: &existing,
                alias: None,
                borrow_tuned: false,
            },
        )
        .with_context(|| format!("for printer {}", printer.name))?;
        let base = choose_base(library, filament, &printer.name, args.base.as_deref())?;
        let line = format!(
            "{}  ({}based on {}: {}, {} settings changed)",
            profile.name,
            if profile.filament_id.is_empty() { String::new() } else { format!("id {}, ", profile.filament_id) },
            base.preset,
            base.source.label(),
            recipe.changes.len()
        );
        let item = PlannedProfile { catalogue_key: filament.key.clone(), printer: printer.name.clone(), profile, recipe: Some(recipe) };
        if item.add_to(&mut planned) {
            println!("{line}");
        }
    }

    if let Some(out_dir) = &args.export {
        let profiles: Vec<_> = planned.into_iter().map(|p| p.profile).collect();
        for path in install::export_bundles(&profiles, out_dir, conf.user_id())? {
            println!("Wrote {}", path.display());
        }
        let menu = match slicer.kind.family() {
            toona_core::Family::Bambu => "Import Configs",
            toona_core::Family::Prusa => "Import Config Bundle",
        };
        println!("Import with File > Import > {menu} in {}.", slicer.kind.display_name());
        return Ok(());
    }

    let options = InstallOptions { confirm_cloud_sync: args.allow_cloud_sync, dry_run: args.dry_run };
    let report = install::install(slicer, conf, dirs, &planned, options)?;
    let verb = if report.dry_run { "Would write" } else { "Wrote" };
    for file in &report.files {
        println!("{verb} {}", file.display());
    }
    if let Some(backup) = &report.backup {
        println!("Backup of the user folder: {}", backup.display());
    }
    if report.will_cloud_sync {
        println!(
            "{} syncs user presets for account {}: it {} upload {} to Bambu Cloud at its next start.",
            slicer.kind.display_name(),
            conf.preset_folder,
            if report.dry_run { "would" } else { "will" },
            if planned.len() == 1 { "this profile" } else { "these profiles" },
        );
        if report.dry_run && !args.allow_cloud_sync {
            println!("Installing for real needs --allow-cloud-sync.");
        }
    }
    if !report.dry_run {
        println!("Start {} and look for \"{}\".", slicer.kind.display_name(), profile_alias(filament, variant));
    }
    Ok(())
}

fn convert_command(context: &Context_, args: &ConvertArgs) -> Result<()> {
    let Context_ { slicer, conf, library, dirs } = context;
    let existing = slicer.user_filaments(conf)?;
    let inherited: Vec<&UserFilament> = existing.iter().filter(|f| !f.is_root).collect();

    let sources: Vec<&UserFilament> = match (&args.profile, args.all) {
        (Some(name), _) => {
            let found = inherited.iter().find(|f| f.name.eq_ignore_ascii_case(name.trim()));
            vec![*found.with_context(|| format!("no imported profile is called \"{name}\". Run `convert` to list them."))?]
        }
        (None, true) => inherited,
        (None, false) => {
            if inherited.is_empty() {
                println!("No imported profiles to convert.");
            }
            for filament in &inherited {
                println!("{}", filament.name);
            }
            if !inherited.is_empty() {
                println!("Convert one with `convert \"<name>\"`, or all of them with `convert --all`.");
            }
            return Ok(());
        }
    };

    let printers = slicer.printers(conf, library);
    let mut planned = Vec::new();
    for source in sources {
        let profiles = convert_inherited_with_recipes(
            library,
            &ConvertRequest {
                source,
                alias: args.name.as_deref(),
                vendor: args.vendor.as_deref(),
                printers: &printers,
                user_id: conf.user_id(),
                existing: &existing,
            },
        )?;
        for (profile, recipe) in profiles {
            println!(
                "{}  ->  {}  (id {}, brand {})",
                source.name, profile.name, profile.filament_id, profile.filament_vendor[0]
            );
            planned.push(PlannedProfile {
                catalogue_key: format!("converted/{}", source.name),
                printer: profile.compatible_printers[0].clone(),
                profile,
                recipe: Some(recipe),
            });
        }
    }

    if let Some(out_dir) = &args.export {
        let profiles: Vec<_> = planned.into_iter().map(|p| p.profile).collect();
        for path in install::export_bundles(&profiles, out_dir, conf.user_id())? {
            println!("Wrote {}", path.display());
        }
        return Ok(());
    }

    let options = InstallOptions { confirm_cloud_sync: args.allow_cloud_sync, dry_run: args.dry_run };
    let report = install::install(slicer, conf, dirs, &planned, options)?;
    let verb = if report.dry_run { "Would write" } else { "Wrote" };
    for file in &report.files {
        println!("{verb} {}", file.display());
    }
    if let Some(backup) = &report.backup {
        println!("Backup of the user folder: {}", backup.display());
    }
    if report.will_cloud_sync {
        println!(
            "{} syncs user presets for account {}: it {} upload the new profiles to Bambu Cloud at its next start.",
            slicer.kind.display_name(),
            conf.preset_folder,
            if report.dry_run { "would" } else { "will" },
        );
        if report.dry_run && !args.allow_cloud_sync {
            println!("Converting for real needs --allow-cloud-sync.");
        }
    }
    println!("The imported profiles are left as they are. Delete them in the slicer once the new ones work.");
    Ok(())
}

fn target_printers(context: &Context_, args: &InstallArgs) -> Result<Vec<PrinterPreset>> {
    let available = context.slicer.printers(&context.conf, &context.library);
    if available.is_empty() {
        bail!(toona_core::Error::NoPrinters);
    }
    if args.all_printers {
        return Ok(available);
    }
    if args.printer.is_empty() {
        let selected: Vec<_> = available.iter().filter(|p| p.selected).cloned().collect();
        if selected.is_empty() {
            bail!("no printer preset is selected in the slicer; pass --printer or --all-printers");
        }
        return Ok(selected);
    }
    args.printer
        .iter()
        .map(|name| {
            available
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .cloned()
                .ok_or_else(|| toona_core::Error::PrinterNotFound(name.clone()).into())
        })
        .collect()
}

fn uninstall_command(context: &Context_, query: &str) -> Result<()> {
    let Context_ { slicer, dirs, .. } = context;
    let manifest = Manifest::load(dirs)?;
    for profile in installed_matching(&manifest, slicer, query) {
        println!("Removing {}", profile.profile_name);
    }
    let report = install::uninstall(slicer, dirs, query)?;
    for file in &report.removed {
        println!("Removed {}", file.display());
    }
    for file in &report.missing {
        println!("Already gone: {}", file.display());
    }
    if let Some(backup) = &report.backup {
        println!("Backup of the user folder: {}", backup.display());
    }
    if !report.cloud_synced.is_empty() {
        println!(
            "Bambu Cloud still holds a copy of: {}. {} will download {} again at its next sync; to remove {} for good, delete {} inside {}.",
            report.cloud_synced.join(", "),
            slicer.kind.display_name(),
            if report.cloud_synced.len() == 1 { "it" } else { "them" },
            if report.cloud_synced.len() == 1 { "it" } else { "them" },
            if report.cloud_synced.len() == 1 { "it" } else { "them" },
            slicer.kind.display_name(),
        );
    }
    Ok(())
}

fn list_installed(dirs: &AppDirs) -> Result<()> {
    let manifest = Manifest::load(dirs)?;
    if manifest.profiles.is_empty() {
        println!("Nothing installed by this tool.");
        return Ok(());
    }
    for profile in &manifest.profiles {
        let present = profile.files.iter().all(|f| f.exists());
        let synced = profile
            .files
            .iter()
            .filter(|f| f.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|f| Slicer::preset_info(f))
            .any(|info| info.is_cloud_synced());
        println!(
            "{}  [{}, id {}, {}, installed {}{}{}]",
            profile.profile_name,
            profile.catalogue_key,
            profile.filament_id,
            profile.slicer,
            profile.installed_at,
            if synced { ", synced to cloud" } else { "" },
            if present { "" } else { ", FILES MISSING" },
        );
    }
    Ok(())
}

fn printers_command(context: &Context_) -> Result<()> {
    let Context_ { slicer, conf, library, .. } = context;
    println!("{} config: {}", slicer.kind.display_name(), slicer.config_dir.display());
    println!("Running: {}", if slicer.is_running()? { "yes" } else { "no" });
    println!("System profiles: {}", library.version);
    println!("User folders: {}", slicer.user_folders()?.join(", "));
    println!(
        "Active account: {}  (cloud sync of user presets: {})",
        conf.user_id().unwrap_or("signed out"),
        if conf.uploads_new_presets() { "on" } else { "off" },
    );
    println!("Profiles are written to: {}", slicer.custom_filament_dir(conf).display());

    let printers = slicer.printers(conf, library);
    if printers.is_empty() {
        println!("No printers are set up.");
    }
    for printer in &printers {
        println!(
            "  {}{}  ({} system filament presets)",
            printer.name,
            if printer.selected { "  [selected]" } else { "" },
            library.filaments_for_printer(&printer.name).len(),
        );
    }

    let filaments = slicer.user_filaments(conf)?;
    println!("Existing user filament profiles: {}", filaments.len());
    for filament in &filaments {
        println!(
            "  {}  ({})",
            filament.name,
            if filament.is_root { "custom, usable in the AMS" } else { "inherits a system preset, not offered for the AMS" },
        );
    }
    Ok(())
}
