//! Message runs: consecutive messages that the history draws as one block.
//!
//! Every app uses this to shape bubbles the same way: the corner on the
//! sender's side (right for sent, left for received) is always small at the
//! top, and small at the bottom too on every bubble of a run but its last.

use chrono::{DateTime, Local, NaiveDateTime};

use crate::models::ChatMessage;

/// Longest gap between two messages of one run.
pub const RUN_GAP_SECS: i64 = 300;

/// Whether `next` (the message right after `prev` in the history) continues
/// `prev`'s run: same direction and sender, same local day, at most
/// [`RUN_GAP_SECS`] apart. Announcements and group invitations stand alone.
pub fn continues_run(prev: &ChatMessage, next: &ChatMessage) -> bool {
    if prev.outgoing != next.outgoing
        || prev.is_announcement
        || next.is_announcement
        || prev.group_invitation.is_some()
        || next.group_invitation.is_some()
    {
        return false;
    }
    let sender = |m: &ChatMessage| m.sender.as_ref().map(|s| s.group_member_id);
    if sender(prev) != sender(next) {
        return false;
    }
    match (
        prev.created_at.as_deref().and_then(parse_local),
        next.created_at.as_deref().and_then(parse_local),
    ) {
        (Some(a), Some(b)) => {
            a.date_naive() == b.date_naive() && (b - a).num_seconds().abs() <= RUN_GAP_SECS
        }
        _ => true,
    }
}

/// `(joined_prev, joined_next)` for every message of a history in order.
pub fn run_links(messages: &[ChatMessage]) -> Vec<(bool, bool)> {
    (0..messages.len())
        .map(|i| {
            let prev = i > 0 && continues_run(&messages[i - 1], &messages[i]);
            let next = i + 1 < messages.len() && continues_run(&messages[i], &messages[i + 1]);
            (prev, next)
        })
        .collect()
}

fn parse_local(raw: &str) -> Option<DateTime<Local>> {
    let raw = raw.trim();
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
        .or_else(|| {
            NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.fZ")
                .ok()
                .map(|ndt| ndt.and_utc().with_timezone(&Local))
        })
}
