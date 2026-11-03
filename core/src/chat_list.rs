//! Chat-list merge helpers (seeded channels while core hides `creating_in_progress`).

use std::collections::HashSet;

use crate::models::ChatPreview;

/// Merge a fresh core chat list with locally seeded channel rows.
///
/// Channels stay `creating_in_progress` until relays publish short-link data, so
/// `/_get chats` omits them. We seed those rows into the UI and keep them across
/// reloads **only** while their group id remains in `seeded_channel_ids`.
///
/// Callers must remove an id from `seeded_channel_ids` when the user deletes the
/// channel (or when core starts returning it), otherwise a ghost row survives.
pub fn merge_seeded_channels(
    from_core: Vec<ChatPreview>,
    previous: &[ChatPreview],
    seeded_channel_ids: &HashSet<i64>,
) -> Vec<ChatPreview> {
    let mut out = from_core;
    for seed in previous.iter().rev() {
        if seed.is_channel
            && seeded_channel_ids.contains(&seed.id.api_id)
            && !out.iter().any(|c| c.id == seed.id)
        {
            out.insert(0, seed.clone());
        }
    }
    out
}

/// Drop seed ids that core now returns (no longer need a local placeholder).
pub fn prune_resolved_seeds(seeded_channel_ids: &mut HashSet<i64>, from_core: &[ChatPreview]) {
    seeded_channel_ids.retain(|id| {
        !from_core
            .iter()
            .any(|c| c.is_channel && c.id.api_id == *id)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChatId, ChatSettings, ChatType, ProfileFields};

    fn channel(api_id: i64, title: &str) -> ChatPreview {
        ChatPreview {
            id: ChatId::new(ChatType::Group, api_id),
            title: title.into(),
            preview: String::new(),
            unread: 0,
            unread_chat: false,
            settings: ChatSettings::default(),
            tag_ids: Vec::new(),
            pending_request_id: None,
            peer_profile: ProfileFields::default(),
            local_alias: String::new(),
            is_channel: true,
            membership_role: Some("owner".into()),
            group_preferences: None,
            member_admission: None,
            public_group: None,
            contact_preferences: None,
            contact_merged_preferences: None,
            members_require_attention: 0,
            membership_support_unread: 0,
            membership_has_support_chat: false,
        }
    }

    fn group(api_id: i64, title: &str) -> ChatPreview {
        let mut c = channel(api_id, title);
        c.is_channel = false;
        c
    }

    #[test]
    fn keeps_seeded_channel_missing_from_core() {
        let previous = vec![channel(5, "diocan")];
        let seeded = HashSet::from([5_i64]);
        let merged = merge_seeded_channels(vec![group(1, "Friends")], &previous, &seeded);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|c| c.id.api_id == 5 && c.is_channel));
    }

    #[test]
    fn drops_seed_after_user_deletes_id_from_set() {
        let previous = vec![channel(5, "diocan")];
        let seeded = HashSet::new(); // deleted → id removed from set
        let merged = merge_seeded_channels(Vec::new(), &previous, &seeded);
        assert!(merged.is_empty(), "ghost channel must not reappear");
    }

    #[test]
    fn does_not_duplicate_when_core_lists_channel() {
        let previous = vec![channel(5, "diocan")];
        let seeded = HashSet::from([5_i64]);
        let from_core = vec![channel(5, "diocan")];
        let merged = merge_seeded_channels(from_core, &previous, &seeded);
        assert_eq!(merged.iter().filter(|c| c.id.api_id == 5).count(), 1);
    }

    #[test]
    fn prune_clears_ids_present_in_core() {
        let mut seeded = HashSet::from([5_i64, 9_i64]);
        prune_resolved_seeds(&mut seeded, &[channel(5, "diocan")]);
        assert!(!seeded.contains(&5));
        assert!(seeded.contains(&9));
    }
}
