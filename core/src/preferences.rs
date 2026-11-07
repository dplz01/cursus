//! Chat features ("preferences") of contacts and groups: what each side
//! allows, whether a feature works, and the JSON the core expects back.
//!
//! Platform-independent rules shared by every Cursus app; how they are shown
//! (labels, menus) stays in each app.
//!
//! - Contacts: you and the contact each set `no` / `yes` / `always` per
//!   feature (`userPreferences`); the core merges both into
//!   `mergedPreferences` (`enabled.forUser` = it works for you).
//! - Groups: owners switch features `on` / `off` for everyone
//!   (`groupPreferences`); some carry a minimum `role`.

use serde_json::{json, Value};

/// Contact features Cursus shows, in display order.
pub const CONTACT_FEATURES: [&str; 5] = ["timedMessages", "fullDelete", "reactions", "voice", "calls"];

/// Group features Cursus shows, in display order.
pub const GROUP_FEATURES: [&str; 9] = [
    "timedMessages",
    "directMessages",
    "fullDelete",
    "reactions",
    "voice",
    "files",
    "simplexLinks",
    "history",
    "support",
];

/// Disappearing-messages time when switched on without one: 1 day.
pub const DEFAULT_TIMED_TTL_SECS: i64 = 86_400;

/// How much one side allows a contact feature (`FeatureAllowed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeatureAllow {
    No,
    Yes,
    /// Allowed even when the contact only says `yes`… or never asked.
    Always,
}

impl FeatureAllow {
    /// Wire value (`yes` / `always`; anything else is `No`).
    pub fn from_api(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "yes" | "on" | "true" => Self::Yes,
            "always" => Self::Always,
            _ => Self::No,
        }
    }

    pub fn as_api(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::Yes => "yes",
            Self::Always => "always",
        }
    }

    pub fn is_on(self) -> bool {
        !matches!(self, Self::No)
    }

    /// SimpleX default for a contact feature nobody set.
    pub fn contact_default(key: &str) -> Self {
        match key {
            "reactions" | "voice" | "calls" => Self::Yes,
            _ => Self::No,
        }
    }
}

/// One contact feature as both sides see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactFeature {
    /// What you allow this contact.
    pub mine: FeatureAllow,
    /// What the contact allows you.
    pub contact: FeatureAllow,
    /// It works for you in this chat.
    pub enabled_for_user: bool,
    /// It works for the contact in this chat.
    pub enabled_for_contact: bool,
}

/// `key` for a contact: `prefs` = your `userPreferences` for them,
/// `merged` = the core's `mergedPreferences`. Your explicit choice wins, then
/// your global default (from the merged view), then the SimpleX default.
pub fn contact_feature(prefs: Option<&Value>, merged: Option<&Value>, key: &str) -> ContactFeature {
    let allow_at = |v: Option<&Value>, path: &str| {
        v.and_then(|v| v.pointer(path)).and_then(Value::as_str).map(FeatureAllow::from_api)
    };
    let mine = if prefs.and_then(|p| p.get(key)).is_some() {
        allow_at(prefs, &format!("/{key}/allow")).unwrap_or(FeatureAllow::No)
    } else {
        allow_at(merged, &format!("/{key}/userPreference/preference/allow"))
            .unwrap_or_else(|| FeatureAllow::contact_default(key))
    };
    let enabled = |side: &str| {
        merged
            .and_then(|m| m.pointer(&format!("/{key}/enabled/{side}")))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    ContactFeature {
        mine,
        contact: allow_at(merged, &format!("/{key}/contactPreference/allow")).unwrap_or(FeatureAllow::No),
        enabled_for_user: enabled("forUser"),
        enabled_for_contact: enabled("forContact"),
    }
}

/// Your disappearing-messages time for a contact, if you set one.
pub fn contact_timed_ttl(prefs: Option<&Value>, merged: Option<&Value>) -> Option<i64> {
    let at = |v: Option<&Value>, path: &str| v.and_then(|v| v.pointer(path)).and_then(Value::as_i64);
    at(prefs, "/timedMessages/ttl")
        .filter(|n| *n > 0)
        .or_else(|| at(merged, "/timedMessages/userPreference/preference/ttl").filter(|n| *n > 0))
}

/// New `userPreferences` for `/_set prefs @id`: `existing` with each
/// `(key, allow)` set; other keys are kept. Disappearing messages carry
/// `timed_ttl` when allowed; they have no `always` on the wire (one-way
/// deletion is the local chat TTL), so `Always` is sent as `yes`.
pub fn contact_preferences_json(
    existing: Option<&Value>,
    allows: &[(&str, FeatureAllow)],
    timed_ttl: i64,
) -> Value {
    let mut prefs = existing.cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    if let Some(obj) = prefs.as_object_mut() {
        for &(key, allow) in allows {
            let value = if key == "timedMessages" {
                if allow.is_on() {
                    json!({ "allow": "yes", "ttl": timed_ttl.max(1) })
                } else {
                    json!({ "allow": "no" })
                }
            } else {
                json!({ "allow": allow.as_api() })
            };
            obj.insert(key.to_string(), value);
        }
    }
    prefs
}

/// Whether a group feature is on. Member support ("chat with admins") is on
/// unless switched off; every other feature is off unless switched on.
pub fn group_feature_on(prefs: Option<&Value>, key: &str) -> bool {
    match prefs.and_then(|p| p.pointer(&format!("/{key}/enable"))).and_then(Value::as_str) {
        Some(s) => s.eq_ignore_ascii_case("on"),
        None => key == "support",
    }
}

/// The group's disappearing-messages time, if set.
pub fn group_timed_ttl(prefs: Option<&Value>) -> Option<i64> {
    prefs
        .and_then(|p| p.pointer("/timedMessages/ttl"))
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
}

/// Switch one group feature in `groupPreferences`. Role-gated features keep
/// (or get) `role: member`; disappearing messages get `ttl` (else the current
/// one, else 1 day) when on, and lose it when off.
pub fn set_group_feature(prefs: &mut Option<Value>, key: &str, on: bool, ttl: Option<i64>) {
    let root = prefs.get_or_insert_with(|| json!({}));
    if !root.is_object() {
        *root = json!({});
    }
    let Some(obj) = root.as_object_mut() else { return };
    let role_gated = matches!(key, "directMessages" | "voice" | "files" | "simplexLinks" | "sessions");
    let entry = obj.entry(key.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let Some(map) = entry.as_object_mut() else { return };
    map.insert("enable".into(), json!(if on { "on" } else { "off" }));
    if role_gated && !map.contains_key("role") {
        map.insert("role".into(), json!("member"));
    }
    if key == "timedMessages" {
        if on {
            let current = map.get("ttl").and_then(Value::as_i64).filter(|n| *n > 0);
            let ttl = ttl.or(current).unwrap_or(DEFAULT_TIMED_TTL_SECS).max(1);
            map.insert("ttl".into(), json!(ttl));
        } else {
            map.insert("ttl".into(), Value::Null);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_defaults_without_prefs() {
        assert_eq!(contact_feature(None, None, "reactions").mine, FeatureAllow::Yes);
        assert_eq!(contact_feature(None, None, "timedMessages").mine, FeatureAllow::No);
        assert_eq!(contact_feature(None, None, "calls").contact, FeatureAllow::No);
        assert_eq!(contact_timed_ttl(None, None), None);
    }

    #[test]
    fn contact_feature_reads_explicit_then_merged() {
        let prefs = json!({ "voice": { "allow": "always" } });
        let merged = json!({
            "voice": { "contactPreference": { "allow": "yes" }, "enabled": { "forUser": true, "forContact": false } },
            "reactions": { "userPreference": { "preference": { "allow": "no" } } },
            "timedMessages": { "userPreference": { "preference": { "allow": "yes", "ttl": 3600 } } }
        });
        let voice = contact_feature(Some(&prefs), Some(&merged), "voice");
        assert_eq!(voice.mine, FeatureAllow::Always);
        assert_eq!(voice.contact, FeatureAllow::Yes);
        assert!(voice.enabled_for_user && !voice.enabled_for_contact);
        let reactions = contact_feature(Some(&prefs), Some(&merged), "reactions");
        assert_eq!(reactions.mine, FeatureAllow::No, "your global preference");
        assert_eq!(contact_timed_ttl(Some(&prefs), Some(&merged)), Some(3600));
    }

    #[test]
    fn contact_json_sets_listed_features_and_keeps_others() {
        let existing = json!({ "voice": { "allow": "no" }, "sessions": { "allow": "yes" } });
        let out = contact_preferences_json(Some(&existing), &[("calls", FeatureAllow::Always)], 60);
        assert_eq!(out["calls"], json!({ "allow": "always" }));
        assert_eq!(out["voice"], json!({ "allow": "no" }));
        assert_eq!(out["sessions"], json!({ "allow": "yes" }), "unknown keys kept");
    }

    #[test]
    fn contact_timed_messages_carry_a_ttl() {
        let out = contact_preferences_json(None, &[("timedMessages", FeatureAllow::Yes)], 300);
        assert_eq!(out["timedMessages"], json!({ "allow": "yes", "ttl": 300 }));
        let out = contact_preferences_json(None, &[("timedMessages", FeatureAllow::Always)], 300);
        assert_eq!(out["timedMessages"], json!({ "allow": "yes", "ttl": 300 }), "always → yes");
        let out = contact_preferences_json(Some(&out), &[("timedMessages", FeatureAllow::No)], 300);
        assert_eq!(out["timedMessages"], json!({ "allow": "no" }));
    }

    #[test]
    fn group_support_defaults_on() {
        assert!(group_feature_on(None, "support"));
        assert!(!group_feature_on(None, "reactions"));
        let p = json!({ "support": { "enable": "off" }, "voice": { "enable": "on" } });
        assert!(!group_feature_on(Some(&p), "support"));
        assert!(group_feature_on(Some(&p), "voice"));
    }

    #[test]
    fn set_group_feature_adds_role_and_ttl() {
        let mut p = None;
        set_group_feature(&mut p, "voice", true, None);
        assert_eq!(p.as_ref().unwrap()["voice"], json!({ "enable": "on", "role": "member" }));
        set_group_feature(&mut p, "timedMessages", true, None);
        assert_eq!(group_timed_ttl(p.as_ref()), Some(DEFAULT_TIMED_TTL_SECS));
        set_group_feature(&mut p, "timedMessages", true, Some(604_800));
        assert_eq!(group_timed_ttl(p.as_ref()), Some(604_800));
        set_group_feature(&mut p, "timedMessages", false, None);
        assert_eq!(p.as_ref().unwrap()["timedMessages"], json!({ "enable": "off", "ttl": null }));
        assert!(group_feature_on(p.as_ref(), "voice"), "other features kept");
    }
}
