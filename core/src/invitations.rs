//! One-time invitation links — the rule every app's Connect card follows.
//!
//! A one-time link stays valid until someone uses it, and the user may
//! already have sent it. So opening the Connect card never replaces it: the
//! apps show the newest *unused* invitation (the core keeps it as a pending
//! contact connection, `pccConnStatus = "new"`, across restarts) and create a
//! new one only when there is none or the user asks to regenerate.
//!
//! Incognito follows the official apps: a pending invitation can switch
//! between the user's profile and a fresh random profile
//! (`/_set incognito :<connId> on|off`) while it is unused, keeping the same
//! link; links pasted to connect use the same choice (`/connect incognito …`).

use serde_json::Value;

use crate::models::{parse_conn_link, ConnLink};

/// A one-time invitation the user can share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    pub link: ConnLink,
    /// Pending contact connection id (`pccConnId`).
    pub conn_id: i64,
    /// Whoever connects sees a random incognito profile instead of the user's.
    pub incognito: bool,
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
    let incognito = conn.get("customUserProfileId").is_some_and(|v| !v.is_null());
    Some(Invitation { link, conn_id, incognito })
}

/// Whether the pending connection is still waiting for its first use.
fn is_unused(conn: &Value) -> bool {
    conn.get("pccConnStatus").and_then(Value::as_str) == Some("new")
}

/// The newest unused one-time invitation in a `/_get chats … pcc=on` reply.
pub(crate) fn newest_unused_invitation(chats: &Value) -> Option<Invitation> {
    let list = chats.get("chats")?.as_array()?;
    list.iter()
        .filter_map(|chat| {
            let info = chat.get("chatInfo")?;
            if info.get("type").and_then(Value::as_str) != Some("contactConnection") {
                return None;
            }
            let conn = info.get("contactConnection")?;
            if !is_unused(conn) {
                return None;
            }
            let created = conn.get("createdAt").and_then(Value::as_str).unwrap_or("").to_string();
            Some((created, parse_pending_invitation(conn)?))
        })
        // ISO-8601 timestamps sort chronologically as strings.
        .max_by(|a, b| a.0.cmp(&b.0).then(a.1.conn_id.cmp(&b.1.conn_id)))
        .map(|(_, inv)| inv)
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
    })
}

/// Incognito state after `/_set incognito` (`connectionIncognitoUpdated`).
pub(crate) fn parse_incognito_updated(value: &Value) -> Option<bool> {
    let conn = value.get("toConnection")?;
    Some(conn.get("customUserProfileId").is_some_and(|v| !v.is_null()))
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
                "createdAt": created,
            }},
            "chatItems": [],
        })
    }

    #[test]
    fn picks_newest_unused_own_invitation() {
        let chats = json!({"chats": [
            pcc(1, "new", "2026-11-01T10:00:00Z", false),
            pcc(2, "joined", "2026-11-03T10:00:00Z", false),
            pcc(3, "new", "2026-11-02T10:00:00Z", true),
            {"chatInfo": {"type": "direct", "contact": {"contactId": 5}}, "chatItems": []},
        ]});
        let inv = newest_unused_invitation(&chats).unwrap();
        assert_eq!(inv.conn_id, 3);
        assert!(inv.incognito);
        assert_eq!(inv.link.display, "https://smp.example/i#3");
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
        assert_eq!(newest_unused_invitation(&chats), None);
    }

    #[test]
    fn parses_created_and_updated_replies() {
        let created = json!({
            "type": "invitation",
            "connLinkInvitation": {"connFullLink": "simplex:/invitation#x", "connShortLink": null},
            "connection": {"pccConnId": 42, "customUserProfileId": 3},
        });
        let inv = parse_created_invitation(&created).unwrap();
        assert_eq!((inv.conn_id, inv.incognito, inv.link.display.as_str()), (42, true, "simplex:/invitation#x"));
        let updated = json!({"type": "connectionIncognitoUpdated", "toConnection": {"pccConnId": 42, "customUserProfileId": null}});
        assert_eq!(parse_incognito_updated(&updated), Some(false));
    }
}
