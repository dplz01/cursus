//! Shared navigation for header Back and Escape. Dismiss the visible layer first.
use iced::Task;

use crate::{
    App, AuthScreen, ContactProfilePane, GroupSettingsPane, Message, NavSection, SupportPane,
    HISTORY_PAGE,
};

impl App {
    /// Capture drafts in tasks before dismissing their pages.
    pub(crate) fn save_contact_draft(&mut self) -> Task<Message> {
        if self.contact_profile.as_ref().is_some_and(|p| p.alias_dirty) {
            self.update(Message::SaveContactAlias)
        } else {
            Task::none()
        }
    }

    pub(crate) fn save_group_drafts(&mut self) -> Task<Message> {
        let alias = self.group_settings.as_ref().is_some_and(|g| g.alias_dirty);
        let profile = self.group_settings.as_ref().is_some_and(|g| g.dirty);
        Task::batch([
            if alias {
                self.update(Message::SaveGroupAlias)
            } else {
                Task::none()
            },
            if profile {
                self.update(Message::SaveGroupProfile)
            } else {
                Task::none()
            },
        ])
    }

    /// Pages and sheets belong to the destination that opened them.
    pub(crate) fn clear_section_pages(&mut self) {
        self.clear_gallery();
        self.pasted_image = None;
        self.contact_profile = None;
        self.group_settings = None;
        self.support = None;
        self.compose_open = false;
        self.create_chat = None;
        self.pref_menu = None;
        self.field_editor = None;
        self.setting_info = None;
        self.privacy_confirm = None;
        self.photo_menu = None;
        self.link_confirm = None;
        self.pending_links_open = false;
        self.renaming_link = None;
        self.chat_menu = None;
        self.message_menu = None;
        self.reaction_members = None;
        self.pending_url = None;
        self.settings_open = false;
        self.reply_to = None;
        self.editing = None;
        self.selection_mode = false;
        self.selected_items.clear();
        self.clear_pointer_gesture();
    }

    pub(crate) fn go_back(&mut self) -> Task<Message> {
        self.clear_pointer_gesture();
        // The lock and initial unlock form are roots, never reveal content below them.
        if self.locked.is_some() {
            return Task::none();
        }
        if self.auth.is_some() {
            return if matches!(self.auth, Some(AuthScreen::WelcomePassword { .. })) {
                self.update(Message::WelcomeBack)
            } else {
                Task::none()
            };
        }
        if self.image_viewer.is_some() {
            self.clear_gallery();
            return Task::none();
        }
        // Reverse the stacking order in ui::shell.
        if self.field_editor.take().is_some() {
            return Task::none();
        }
        if self.pref_menu.take().is_some() {
            return Task::none();
        }
        if self.create_chat.take().is_some() {
            return Task::none();
        }
        if self.reaction_members.take().is_some() {
            return Task::none();
        }
        if self.message_menu.take().is_some() {
            return Task::none();
        }
        if self.setting_info.take().is_some() {
            return Task::none();
        }
        if self.link_confirm.take().is_some() {
            return Task::none();
        }
        if self.photo_menu.take().is_some() {
            return Task::none();
        }
        if self.pending_links_open {
            return self.update(Message::ClosePendingLinks);
        }
        if self.privacy_confirm.take().is_some() {
            return Task::none();
        }
        if self.pending_url.take().is_some() {
            return Task::none();
        }
        if self.chat_menu.take().is_some() {
            return Task::none();
        }
        if self.nav == NavSection::Settings {
            self.settings_open = false;
            return Task::none();
        }
        if let Some(g) = self.group_settings.as_mut() {
            if g.member_dropdown.take().is_some() {
                return Task::none();
            }
        }
        if let Some(support) = self.support.as_ref() {
            return match &support.pane {
                SupportPane::Thread {
                    from_list: true, ..
                } => self.open_support_list(support.group_id),
                _ => self.close_support_overlay(),
            };
        }
        if let Some(g) = self.group_settings.as_mut() {
            if g.pane != GroupSettingsPane::Overview {
                g.pane = GroupSettingsPane::Overview;
                g.selected_member_id = None;
                return Task::none();
            }
            return self.update(Message::CloseGroupSettings);
        }
        if let Some(p) = self.contact_profile.as_mut() {
            if matches!(p.pane, ContactProfilePane::Preferences) {
                return self.update(Message::CloseContactPreferences);
            }
            return self.update(Message::CloseContactProfile);
        }
        if self.compose_open {
            return self.update(Message::CloseCompose);
        }
        if self.editing.take().is_some() {
            self.clear_composer();
            return Task::none();
        }
        if self.reply_to.take().is_some() {
            return Task::none();
        }
        if self.selection_mode {
            self.selection_mode = false;
            self.selected_items.clear();
            return Task::none();
        }
        if cursus_core::pending_shortcode_query(&self.composer_text()).is_some() {
            let mut text = self.composer_text();
            if let Some(start) = text.rfind(':') {
                text.truncate(start);
            }
            self.set_composer_text(text);
            self.emoji_suggest_index = 0;
            return Task::none();
        }
        self.selected = None;
        self.selected_id = None;
        self.messages.clear();
        self.messages_load_gen = self.messages_load_gen.wrapping_add(1);
        self.history_count = HISTORY_PAGE;
        Task::none()
    }
}
