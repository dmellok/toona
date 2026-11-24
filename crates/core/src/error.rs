use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{slicer} config folder not found at {path}")]
    SlicerNotFound { slicer: &'static str, path: PathBuf },

    #[error("{0} is running. Quit it before changing its profiles.")]
    SlicerRunning(&'static str),

    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("system preset \"{0}\" not found")]
    PresetNotFound(String),

    #[error("system preset \"{0}\" has a circular inherits/include chain")]
    PresetCycle(String),

    #[error("invalid profile \"{name}\": {}", problems.join("; "))]
    InvalidProfile { name: String, problems: Vec<String> },

    #[error("no Bambu base profile for material \"{material}\" on printer \"{printer}\". Pass a base profile explicitly.")]
    NoBaseProfile { material: String, printer: String },

    #[error("the slicer has no \"{base}\" preset for {printer}. Choose another base profile, or leave that nozzle out.")]
    BaseNotForPrinter { base: String, printer: String },

    #[error("printer preset \"{0}\" not found. Run `printers` to list the available ones.")]
    PrinterNotFound(String),

    #[error("no printer is set up in the slicer")]
    NoPrinters,

    #[error("\"{0}\" matches no filament in the catalogue")]
    FilamentNotFound(String),

    #[error("\"{query}\" is ambiguous: {}", candidates.join(", "))]
    Ambiguous { query: String, candidates: Vec<String> },

    #[error("{0} is only available in 2.85 mm, which Bambu printers cannot use")]
    UnsupportedDiameter(String),

    #[error("{product} is a {class}; {slicer} only takes filament profiles")]
    UnsupportedMaterialClass { product: String, class: &'static str, slicer: &'static str },

    #[error("\"{name}\" cannot be converted: {reason}")]
    NotConvertible { name: String, reason: String },

    #[error("{path} already exists and was not created by this tool; refusing to overwrite it")]
    ForeignFile { path: PathBuf },

    #[error(
        "the slicer syncs user presets to Bambu Cloud for account {user_id}; installing would upload the new profiles at its next start. Confirm cloud sync to proceed, or export a bundle instead."
    )]
    CloudSyncNotConfirmed { user_id: String },

    #[error("nothing installed matches \"{0}\"")]
    NotInstalled(String),

    #[error("catalogue download failed: {0}")]
    Download(String),

    #[error("catalogue is not cached yet and could not be downloaded: {0}")]
    NoCatalogue(String),

    #[error("backup failed: {0}")]
    Backup(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub(crate) fn read(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Error {
        let path = path.into();
        move |source| Error::Read { path, source }
    }

    pub(crate) fn write(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Error {
        let path = path.into();
        move |source| Error::Write { path, source }
    }

    pub(crate) fn json(path: impl Into<PathBuf>) -> impl FnOnce(serde_json::Error) -> Error {
        let path = path.into();
        move |source| Error::Json { path, source }
    }
}
