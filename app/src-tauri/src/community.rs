//! The link to the Toona API: who is signed in, and profiles other people shared.
//! The rest of the app works without it; when the server cannot be reached these
//! calls fail with kind `offline` and the window leaves the community parts out.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use toona_core::{Change, FilamentProfile, Layer, Recipe};

use crate::api::UiError;

type UiResult<T> = Result<T, UiError>;

/// Used unless `TOONA_API` names another server (for example a local one).
const DEFAULT_API: &str = "https://api.toona.wiki";

/// Settings that are never taken from, or sent to, other people: G-code could make a
/// printer do anything, and the rest say what a profile is, not how it prints.
const NEVER_SHARED: &[&str] = &[
    "default_filament_colour",
    "filament_colour",
    "compatible_printers",
    "compatible_printers_condition",
    "compatible_prints",
    "compatible_prints_condition",
    "filament_notes",
    "filament_cost",
    "filament_spool_weight",
    "filament_vendor",
    "filament_type",
    "filament_settings_id",
    "filament_id",
    "setting_id",
    "name",
    "inherits",
    "from",
    "version",
];

pub fn shareable(key: &str) -> bool {
    !key.contains("gcode") && !NEVER_SHARED.contains(&key)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub handle: String,
    pub name: Option<String>,
    pub avatar: Option<String>,
    #[serde(default)]
    pub role: String,
}

#[derive(Serialize, Deserialize)]
struct Session {
    /// The server the token belongs to; a token is never sent anywhere else.
    api: String,
    token: String,
    user: User,
}

/// Which shared version an installed profile came from, so an edit can be shared as its child.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Link {
    pub version_id: u64,
    pub profile_id: u64,
}

/// A shared version as the server sends it.
#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub id: u64,
    pub profile_id: u64,
    pub product_key: String,
    pub printer: String,
    pub nozzle: String,
    pub parent_version: Option<u64>,
    pub parent_preset: String,
    pub changes: Vec<SharedChange>,
    pub note: String,
    pub created: String,
    pub author: Author,
    pub worked: u64,
    pub problems: u64,
    pub badges: Vec<String>,
    pub mine: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Author {
    pub id: u64,
    pub handle: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SharedChange {
    pub key: String,
    #[serde(default)]
    pub layer: String,
    #[serde(default)]
    pub from: Option<Value>,
    pub to: Value,
}

#[derive(Serialize)]
pub struct SignIn {
    pub done: bool,
    pub user: Option<User>,
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub interval: u64,
}

pub struct Community {
    api: String,
    data_dir: PathBuf,
    client: reqwest::blocking::Client,
}

fn offline(message: impl Into<String>) -> UiError {
    UiError { kind: "offline", message: message.into() }
}

/// Percent-encodes everything outside the unreserved set, keeping `/` when `path` is set.
fn encode(text: &str, path: bool) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) || (path && byte == b'/') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

impl Community {
    pub fn new(data_dir: PathBuf) -> Self {
        let api = std::env::var("TOONA_API").unwrap_or_else(|_| DEFAULT_API.to_string());
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("toona/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self { api: api.trim_end_matches('/').to_string(), data_dir, client }
    }

    fn session_path(&self) -> PathBuf {
        self.data_dir.join("session.json")
    }

    fn links_path(&self) -> PathBuf {
        self.data_dir.join("community.json")
    }

    fn session(&self) -> Option<Session> {
        let session: Session = serde_json::from_str(&std::fs::read_to_string(self.session_path()).ok()?).ok()?;
        (session.api == self.api).then_some(session)
    }

    fn save_session(&self, token: &str, user: &User) -> UiResult<()> {
        let session = Session { api: self.api.clone(), token: token.to_string(), user: user.clone() };
        let path = self.session_path();
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&self.data_dir)?;
            std::fs::write(&path, serde_json::to_string(&session).unwrap_or_default())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            }
            Ok(())
        };
        write().map_err(|error| UiError { kind: "other", message: format!("Could not save the sign-in: {error}") })
    }

    /// Links are kept per server, so a local test server's ids are never mixed with the real one's.
    fn all_links(&self) -> BTreeMap<String, BTreeMap<String, Link>> {
        std::fs::read_to_string(self.links_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save_links(&self, links: BTreeMap<String, Link>) {
        let mut all = self.all_links();
        all.insert(self.api.clone(), links);
        // Losing this only means a later edit is shared as a new profile, not as a child.
        let _ = std::fs::create_dir_all(&self.data_dir)
            .and_then(|_| std::fs::write(self.links_path(), serde_json::to_string(&all).unwrap_or_default()));
    }

    pub fn links(&self) -> BTreeMap<String, Link> {
        self.all_links().remove(&self.api).unwrap_or_default()
    }

    /// Remembers which shared version a profile in the slicer corresponds to.
    pub fn link(&self, profile_name: &str, link: Link) {
        let mut links = self.links();
        links.insert(profile_name.to_string(), link);
        self.save_links(links);
    }

    pub fn unlink(&self, profile_name: &str) {
        let mut links = self.links();
        if links.remove(profile_name).is_some() {
            self.save_links(links);
        }
    }

    fn call(&self, method: &str, path: &str, body: Option<Value>, timeout: Option<Duration>) -> UiResult<(u16, Value)> {
        let url = format!("{}{path}", self.api);
        let mut request = match method {
            "POST" => self.client.post(&url),
            _ => self.client.get(&url),
        };
        if let Some(session) = self.session() {
            request = request.header("authorization", format!("Bearer {}", session.token));
        }
        if let Some(body) = body {
            request = request.header("content-type", "application/json").body(body.to_string());
        }
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().map_err(|_| offline("Could not reach the Toona server."))?;
        let status = response.status().as_u16();
        let value: Value = response.text().ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or(Value::Null);
        Ok((status, value))
    }

    /// The server this app talks to.
    pub fn api(&self) -> &str {
        &self.api
    }

    /// The opt-in usage hello. Sent without the sign-in, so it is not tied to an account.
    pub fn hello(&self, report: Value) -> UiResult<()> {
        let response = self
            .client
            .post(format!("{}/app/ping", self.api))
            .header("content-type", "application/json")
            .body(report.to_string())
            .send()
            .map_err(|_| offline("Could not reach the Toona server."))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(UiError { kind: "other", message: "The Toona server refused the usage numbers.".into() })
        }
    }

    /// A call that must succeed; the server's own message is passed on when it refuses.
    fn expect(&self, method: &str, path: &str, body: Option<Value>) -> UiResult<Value> {
        let (status, value) = self.call(method, path, body, None)?;
        if (200..300).contains(&status) {
            return Ok(value);
        }
        if status == 401 {
            // The session ran out or was ended elsewhere.
            let _ = std::fs::remove_file(self.session_path());
        }
        let message = value.get("message").and_then(Value::as_str).unwrap_or("The Toona server refused the request.");
        Err(UiError { kind: if status == 401 { "sign_in" } else { "other" }, message: message.to_string() })
    }

    fn parse<T: serde::de::DeserializeOwned>(value: Value) -> UiResult<T> {
        serde_json::from_value(value)
            .map_err(|error| UiError { kind: "other", message: format!("The Toona server sent something unexpected: {error}") })
    }

    /// Whether the server answers at all. Short timeout: this runs when the window opens.
    pub fn available(&self) -> bool {
        matches!(self.call("GET", "/health", None, Some(Duration::from_secs(3))), Ok((200, _)))
    }

    pub fn user(&self) -> Option<User> {
        self.session().map(|session| session.user)
    }

    fn open(&self, value: Value) -> UiResult<User> {
        let token = value.get("token").and_then(Value::as_str).unwrap_or_default().to_string();
        let user: User = Self::parse(value.get("user").cloned().unwrap_or(Value::Null))?;
        self.save_session(&token, &user)?;
        Ok(user)
    }

    /// Starts sign-in with GitHub: the user types `user_code` in at `verification_uri`.
    /// With `TOONA_DEV_HANDLE` set (local development against a server that allows it),
    /// signs in as that handle at once.
    pub fn sign_in_start(&self) -> UiResult<SignIn> {
        let blank = |done, user| SignIn {
            done,
            user,
            user_code: String::new(),
            verification_uri: String::new(),
            device_code: String::new(),
            interval: 5,
        };
        if let Ok(handle) = std::env::var("TOONA_DEV_HANDLE") {
            let user = self.open(self.expect("POST", "/auth/dev", Some(json!({ "handle": handle })))?)?;
            return Ok(blank(true, Some(user)));
        }
        let value = self.expect("POST", "/auth/github/device", None)?;
        let field = |name: &str| value.get(name).and_then(Value::as_str).unwrap_or_default().to_string();
        Ok(SignIn {
            user_code: field("user_code"),
            verification_uri: field("verification_uri"),
            device_code: field("device_code"),
            interval: value.get("interval").and_then(Value::as_u64).unwrap_or(5),
            ..blank(false, None)
        })
    }

    /// Asks whether the user has approved yet. `None` means keep waiting.
    pub fn sign_in_poll(&self, device_code: &str) -> UiResult<Option<User>> {
        let (status, value) = self.call("POST", "/auth/session", Some(json!({ "device_code": device_code })), None)?;
        match status {
            200 => self.open(value).map(Some),
            202 => Ok(None),
            _ => Err(UiError {
                kind: "other",
                message: value.get("message").and_then(Value::as_str).unwrap_or("Sign-in failed.").to_string(),
            }),
        }
    }

    pub fn sign_out(&self) {
        if self.session().is_some() {
            let _ = self.call("POST", "/auth/logout", None, None);
        }
        let _ = std::fs::remove_file(self.session_path());
    }

    pub fn versions(&self, key: &str, printer: &str, nozzle: &str, slicer: &str) -> UiResult<Vec<Version>> {
        let path = format!(
            "/products/{}/profiles?printer={}&nozzle={}&slicer={}",
            encode(key, true),
            encode(printer, false),
            encode(nozzle, false),
            encode(slicer, false)
        );
        Self::parse(self.expect("GET", &path, None)?.get("versions").cloned().unwrap_or(Value::Null))
    }

    pub fn version(&self, id: u64) -> UiResult<Version> {
        Self::parse(self.expect("GET", &format!("/versions/{id}"), None)?)
    }

    /// Every version of the profile a version belongs to, oldest first.
    pub fn family(&self, version_id: u64) -> UiResult<Vec<Version>> {
        let profile_id = self.version(version_id)?.profile_id;
        Self::parse(self.expect("GET", &format!("/profiles/{profile_id}/versions"), None)?.get("versions").cloned().unwrap_or(Value::Null))
    }

    /// Shares a recipe: as a child of `parent` when it was built on someone's version, else as a new profile.
    pub fn publish(&self, parent: Option<Link>, mut body: Value) -> UiResult<Version> {
        let path = match parent {
            Some(link) => {
                body["parent_version"] = json!(link.version_id);
                format!("/profiles/{}/versions", link.profile_id)
            }
            None => "/profiles".to_string(),
        };
        Self::parse(self.expect("POST", &path, Some(body))?)
    }

    pub fn confirm(&self, version_id: u64, outcome: &str, printer: &str, nozzle: &str, slicer_version: &str) -> UiResult<Version> {
        let body = json!({ "outcome": outcome, "printer": printer, "nozzle": nozzle, "slicer_version": slicer_version });
        Self::parse(self.expect("POST", &format!("/versions/{version_id}/confirmations"), Some(body))?)
    }
}

pub fn layer_named(name: &str) -> Layer {
    match name {
        "manufacturer" => Layer::Manufacturer,
        "colour" => Layer::Colour,
        "you" => Layer::You,
        _ => Layer::Community,
    }
}

/// A shared value may replace a preset's value only if it has the same form:
/// text for text, a list of short texts for a list.
fn same_form(base: &Value, shared: &Value) -> bool {
    let short = |value: &Value| value.as_str().is_some_and(|text| text.len() <= 200);
    match (base, shared) {
        (Value::String(_), Value::String(_)) => short(shared),
        (Value::Array(base), Value::Array(shared)) => {
            !shared.is_empty() && shared.len() <= base.len().max(8) && shared.iter().all(short)
        }
        _ => false,
    }
}

/// Lays shared changes over a profile built from the catalogue. Only settings the base
/// preset already has are touched, so a shared version cannot add anything new to a
/// profile. Returns how many settings it changed.
pub fn apply_shared(
    profile: &mut FilamentProfile,
    recipe: &mut Recipe,
    base_settings: &BTreeMap<String, Value>,
    changes: &[SharedChange],
) -> usize {
    let mut applied = 0;
    for change in changes {
        let Some(base_value) = base_settings.get(&change.key) else { continue };
        if !shareable(&change.key) || !same_form(base_value, &change.to) {
            continue;
        }
        if profile.settings.get(&change.key) == Some(&change.to) {
            continue;
        }
        profile.settings.insert(change.key.clone(), change.to.clone());
        recipe.changes.retain(|existing| existing.key != change.key);
        if base_value != &change.to {
            recipe.changes.push(Change {
                key: change.key.clone(),
                layer: Layer::Community,
                from: Some(base_value.clone()),
                to: change.to.clone(),
            });
        }
        applied += 1;
    }
    recipe.changes.sort_by(|a, b| a.key.cmp(&b.key));
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared(key: &str, to: Value) -> SharedChange {
        SharedChange { key: key.into(), layer: "you".into(), from: None, to }
    }

    fn profile(settings: &[(&str, Value)]) -> FilamentProfile {
        let mut json = json!({
            "name": "Test @Printer", "from": "User", "inherits": "", "version": "1",
            "filament_id": "P1", "filament_settings_id": ["Test @Printer"],
            "compatible_printers": ["Printer"], "filament_vendor": ["Test"], "filament_type": ["PLA"]
        });
        for (key, value) in settings {
            json[*key] = value.clone();
        }
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn shared_changes_apply_only_to_known_safe_settings() {
        let base: BTreeMap<String, Value> = [
            ("nozzle_temperature".to_string(), json!(["220"])),
            ("filament_start_gcode".to_string(), json!(["; start"])),
            ("default_filament_colour".to_string(), json!(["#FFFFFF"])),
            ("filament_density".to_string(), json!(["1.24"])),
        ]
        .into();
        let mut profile = profile(&[
            ("nozzle_temperature", json!(["220"])),
            ("filament_start_gcode", json!(["; start"])),
            ("default_filament_colour", json!(["#000000"])),
            ("filament_density", json!(["1.17"])),
        ]);
        let mut recipe = Recipe {
            parent: "Base".into(),
            changes: vec![Change { key: "filament_density".into(), layer: Layer::Manufacturer, from: Some(json!(["1.24"])), to: json!(["1.17"]) }],
        };
        let applied = apply_shared(
            &mut profile,
            &mut recipe,
            &base,
            &[
                shared("nozzle_temperature", json!(["215"])),
                shared("filament_start_gcode", json!(["M104 S300"])),
                shared("default_filament_colour", json!(["#FF0000"])),
                shared("made_up_setting", json!(["1"])),
                shared("filament_density", json!("1.3")),
            ],
        );
        assert_eq!(applied, 1);
        assert_eq!(profile.settings["nozzle_temperature"], json!(["215"]));
        assert_eq!(profile.settings["filament_start_gcode"], json!(["; start"]), "G-code is never taken");
        assert_eq!(profile.settings["default_filament_colour"], json!(["#000000"]), "colour stays the chosen one");
        assert!(!profile.settings.contains_key("made_up_setting"), "unknown settings are not added");
        assert_eq!(profile.settings["filament_density"], json!(["1.17"]), "a value of the wrong form is skipped");
        let layers: Vec<_> = recipe.changes.iter().map(|c| (c.key.as_str(), c.layer)).collect();
        assert_eq!(layers, [("filament_density", Layer::Manufacturer), ("nozzle_temperature", Layer::Community)]);
    }

    #[test]
    fn keys_are_encoded_for_urls() {
        assert_eq!(encode("polymaker/PLA/polylite pla", true), "polymaker/PLA/polylite%20pla");
        assert_eq!(encode("Bambu Lab X2D", false), "Bambu%20Lab%20X2D");
        assert_eq!(encode("a/b&c", false), "a%2Fb%26c");
    }
}
