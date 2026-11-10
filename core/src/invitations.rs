//! One-time invitation links — the rules every app's Connect card and
//! pending-links list follow.
//!
//! Opening the New page ("+") creates a one-time link, as the official apps
//! do. A link stays valid until someone uses it, so unused links must never
//! pile up out of sight: the core keeps them as pending contact connections
//! (`pccConnStatus = "new"`), [`pending_invitations`] lists them from the
//! chat list the apps load anyway, and the apps show a counter and a list
//! where each can be copied, named (`/_set alias :<id> …`) or deleted.
//! "New link" deletes the link it replaces (after the user confirms).
//!
//! Incognito follows the official apps: an unused link can switch between
//! the user's profile and a fresh random one (`/_set incognito :<id> on|off`),
//! keeping the same link; pasted links can connect incognito too
//! (`/connect incognito …`).

use serde_json::Value;

use crate::models::{parse_conn_link, ConnLink};

/// A one-time invitation the user created and nobody has used yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    /// `display` is the short link when there is one; `full` always set.
    pub link: ConnLink,
    /// Pending contact connection id (`pccConnId`).
    pub conn_id: i64,
    /// Whoever connects sees a random incognito profile instead of the user's.
    pub incognito: bool,
    /// Name the user gave the link (empty: none).
    pub alias: String,
    /// When it was created (ISO 8601 from the core; may be empty).
    pub created_at: String,
}

/// An invitation from a `PendingContactConnection` JSON object; `None` for
/// pending connections that aren't the user's own one-time links (requests
/// sent to someone's address, group links, invitations being accepted).
pub(crate) fn parse_pending_invitation(conn: &Value) -> Option<Invitation> {
    let conn_id = conn.get("pccConnId")?.as_i64()?;
    let link = parse_conn_link(conn, "/connLinkInv/connFullLink", "/connLinkInv/connShortLink")?;
    if conn.get("viaContactUri").and_then(Value::as_bool).unwrap_or(false)
        || conn.get("groupLinkId").is_some_and(|v| !v.is_null())
    {
        return None;
    }
    Some(Invitation {
        link,
        conn_id,
        incognito: conn.get("customUserProfileId").is_some_and(|v| !v.is_null()),
        alias: str_field(conn, "localAlias"),
        created_at: str_field(conn, "createdAt"),
    })
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Unused own one-time links in a `/_get chats … pcc=on` reply, newest first.
pub(crate) fn pending_invitations(chats: &Value) -> Vec<Invitation> {
    let mut out: Vec<Invitation> = chats
        .get("chats")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|chat| {
            let info = chat.get("chatInfo")?;
            if info.get("type").and_then(Value::as_str) != Some("contactConnection") {
                return None;
            }
            let conn = info.get("contactConnection")?;
            if conn.get("pccConnStatus").and_then(Value::as_str) != Some("new") {
                return None;
            }
            parse_pending_invitation(conn)
        })
        .collect();
    // ISO-8601 timestamps sort chronologically as strings.
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.conn_id.cmp(&a.conn_id)));
    out
}

/// The invitation in a `/_connect <userId> incognito=…` reply (`invitation`).
pub(crate) fn parse_created_invitation(value: &Value) -> Option<Invitation> {
    let link = parse_conn_link(
        value,
        "/connLinkInvitation/connFullLink",
        "/connLinkInvitation/connShortLink",
    )?;
    let conn = value.get("connection")?;
    Some(Invitation {
        link,
        conn_id: conn.get("pccConnId")?.as_i64()?,
        incognito: conn.get("customUserProfileId").is_some_and(|v| !v.is_null()),
        alias: str_field(conn, "localAlias"),
        created_at: str_field(conn, "createdAt"),
    })
}

/// Incognito state after `/_set incognito` (`connectionIncognitoUpdated`).
pub(crate) fn parse_incognito_updated(value: &Value) -> Option<bool> {
    let conn = value.get("toConnection")?;
    Some(conn.get("customUserProfileId").is_some_and(|v| !v.is_null()))
}

/// Alias after `/_set alias :<id>` (`connectionAliasUpdated`).
pub(crate) fn parse_alias_updated(value: &Value) -> Option<String> {
    Some(str_field(value.get("toConnection")?, "localAlias"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pcc(id: i64, status: &str, created: &str, incognito: bool) -> Value {
        json!({
            "chatInfo": {"type": "contactConnection", "contactConnection": {
                "pccConnId": id, "pccConnStatus": status, "viaContactUri": false,
                "customUserProfileId": if incognito { json!(9) } else { Value::Null },
                "connLinkInv": {"connFullLink": format!("simplex:/invitation#{id}"),
                                "connShortLink": format!("https://smp.example/i#{id}")},
                "localAlias": if id == 3 { "for Anna" } else { "" },
                "createdAt": created,
            }},
            "chatItems": [],
        })
    }

    #[test]
    fn lists_unused_own_invitations_newest_first() {
        let chats = json!({"chats": [
            pcc(1, "new", "2026-11-01T10:00:00Z", false),
            pcc(2, "joined", "2026-11-03T10:00:00Z", false),
            pcc(3, "new", "2026-11-02T10:00:00Z", true),
            {"chatInfo": {"type": "direct", "contact": {"contactId": 5}}, "chatItems": []},
        ]});
        let list = pending_invitations(&chats);
        assert_eq!(list.iter().map(|i| i.conn_id).collect::<Vec<_>>(), vec![3, 1]);
        assert!(list[0].incognito);
        assert_eq!(list[0].alias, "for Anna");
        assert_eq!(list[0].link.display, "https://smp.example/i#3");
        assert_eq!(list[0].link.full, "simplex:/invitation#3");
    }

    #[test]
    fn ignores_address_requests_and_group_links() {
        let mut via_address = pcc(4, "new", "2026-11-05T00:00:00Z", false);
        via_address["chatInfo"]["contactConnection"]["viaContactUri"] = json!(true);
        let mut group = pcc(6, "new", "2026-11-06T00:00:00Z", false);
        group["chatInfo"]["contactConnection"]["groupLinkId"] = json!("abc");
        let mut accepting = pcc(7, "new", "2026-11-07T00:00:00Z", false);
        accepting["chatInfo"]["contactConnection"]["connLinkInv"] = Value::Null;
        let chats = json!({"chats": [via_address, group, accepting]});
        assert!(pending_invitations(&chats).is_empty());
    }

    #[test]
    fn parses_created_and_updated_replies() {
        let created = json!({
            "type": "invitation",
            "connLinkInvitation": {"connFullLink": "simplex:/invitation#x", "connShortLink": null},
            "connection": {"pccConnId": 42, "customUserProfileId": 3, "localAlias": "", "createdAt": "2026-11-10T12:00:00Z"},
        });
        let inv = parse_created_invitation(&created).unwrap();
        assert_eq!((inv.conn_id, inv.incognito, inv.link.display.as_str()), (42, true, "simplex:/invitation#x"));
        assert_eq!(inv.created_at, "2026-11-10T12:00:00Z");
        let updated = json!({"type": "connectionIncognitoUpdated", "toConnection": {"pccConnId": 42, "customUserProfileId": null}});
        assert_eq!(parse_incognito_updated(&updated), Some(false));
        let named = json!({"type": "connectionAliasUpdated", "toConnection": {"pccConnId": 42, "localAlias": "Bob"}});
        assert_eq!(parse_alias_updated(&named).as_deref(), Some("Bob"));
    }
}
