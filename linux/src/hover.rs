//! Hover tracking that does **not** publish iced Messages.
//!
//! Enter/exit on chat rows and message bubbles previously went through
//! `update` → full `view()` rebuild (entire history). That made list hover
//! lag and made scrolling worse whenever hover also fired.
//!
//! Widget-local hover state paints list highlights; shared cells arm
//! long-press without invalidating the UI tree.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::widget::Widget;
use iced::advanced::{Clipboard, Shell};
use iced::mouse;
use iced::touch;
use iced::window;
use iced::{Background, Border, Element, Event, Length, Rectangle, Size, Vector};

/// Shared hover targets read from [`crate::App`] on pointer press.
#[derive(Clone, Default)]
pub struct HoverSlots {
    pub chat: Rc<Cell<Option<usize>>>,
    pub msg: Rc<Cell<Option<i64>>>,
    pub reaction: Rc<RefCell<Option<(i64, String)>>>,
}

impl HoverSlots {
    pub fn clear_all(&self) {
        self.chat.set(None);
        self.msg.set(None);
        *self.reaction.borrow_mut() = None;
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct HoverState {
    is_hovered: bool,
}

/// Mouse/touch area: release / right-press publish Messages; enter/exit only
/// update slots (and optional local hover paint).
pub struct HoverArea<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_release: Option<Message>,
    on_right_press: Option<Message>,
    set_enter: Option<Rc<dyn Fn()>>,
    set_exit: Option<Rc<dyn Fn()>>,
    hover_bg: Option<Background>,
    selected_bg: Option<Background>,
    idle_bg: Option<Background>,
    /// When true, enter/exit requests a redraw so local hover paint updates
    /// without an App Message.
    redraw_on_hover: bool,
    radius: f32,
}

impl<'a, Message, Theme, Renderer> HoverArea<'a, Message, Theme, Renderer> {
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            content: content.into(),
            on_release: None,
            on_right_press: None,
            set_enter: None,
            set_exit: None,
            hover_bg: None,
            selected_bg: None,
            idle_bg: None,
            redraw_on_hover: false,
            radius: 10.0,
        }
    }

    pub fn on_release(mut self, msg: Message) -> Self {
        self.on_release = Some(msg);
        self
    }

    pub fn on_right_press(mut self, msg: Message) -> Self {
        self.on_right_press = Some(msg);
        self
    }

    pub fn on_hover_slots(
        mut self,
        enter: impl Fn() + 'static,
        exit: impl Fn() + 'static,
    ) -> Self {
        self.set_enter = Some(Rc::new(enter));
        self.set_exit = Some(Rc::new(exit));
        self
    }

    pub fn paint_row(
        mut self,
        selected: bool,
        hover_ok: bool,
        selected_bg: iced::Color,
        hover_bg: iced::Color,
    ) -> Self {
        self.selected_bg = selected.then_some(Background::Color(selected_bg));
        self.hover_bg = hover_ok.then_some(Background::Color(hover_bg));
        self.idle_bg = Some(Background::Color(iced::Color::TRANSPARENT));
        self.redraw_on_hover = hover_ok && !selected;
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for HoverArea<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<HoverState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(HoverState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> iced::event::Status {
        if let iced::event::Status::Captured = self.content.as_widget_mut().on_event(
            &mut tree.children[0],
            event.clone(),
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        ) {
            return iced::event::Status::Captured;
        }

        let state = tree.state.downcast_mut::<HoverState>();
        let was = state.is_hovered;
        state.is_hovered = cursor.is_over(layout.bounds());

        if state.is_hovered && !was {
            if let Some(f) = self.set_enter.as_ref() {
                f();
            }
            if self.redraw_on_hover {
                shell.request_redraw(window::RedrawRequest::NextFrame);
            }
        } else if !state.is_hovered && was {
            if let Some(f) = self.set_exit.as_ref() {
                f();
            }
            if self.redraw_on_hover {
                shell.request_redraw(window::RedrawRequest::NextFrame);
            }
        }

        if !cursor.is_over(layout.bounds()) {
            return iced::event::Status::Ignored;
        }

        match event {
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. }) => {
                if let Some(msg) = self.on_release.as_ref() {
                    shell.publish(msg.clone());
                    return iced::event::Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(msg) = self.on_right_press.as_ref() {
                    shell.publish(msg.clone());
                    return iced::event::Status::Captured;
                }
            }
            _ => {}
        }

        iced::event::Status::Ignored
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<HoverState>();
        let bg = if self.selected_bg.is_some() {
            self.selected_bg
        } else if state.is_hovered {
            self.hover_bg.or(self.idle_bg)
        } else {
            self.idle_bg
        };

        if let Some(background) = bg {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: iced::Color::TRANSPARENT,
                        width: 0.0,
                        radius: self.radius.into(),
                    },
                    ..renderer::Quad::default()
                },
                background,
            );
        }

        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(&mut tree.children[0], layout, renderer, translation)
    }
}

impl<'a, Message, Theme, Renderer> From<HoverArea<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(value: HoverArea<'a, Message, Theme, Renderer>) -> Self {
        Element::new(value)
    }
}

/// Convenience: chat-list / channel-list row.
pub fn chat_row<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
    index: usize,
    selected: bool,
    hover_ok: bool,
    slots: &HoverSlots,
    on_release: Message,
    on_right_press: Message,
) -> Element<'a, Message> {
    let chat_slot = Rc::clone(&slots.chat);
    let chat_slot_exit = Rc::clone(&slots.chat);
    HoverArea::new(content)
        .on_release(on_release)
        .on_right_press(on_right_press)
        .on_hover_slots(
            move || chat_slot.set(Some(index)),
            move || {
                if chat_slot_exit.get() == Some(index) {
                    chat_slot_exit.set(None);
                }
            },
        )
        .paint_row(
            selected,
            hover_ok,
            crate::theme::t().accent_soft,
            crate::theme::t().bg_elevated,
        )
        .into()
}

/// Message bubble probe — slot only (no local paint).
pub fn track_msg<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
    item_id: i64,
    slots: &HoverSlots,
    on_release: Message,
    on_right_press: Message,
) -> Element<'a, Message> {
    let enter = Rc::clone(&slots.msg);
    let exit = Rc::clone(&slots.msg);
    HoverArea::new(content)
        .on_release(on_release)
        .on_right_press(on_right_press)
        .on_hover_slots(
            move || enter.set(Some(item_id)),
            move || {
                if exit.get() == Some(item_id) {
                    exit.set(None);
                }
            },
        )
        .into()
}

pub fn track_reaction<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
    item_id: i64,
    emoji: String,
    slots: &HoverSlots,
    on_release: Message,
    on_right_press: Message,
) -> Element<'a, Message> {
    let enter_slot = Rc::clone(&slots.reaction);
    let exit_slot = Rc::clone(&slots.reaction);
    let emoji_enter = emoji.clone();
    HoverArea::new(content)
        .on_release(on_release)
        .on_right_press(on_right_press)
        .on_hover_slots(
            move || {
                *enter_slot.borrow_mut() = Some((item_id, emoji_enter.clone()));
            },
            move || {
                let mut g = exit_slot.borrow_mut();
                if g.as_ref().is_some_and(|(id, e)| *id == item_id && e == &emoji) {
                    *g = None;
                }
            },
        )
        .into()
}
