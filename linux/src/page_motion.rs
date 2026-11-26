//! Phone page transitions, the way GTK / libadwaita's `AdwNavigationView`
//! does them (one column only; two-column windows switch pages at once):
//!
//! - Push: the new page slides in from the right edge over the previous
//!   page, which stays still and is dimmed (no parallax). A soft 56px shadow
//!   runs along the moving page's left edge.
//! - Pop: the exact reverse — the page slides out to the right, uncovering
//!   the previous one. The closing message is held back until the page has
//!   left, so the page can still be drawn while it moves.
//! - Timing: libadwaita's critically damped spring (damping ratio 1, mass 1,
//!   stiffness 1000), about 0.29 s.
//!
//! The moving page is always wrapped (at rest the wrapper is a no-op), so
//! its widget state — focus, scroll — is the same before and after.
use std::time::{Duration, Instant};

use crate::Message;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{overlay, Clipboard, Shell};
use iced::gradient::Linear;
use iced::{mouse, Color, Element, Event, Gradient, Length, Radians, Rectangle, Size, Theme, Vector};

/// Which page is showing, as far as transitions care.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PageKey {
    pub nav: Option<crate::ui::NavSection>,
    pub chat: Option<cursus_core::ChatId>,
    /// 0 none · 1 a profile page · 2 a page inside it (settings, members…).
    pub info_level: u8,
    /// Which page inside the profile, so moving between them animates too.
    pub info_page: u8,
    /// Member support pages (list / thread): never animated.
    pub support: bool,
    pub settings: Option<crate::SettingsSection>,
}

impl PageKey {
    pub(crate) fn depth(&self) -> u8 {
        u8::from(self.chat.is_some()) + self.info_level + u8::from(self.settings.is_some())
    }
}

/// The page under the moving one (what a push covers / a pop uncovers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Under {
    /// The destination's list (chats, settings categories…).
    List,
    /// The open conversation.
    Conversation,
    /// The contact page's overview, under its settings.
    ContactOverview,
    /// The group page's overview, under its inner pages.
    GroupOverview,
}

/// What the moving page is doing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Phase {
    /// Sliding in over the page below.
    Push,
    /// Sliding out (Back), uncovering the page below.
    Pop,
    /// Following a finger swiping back from the left edge: offset 0…1.
    Drag(f32),
    /// Let go: springing from `from` (with the finger's `velocity`, widths
    /// per second) to gone (`pop`) or back to rest.
    Settle { from: f32, velocity: f32, pop: bool },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Motion {
    start: Instant,
    pub phase: Phase,
    pub under: Under,
}

/// libadwaita `SPRING_STIFFNESS` with damping ratio 1 and mass 1.
const STIFFNESS: f32 = 1000.0;
/// When the spring is within libadwaita's 0.001 epsilon of rest:
/// (1 + ωt)·e^(−ωt) = 0.001 → ωt ≈ 9.233, ω = √1000.
const DURATION: Duration = Duration::from_millis(292);
/// libadwaita's spring epsilon.
const EPSILON: f32 = 0.001;

/// Critically damped spring from 0 to 1.
fn spring(t: f32) -> f32 {
    settle(0.0, 0.0, 1.0, t)
}

/// Critically damped spring from `from` with `velocity` (units / s) to `to`.
fn settle(from: f32, velocity: f32, to: f32, t: f32) -> f32 {
    let w = STIFFNESS.sqrt();
    let c1 = from - to;
    let c2 = velocity + w * c1;
    to + (c1 + c2 * t) * (-w * t).exp()
}

impl Motion {
    pub(crate) fn push(under: Under, now: Instant) -> Self {
        Self { start: now, phase: Phase::Push, under }
    }

    pub(crate) fn pop(under: Under, now: Instant) -> Self {
        Self { start: now, phase: Phase::Pop, under }
    }

    pub(crate) fn drag(under: Under, now: Instant) -> Self {
        Self { start: now, phase: Phase::Drag(0.0), under }
    }

    /// How far the moving page sits from its resting place, as a fraction
    /// of the width: push 1 → 0, pop 0 → 1.
    pub(crate) fn offset(&self, now: Instant) -> f32 {
        let t = now.saturating_duration_since(self.start).as_secs_f32();
        let o = match self.phase {
            Phase::Push | Phase::Pop => {
                let x = if self.done(now) { 1.0 } else { spring(t) };
                if self.phase == Phase::Pop { x } else { 1.0 - x }
            }
            Phase::Drag(o) => o,
            Phase::Settle { from, velocity, pop } => {
                if self.done(now) {
                    if pop { 1.0 } else { 0.0 }
                } else {
                    settle(from, velocity, if pop { 1.0 } else { 0.0 }, t)
                }
            }
        };
        o.clamp(0.0, 1.0)
    }

    pub(crate) fn done(&self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.start);
        match self.phase {
            Phase::Push | Phase::Pop => elapsed >= DURATION,
            Phase::Drag(_) => false,
            Phase::Settle { from, velocity, pop } => {
                let t = elapsed.as_secs_f32();
                let to = if pop { 1.0 } else { 0.0 };
                let at = settle(from, velocity, to, t);
                // Going: stop on arrival (libadwaita clamps pops);
                // staying: stop once it has settled.
                (pop && at >= 1.0) || (at - to).abs() < EPSILON && t > 0.05 || elapsed >= Duration::from_secs(1)
            }
        }
    }

    /// The finger let go of a back swipe: spring from where it is.
    pub(crate) fn release(&mut self, pop: bool, velocity: f32, now: Instant) {
        if let Phase::Drag(from) = self.phase {
            self.phase = Phase::Settle { from, velocity, pop };
            self.start = now;
        }
    }

    /// Frames are only needed while the page moves on its own.
    pub(crate) fn animating(&self) -> bool {
        !matches!(self.phase, Phase::Drag(_))
    }
}

/// libadwaita's swipe end (touch): a flick faster than 0.3 px/ms goes the
/// way it was flung; a slower release goes to the nearer end.
pub(crate) fn swipe_pops(offset: f32, velocity_px_ms: f32) -> bool {
    if velocity_px_ms.abs() < VELOCITY_THRESHOLD {
        offset > 0.5
    } else {
        velocity_px_ms > 0.0
    }
}

/// Whether moving from `old` to `new` is a push (a page opened over another).
pub(crate) fn is_push(old: &PageKey, new: &PageKey) -> bool {
    old != new
        && old.nav == new.nav
        && !old.support
        && !new.support
        && new.depth() > 0
        && new.depth() >= old.depth()
}

/// libadwaita's `--shade-color`: RGB(0 0 6) at 7% (light) / 25% (dark).
pub(crate) fn shade(dark: bool) -> Color {
    Color::from_rgba8(0, 0, 6, if dark { 0.25 } else { 0.07 })
}

/// Wrap the top page. `offset` = fraction of the width it is moved right
/// (0 at rest); the area it uncovers is dimmed and shadowed with `shade`.
pub(crate) fn slide<'a>(content: impl Into<Element<'a, Message>>, offset: f32, shade: Color) -> Element<'a, Message> {
    Element::new(Slide { content: content.into(), offset: offset.clamp(0.0, 1.0), shade })
}

struct Slide<'a> {
    content: Element<'a, Message>,
    offset: f32,
    shade: Color,
}

/// Width of the shadow along the moving page's edge (libadwaita `shadow`).
const SHADOW: f32 = 56.0;

impl Slide<'_> {
    fn with_alpha(&self, factor: f32) -> Color {
        Color { a: self.shade.a * factor, ..self.shade }
    }

    /// libadwaita `background-shadow(to left)` from the page edge at `edge`,
    /// with its overall `opacity`.
    fn shadow_layers(&self, edge: f32, bounds: Rectangle, opacity: f32) -> [(Rectangle, Gradient); 2] {
        let layer = |width: f32, stops: [(f32, f32); 3]| {
            let rect = Rectangle { x: edge - width, y: bounds.y, width, height: bounds.height };
            // Darkest at the page edge (right), clear at the far end (left).
            let mut g = Linear::new(Radians(std::f32::consts::FRAC_PI_2));
            // `add_stop` keeps stops sorted only when added in order.
            for (at, alpha) in stops.into_iter().rev() {
                g = g.add_stop(1.0 - at / width, self.with_alpha(alpha * opacity));
            }
            (rect, Gradient::Linear(g))
        };
        [
            layer(SHADOW, [(0.0, 0.70), (40.0, 0.14), (56.0, 0.0)]),
            layer(24.0, [(0.0, 0.40), (7.0, 0.14), (24.0, 0.0)]),
        ]
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
        let shift = (self.offset * size.width).round();
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
        if self.offset <= 0.0 {
            self.content.as_widget().draw(&tree.children[0], renderer, theme, style, child, cursor, viewport);
            return;
        }
        let Some(clip) = bounds.intersection(viewport) else { return };
        let edge = child.bounds().x;
        let uncovered = edge - bounds.x;
        // Over the page underneath: dimming (opacity 1 − offset) and the
        // shadow, which fades in over its own width as the page leaves.
        renderer.with_layer(clip, |r| {
            r.fill_quad(
                renderer::Quad {
                    bounds: Rectangle { width: uncovered, ..bounds },
                    ..renderer::Quad::default()
                },
                self.with_alpha(1.0 - self.offset),
            );
            let remaining = (1.0 - self.offset) * bounds.width;
            let opacity = (remaining / SHADOW).min(1.0);
            for (rect, gradient) in self.shadow_layers(edge, bounds, opacity) {
                r.fill_quad(renderer::Quad { bounds: rect, ..renderer::Quad::default() }, gradient);
            }
        });
        renderer.with_layer(clip, |r| {
            self.content.as_widget().draw(&tree.children[0], r, theme, style, child, cursor, &clip);
        });
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

// ── Sideways between destinations ────────────────────────────────────

/// `content` moved sideways by `offset` × its width (negative = left),
/// clipped to its own bounds: the lists of two destinations (Chats,
/// Channels, Calls, Settings) slide past each other side by side.
pub(crate) fn shift<'a>(content: impl Into<Element<'a, Message>>, offset: f32) -> Element<'a, Message> {
    Element::new(Shift { content: content.into(), offset: offset.clamp(-1.0, 1.0) })
}

struct Shift<'a> {
    content: Element<'a, Message>,
    offset: f32,
}

impl Widget<Message, Theme, iced::Renderer> for Shift<'_> {
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
        let shift = (self.offset * size.width).round();
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
        let Some(child) = layout.children().next() else { return };
        if self.offset == 0.0 {
            self.content.as_widget().draw(&tree.children[0], renderer, theme, style, child, cursor, viewport);
            return;
        }
        let Some(clip) = layout.bounds().intersection(viewport) else { return };
        renderer.with_layer(clip, |r| {
            self.content.as_widget().draw(&tree.children[0], r, theme, style, child, cursor, &clip);
        });
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
}

// ── Swipe back from the left edge ────────────────────────────────────

/// What the edge swipe tells the app.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Swipe {
    /// A back swipe took over the finger.
    Begin,
    /// The page follows the finger: offset 0…1 of the width.
    Update(f32),
    /// Let go: finish (`pop`) or return, starting at the finger's speed
    /// (widths per second).
    End { pop: bool, velocity: f32 },
}

/// Where a back swipe may start: this close to the left edge.
const EDGE: f32 = 24.0;
/// libadwaita `DRAG_THRESHOLD_DISTANCE`.
const DRAG_THRESHOLD: f32 = 16.0;
/// libadwaita `VELOCITY_THRESHOLD_TOUCH` (px / ms).
const VELOCITY_THRESHOLD: f32 = 0.3;
/// libadwaita `EVENT_HISTORY_THRESHOLD_MS`: speed is measured over this.
const HISTORY: Duration = Duration::from_millis(150);

/// Swipe right from the left edge to go back, like a GTK navigation view
/// on a touchscreen. Touches elsewhere (and the mouse) pass through.
pub(crate) fn back_swipe<'a>(
    content: impl Into<Element<'a, Message>>,
    enabled: bool,
    on_swipe: fn(Swipe) -> Message,
) -> Element<'a, Message> {
    Element::new(BackSwipe { content: content.into(), enabled, on_swipe })
}

struct BackSwipe<'a> {
    content: Element<'a, Message>,
    enabled: bool,
    on_swipe: fn(Swipe) -> Message,
}

#[derive(Default)]
struct SwipeState {
    finger: Option<Finger>,
}

struct Finger {
    id: iced::touch::Finger,
    start: iced::Point,
    /// Set once the swipe took over: x where progress 0 is.
    claimed_at: Option<f32>,
    /// (time, x) of recent moves, for the release speed.
    history: Vec<(Instant, f32)>,
}

impl Finger {
    fn record(&mut self, x: f32) {
        let now = Instant::now();
        self.history.retain(|(t, _)| now.saturating_duration_since(*t) <= HISTORY);
        self.history.push((now, x));
    }

    /// Speed over the recent history, px / ms (positive = rightwards).
    fn velocity(&self) -> f32 {
        match (self.history.first(), self.history.last()) {
            (Some((t0, x0)), Some((t1, x1))) if t1 > t0 => {
                (x1 - x0) / (t1.saturating_duration_since(*t0).as_secs_f32() * 1000.0)
            }
            _ => 0.0,
        }
    }
}

impl BackSwipe<'_> {
    #[allow(clippy::too_many_arguments)]
    fn forward(
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
}

impl Widget<Message, Theme, iced::Renderer> for BackSwipe<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<SwipeState>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(SwipeState::default())
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
    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let child = self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(child.size(), vec![child])
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
        if let Some(child) = layout.children().next() {
            self.content.as_widget().draw(&tree.children[0], renderer, theme, style, child, cursor, viewport);
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
        use iced::touch::Event as T;
        let bounds = layout.bounds();
        let Event::Touch(touch) = event else {
            return self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
        };
        match *touch {
            T::FingerPressed { id, position } => {
                let state = tree.state.downcast_mut::<SwipeState>();
                if self.enabled
                    && state.finger.is_none()
                    && bounds.contains(position)
                    && position.x - bounds.x < EDGE
                {
                    let mut finger = Finger { id, start: position, claimed_at: None, history: Vec::new() };
                    finger.record(position.x);
                    state.finger = Some(finger);
                }
                // The press still reaches the page (a tap on Back works);
                // a swipe cancels it below.
                self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
            }
            T::FingerMoved { id, position } => {
                let state = tree.state.downcast_mut::<SwipeState>();
                let Some(finger) = state.finger.as_mut().filter(|f| f.id == id) else {
                    return self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
                };
                finger.record(position.x);
                match finger.claimed_at {
                    Some(zero) => {
                        let offset = ((position.x - zero) / bounds.width.max(1.0)).clamp(0.0, 1.0);
                        shell.publish((self.on_swipe)(Swipe::Update(offset)));
                        shell.capture_event();
                    }
                    None => {
                        let (dx, dy) = (position.x - finger.start.x, position.y - finger.start.y);
                        if dx.hypot(dy) < DRAG_THRESHOLD {
                            return self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
                        }
                        if dx > 0.0 && dx.abs() > dy.abs() {
                            // A back swipe: release the page's hold on the
                            // finger without a click, and follow it from here.
                            finger.claimed_at = Some(position.x);
                            let lost = Event::Touch(T::FingerLost { id, position });
                            self.forward(tree, &lost, layout, cursor, renderer, clipboard, shell, viewport);
                            shell.publish((self.on_swipe)(Swipe::Begin));
                            shell.capture_event();
                        } else {
                            // Vertical or leftwards: an ordinary gesture.
                            state.finger = None;
                            self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
                        }
                    }
                }
            }
            T::FingerLifted { id, position } | T::FingerLost { id, position } => {
                let state = tree.state.downcast_mut::<SwipeState>();
                let Some(mut finger) = state.finger.take_if(|f| f.id == id) else {
                    return self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
                };
                let Some(zero) = finger.claimed_at else {
                    return self.forward(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
                };
                finger.record(position.x);
                let offset = ((position.x - zero) / bounds.width.max(1.0)).clamp(0.0, 1.0);
                let velocity = finger.velocity();
                let lost = matches!(touch, T::FingerLost { .. });
                shell.publish((self.on_swipe)(Swipe::End {
                    // A cancelled touch (the system took it) returns the page.
                    pop: !lost && swipe_pops(offset, velocity),
                    velocity: velocity * 1000.0 / bounds.width.max(1.0),
                }));
                shell.capture_event();
            }
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
    fn opening_pages_pushes() {
        let (list, chat, profile) = (key(false, 0), key(true, 0), key(true, 1));
        assert!(is_push(&list, &chat));
        assert!(is_push(&chat, &profile));
        assert!(!is_push(&profile, &chat), "Back is a pop, not a push");
        assert!(!is_push(&chat, &list));
        let mut other_nav = chat.clone();
        other_nav.nav = Some(crate::ui::NavSection::Settings);
        assert!(!is_push(&other_nav, &chat), "destination switches are instant");
        let mut support = profile.clone();
        support.support = true;
        assert!(!is_push(&chat, &support));
    }

    #[test]
    fn swipe_end_follows_libadwaita() {
        assert!(swipe_pops(0.6, 0.0), "slow past half: go back");
        assert!(!swipe_pops(0.4, 0.1), "slow before half: stay");
        assert!(swipe_pops(0.1, 0.5), "a flick right goes back from anywhere");
        assert!(!swipe_pops(0.9, -0.5), "a flick left stays");
    }

    #[test]
    fn spring_matches_libadwaita() {
        let now = Instant::now();
        let push = Motion::push(Under::List, now);
        assert_eq!(push.offset(now), 1.0, "a push starts off-screen");
        // Critically damped: about half-way after ~53 ms, no overshoot.
        let mid = push.offset(now + Duration::from_millis(53));
        assert!((0.45..0.55).contains(&mid), "{mid}");
        let mut last = 1.0;
        for ms in (0..=300).step_by(10) {
            let o = push.offset(now + Duration::from_millis(ms));
            assert!(o <= last && o >= 0.0);
            last = o;
        }
        assert_eq!(push.offset(now + DURATION), 0.0);
        let gone = Motion { start: now, phase: Phase::Settle { from: 0.6, velocity: 2.0, pop: true }, under: Under::List };
        assert!(gone.offset(now + Duration::from_millis(30)) > 0.6, "keeps the finger's direction");
        assert!(gone.done(now + Duration::from_millis(400)));
        assert_eq!(gone.offset(now + Duration::from_millis(400)), 1.0);
        let back = Motion { start: now, phase: Phase::Settle { from: 0.3, velocity: 0.0, pop: false }, under: Under::List };
        assert!(back.offset(now + Duration::from_millis(100)) < 0.3);
        assert!(back.done(now + Duration::from_millis(600)));
        let pop = Motion::pop(Under::Conversation, now);
        assert_eq!(pop.offset(now), 0.0);
        assert_eq!(pop.offset(now + DURATION), 1.0);
        assert!(pop.done(now + DURATION));
    }
}
