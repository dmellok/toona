//! Writes real answers from the backend as JSON, so the window can be developed
//! and screenshotted in a plain browser. Read-only: installs are dry runs.
//! Usage: cargo run -p toona-app --example dump -- <out dir>

use toona_app_lib::api::{Backend, InstallRequest};

fn json<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: dump <out dir>"));
    std::fs::create_dir_all(&out).unwrap();
    let backend = Backend::standard().unwrap();
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
