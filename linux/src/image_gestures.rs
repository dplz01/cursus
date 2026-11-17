//! Pinch zoom, wheel zoom and photo swipes, leaving zoomed panning to the scrollable.
use crate::Message;
use iced::advanced::{layout, overlay, renderer, widget, Clipboard, Layout, Shell, Widget};
use iced::{mouse, Element, Event, Length, Rectangle, Size, Vector};

pub(crate) fn view<'a>(
    content: impl Into<Element<'a, Message>>,
    zoom: f32,
) -> Element<'a, Message> {
    Element::new(ImageGestures {
        content: content.into(),
        zoom,
    })
}

struct ImageGestures<'a> {
    content: Element<'a, Message>,
    zoom: f32,
}

impl Widget<Message, iced::Theme, iced::Renderer> for ImageGestures<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<Gesture>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(Gesture::default())
    }
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut widget::Tree) {
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
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        let state = tree.state.downcast_mut::<Gesture>();
        let mut consumed = false;
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if cursor.is_over(layout.bounds()) =>
            {
                let y = match delta {
                    mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => {
                        *y
                    }
                };
                if y != 0.0 {
                    shell.publish(Message::ImageZoom(if y > 0.0 { 1.15 } else { 1.0 / 1.15 }));
                }
                consumed = true;
            }
            Event::Touch(iced::touch::Event::FingerPressed { id, position })
                if layout.bounds().contains(*position) =>
            {
                if state.fingers.is_empty() {
                    state.origin = Some(*position);
                    state.pinching = false;
                }
                state.fingers.push((*id, *position));
                if state.fingers.len() == 2 {
                    state.pinching = true;
                    state.distance = Some(state.fingers[0].1.distance(state.fingers[1].1));
                    consumed = true;
                }
            }
            Event::Touch(iced::touch::Event::FingerMoved { id, position }) => {
                if let Some((_, point)) = state.fingers.iter_mut().find(|(finger, _)| finger == id)
                {
                    *point = *position;
                }
                if state.fingers.len() >= 2 {
                    let distance = state.fingers[0].1.distance(state.fingers[1].1);
                    if let Some(previous) = state.distance.replace(distance) {
                        if previous > 1.0 && distance > 1.0 {
                            shell.publish(Message::ImageZoom(distance / previous));
                        }
                    }
                }
                consumed = state.pinching;
            }
            Event::Touch(iced::touch::Event::FingerLifted { id, position }) => {
                state.fingers.retain(|(finger, _)| finger != id);
                if !state.pinching && self.zoom <= 1.0 {
                    if let Some(origin) = state.origin {
                        let delta = *position - origin;
                        if delta.x.abs() > 80.0 && delta.x.abs() > delta.y.abs() * 1.5 {
                            shell.publish(Message::ImageStep(if delta.x < 0.0 { 1 } else { -1 }));
                            consumed = true;
                        }
                    }
                }
                consumed |= state.pinching;
                if state.fingers.is_empty() {
                    state.origin = None;
                    state.distance = None;
                    state.pinching = false;
                }
            }
            Event::Touch(iced::touch::Event::FingerLost { .. })
            | Event::Window(iced::window::Event::Unfocused) => {
                *state = Gesture::default();
            }
            _ => {}
        }
        if consumed {
            // Clear the scrollable's first-finger drag before taking over a pinch.
            if let Event::Touch(iced::touch::Event::FingerPressed { id, position }) = event {
                self.content.as_widget_mut().update(
                    &mut tree.children[0],
                    &Event::Touch(iced::touch::Event::FingerLost {
                        id: *id,
                        position: *position,
                    }),
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            shell.capture_event();
            return;
        }
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
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[derive(Default)]
struct Gesture {
    fingers: Vec<(iced::touch::Finger, iced::Point)>,
    origin: Option<iced::Point>,
    distance: Option<f32>,
    pinching: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced::widget::{container, Space};
    use iced::{touch, Point};
    use iced_runtime::user_interface::{Cache, UserInterface};

    #[test]
    fn pinch_zoom_swipe_and_zoomed_pan_are_distinct() {
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        for zoom in [1.0, 2.0] {
            let mut cache = Cache::default();
            let mut messages = vec![];
            let first = touch::Finger(1);
            let second = touch::Finger(2);
            for event in [
                touch::Event::FingerPressed {
                    id: first,
                    position: Point::new(180.0, 100.0),
                },
                touch::Event::FingerMoved {
                    id: first,
                    position: Point::new(30.0, 105.0),
                },
                touch::Event::FingerLifted {
                    id: first,
                    position: Point::new(30.0, 105.0),
                },
                touch::Event::FingerPressed {
                    id: first,
                    position: Point::new(100.0, 100.0),
                },
                touch::Event::FingerPressed {
                    id: second,
                    position: Point::new(200.0, 100.0),
                },
                touch::Event::FingerMoved {
                    id: second,
                    position: Point::new(250.0, 100.0),
                },
                touch::Event::FingerLifted {
                    id: second,
                    position: Point::new(250.0, 100.0),
                },
                touch::Event::FingerLifted {
                    id: first,
                    position: Point::new(100.0, 100.0),
                },
            ] {
                let body = view(container(Space::new()).width(300).height(300), zoom);
                let mut ui =
                    UserInterface::build(body, Size::new(300.0, 300.0), cache, &mut renderer);
                let _ = ui.update(
                    &[Event::Touch(event)],
                    mouse::Cursor::Unavailable,
                    &mut renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut messages,
                );
                cache = ui.into_cache();
            }
            assert_eq!(
                messages
                    .iter()
                    .filter(|m| matches!(m, Message::ImageStep(1)))
                    .count(),
                if zoom == 1.0 { 1 } else { 0 }
            );
            assert_eq!(
                messages
                    .iter()
                    .filter(|m| matches!(m, Message::ImageZoom(f) if (*f - 1.5).abs() < 0.01))
                    .count(),
                1
            );
        }
    }
}
