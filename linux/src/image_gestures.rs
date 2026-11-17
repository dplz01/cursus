//! Iced's native, cursor-anchored image viewer with touch pinch/pan and photo swipes.
use crate::Message;
use iced::advanced::{layout, renderer, widget, Clipboard, Layout, Shell, Widget};
use iced::widget::image::{self, Handle};
use iced::{mouse, touch, ContentFit, Element, Event, Length, Point, Rectangle, Size};
use std::path::{Path, PathBuf};

pub(crate) fn view<'a>(path: &Path, zoom: f32) -> Element<'a, Message> {
    let handle = Handle::from_path(path);
    Element::new(Photo {
        path: path.into(),
        initial_zoom: zoom,
        content: native(handle.clone(), 0.1),
        handle,
    })
}

fn native<'a>(handle: Handle, step: f32) -> Element<'a, Message> {
    image::viewer(handle)
        .width(Length::Fill)
        .height(Length::Fill)
        .content_fit(ContentFit::Contain)
        .min_scale(1.0)
        .max_scale(8.0)
        .scale_step(step)
        .into()
}

pub(crate) fn zoom(factor: f32) -> iced::Task<Message> {
    struct Zoom(f32);
    impl widget::Operation<Message> for Zoom {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation<Message>)) {
            operate(self);
        }
        fn custom(&mut self, _: Option<&widget::Id>, _: Rectangle, state: &mut dyn std::any::Any) {
            if let Some(gesture) = state.downcast_mut::<Gesture>() {
                gesture.pending_zoom = Some(gesture.pending_zoom.unwrap_or(1.0) * self.0);
            }
        }
    }
    widget::operate(Zoom(factor))
}

struct Photo<'a> {
    path: PathBuf,
    handle: Handle,
    initial_zoom: f32,
    content: Element<'a, Message>,
}

#[derive(Default)]
struct Gesture {
    path: PathBuf,
    fingers: Vec<(touch::Finger, Point)>,
    origin: Option<Point>,
    distance: Option<f32>,
    midpoint: Option<Point>,
    pinching: bool,
    pending_zoom: Option<f32>,
    wheel_travel: f32,
    wheel_at: Option<std::time::Instant>,
}

impl Photo<'_> {
    fn scale(&self, tree: &widget::Tree, renderer: &iced::Renderer, bounds: Rectangle) -> f32 {
        let state = tree.children[0]
            .state
            .downcast_ref::<image::viewer::State>();
        let scaled = image::viewer::scaled_image_size(
            renderer,
            &self.handle,
            state,
            bounds.size(),
            ContentFit::Contain,
        );
        let raw = image::viewer::scaled_image_size(
            renderer,
            &self.handle,
            &image::viewer::State::new(),
            bounds.size(),
            ContentFit::Contain,
        );
        if raw.width > 0.0 {
            scaled.width / raw.width
        } else {
            1.0
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn forward(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
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

    #[allow(clippy::too_many_arguments)]
    fn zoom_at(
        &mut self,
        factor: f32,
        at: Point,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if !factor.is_finite() || factor <= 0.0 || (factor - 1.0).abs() < 0.0001 {
            return;
        }
        self.content = native(
            self.handle.clone(),
            if factor > 1.0 {
                factor - 1.0
            } else {
                1.0 / factor - 1.0
            },
        );
        self.forward(
            tree,
            &Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines {
                    x: 0.0,
                    y: if factor > 1.0 { 1.0 } else { -1.0 },
                },
            }),
            layout,
            mouse::Cursor::Available(at),
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
}

impl Widget<Message, iced::Theme, iced::Renderer> for Photo<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<Gesture>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(Gesture {
            path: self.path.clone(),
            pending_zoom: Some(self.initial_zoom),
            ..Default::default()
        })
    }
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut widget::Tree) {
        if tree.state.downcast_ref::<Gesture>().path != self.path {
            tree.state = self.state();
            tree.children = self.children();
        } else {
            tree.diff_children(std::slice::from_ref(&self.content));
        }
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }
    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        _: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.custom(None, layout.bounds(), tree.state.downcast_mut::<Gesture>());
    }
    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let pending = tree.state.downcast_mut::<Gesture>().pending_zoom.take();
        if let Some(factor) = pending {
            self.zoom_at(
                factor,
                bounds.center(),
                tree,
                layout,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        let scale = self.scale(tree, renderer, bounds);
        let state = tree.state.downcast_mut::<Gesture>();
        let mut zoom = None;
        let mut mapped = None;
        let mut pan = None;
        let mut point = cursor.position().unwrap_or(bounds.center());
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let (x, y, pixels) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (*x, *y, false),
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y, true),
                };
                if x.abs() > y.abs() && scale <= 1.01 {
                    let now = std::time::Instant::now();
                    if state
                        .wheel_at
                        .is_none_or(|at| now.duration_since(at).as_millis() > 250)
                    {
                        state.wheel_travel = 0.0;
                    }
                    state.wheel_at = Some(now);
                    state.wheel_travel += x * if pixels { 1.0 } else { 24.0 };
                    if state.wheel_travel.abs() >= 100.0 {
                        shell.publish(Message::ImageStep(if state.wheel_travel > 0.0 {
                            -1
                        } else {
                            1
                        }));
                        state.wheel_travel = 0.0;
                    }
                } else if y != 0.0 {
                    zoom = Some(
                        (y * if pixels { 0.008 } else { 0.14 })
                            .exp()
                            .clamp(0.7, 1.4),
                    );
                }
            }
            Event::Touch(touch::Event::FingerPressed { id, position })
                if bounds.contains(*position) =>
            {
                point = *position;
                if state.fingers.is_empty() {
                    state.origin = Some(*position);
                    state.pinching = false;
                }
                state.fingers.push((*id, *position));
                if state.fingers.len() == 1 {
                    mapped = Some(mouse::Event::ButtonPressed(mouse::Button::Left));
                } else {
                    state.pinching = true;
                    state.distance = Some(state.fingers[0].1.distance(state.fingers[1].1));
                    state.midpoint = Some(Point::new(
                        (state.fingers[0].1.x + state.fingers[1].1.x) / 2.0,
                        (state.fingers[0].1.y + state.fingers[1].1.y) / 2.0,
                    ));
                    mapped = Some(mouse::Event::ButtonReleased(mouse::Button::Left));
                }
            }
            Event::Touch(touch::Event::FingerMoved { id, position }) => {
                point = *position;
                if let Some((_, p)) = state.fingers.iter_mut().find(|(f, _)| f == id) {
                    *p = *position;
                }
                if state.fingers.len() >= 2 {
                    let a = state.fingers[0].1;
                    let b = state.fingers[1].1;
                    point = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                    let midpoint = point;
                    point = state.midpoint.replace(midpoint).unwrap_or(midpoint);
                    if point != midpoint {
                        pan = Some((point, midpoint));
                    }
                    let distance = a.distance(b);
                    if let Some(old) = state.distance.replace(distance) {
                        if old > 1.0 && distance > 1.0 {
                            zoom = Some(distance / old);
                        }
                    }
                } else if !state.pinching {
                    mapped = Some(mouse::Event::CursorMoved {
                        position: *position,
                    });
                }
            }
            Event::Touch(touch::Event::FingerLifted { id, position }) => {
                point = *position;
                let owned = state.fingers.iter().any(|(f, _)| f == id);
                if owned && !state.pinching && scale <= 1.01 {
                    if let Some(origin) = state.origin {
                        let delta = *position - origin;
                        if delta.x.abs() > 80.0 && delta.x.abs() > delta.y.abs() * 1.5 {
                            shell.publish(Message::ImageStep(if delta.x < 0.0 { 1 } else { -1 }));
                        }
                    }
                }
                state.fingers.retain(|(f, _)| f != id);
                if state.fingers.len() == 1 && state.pinching {
                    point = state.fingers[0].1;
                    state.origin = None;
                    state.distance = None;
                    state.midpoint = None;
                    state.pinching = false;
                    mapped = Some(mouse::Event::ButtonPressed(mouse::Button::Left));
                } else {
                    if state.fingers.is_empty() {
                        state.origin = None;
                        state.distance = None;
                        state.midpoint = None;
                        state.pinching = false;
                    }
                    mapped = Some(mouse::Event::ButtonReleased(mouse::Button::Left));
                }
            }
            Event::Touch(touch::Event::FingerLost { .. })
            | Event::Window(iced::window::Event::Unfocused) => {
                state.fingers.clear();
                state.origin = None;
                state.distance = None;
                state.midpoint = None;
                state.pinching = false;
                mapped = Some(mouse::Event::ButtonReleased(mouse::Button::Left));
            }
            _ => {
                self.forward(
                    tree, event, layout, cursor, renderer, clipboard, shell, viewport,
                );
                return;
            }
        }
        if let Some(factor) = zoom {
            self.zoom_at(
                factor, point, tree, layout, renderer, clipboard, shell, viewport,
            );
        }
        if let Some((from, to)) = pan {
            for (event, cursor) in [
                (mouse::Event::ButtonPressed(mouse::Button::Left), from),
                (mouse::Event::CursorMoved { position: to }, to),
                (mouse::Event::ButtonReleased(mouse::Button::Left), to),
            ] {
                self.forward(
                    tree,
                    &Event::Mouse(event),
                    layout,
                    mouse::Cursor::Available(cursor),
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
        }
        if let Some(event) = mapped {
            self.forward(
                tree,
                &Event::Mouse(event),
                layout,
                mouse::Cursor::Available(point),
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        shell.capture_event();
    }
    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
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
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced_runtime::user_interface::{Cache, UserInterface};

    struct Harness {
        renderer: iced::Renderer,
        cache: Cache,
        path: PathBuf,
        messages: Vec<Message>,
    }
    impl Harness {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "cursus-anchor-{}-{:?}.png",
                std::process::id(),
                std::thread::current().id()
            ));
            ::image::RgbaImage::from_fn(400, 300, |x, y| {
                ::image::Rgba([(x * 255 / 400) as u8, (y * 255 / 300) as u8, 100, 255])
            })
            .save(&path)
            .unwrap();
            Self {
                renderer: iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
                    iced::Font::DEFAULT,
                    iced::Pixels(16.0),
                    Some("tiny-skia"),
                ))
                .unwrap(),
                cache: Cache::default(),
                path,
                messages: vec![],
            }
        }
        fn frame(&mut self, events: &[Event], point: Point, zoom: f32) -> Vec<u8> {
            let mut ui = UserInterface::build(
                view(&self.path, zoom),
                Size::new(400.0, 300.0),
                std::mem::take(&mut self.cache),
                &mut self.renderer,
            );
            let _ = ui.update(
                events,
                mouse::Cursor::Available(point),
                &mut self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut self.messages,
            );
            ui.draw(
                &mut self.renderer,
                &iced::Theme::Dark,
                &renderer::Style {
                    text_color: iced::Color::WHITE,
                },
                mouse::Cursor::Available(point),
            );
            self.cache = ui.into_cache();
            self.renderer
                .screenshot(Size::new(400, 300), 1.0, iced::Color::BLACK)
        }
    }
    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    fn redraw() -> Event {
        Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))
    }
    fn assert_anchor(before: &[u8], after: &[u8], x: usize, y: usize) {
        let i = (y * 400 + x) * 4;
        for channel in 0..4 {
            assert!(
                (before[i + channel] as i16 - after[i + channel] as i16).abs() <= 3,
                "image content moved away from zoom anchor at ({x}, {y})"
            );
        }
        assert_ne!(before, after, "the image must actually zoom");
    }
    #[test]
    fn wheel_zoom_preserves_content_under_the_cursor() {
        let mut h = Harness::new();
        let at = Point::new(100.0, 80.0);
        let before = h.frame(&[redraw()], at, 1.0);
        let after = h.frame(
            &[
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels { x: 0.0, y: 50.0 },
                }),
                redraw(),
            ],
            at,
            1.0,
        );
        assert_anchor(&before, &after, 100, 80);
        assert!(
            h.messages.is_empty(),
            "zoom must not rebuild the App on each motion"
        );
    }
    #[test]
    fn pinch_zoom_preserves_content_at_the_finger_midpoint() {
        let mut h = Harness::new();
        let at = Point::new(110.0, 80.0);
        let before = h.frame(&[redraw()], at, 1.0);
        let after = h.frame(
            &[
                Event::Touch(touch::Event::FingerPressed {
                    id: touch::Finger(1),
                    position: Point::new(80.0, 80.0),
                }),
                Event::Touch(touch::Event::FingerPressed {
                    id: touch::Finger(2),
                    position: Point::new(120.0, 80.0),
                }),
                Event::Touch(touch::Event::FingerMoved {
                    id: touch::Finger(2),
                    position: Point::new(140.0, 80.0),
                }),
                redraw(),
            ],
            at,
            1.0,
        );
        let old = (80 * 400 + 100) * 4;
        let new = (80 * 400 + 110) * 4;
        for channel in 0..4 {
            assert!(
                (before[old + channel] as i16 - after[new + channel] as i16).abs() <= 3,
                "pinch should carry its focal point with the moving midpoint"
            );
        }
        assert!(h.messages.is_empty());
    }
    #[test]
    fn swipes_change_photos_only_at_normal_zoom() {
        for zoom in [1.0, 2.0] {
            let mut h = Harness::new();
            let _ = h.frame(&[redraw()], Point::new(200.0, 150.0), zoom);
            let _ = h.frame(
                &[
                    Event::Touch(touch::Event::FingerPressed {
                        id: touch::Finger(1),
                        position: Point::new(300.0, 100.0),
                    }),
                    Event::Touch(touch::Event::FingerMoved {
                        id: touch::Finger(1),
                        position: Point::new(30.0, 105.0),
                    }),
                    Event::Touch(touch::Event::FingerLifted {
                        id: touch::Finger(1),
                        position: Point::new(30.0, 105.0),
                    }),
                ],
                Point::new(30.0, 105.0),
                zoom,
            );
            assert_eq!(
                h.messages
                    .iter()
                    .filter(|m| matches!(m, Message::ImageStep(1)))
                    .count(),
                if zoom == 1.0 { 1 } else { 0 }
            );
        }
    }
}
