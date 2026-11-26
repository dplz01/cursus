//! Page transitions: a newly opened page (chat, profile, a profile's
//! settings, a settings category) slides in from the right.
//!
//! - One column, opened from the list: the page travels the full width over
//!   the list, which dims under it (a phone "push").
//! - Anything else (a pane beside the list, a page opened from a page): a
//!   short slide while the page fades in from the panel colour.
//!
//! Only forward moves animate; Back and destination switches are instant.
//! The wrapper is always in the tree (at rest it is a no-op), so the page's
//! widget state — focus, scroll — is the same before and after the motion.
use std::time::{Duration, Instant};

use crate::Message;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{overlay, Clipboard, Shell};
use iced::{mouse, Color, Element, Event, Length, Rectangle, Size, Theme, Vector};

/// Which page is showing, as far as transitions care.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PageKey {
    pub nav: Option<crate::ui::NavSection>,
    pub chat: Option<cursus_core::ChatId>,
    /// 0 none · 1 a profile page · 2 a page inside it (settings, members…).
    pub info_level: u8,
    /// Which page inside the profile, so moving between them animates too.
    pub info_page: u8,
    pub settings: Option<crate::SettingsSection>,
}

impl PageKey {
    fn depth(&self) -> u8 {
        u8::from(self.chat.is_some()) + self.info_level + u8::from(self.settings.is_some())
    }
}

/// Where the moving page sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    /// The main pane (one column: the whole window).
    Detail,
    /// The third column beside the conversation.
    Info,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Motion {
    start: Instant,
    pub slot: Slot,
    /// Travel the whole width over the list (one column, from the list).
    pub full: bool,
}

impl Motion {
    fn duration(&self) -> Duration {
        Duration::from_millis(if self.full { 300 } else { 220 })
    }

    /// 1 when the motion starts → 0 at rest, eased out (fast, then settling).
    pub(crate) fn remaining(&self, now: Instant) -> f32 {
        let t = now.saturating_duration_since(self.start).as_secs_f32() / self.duration().as_secs_f32();
        let t = t.clamp(0.0, 1.0);
        (1.0 - t).powi(3)
    }

    pub(crate) fn done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= self.duration()
    }
}

/// The motion for a move from `old` to `new`, if it is a forward move.
/// `two_panes`: the page opens beside the list; `info_column`: profiles
/// open in their own column beside the conversation.
pub(crate) fn motion_for(old: &PageKey, new: &PageKey, two_panes: bool, info_column: bool, now: Instant) -> Option<Motion> {
    if old == new || old.nav != new.nav || new.depth() == 0 || new.depth() < old.depth() {
        return None;
    }
    let slot = if info_column && new.info_level > 0 && old.chat == new.chat && old.chat.is_some() {
        Slot::Info
    } else {
        Slot::Detail
    };
    Some(Motion { start: now, slot, full: !two_panes && old.depth() == 0 })
}

/// How far a short (non-full) slide travels.
const SHORT_TRAVEL: f32 = 48.0;

/// Wrap a page so it can slide in. `remaining` = 1 (start) … 0 (at rest).
pub(crate) fn slide<'a>(content: impl Into<Element<'a, Message>>, remaining: f32, full: bool, veil: Color) -> Element<'a, Message> {
    Element::new(Slide { content: content.into(), remaining: remaining.clamp(0.0, 1.0), full, veil })
}

struct Slide<'a> {
    content: Element<'a, Message>,
    remaining: f32,
    full: bool,
    /// Panel colour the page fades in from (short slides).
    veil: Color,
}

impl Slide<'_> {
    fn shift(&self, width: f32) -> f32 {
        self.remaining * if self.full { width } else { SHORT_TRAVEL }
    }
}

impl Widget<Message, Theme, iced::Renderer> for Slide<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let child = self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits);
        let size = child.size();
        let shift = self.shift(size.width);
        layout::Node::with_children(size, vec![child.move_to((shift, 0.0))])
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(child) = layout.children().next() else { return };
        if self.remaining <= 0.0 {
            self.content.as_widget().draw(&tree.children[0], renderer, theme, style, child, cursor, viewport);
            return;
        }
        let Some(clip) = bounds.intersection(viewport) else { return };
        renderer.with_layer(clip, |r| {
            self.content.as_widget().draw(&tree.children[0], r, theme, style, child, cursor, &clip);
        });
        if !self.full {
            // Fade in: a veil of the panel colour over the page, on its own
            // layer so it also covers the page's text and images.
            renderer.with_layer(clip, |r| {
                r.fill_quad(
                    renderer::Quad { bounds: child.bounds(), ..renderer::Quad::default() },
                    Color { a: self.veil.a * self.remaining, ..self.veil },
                );
            });
        }
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Some(child) = layout.children().next() {
            self.content
                .as_widget_mut()
                .update(&mut tree.children[0], event, child, cursor, renderer, clipboard, shell, viewport);
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        layout.children().next().map_or_else(mouse::Interaction::default, |child| {
            self.content.as_widget().mouse_interaction(&tree.children[0], child, cursor, viewport, renderer)
        })
    }
    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn widget::Operation) {
        if let Some(child) = layout.children().next() {
            self.content.as_widget_mut().operate(&mut tree.children[0], child, renderer, operation);
        }
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        let child = layout.children().next()?;
        self.content.as_widget_mut().overlay(&mut tree.children[0], child, renderer, viewport, translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(chat: bool, info_level: u8) -> PageKey {
        PageKey {
            nav: Some(crate::ui::NavSection::Chats),
            chat: chat.then(|| cursus_core::ChatId::new(cursus_core::ChatType::Direct, 1)),
            info_level,
            ..PageKey::default()
        }
    }

    #[test]
    fn only_forward_moves_animate() {
        let now = Instant::now();
        let list = key(false, 0);
        let chat = key(true, 0);
        let profile = key(true, 1);
        let m = motion_for(&list, &chat, false, false, now).expect("opening a chat slides");
        assert!(m.full && m.slot == Slot::Detail);
        assert!(!motion_for(&chat, &profile, false, false, now).unwrap().full, "page over page: short slide");
        assert_eq!(motion_for(&chat, &profile, true, true, now).unwrap().slot, Slot::Info);
        assert!(motion_for(&profile, &chat, false, false, now).is_none(), "Back is instant");
        assert!(motion_for(&chat, &list, false, false, now).is_none());
        let mut other_nav = chat.clone();
        other_nav.nav = Some(crate::ui::NavSection::Settings);
        assert!(motion_for(&other_nav, &chat, false, false, now).is_none(), "destination switches are instant");
    }

    #[test]
    fn motion_settles() {
        let now = Instant::now();
        let m = motion_for(&key(false, 0), &key(true, 0), true, false, now).unwrap();
        assert_eq!(m.remaining(now), 1.0);
        let end = now + Duration::from_millis(400);
        assert_eq!(m.remaining(end), 0.0);
        assert!(m.done(end));
    }
}
