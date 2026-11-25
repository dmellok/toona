//! What the person chose about update checks and usage numbers, and the hello that
//! the second choice allows.
//!
//! Nothing here runs until they have answered the first-run question. The hello carries
//! no id: the app remembers the day it last said hello and tells the server whether
//! this is its first of the week or month, so installs can be counted but not told apart.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::{Backend, UiError};

type UiResult<T> = Result<T, UiError>;

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
}

#[derive(Serialize)]
pub struct PrefsDto {
    pub asked: bool,
    pub updates: bool,
    pub usage: bool,
    pub version: &'static str,
    /// Exactly what a hello sent now would contain.
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

    fn report(&self, stored: &Stored, day: i64) -> Value {
        let status = self.status();
        let mut printers: Vec<String> = status.printers.iter().map(|p| p.model.clone()).collect();
        printers.sort();
        printers.dedup();
        let last = stored.last_hello;
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "os_version": os_version(),
            "arch": std::env::consts::ARCH,
            "slicer": if status.found { status.slicer } else { "" },
            "printers": printers,
            "added": self.added_count(),
            "first": last.is_none(),
            "new_week": last.is_none_or(|last| week(last) != week(day)),
            "new_month": last.is_none_or(|last| month(last) != month(day)),
        })
    }

    pub fn prefs(&self) -> PrefsDto {
        let stored = self.stored();
        PrefsDto {
            asked: stored.asked,
            updates: stored.updates,
            usage: stored.usage,
            version: env!("CARGO_PKG_VERSION"),
            report: self.report(&stored, today()),
        }
    }

    pub fn set_prefs(&self, updates: bool, usage: bool) -> UiResult<PrefsDto> {
        let mut stored = self.stored();
        stored.asked = true;
        stored.updates = updates;
        stored.usage = usage;
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
        self.server().hello(self.report(&stored, day))?;
        stored.last_hello = Some(day);
        self.store(&stored)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
