//! Writes real answers from the backend as JSON, so the window can be developed
//! and screenshotted in a plain browser. Read-only: installs are dry runs.
//! Usage: cargo run -p toona-app --example dump -- <out dir> [catalogue key]...

use toona_app_lib::api::{Backend, InstallRequest};

fn json<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: dump <out dir>"));
    std::fs::create_dir_all(&out).unwrap();
    // TOONA_DATA_DIR keeps Toona's own files (what it added, the catalogue) apart from the real ones.
    let mut backend = match std::env::var_os("TOONA_DATA_DIR") {
        Some(root) => Backend::new(toona_core::AppDirs::under(std::path::Path::new(&root))),
        None => Backend::standard().unwrap(),
    };
    // TOONA_SLICER (a slicer's id) and TOONA_SLICER_DIR (its settings folder) pick what to dump.
    if let Some(kind) = std::env::var("TOONA_SLICER").ok().and_then(|id| toona_core::SlicerKind::from_id(&id)) {
        backend = backend.with_slicer(kind, std::env::var_os("TOONA_SLICER_DIR").map(std::path::PathBuf::from));
    }
    let write = |name: &str, value: serde_json::Value| {
        std::fs::write(out.join(format!("{name}.json")), serde_json::to_string(&value).unwrap()).unwrap();
    };
    let result = |r: Result<serde_json::Value, toona_app_lib::api::UiError>| match r {
        Ok(value) => serde_json::json!({ "ok": value }),
        Err(error) => serde_json::json!({ "err": error }),
    };

    let status = backend.status();
    let printer = status.printers.iter().find(|p| p.selected).or(status.printers.first()).map(|p| p.name.clone());
    write("status", json(&status));
    write("catalogue", result(backend.catalogue(false).map(|c| json(&c))));
    write("in_studio", result(backend.in_studio().map(|c| json(&c))));
    for key in std::env::args().skip(2) {
        let detail = backend.detail(&key, printer.as_deref(), None, None, None, false);
        write(&format!("detail-{}", key.replace('/', "~")), result(detail.map(|d| json(&d))));
    }
    if let Some(printer) = printer {
        let request = InstallRequest {
            key: "polymaker/PLA/polylite_pla".into(),
            variant: Some("Cold White".into()),
            printers: vec![printer],
            base: None,
            alias: None,
            borrow_tuned: false,
            allow_cloud_sync: false,
            dry_run: true,
        };
        write("install-dry", result(backend.install(&request).map(|r| json(&r))));
    }
}
