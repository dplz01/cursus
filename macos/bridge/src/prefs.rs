//! Contact and group preferences: the SimpleX JSON ⇄ the simple shapes the
//! Swift app shows. Same rules as the Linux app (`linux/src/main.rs`:
//! `fill_contact_feature_overlay`, `build_contact_prefs_json`,
//! `set_group_pref`), kept pure so they are unit-tested here.

use serde_json::{json, Map, Value};

/// Contact features shown in the app, in display order.
pub const CONTACT_FEATURES: [&str; 5] = ["timedMessages", "fullDelete", "reactions", "voice", "calls"];

/// Group features shown in the app, in display order.
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

/// Disappearing messages default when switched on without a time.
pub const DEFAULT_TIMED_TTL: i64 = 86_400;

/// `no` / `yes` / `always` (anything unknown is `no`).
pub fn allow_value(raw: &str) -> &'static str {
    match raw.to_ascii_lowercase().as_str() {
        "yes" | "on" | "true" => "yes",
        "always" => "always",
        _ => "no",
    }
}

fn contact_default(key: &str) -> &'static str {
    match key {
        "reactions" | "voice" | "calls" => "yes",
        _ => "no",
    }
}

/// What *you* allow for `key`: your explicit preference for this contact,
/// else your global one from the merged view, else the SimpleX default.
fn you_allow(prefs: Option<&Value>, merged: Option<&Value>, key: &str) -> &'static str {
    if let Some(p) = prefs.and_then(|p| p.get(key)) {
        return allow_value(p.get("allow").and_then(Value::as_str).unwrap_or("no"));
    }
    merged
        .and_then(|m| m.pointer(&format!("/{key}/userPreference/preference/allow")))
        .and_then(Value::as_str)
        .map(allow_value)
        .unwrap_or_else(|| contact_default(key))
}

/// Contact features for the info panel:
/// `{ "features": { key: { mine, contact, enabled } }, "timedTtl": secs }`.
/// `enabled` = the feature works in this chat (both sides allow it).
pub fn contact_view(prefs: Option<&Value>, merged: Option<&Value>) -> Value {
    let mut out = Map::new();
    for key in CONTACT_FEATURES {
        let contact = merged
            .and_then(|m| m.pointer(&format!("/{key}/contactPreference/allow")))
            .and_then(Value::as_str)
            .map(allow_value)
            .unwrap_or("no");
        let enabled = merged
            .and_then(|m| m.pointer(&format!("/{key}/enabled/forUser")))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        out.insert(
            key.into(),
            json!({ "mine": you_allow(prefs, merged, key), "contact": contact, "enabled": enabled }),
        );
    }
    let ttl = prefs
        .and_then(|p| p.pointer("/timedMessages/ttl"))
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .or_else(|| {
            merged
                .and_then(|m| m.pointer("/timedMessages/userPreference/preference/ttl"))
                .and_then(Value::as_i64)
                .filter(|n| *n > 0)
        })
        .unwrap_or(DEFAULT_TIMED_TTL);
    json!({ "features": out, "timedTtl": ttl })
}

/// New `userPreferences` for `/_set prefs @id`: `existing` with `changes`
/// applied (`{ key: "no"|"yes"|"always" }`, `timedTtl: secs`). Keys the app
/// doesn't show are kept. Disappearing messages have no "always" on the wire
/// (that is the local chat TTL), so it is sent as `yes`.
pub fn contact_update(existing: Option<&Value>, merged: Option<&Value>, changes: &Value) -> Value {
    let mut prefs = existing.cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let current = contact_view(existing, merged);
    let obj = prefs.as_object_mut().expect("object");
    for key in CONTACT_FEATURES {
        let allow = changes
            .get(key)
            .and_then(Value::as_str)
            .map(allow_value)
            .unwrap_or_else(|| current["features"][key]["mine"].as_str().map(allow_value).unwrap_or("no"));
        let value = if key == "timedMessages" {
            if allow == "no" {
                json!({ "allow": "no" })
            } else {
                let ttl = changes
                    .get("timedTtl")
                    .and_then(Value::as_i64)
                    .or_else(|| current["timedTtl"].as_i64())
                    .unwrap_or(DEFAULT_TIMED_TTL)
                    .max(1);
                json!({ "allow": "yes", "ttl": ttl })
            }
        } else {
            json!({ "allow": allow })
        };
        obj.insert(key.into(), value);
    }
    prefs
}

/// Member support is on unless explicitly off; everything else is off unless on.
pub fn group_pref_on(prefs: Option<&Value>, key: &str) -> bool {
    match prefs.and_then(|p| p.get(key)).and_then(|v| v.get("enable")).and_then(Value::as_str) {
        Some(s) => s.eq_ignore_ascii_case("on"),
        None => key == "support",
    }
}

/// Group features for the info panel: `{ "features": { key: { on } }, "timedTtl": secs }`.
pub fn group_view(prefs: Option<&Value>) -> Value {
    let mut out = Map::new();
    for key in GROUP_FEATURES {
        out.insert(key.into(), json!({ "on": group_pref_on(prefs, key) }));
    }
    let ttl = prefs
        .and_then(|p| p.pointer("/timedMessages/ttl"))
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_TIMED_TTL);
    json!({ "features": out, "timedTtl": ttl })
}

/// `groupPreferences` with one feature switched; role-gated features keep
/// (or get) `role: member`, disappearing messages get a TTL when on.
pub fn group_set(prefs: Option<&Value>, key: &str, on: bool, ttl: Option<i64>) -> Value {
    let mut prefs = prefs.cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let obj = prefs.as_object_mut().expect("object");
    let role_pref = matches!(key, "directMessages" | "voice" | "files" | "simplexLinks" | "sessions");
    let entry = obj.entry(key.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let map = entry.as_object_mut().expect("object");
    map.insert("enable".into(), json!(if on { "on" } else { "off" }));
    if role_pref && !map.contains_key("role") {
        map.insert("role".into(), json!("member"));
    }
    if key == "timedMessages" {
        if on {
            let current = map.get("ttl").and_then(Value::as_i64).filter(|n| *n > 0);
            map.insert("ttl".into(), json!(ttl.or(current).unwrap_or(DEFAULT_TIMED_TTL).max(1)));
        } else {
            map.insert("ttl".into(), Value::Null);
        }
    }
    prefs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_defaults_without_prefs() {
        let v = contact_view(None, None);
        assert_eq!(v["features"]["reactions"]["mine"], "yes");
        assert_eq!(v["features"]["timedMessages"]["mine"], "no");
        assert_eq!(v["features"]["calls"]["contact"], "no");
        assert_eq!(v["timedTtl"], DEFAULT_TIMED_TTL);
    }

    #[test]
    fn contact_view_reads_explicit_then_merged() {
        let prefs = json!({ "voice": { "allow": "always" } });
        let merged = json!({
            "voice": { "contactPreference": { "allow": "yes" }, "enabled": { "forUser": true } },
            "reactions": { "userPreference": { "preference": { "allow": "no" } } },
            "timedMessages": { "userPreference": { "preference": { "allow": "yes", "ttl": 3600 } } }
        });
        let v = contact_view(Some(&prefs), Some(&merged));
        assert_eq!(v["features"]["voice"]["mine"], "always");
        assert_eq!(v["features"]["voice"]["contact"], "yes");
        assert_eq!(v["features"]["voice"]["enabled"], true);
        assert_eq!(v["features"]["reactions"]["mine"], "no", "global preference from the merged view");
        assert_eq!(v["timedTtl"], 3600);
    }

    #[test]
    fn contact_update_changes_one_feature_and_keeps_the_rest() {
        let existing = json!({ "voice": { "allow": "no" }, "sessions": { "allow": "yes" } });
        let out = contact_update(Some(&existing), None, &json!({ "calls": "always" }));
        assert_eq!(out["calls"], json!({ "allow": "always" }));
        assert_eq!(out["voice"], json!({ "allow": "no" }), "unchanged");
        assert_eq!(out["sessions"], json!({ "allow": "yes" }), "unknown keys kept");
        assert_eq!(out["timedMessages"], json!({ "allow": "no" }));
    }

    #[test]
    fn contact_timed_messages_carry_a_ttl() {
        let out = contact_update(None, None, &json!({ "timedMessages": "yes", "timedTtl": 300 }));
        assert_eq!(out["timedMessages"], json!({ "allow": "yes", "ttl": 300 }));
        let out = contact_update(Some(&out), None, &json!({ "timedMessages": "always" }));
        assert_eq!(out["timedMessages"], json!({ "allow": "yes", "ttl": 300 }), "always → yes, ttl kept");
    }

    #[test]
    fn group_defaults_and_support() {
        let v = group_view(None);
        assert_eq!(v["features"]["support"]["on"], true, "support defaults on");
        assert_eq!(v["features"]["reactions"]["on"], false);
        let v = group_view(Some(&json!({ "support": { "enable": "off" }, "voice": { "enable": "on" } })));
        assert_eq!(v["features"]["support"]["on"], false);
        assert_eq!(v["features"]["voice"]["on"], true);
    }

    #[test]
    fn group_set_adds_role_and_ttl() {
        let p = group_set(None, "voice", true, None);
        assert_eq!(p["voice"], json!({ "enable": "on", "role": "member" }));
        let p = group_set(Some(&p), "timedMessages", true, None);
        assert_eq!(p["timedMessages"]["ttl"], DEFAULT_TIMED_TTL);
        let p = group_set(Some(&p), "timedMessages", true, Some(604_800));
        assert_eq!(p["timedMessages"]["ttl"], 604_800);
        let p = group_set(Some(&p), "timedMessages", false, None);
        assert_eq!(p["timedMessages"], json!({ "enable": "off", "ttl": null }));
        assert_eq!(p["voice"]["enable"], "on", "other features kept");
    }
}
