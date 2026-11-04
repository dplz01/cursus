//! GTK-style kinetic (inertial) scrolling for iced scrollables.
//!
//! iced's built-in `scrollable` tracks the finger while down but stops dead on
//! lift. GTK `ScrolledWindow` continues with deceleration (`kinetic-scrolling`).
//! This wrapper samples touch motion and publishes a fling velocity on lift so
//! the app can keep scrolling via `scroll_by` ticks.

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
const MIN_FLING_PX_PER_SEC: f32 = 550.0;
const MAX_FLING_PX_PER_SEC: f32 = 2_200.0;
/// Soften so flings coast instead of launching.
const FLING_GAIN: f32 = 0.55;

#[derive(Debug, Clone)]
pub enum FlingMessage {
    /// Finger lifted with enough velocity — start inertial scroll.
    Start { id: String, velocity_y: f32 },
    /// New press — cancel any active fling on this scrollable.
    Cancel { id: String },
}

pub fn track<'a, Message, Theme, Renderer>(
    scroll_id: &'static str,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    on_fling: impl Fn(FlingMessage) -> Message + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    track_anchored(scroll_id, false, content, on_fling)
}

/// Like [`track`], but for scrollables with `Anchor::End` (chat history).
///
/// iced negates touch deltas for end-anchored scrollables via `Direction::align`,
/// while `scroll_by` does **not** — fling velocity must not be pre-negated or the
/// coast fights the swipe (spring feel).
pub fn track_end_anchored<'a, Message, Theme, Renderer>(
    scroll_id: &'static str,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    on_fling: impl Fn(FlingMessage) -> Message + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    track_anchored(scroll_id, true, content, on_fling)
}

fn track_anchored<'a, Message, Theme, Renderer>(
    scroll_id: &'static str,
    end_anchored: bool,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    on_fling: impl Fn(FlingMessage) -> Message + 'a,
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
        on_fling: Box::new(on_fling),
    })
}

struct KineticTrack<'a, Message, Theme, Renderer> {
    scroll_id: &'static str,
    /// When true, fling velocity matches finger travel (no sign flip).
    end_anchored: bool,
    content: Element<'a, Message, Theme, Renderer>,
    on_fling: Box<dyn Fn(FlingMessage) -> Message + 'a>,
}

#[derive(Default)]
struct TrackState {
    samples: VecDeque<(Instant, f32)>,
    pressed: bool,
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
        let over = cursor.is_over(layout.bounds());

        match event {
            Event::Touch(touch::Event::FingerPressed { position, .. }) if over => {
                state.pressed = true;
                state.samples.clear();
                state.samples.push_back((Instant::now(), position.y));
                shell.publish((self.on_fling)(FlingMessage::Cancel {
                    id: self.scroll_id.to_string(),
                }));
            }
            Event::Touch(touch::Event::FingerMoved { position, .. }) if state.pressed => {
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
            Event::Touch(touch::Event::FingerLifted { .. } | touch::Event::FingerLost { .. })
                if state.pressed =>
            {
                state.pressed = false;
                if let Some(finger_vy) = estimate_velocity(&state.samples) {
                    // Start-anchored: iced touch uses (old-new) = -finger.
                    // End-anchored: iced also runs Direction::align → flips again,
                    // so live scroll matches +finger; scroll_by must too.
                    let mut scroll_vy = if self.end_anchored {
                        finger_vy
                    } else {
                        -finger_vy
                    };
                    scroll_vy = (scroll_vy * FLING_GAIN)
                        .clamp(-MAX_FLING_PX_PER_SEC, MAX_FLING_PX_PER_SEC);
                    if scroll_vy.abs() >= MIN_FLING_PX_PER_SEC * FLING_GAIN {
                        shell.publish((self.on_fling)(FlingMessage::Start {
                            id: self.scroll_id.to_string(),
                            velocity_y: scroll_vy,
                        }));
                    }
                }
                state.samples.clear();
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
