//! Touch taps that don't steal scrolls.
//!
//! iced's `button` / `toggler` / `text_input` capture `FingerPressed`, and a
//! scrollable only starts touch-scrolling when the press reaches it
//! uncaptured — so a swipe that starts on a button never scrolls the page.
//! iced's `rich_text` links only listen to the mouse, so links never open on
//! touch at all.
//!
//! - [`defer_touch`] holds a finger press back from its content: a lift
//!   without travel replays press + lift (a tap); travel past [`SLOP`] leaves
//!   the gesture to the enclosing scrollable; a finger held still for
//!   [`HOLD`] forwards the press (long-press menus, pressed look), and later
//!   travel still cancels it with `FingerLost` so the page can scroll.
//! - [`tap_links`] turns a short, still touch tap into a mouse click for
//!   `rich_text` link spans, without ever capturing the press.
//!
//! Mouse events pass through both untouched, so desktop behaviour is the same.

use std::time::Duration;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::time::Instant;
use iced::{mouse, touch, window, Element, Event, Length, Point, Rectangle, Size, Vector};

/// Finger travel (logical px) that turns a press into a scroll.
pub const SLOP: f32 = 12.0;
/// Hold time after which a still finger's press reaches the content.
pub const HOLD: Duration = Duration::from_millis(160);
/// Longest press still treated as a link tap (longer = long-press menu).
const LINK_TAP_MAX: Duration = Duration::from_millis(450);

fn far(a: Point, b: Point) -> bool {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy > SLOP * SLOP
}

#[derive(Debug, Clone, Copy)]
struct Press {
    id: touch::Finger,
    origin: Point,
    at: Instant,
}

#[derive(Debug, Default)]
struct DeferState {
    /// Press held back from the content.
    pending: Option<Press>,
    /// Press already handed to the content (held past [`HOLD`]).
    forwarded: Option<Press>,
}

/// Wrap scrollable page content so taps work and swipes scroll everywhere.
pub fn defer_touch<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(Wrap {
        content: content.into(),
        kind: Kind::Defer,
    })
}

#[derive(Debug, Default)]
struct LinkState {
    press: Option<Press>,
}

/// Make `rich_text` link spans open on a touch tap (scrolling unaffected).
pub fn tap_links<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(Wrap {
        content: content.into(),
        kind: Kind::Links,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Defer,
    Links,
}

struct Wrap<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    kind: Kind,
}

impl<Message, Theme, Renderer> Wrap<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    #[allow(clippy::too_many_arguments)]
    fn forward(
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
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    /// Forward a synthesized event through its own [`Shell`] so a capture by
    /// an earlier synthetic event (e.g. the replayed press) doesn't make the
    /// content ignore this one (iced's `button` bails on captured events).
    #[allow(clippy::too_many_arguments)]
    fn forward_fresh(
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
        let mut messages = Vec::new();
        let mut sub = Shell::new(&mut messages);
        self.forward(tree, event, layout, cursor, renderer, clipboard, &mut sub, viewport);
        shell.merge(sub, |m| m);
    }

    #[allow(clippy::too_many_arguments)]
    fn update_defer(
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
        let (pending, forwarded) = {
            let s = tree.state.downcast_ref::<DeferState>();
            (s.pending, s.forwarded)
        };
        let set = |tree: &mut Tree, p: Option<Press>, f: Option<Press>| {
            let s = tree.state.downcast_mut::<DeferState>();
            s.pending = p;
            s.forwarded = f;
        };
        match event {
            Event::Touch(touch::Event::FingerPressed { id, position }) => {
                if pending.is_none() && forwarded.is_none() && cursor.is_over(layout.bounds()) {
                    let at = Instant::now();
                    set(
                        tree,
                        Some(Press {
                            id: *id,
                            origin: *position,
                            at,
                        }),
                        None,
                    );
                    shell.request_redraw_at(at + HOLD);
                    return;
                }
                // Second finger while one is tracked: drop the tap, pass through.
                if pending.is_some() {
                    set(tree, None, forwarded);
                }
                self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
            }
            Event::Touch(touch::Event::FingerMoved { id, position }) => {
                if let Some(p) = pending.filter(|p| p.id == *id) {
                    if far(p.origin, *position) {
                        set(tree, None, None);
                    }
                    return;
                }
                if let Some(p) = forwarded.filter(|p| p.id == *id) {
                    if far(p.origin, *position) {
                        // Became a scroll: release the content without a click.
                        set(tree, None, None);
                        let lost = Event::Touch(touch::Event::FingerLost {
                            id: *id,
                            position: *position,
                        });
                        self.forward_fresh(tree, &lost, layout, cursor, renderer, clipboard, shell, viewport);
                    } else {
                        self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
                    }
                    return;
                }
                self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
            }
            Event::Touch(touch::Event::FingerLifted { id, .. }) => {
                if let Some(p) = pending.filter(|p| p.id == *id) {
                    set(tree, None, None);
                    let press = Event::Touch(touch::Event::FingerPressed {
                        id: p.id,
                        position: p.origin,
                    });
                    self.forward_fresh(tree, &press, layout, cursor, renderer, clipboard, shell, viewport);
                    self.forward_fresh(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
                    return;
                }
                if forwarded.is_some_and(|p| p.id == *id) {
                    set(tree, None, None);
                    self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
                    return;
                }
                // Lift of a gesture that turned into a scroll: the content never
                // saw its press, so don't let release handlers fire.
                if pending.is_none() && forwarded.is_none() {
                    return;
                }
                self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
            }
            Event::Touch(touch::Event::FingerLost { id, .. }) => {
                if pending.is_some_and(|p| p.id == *id) {
                    set(tree, None, None);
                    return;
                }
                if forwarded.is_some_and(|p| p.id == *id) {
                    set(tree, None, None);
                }
                self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
                if let Some(p) = pending {
                    if *now >= p.at + HOLD {
                        set(tree, None, Some(p));
                        let press = Event::Touch(touch::Event::FingerPressed {
                            id: p.id,
                            position: p.origin,
                        });
                        self.forward_fresh(tree, &press, layout, cursor, renderer, clipboard, shell, viewport);
                        shell.request_redraw();
                    } else {
                        shell.request_redraw_at(p.at + HOLD);
                    }
                }
            }
            _ => self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_links(
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
        let press = tree.state.downcast_ref::<LinkState>().press;
        match event {
            Event::Touch(touch::Event::FingerPressed { id, position }) => {
                tree.state.downcast_mut::<LinkState>().press =
                    cursor.is_over(layout.bounds()).then_some(Press {
                        id: *id,
                        origin: *position,
                        at: Instant::now(),
                    });
            }
            Event::Touch(touch::Event::FingerMoved { id, position }) => {
                if press.is_some_and(|p| p.id == *id && far(p.origin, *position)) {
                    tree.state.downcast_mut::<LinkState>().press = None;
                }
            }
            Event::Touch(touch::Event::FingerLifted { id, .. }) => {
                tree.state.downcast_mut::<LinkState>().press = None;
                let Some(p) = press.filter(|p| p.id == *id) else {
                    return;
                };
                if p.at.elapsed() > LINK_TAP_MAX || !cursor.is_over(layout.bounds()) {
                    return;
                }
                // `rich_text` hit-tests links against the cursor (= the finger).
                let down = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
                let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
                self.forward_fresh(tree, &down, layout, cursor, renderer, clipboard, shell, viewport);
                self.forward_fresh(tree, &up, layout, cursor, renderer, clipboard, shell, viewport);
            }
            Event::Touch(touch::Event::FingerLost { .. }) => {
                tree.state.downcast_mut::<LinkState>().press = None;
            }
            _ => self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport),
        }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Wrap<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        match self.kind {
            Kind::Defer => tree::Tag::of::<DeferState>(),
            Kind::Links => tree::Tag::of::<LinkState>(),
        }
    }

    fn state(&self) -> tree::State {
        match self.kind {
            Kind::Defer => tree::State::new(DeferState::default()),
            Kind::Links => tree::State::new(LinkState::default()),
        }
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

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
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

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        match self.kind {
            Kind::Defer => {
                self.update_defer(tree, event, layout, cursor, renderer, clipboard, shell, viewport)
            }
            Kind::Links => {
                self.update_links(tree, event, layout, cursor, renderer, clipboard, shell, viewport)
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced::widget::{button, column, rich_text, scrollable, span, text};
    use iced::Theme;
    use iced_runtime::user_interface::{Cache, UserInterface};

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Clicked(usize),
        Scrolled(f32),
        Link(&'static str),
    }

    struct NoClip;
    impl Clipboard for NoClip {
        fn read(&self, _: iced::advanced::clipboard::Kind) -> Option<String> {
            None
        }
        fn write(&mut self, _: iced::advanced::clipboard::Kind, _: String) {}
    }

    struct H {
        renderer: iced::Renderer,
        cache: Option<Cache>,
        msgs: Vec<Msg>,
        links: bool,
    }

    impl H {
        fn new(links: bool) -> Self {
            let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
                iced::Font::DEFAULT,
                iced::Pixels(16.0),
                Some("tiny-skia"),
            ))
            .expect("headless renderer");
            Self {
                renderer,
                cache: Some(Cache::default()),
                msgs: Vec::new(),
                links,
            }
        }

        fn view(links: bool) -> Element<'static, Msg> {
            if links {
                let rich = rich_text([
                    span("go "),
                    span("LINKLINKLINK").link(Msg::Link("x")),
                ])
                .on_link_click(|m: Msg| m)
                .size(20);
                let col = column![tap_links(rich), text("filler").height(2000)];
                return scrollable(col)
                    .height(300)
                    .on_scroll(|v| Msg::Scrolled(v.absolute_offset().y))
                    .into();
            }
            let mut col = iced::widget::Column::new();
            for i in 0..30 {
                col = col.push(button(text(format!("b{i}"))).height(80).width(300).on_press(Msg::Clicked(i)));
            }
            scrollable(defer_touch(col))
                .height(300)
                .on_scroll(|v| Msg::Scrolled(v.absolute_offset().y))
                .into()
        }

        fn frame(&mut self, events: &[Event], at: Point) {
            let mut ui = UserInterface::build(
                Self::view(self.links),
                Size::new(400.0, 300.0),
                self.cache.take().unwrap(),
                &mut self.renderer,
            );
            let _ = ui.update(
                events,
                mouse::Cursor::Available(at),
                &mut self.renderer,
                &mut NoClip,
                &mut self.msgs,
            );
            ui.draw(
                &mut self.renderer,
                &Theme::Dark,
                &renderer::Style::default(),
                mouse::Cursor::Available(at),
            );
            self.cache = Some(ui.into_cache());
        }

        fn touch(&mut self, ev: touch::Event) {
            let at = match ev {
                touch::Event::FingerPressed { position, .. }
                | touch::Event::FingerMoved { position, .. }
                | touch::Event::FingerLifted { position, .. }
                | touch::Event::FingerLost { position, .. } => position,
            };
            self.frame(&[Event::Touch(ev)], at);
        }

        fn clicks(&self) -> Vec<usize> {
            self.msgs
                .iter()
                .filter_map(|m| match m {
                    Msg::Clicked(i) => Some(*i),
                    _ => None,
                })
                .collect()
        }

        fn scrolled(&self) -> bool {
            self.msgs.iter().any(|m| matches!(m, Msg::Scrolled(y) if *y > 1.0))
        }
    }

    const F: touch::Finger = touch::Finger(1);

    #[test]
    fn swipe_starting_on_button_scrolls_without_click() {
        let mut h = H::new(false);
        let p = Point::new(50.0, 200.0);
        h.touch(touch::Event::FingerPressed { id: F, position: p });
        for dy in [10.0, 30.0, 60.0, 100.0] {
            h.touch(touch::Event::FingerMoved { id: F, position: Point::new(50.0, 200.0 - dy) });
        }
        h.touch(touch::Event::FingerLifted { id: F, position: Point::new(50.0, 100.0) });
        assert!(h.scrolled(), "page should scroll: {:?}", h.msgs);
        assert!(h.clicks().is_empty(), "no click after a swipe: {:?}", h.msgs);
    }

    #[test]
    fn tap_clicks_button() {
        let mut h = H::new(false);
        let p = Point::new(50.0, 120.0);
        h.touch(touch::Event::FingerPressed { id: F, position: p });
        h.touch(touch::Event::FingerMoved { id: F, position: Point::new(52.0, 122.0) });
        h.touch(touch::Event::FingerLifted { id: F, position: Point::new(52.0, 122.0) });
        assert_eq!(h.clicks(), vec![1]);
        assert!(!h.scrolled());
    }

    #[test]
    fn held_press_is_forwarded_then_scroll_cancels_it() {
        let mut h = H::new(false);
        let p = Point::new(50.0, 200.0);
        h.touch(touch::Event::FingerPressed { id: F, position: p });
        h.frame(
            &[Event::Window(window::Event::RedrawRequested(Instant::now() + HOLD * 2))],
            p,
        );
        // Held and lifted in place: still a click.
        h.touch(touch::Event::FingerLifted { id: F, position: p });
        assert_eq!(h.clicks(), vec![2]);

        h.msgs.clear();
        h.touch(touch::Event::FingerPressed { id: F, position: p });
        h.frame(
            &[Event::Window(window::Event::RedrawRequested(Instant::now() + HOLD * 2))],
            p,
        );
        for dy in [20.0, 60.0, 100.0] {
            h.touch(touch::Event::FingerMoved { id: F, position: Point::new(50.0, 200.0 - dy) });
        }
        h.touch(touch::Event::FingerLifted { id: F, position: Point::new(50.0, 100.0) });
        assert!(h.clicks().is_empty(), "drag after hold must not click: {:?}", h.msgs);
    }

    #[test]
    fn mouse_click_unchanged() {
        let mut h = H::new(false);
        let p = Point::new(50.0, 40.0);
        h.frame(&[Event::Mouse(mouse::Event::CursorMoved { position: p })], p);
        h.frame(&[Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))], p);
        h.frame(&[Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))], p);
        assert_eq!(h.clicks(), vec![0]);
    }

    #[test]
    fn touch_tap_opens_link_and_swipe_does_not() {
        let mut h = H::new(true);
        let on_link = Point::new(90.0, 10.0);
        h.touch(touch::Event::FingerPressed { id: F, position: on_link });
        h.touch(touch::Event::FingerLifted { id: F, position: on_link });
        assert!(h.msgs.contains(&Msg::Link("x")), "tap should open: {:?}", h.msgs);

        h.msgs.clear();
        h.touch(touch::Event::FingerPressed { id: F, position: on_link });
        for dy in [20.0, 60.0] {
            h.touch(touch::Event::FingerMoved { id: F, position: Point::new(90.0, 10.0 + dy) });
        }
        h.touch(touch::Event::FingerLifted { id: F, position: Point::new(90.0, 70.0) });
        assert!(!h.msgs.contains(&Msg::Link("x")), "swipe must not open: {:?}", h.msgs);
    }
}
