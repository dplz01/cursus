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
    /// Finger/button down tracking for an owned short release.
    press_at: Option<std::time::Instant>,
    press_x: f32,
    press_y: f32,
    press_moved: bool,
    /// Touch is owned by this widget, not a hover target in an App subscription.
    touch_hold: Option<(touch::Finger, iced::Point, std::time::Instant)>,
    hold_fired: bool,
    press_viewport: Option<iced::Point>,
    mouse_hold: Option<(iced::Point, std::time::Instant)>,
}

/// Mouse/touch area: release / right-press publish Messages; enter/exit only
/// update slots (and optional local hover paint).
pub struct HoverArea<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_release: Option<Message>,
    on_right_press: Option<Message>,
    on_long_press: Option<Message>,
    set_enter: Option<Rc<dyn Fn()>>,
    set_exit: Option<Rc<dyn Fn()>>,
    hover_bg: Option<Background>,
    selected_bg: Option<Background>,
    idle_bg: Option<Background>,
    /// When true, enter/exit requests a redraw so local hover paint updates
    /// without an App Message.
    redraw_on_hover: bool,
    radius: f32,
    /// Show the hand cursor over the area (desktop) when content has none.
    pointer: bool,
    center_hover: bool,
    glow_y: Option<f32>,
}

impl<'a, Message, Theme, Renderer> HoverArea<'a, Message, Theme, Renderer> {
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            content: content.into(),
            on_release: None,
            on_right_press: None,
            on_long_press: None,
            set_enter: None,
            set_exit: None,
            hover_bg: None,
            selected_bg: None,
            idle_bg: None,
            redraw_on_hover: false,
            radius: 10.0,
            pointer: false,
            center_hover: false,
            glow_y: None,
        }
    }

    /// Keep a centred glyph lit across the complete icon hit area.
    pub fn icon_glow(mut self) -> Self {
        self.center_hover = true;
        self.redraw_on_hover = true;
        self
    }

    /// Hand cursor on hover, like a button.
    pub fn pointer(mut self) -> Self {
        self.pointer = true;
        self
    }

    pub fn on_release(mut self, msg: Message) -> Self {
        self.on_release = Some(msg);
        self
    }

    pub fn on_right_press(mut self, msg: Message) -> Self {
        self.on_right_press = Some(msg);
        self
    }

    pub fn on_long_press(mut self, msg: Message) -> Self {
        self.on_long_press = Some(msg);
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

    /// Redraw on enter/exit without painting a hover fill (e.g. SVG tint swaps).
    pub fn redraw_on_hover(mut self, enabled: bool) -> Self {
        self.redraw_on_hover = enabled;
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
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // Touch positions are available even before the first hover event.
        // Owning the timer here also survives App/view rebuilds and leaves the
        // press uncaptured so the surrounding scrollable can still swipe.
        let state = tree.state.downcast_mut::<HoverState>();
        let mut fire_hold = None;
        let mut swallow_lift = false;
        // Scrolling can keep the same content point under a moving finger.
        // Cancel against viewport travel too, so slow drags cannot become holds.
        if (state.touch_hold.is_some() || state.mouse_hold.is_some())
            && state.press_viewport.is_some_and(|origin| origin.distance(viewport.position()) > 2.0) {
            state.touch_hold = None;
            state.mouse_hold = None;
            state.press_moved = true;
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(layout.bounds())
                    && cursor.position().is_some_and(|p| viewport.contains(p)) =>
            {
                let at = std::time::Instant::now();
                state.mouse_hold = cursor.position().map(|p| (p, at));
                state.press_viewport = Some(viewport.position());
                state.hold_fired = false;
                if self.on_long_press.is_some() {
                    shell.request_redraw_at(
                        at + std::time::Duration::from_millis(crate::LONG_PRESS_MS),
                    );
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state
                    .mouse_hold
                    .is_some_and(|(origin, _)| origin.distance(*position) > 14.0)
                {
                    state.mouse_hold = None;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.mouse_hold = None;
                swallow_lift = state.hold_fired;
                state.hold_fired = false;
            }
            Event::Window(iced::window::Event::Unfocused) => {
                state.mouse_hold = None;
                state.touch_hold = None;
                state.press_at = None;
            }

            Event::Touch(touch::Event::FingerPressed { id, position })
                if layout.bounds().contains(*position) && viewport.contains(*position) =>
            {
                let at = std::time::Instant::now();
                state.touch_hold = Some((*id, *position, at));
                state.press_viewport = Some(viewport.position());
                state.hold_fired = false;
                state.press_at = Some(at);
                state.press_x = position.x;
                state.press_y = position.y;
                state.press_moved = false;
                if self.on_long_press.is_some() {
                    shell.request_redraw_at(
                        at + std::time::Duration::from_millis(crate::LONG_PRESS_MS),
                    );
                }
            }
            Event::Touch(touch::Event::FingerMoved { id, position }) => {
                if let Some((finger, origin, _)) = state.touch_hold {
                    if finger == *id && origin.distance(*position) > 14.0 {
                        state.touch_hold = None;
                        state.press_moved = true;
                    }
                }
            }
            Event::Touch(touch::Event::FingerLost { .. }) => {
                state.touch_hold = None;
                state.press_at = None;
                state.hold_fired = false;
            }
            Event::Touch(touch::Event::FingerLifted { id, .. }) => {
                swallow_lift = state.hold_fired || state.press_moved;
                if let Some((finger, position, at)) = state.touch_hold.take() {
                    if finger == *id
                        && !state.hold_fired
                        && at.elapsed().as_millis() >= crate::LONG_PRESS_MS as u128
                    {
                        fire_hold = self
                            .on_long_press
                            .as_ref()
                            .map(|message| (message.clone(), finger, position));
                    }
                }
                state.hold_fired = false;
            }
            Event::Window(iced::window::Event::RedrawRequested(now)) => {
                if let Some((position, at)) = state.mouse_hold {
                    let deadline = at + std::time::Duration::from_millis(crate::LONG_PRESS_MS);
                    if !state.hold_fired && self.on_long_press.is_some() {
                        if *now >= deadline {
                            state.hold_fired = true;
                            fire_hold = self
                                .on_long_press
                                .as_ref()
                                .map(|m| (m.clone(), touch::Finger(u64::MAX), position));
                        } else {
                            shell.request_redraw_at(deadline);
                        }
                    }
                }
                if let Some((finger, position, at)) = state.touch_hold {
                    let deadline = at + std::time::Duration::from_millis(crate::LONG_PRESS_MS);
                    if !state.hold_fired && self.on_long_press.is_some() {
                        if *now >= deadline {
                            state.hold_fired = true;
                            fire_hold = self
                                .on_long_press
                                .as_ref()
                                .map(|message| (message.clone(), finger, position));
                        } else {
                            shell.request_redraw_at(deadline);
                        }
                    }
                }
            }
            _ => {}
        }
        if fire_hold.is_some() || swallow_lift {
            state.press_at = None;
            let cancelled = match fire_hold.as_ref() {
                Some((_, touch::Finger(u64::MAX), _)) => Event::Mouse(mouse::Event::CursorLeft),
                Some((_, id, position)) => Event::Touch(touch::Event::FingerLost {
                    id: *id,
                    position: *position,
                }),
                None => event.clone(),
            };
            // Cancel any pressed inner control before exposing a modal menu.
            if fire_hold.is_some() {
                self.content.as_widget_mut().update(
                    &mut tree.children[0],
                    &cancelled,
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            if let Some((message, _, _)) = fire_hold {
                shell.publish(message);
            }
            shell.capture_event();
            return;
        }
        let child_cursor = if self.center_hover && cursor.is_over(layout.bounds()) {
            mouse::Cursor::Available(iced::Point::new(layout.bounds().center_x(), self.glow_y.map_or(layout.bounds().center_y(), |y| layout.bounds().y + y)))
        } else {
            cursor
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            child_cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        // Inner controls still own their ordinary taps.
        if shell.is_event_captured() {
            return;
        }

        let state = tree.state.downcast_mut::<HoverState>();
        let was = state.is_hovered;
        state.is_hovered = crate::POINTER_GATE.hover_allowed() && cursor.is_over(layout.bounds());

        if state.is_hovered && !was {
            if let Some(f) = self.set_enter.as_ref() {
                f();
            }
            if self.redraw_on_hover {
                shell.request_redraw();
            }
        } else if !state.is_hovered && was {
            if let Some(f) = self.set_exit.as_ref() {
                f();
            }
            if self.redraw_on_hover {
                shell.request_redraw();
            }
        }

        if !cursor.is_over(layout.bounds()) {
            // Still track move/lift if a press started here (finger may leave bounds).
            if state.press_at.is_none() {
                return;
            }
        }

        const SLOP: f32 = 14.0;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if cursor.is_over(layout.bounds()) {
                    state.press_at = Some(std::time::Instant::now());
                    if let Some(p) = cursor.position() {
                        state.press_x = p.x;
                        state.press_y = p.y;
                    }
                    state.press_moved = false;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Touch(touch::Event::FingerMoved { .. }) => {
                if state.press_at.is_some() {
                    if let Some(p) = cursor.position() {
                        let dx = p.x - state.press_x;
                        let dy = p.y - state.press_y;
                        if dx * dx + dy * dy > SLOP * SLOP {
                            state.press_moved = true;
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. }) => {
                let Some(pressed_at) = state
                    .press_at
                    .take() else {
                    return;
                };
                let held = pressed_at.elapsed().as_millis() as u64;
                let moved = state.press_moved;
                state.press_moved = false;
                if moved {
                    return;
                }
                if matches!(
                    event,
                    Event::Touch(_)) && held >= crate::LONG_PRESS_MS {
                    return;
                }
                if let Some(msg) = self.on_release.as_ref() {
                    shell.publish(msg.clone());
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if cursor.is_over(layout.bounds()) => {
                    if let Some(msg) = self.on_right_press.as_ref() {
                        shell.publish(msg.clone());
                        shell.capture_event();
                    }
                }
            _ => {}
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let inner = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        );
        if self.pointer
            && inner == mouse::Interaction::None
            && cursor.is_over(layout.bounds())
        {
            mouse::Interaction::Pointer
        } else {
            inner
        }
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
        } else if state.is_hovered && crate::POINTER_GATE.hover_allowed() {
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
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
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
        .pointer()
        .on_release(on_release)
        .on_right_press(on_right_press.clone())
        .on_long_press(on_right_press)
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
///
/// Mouse: right-click opens the menu. Touch: a still hold opens the same menu.
pub fn track_msg<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
    item_id: i64,
    slots: &HoverSlots,
    on_release: Message,
    on_menu: Message,
) -> Element<'a, Message> {
    let enter = Rc::clone(&slots.msg);
    let exit = Rc::clone(&slots.msg);
    HoverArea::new(content)
        .on_release(on_release)
        .on_right_press(on_menu.clone())
        .on_long_press(on_menu)
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
        .pointer()
        .on_release(on_release)
        .on_right_press(on_right_press.clone())
        .on_long_press(on_right_press)
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

pub fn icon_glow<'a>(
    content: impl Into<Element<'a, crate::Message>>,
) -> Element<'a, crate::Message> {
    HoverArea::new(content).icon_glow().into()
}

pub fn action_glow<'a>(content: impl Into<Element<'a, crate::Message>>, glyph_y: f32) -> Element<'a, crate::Message> {
    let mut area = HoverArea::new(content).icon_glow();
    area.glow_y = Some(glyph_y);
    area.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced::widget::{column, container, scrollable, text, Space};
    use iced::Point;
    use iced_runtime::user_interface::{Cache, UserInterface};

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {
        Click,
        Menu,
    }

    struct Harness {
        renderer: iced::Renderer,
        cache: Option<Cache>,
        chat: bool,
        slots: HoverSlots,
        messages: Vec<Msg>,
    }
    impl Harness {
        fn new(chat: bool) -> Self {
            Self {
                renderer: iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
                    iced::Font::DEFAULT,
                    iced::Pixels(16.0),
                    Some("tiny-skia"),
                ))
                .unwrap(),
                cache: Some(Cache::default()),
                chat,
                slots: HoverSlots::default(),
                messages: Vec::new(),
            }
        }
        fn frame(&mut self, event: Event) {
            self.frame_at(event, mouse::Cursor::Unavailable);
        }
        fn frame_at(&mut self, event: Event, cursor: mouse::Cursor) {
            let body = container(text("Touch target")).width(280).height(72);
            let target = if self.chat {
                chat_row(body, 0, false, true, &self.slots, Msg::Click, Msg::Menu)
            } else {
                track_msg(body, 10, &self.slots, Msg::Click, Msg::Menu)
            };
            let content: Element<'_, Msg> = scrollable(column![target, Space::new().height(1000)])
                .height(300)
                .into();
            let mut ui = UserInterface::build(
                content,
                Size::new(300.0, 300.0),
                self.cache.take().unwrap(),
                &mut self.renderer,
            );
            let _ = ui.update(
                &[event],
                cursor,
                &mut self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut self.messages,
            );
            self.cache = Some(ui.into_cache());
        }
        fn touch(&mut self, event: touch::Event) {
            self.frame(Event::Touch(event));
        }
        fn hold(&mut self) {
            self.frame(Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now()
                    + std::time::Duration::from_millis(crate::LONG_PRESS_MS + 1),
            )));
        }
    }
    const F: touch::Finger = touch::Finger(1);
    const AT: Point = Point::new(40.0, 32.0);

    #[test]
    fn mouse_emulated_holds_open_once_and_dragging_cancels() {
        for chat in [false, true] {
            for drag in [false, true] {
                let mut h = Harness::new(chat);
                let cursor = mouse::Cursor::Available(AT);
                h.frame_at(
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    cursor,
                );
                if drag {
                    h.frame_at(
                        Event::Mouse(mouse::Event::CursorMoved {
                            position: Point::new(100.0, 90.0),
                        }),
                        mouse::Cursor::Available(Point::new(100.0, 90.0)),
                    );
                }
                h.hold();
                h.frame_at(
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    cursor,
                );
                assert_eq!(h.messages, if drag { vec![] } else { vec![Msg::Menu] });
            }
        }
    }

    #[test]
    fn first_touch_hold_opens_chat_and_bubble_menus_without_hover() {
        for chat in [false, true] {
            let mut h = Harness::new(chat);
            h.touch(touch::Event::FingerPressed {
                id: F,
                position: AT,
            });
            assert!(h.messages.is_empty());
            h.hold();
            assert_eq!(h.messages, vec![Msg::Menu]);
            h.touch(touch::Event::FingerLifted {
                id: F,
                position: AT,
            });
            assert_eq!(
                h.messages,
                vec![Msg::Menu],
                "lifting must not activate the content below the menu"
            );
        }
    }

    #[test]
    fn swipes_and_cancelled_touches_never_open_menus() {
        for chat in [false, true] {
            for cancel in [false, true] {
                let mut h = Harness::new(chat);
                h.touch(touch::Event::FingerPressed {
                    id: F,
                    position: AT,
                });
                if cancel {
                    h.touch(touch::Event::FingerLost {
                        id: F,
                        position: AT,
                    });
                } else {
                    h.touch(touch::Event::FingerMoved {
                        id: F,
                        position: Point::new(40.0, 100.0),
                    });
                }
                h.hold();
                h.touch(touch::Event::FingerLifted {
                    id: F,
                    position: AT,
                });
                assert!(
                    h.messages.is_empty(),
                    "swiping/cancelling is neither a tap nor a menu"
                );
            }
        }
    }

    #[test]
    fn quick_touch_is_still_a_single_click() {
        for chat in [false, true] {
            let mut h = Harness::new(chat);
            h.touch(touch::Event::FingerPressed {
                id: F,
                position: AT,
            });
            h.touch(touch::Event::FingerLifted {
                id: F,
                position: AT,
            });
            assert_eq!(h.messages, vec![Msg::Click]);
            h.hold();
            assert_eq!(h.messages, vec![Msg::Click]);
        }
    }
}
