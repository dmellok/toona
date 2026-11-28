//! What the person chose about update checks and usage numbers, and the hello that
//! the second choice allows.
//!
//! Nothing here runs until they have answered the first-run question. The hello carries
//! no id: the app remembers the day it last said hello and tells the server whether
//! this is its first of the week or month, so installs can be counted but not told apart.
//!
//! A hello also names the catalogue products added since the last one, each told once.
//! People who turned usage numbers on when the question promised "never which filaments"
//! keep sending the hello without them until they have seen the new wording and saved it.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::{Backend, UiError};

type UiResult<T> = Result<T, UiError>;

/// The wording of the usage question this version asks. 2 added which products were added.
const USAGE_TERMS: u32 = 2;
/// The most products one hello names.
const MAX_PRODUCTS: usize = 100;

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    /// The first-run question was answered.
    #[serde(default)]
    asked: bool,
    #[serde(default)]
    updates: bool,
    #[serde(default)]
    usage: bool,
    /// Days since 1970 of the last hello that reached the server.
    #[serde(default)]
    last_hello: Option<i64>,
    /// The [`USAGE_TERMS`] the person last answered. 0 for answers from before it was kept.
    #[serde(default)]
    usage_terms: u32,
    /// When the newest product a hello has named was added, as the manifest writes times.
    #[serde(default)]
    reported_until: Option<String>,
    /// The slicer chosen in the window, as its id. Unset until one is chosen.
    #[serde(default)]
    slicer: Option<String>,
}

#[derive(Serialize)]
pub struct PrefsDto {
    pub asked: bool,
    pub updates: bool,
    pub usage: bool,
    pub version: &'static str,
    /// Usage numbers are on, but were agreed to before they named products: the question
    /// is to be shown again, and until it is answered a hello leaves products out.
    pub review: bool,
    /// Exactly what a hello sent now would contain, once the question as it now reads is agreed to.
    pub report: Value,
}

fn today() -> i64 {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (seconds / 86_400) as i64
}

/// Weeks start on Monday; day 0 (1 January 1970) was a Thursday.
fn week(day: i64) -> i64 {
    (day + 3).div_euclid(7)
}

/// Year and month of a day count, by the usual civil-calendar arithmetic.
fn month(day: i64) -> (i64, i64) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month)
}

/// Major and minor only: "15.1", not a build number.
fn os_version() -> String {
    sysinfo::System::os_version()
        .ok()
        .map(|full| full.split('.').take(2).collect::<Vec<_>>().join("."))
        .filter(|short| short.chars().all(|c| c.is_ascii_digit() || c == '.'))
        .unwrap_or_default()
}

impl Backend {
    fn prefs_path(&self) -> PathBuf {
        self.data_dir().join("prefs.json")
    }

    fn stored(&self) -> Stored {
        std::fs::read_to_string(self.prefs_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn store(&self, stored: &Stored) -> UiResult<()> {
        std::fs::create_dir_all(self.data_dir())
            .and_then(|_| std::fs::write(self.prefs_path(), serde_json::to_string(stored).unwrap_or_default()))
            .map_err(|error| UiError { kind: "other", message: format!("Could not save the setting: {error}") })
    }

    /// A hello, and when the newest product it names was added.
    fn report(&self, stored: &Stored, day: i64, with_products: bool) -> (Value, Option<String>) {
        let (mut products, latest) = match with_products {
            true => self.products_added_since(stored.reported_until.as_deref()),
            false => (Vec::new(), None),
        };
        products.truncate(MAX_PRODUCTS);
        let products: Vec<Value> = products.into_iter().map(|(key, slicer)| json!({ "key": key, "slicer": slicer })).collect();
        let status = self.status();
        let mut printers: Vec<String> = status.printers.iter().map(|p| p.model.clone()).collect();
        printers.sort();
        printers.dedup();
        let last = stored.last_hello;
        let hello = json!({
            "version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "os_version": os_version(),
            "arch": std::env::consts::ARCH,
            "slicer": if status.found { status.slicer } else { "" },
            "printers": printers,
            "added": self.added_count(),
            "products": products,
            "first": last.is_none(),
            "new_week": last.is_none_or(|last| week(last) != week(day)),
            "new_month": last.is_none_or(|last| month(last) != month(day)),
        });
        (hello, latest)
    }

    /// The slicer the person chose to work with, if they chose one.
    pub(crate) fn chosen_slicer(&self) -> Option<toona_core::SlicerKind> {
        toona_core::SlicerKind::from_id(&self.stored().slicer?)
    }

    pub(crate) fn remember_slicer(&self, kind: toona_core::SlicerKind) -> UiResult<()> {
        let mut stored = self.stored();
        stored.slicer = Some(kind.id().to_string());
        self.store(&stored)
    }

    pub fn prefs(&self) -> PrefsDto {
        let stored = self.stored();
        PrefsDto {
            asked: stored.asked,
            updates: stored.updates,
            usage: stored.usage,
            version: env!("CARGO_PKG_VERSION"),
            review: stored.asked && stored.usage && stored.usage_terms < USAGE_TERMS,
            report: self.report(&stored, today(), true).0,
        }
    }

    pub fn set_prefs(&self, updates: bool, usage: bool) -> UiResult<PrefsDto> {
        let mut stored = self.stored();
        stored.asked = true;
        stored.updates = updates;
        stored.usage = usage;
        stored.usage_terms = USAGE_TERMS;
        self.store(&stored)?;
        Ok(self.prefs())
    }

    /// Whether to look for a newer version without being asked.
    pub fn checks_for_updates(&self) -> bool {
        let stored = self.stored();
        stored.asked && stored.updates
    }

    /// Says hello if the person opted in and no hello got through today. Returns whether one was sent.
    pub fn send_usage(&self) -> UiResult<bool> {
        let mut stored = self.stored();
        let day = today();
        if !stored.asked || !stored.usage || stored.last_hello == Some(day) {
            return Ok(false);
        }
        let (hello, latest) = self.report(&stored, day, stored.usage_terms >= USAGE_TERMS);
        self.server().hello(hello)?;
        // Each product is told once: the next hello starts after the newest one in this.
        if latest.is_some() {
            stored.reported_until = latest;
        }
        stored.last_hello = Some(day);
        self.store(&stored)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hello_names_new_products_once_and_only_after_the_new_question_is_answered() {
        let temp = tempfile::tempdir().unwrap();
        let dirs = toona_core::AppDirs::under(temp.path());
        let profile = |key: &str, name: &str, slicer: &str, at: &str| {
            json!({ "catalogue_key": key, "alias": name, "profile_name": name, "filament_id": "", "printer": "P",
                    "slicer": slicer, "files": [], "installed_at": at })
        };
        let write = |profiles: Vec<Value>| {
            std::fs::create_dir_all(&dirs.data_dir).unwrap();
            std::fs::write(dirs.manifest_path(), json!({ "schema": 1, "profiles": profiles }).to_string()).unwrap();
        };
        let mut added = vec![
            profile("esun/PLA/pla_plus", "A @0.4", "bambu-studio", "2026-11-01T10:00:00Z"),
            profile("esun/PLA/pla_plus", "A @0.6", "bambu-studio", "2026-11-01T10:00:00Z"),
            profile("esun/PLA/pla_plus", "A", "prusa-slicer", "2026-11-02T10:00:00Z"),
            // A copy of the user's own imported profile: its key holds their name for it.
            profile("converted/My secret blend", "B", "bambu-studio", "2026-11-03T10:00:00Z"),
        ];
        write(added.clone());
        let backend = Backend::new(dirs.clone());

        // Someone who said yes when the question promised "never which filaments".
        backend.store(&Stored { asked: true, usage: true, ..Stored::default() }).unwrap();
        assert!(backend.prefs().review);
        let (hello, latest) = backend.report(&backend.stored(), 20_785, false);
        assert_eq!(hello["products"], json!([]));
        assert!(latest.is_none());
        assert_eq!(hello["added"], 4);

        // Saving the question as it now reads agrees to it.
        assert!(!backend.set_prefs(true, true).unwrap().review);
        let (hello, latest) = backend.report(&backend.stored(), 20_785, true);
        assert_eq!(
            hello["products"],
            json!([{ "key": "esun/PLA/pla_plus", "slicer": "Bambu Studio" }, { "key": "esun/PLA/pla_plus", "slicer": "PrusaSlicer" }])
        );
        assert_eq!(latest.as_deref(), Some("2026-11-02T10:00:00Z"));

        // After a hello got through, only what was added since is named.
        let mut stored = backend.stored();
        stored.reported_until = latest;
        backend.store(&stored).unwrap();
        assert_eq!(backend.report(&backend.stored(), 20_786, true).0["products"], json!([]));
        added.push(profile("polymaker/PLA/polylite_pla", "C", "prusa-slicer", "2026-11-05T09:00:00Z"));
        write(added);
        assert_eq!(
            backend.report(&backend.stored(), 20_786, true).0["products"],
            json!([{ "key": "polymaker/PLA/polylite_pla", "slicer": "PrusaSlicer" }])
        );
    }

    #[test]
    fn weeks_turn_over_on_monday() {
        // 23 November 2026 is a Monday, day 20780.
        assert_ne!(week(20_779), week(20_780));
        assert_eq!(week(20_780), week(20_786));
        assert_ne!(week(20_786), week(20_787));
    }

    #[test]
    fn months_follow_the_calendar() {
        assert_eq!(month(0), (1970, 1));
        assert_eq!(month(20_780), (2026, 11));
        assert_eq!(month(20_787), (2026, 11));
        assert_eq!(month(20_788), (2026, 12));
        // 29 February 2024 and the day after.
        assert_eq!(month(19_782), (2024, 2));
        assert_eq!(month(19_783), (2024, 3));
    }
}
