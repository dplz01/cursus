//! GTK-style kinetic (inertial) scrolling for iced scrollables.
//!
//! iced's built-in `scrollable` tracks the finger while down but stops dead on
//! lift. GTK `ScrolledWindow` continues with deceleration (`kinetic-scrolling`).
//! Motion sampling, elapsed-time deceleration and redraws stay in the widget.
//! Coasting never publishes app messages or rebuilds the conversation.

use std::collections::VecDeque;
use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::mouse;
use iced::touch;
use iced::{Element, Event, Length, Rectangle, Size, Vector};

const SAMPLE_WINDOW_MS: u128 = 80;
const MIN_FLING_PX_PER_SEC: f32 = 160.0;
const MAX_FLING_PX_PER_SEC: f32 = 5_500.0;

// Opening a modal or navigating cancels coasting without rebuilding scroll state.
static CANCEL_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) fn stop_all() {
    CANCEL_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
fn generation() -> u64 {
    CANCEL_GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn track<'a, Message, Theme, Renderer>(
    scroll_id: &'static str,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    track_anchored(scroll_id, false, content)
}

/// Like [`track`], but for scrollables with `Anchor::End` (chat history).
///
/// iced negates touch deltas for end-anchored scrollables via `Direction::align`,
/// while `scroll_by` does **not** — fling velocity must not be pre-negated or the
/// coast fights the swipe (spring feel).
pub fn track_end_anchored<'a, Message, Theme, Renderer>(
    scroll_id: &'static str,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    track_anchored(scroll_id, true, content)
}

fn track_anchored<'a, Message, Theme, Renderer>(
    scroll_id: &'static str,
    end_anchored: bool,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(KineticTrack {
        scroll_id,
        end_anchored,
        content: content.into(),
    })
}

struct KineticTrack<'a, Message, Theme, Renderer> {
    scroll_id: &'static str,
    /// When true, fling velocity matches finger travel (no sign flip).
    end_anchored: bool,
    content: Element<'a, Message, Theme, Renderer>,
}

#[derive(Default)]
struct TrackState {
    samples: VecDeque<(Instant, f32)>,
    pressed: bool,
    finger: Option<touch::Finger>,
    velocity: f32,
    last_tick: Option<Instant>,
    generation: u64,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for KineticTrack<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<TrackState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(TrackState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content]);
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
        let cursor = match event {
            Event::Touch(
                touch::Event::FingerPressed { position, .. }
                | touch::Event::FingerMoved { position, .. }
                | touch::Event::FingerLifted { position, .. }
                | touch::Event::FingerLost { position, .. },
            ) => mouse::Cursor::Available(*position),
            _ => cursor,
        };
        // Children (scrollable) first — they own the live finger-follow.
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

        let state = tree.state.downcast_mut::<TrackState>();
        if state.generation != generation() {
            state.velocity = 0.0;
            state.pressed = false;
            state.finger = None;
            state.samples.clear();
            state.generation = generation();
        }
        match event {
            Event::Touch(touch::Event::FingerPressed { id, position })
                if layout.bounds().contains(*position) && viewport.contains(*position) =>
            {
                state.velocity = 0.0;
                state.generation = generation();
                state.pressed = true;
                state.finger = Some(*id);
                state.samples.clear();
                state.samples.push_back((Instant::now(), position.y));
            }
            Event::Touch(touch::Event::FingerMoved { id, position })
                if state.finger == Some(*id) =>
            {
                let now = Instant::now();
                state.samples.push_back((now, position.y));
                while state
                    .samples
                    .front()
                    .is_some_and(|(t, _)| now.duration_since(*t).as_millis() > SAMPLE_WINDOW_MS)
                {
                    state.samples.pop_front();
                }
            }
            Event::Touch(touch::Event::FingerLost { .. }) => {
                state.pressed = false;
                state.finger = None;
                state.velocity = 0.0;
                state.samples.clear();
            }
            Event::Touch(touch::Event::FingerLifted { id, .. })
                if state.pressed && state.finger == Some(*id) =>
            {
                state.pressed = false;
                state.finger = None;
                // Include lift time: a stationary pause must stop a fling.
                let now = Instant::now();
                if let Some((_, y)) = state.samples.back().copied() {
                    state.samples.push_back((now, y));
                }
                state
                    .samples
                    .retain(|(time, _)| now.duration_since(*time).as_millis() <= SAMPLE_WINDOW_MS);
                if let Some(finger_vy) = estimate_velocity(&state.samples) {
                    // Start-anchored: iced touch uses (old-new) = -finger.
                    // End-anchored: iced also runs Direction::align → flips again,
                    // so live scroll matches +finger; scroll_by must too.
                    let mut scroll_vy = if self.end_anchored {
                        finger_vy
                    } else {
                        -finger_vy
                    };
                    scroll_vy =
                        scroll_vy.clamp(-MAX_FLING_PX_PER_SEC, MAX_FLING_PX_PER_SEC);
                    if scroll_vy.abs() >= MIN_FLING_PX_PER_SEC {
                        state.velocity = scroll_vy;
                        state.last_tick = Some(now);
                        shell.request_redraw();
                    }
                }
                state.samples.clear();
            }
            Event::Window(iced::window::Event::RedrawRequested(now)) => {
                if state.generation != generation() {
                    state.velocity = 0.0;
                }
                if state.velocity.abs() >= 20.0 && !state.pressed {
                    let dt = state
                        .last_tick
                        .map_or(0.0, |last| {
                            now.saturating_duration_since(last).as_secs_f32()
                        })
                        .min(0.05);
                    state.last_tick = Some(*now);
                    let (dy, velocity) = coast_step(state.velocity, dt);
                    state.velocity = velocity;
                    let mut step = MoveScroll {
                        target: self.scroll_id,
                        dy,
                        at: None,
                    };
                    self.content.as_widget_mut().operate(
                        &mut tree.children[0],
                        layout,
                        renderer,
                        &mut step,
                    );
                    let before = step.at;
                    step.dy = 0.0;
                    self.content.as_widget_mut().operate(
                        &mut tree.children[0],
                        layout,
                        renderer,
                        &mut step,
                    );
                    if before == step.at && dt > 0.0 {
                        state.velocity = 0.0;
                    } else {
                        shell.request_redraw();
                    }
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { .. } | mouse::Event::ButtonPressed(_))
            | Event::Window(iced::window::Event::Unfocused) => {
                state.velocity = 0.0;
            }
            _ => {}
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

fn estimate_velocity(samples: &VecDeque<(Instant, f32)>) -> Option<f32> {
    if samples.len() < 2 {
        return None;
    }
    let (t0, y0) = *samples.front()?;
    let (t1, y1) = *samples.back()?;
    let dt = t1.duration_since(t0).as_secs_f32();
    if dt < 0.008 {
        return None;
    }
    // Finger moved from y0 → y1; positive when finger moves down.
    Some((y1 - y0) / dt)
}

/// Exact exponential integration, independent of refresh rate and frame stalls.
fn coast_step(velocity: f32, dt: f32) -> (f32, f32) {
    let decay = (-4.5 * dt).exp();
    (velocity * (1.0 - decay) / 4.5, velocity * decay)
}

struct MoveScroll {
    target: &'static str,
    dy: f32,
    at: Option<f32>,
}
impl Operation for MoveScroll {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn scrollable(
        &mut self,
        id: Option<&iced::widget::Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        if id == Some(&iced::widget::Id::new(self.target)) {
            self.at = Some(translation.y);
            if self.dy != 0.0 {
                state.scroll_by(
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: self.dy },
                    bounds,
                    content,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coasting_is_the_same_at_60_and_120_hz() {
        let integrate = |hz| {
            let mut v = 2400.0;
            let mut y = 0.0;
            for _ in 0..hz {
                let step = coast_step(v, 1.0 / hz as f32);
                y += step.0;
                v = step.1;
            }
            (y, v)
        };
        let a = integrate(60);
        let b = integrate(120);
        assert!((a.0 - b.0).abs() < 0.01);
        assert!((a.1 - b.1).abs() < 0.01);
    }
    #[test]
    fn a_pause_or_a_single_sample_has_no_release_velocity() {
        let now = Instant::now();
        assert!(estimate_velocity(&VecDeque::from([(now, 0.0)])).is_none());
        assert_eq!(
            estimate_velocity(&VecDeque::from([
                (now, 100.0),
                (now + std::time::Duration::from_millis(50), 100.0)
            ])),
            Some(0.0)
        );
    }
}
