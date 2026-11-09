//! Group roles and member states: who may do what. Shared by every app so
//! a moderator sees the same actions everywhere.

use crate::models::{group_role_rank, GROUP_ROLES};

/// Owners edit the group profile, preferences and the channel link.
pub fn can_edit_group_profile(my_role: &str) -> bool {
    group_role_rank(my_role) >= group_role_rank("owner")
}

/// Admins add / remove members, change roles and manage the group link.
pub fn can_manage_members(my_role: &str) -> bool {
    group_role_rank(my_role) >= group_role_rank("admin")
}

/// Moderators review join requests, block for all and see support chats.
pub fn can_moderate(my_role: &str) -> bool {
    group_role_rank(my_role) >= group_role_rank("moderator")
}

/// Roles you may give someone (invite or role change): below your own.
pub fn assignable_roles(my_role: &str) -> Vec<&'static str> {
    let max = group_role_rank(my_role).saturating_sub(1).max(1);
    GROUP_ROLES.iter().copied().filter(|r| group_role_rank(r) <= max).collect()
}

/// Roles a group-link joiner may receive (core: `CEGroupMemberInitialRole`).
pub const LINK_JOINER_ROLES: &[&str] = &["observer", "member"];

/// Invite outstanding / connecting — clock glyph, no Accept yet.
pub fn member_status_is_invited(status: &str) -> bool {
    matches!(status.to_ascii_lowercase().as_str(), "invited" | "intro-inv")
}

/// A moderator must Accept (`/_accept member`) — clock glyph + Accept / Reject.
pub fn member_status_is_pending(status: &str) -> bool {
    matches!(status.to_ascii_lowercase().as_str(), "pending_approval" | "pending_review")
}

pub fn member_status_shows_pending_icon(status: &str) -> bool {
    member_status_is_pending(status) || member_status_is_invited(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_follow_rank() {
        assert!(can_edit_group_profile("owner") && !can_edit_group_profile("admin"));
        assert!(can_manage_members("admin") && !can_manage_members("moderator"));
        assert!(can_moderate("moderator") && !can_moderate("member"));
        assert!(!can_moderate("relay"));
    }

    #[test]
    fn assignable_roles_are_below_yours() {
        assert_eq!(assignable_roles("owner"), ["observer", "member", "moderator", "admin"]);
        assert_eq!(assignable_roles("admin"), ["observer", "member", "moderator"]);
        assert_eq!(assignable_roles("member"), ["observer"]);
        assert_eq!(assignable_roles(""), ["observer"]);
    }

    #[test]
    fn pending_is_approval_or_review_only() {
        assert!(member_status_is_pending("pending_approval"));
        assert!(member_status_is_pending("pending_review"));
        assert!(!member_status_is_pending("invited"));
        assert!(!member_status_is_pending("connected"));
        assert!(!member_status_is_pending("complete"));
        assert!(!member_status_is_pending("intro-inv"));
    }

    #[test]
    fn invited_is_not_accept_member() {
        assert!(member_status_is_invited("invited"));
        assert!(member_status_is_invited("intro-inv"));
        assert!(!member_status_is_invited("pending_approval"));
        assert!(!member_status_is_invited("connected"));
    }

    #[test]
    fn pending_icon_covers_both_states() {
        assert!(member_status_shows_pending_icon("pending_approval"));
        assert!(member_status_shows_pending_icon("invited"));
        assert!(!member_status_shows_pending_icon("connected"));
        assert!(!member_status_shows_pending_icon("complete"));
    }
}
