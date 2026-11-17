//! Align pointer event positions with iced's translated scroll content coordinates.
use crate::Message;
use iced::advanced::{layout, overlay, renderer, widget, Clipboard, Layout, Shell, Widget};
use iced::{mouse, Element, Event, Length, Rectangle, Size, Vector};

pub(crate) fn content<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(TouchContent {
        content: content.into(),
    })
}

struct TouchContent<'a> {
    content: Element<'a, Message>,
}

impl Widget<Message, iced::Theme, iced::Renderer> for TouchContent<'_> {
    // Transparent state delegation preserves scroll offsets and focus.
    fn tag(&self) -> widget::tree::Tag {
        self.content.as_widget().tag()
    }
    fn state(&self) -> widget::tree::State {
        self.content.as_widget().state()
    }
    fn children(&self) -> Vec<widget::Tree> {
        self.content.as_widget().children()
    }
    fn diff(&self, tree: &mut widget::Tree) {
        self.content.as_widget().diff(tree);
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
        self.content.as_widget_mut().layout(tree, renderer, limits)
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
            .operate(tree, layout, renderer, operation);
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
        let shift = viewport.position() - layout.bounds().position();
        let event = match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Event::Mouse(mouse::Event::CursorMoved {
                    position: *position + shift,
                })
            }
            Event::Touch(touch) => Event::Touch(match *touch {
                iced::touch::Event::FingerPressed { id, position } => {
                    iced::touch::Event::FingerPressed {
                        id,
                        position: position + shift,
                    }
                }
                iced::touch::Event::FingerMoved { id, position } => {
                    iced::touch::Event::FingerMoved {
                        id,
                        position: position + shift,
                    }
                }
                iced::touch::Event::FingerLifted { id, position } => {
                    iced::touch::Event::FingerLifted {
                        id,
                        position: position + shift,
                    }
                }
                iced::touch::Event::FingerLost { id, position } => iced::touch::Event::FingerLost {
                    id,
                    position: position + shift,
                },
            }),
            other => other.clone(),
        };
        self.content.as_widget_mut().update(
            tree, &event, layout, cursor, renderer, clipboard, shell, viewport,
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::{operation::Scrollable, Operation};
    use iced::widget::{column, container, scrollable, Space};
    use iced::{touch, Point};
    use iced_runtime::user_interface::{Cache, UserInterface};

    #[test]
    fn holds_work_after_scrolling_and_in_end_anchored_history() {
        struct SetOffset;
        impl Operation for SetOffset {
            fn traverse(&mut self, op: &mut dyn FnMut(&mut dyn Operation)) {
                op(self);
            }
            fn scrollable(
                &mut self,
                _: Option<&iced::widget::Id>,
                _: Rectangle,
                _: Rectangle,
                _: Vector,
                state: &mut dyn Scrollable,
            ) {
                state.scroll_to(iced::widget::scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(950.0),
                });
            }
        }
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        for native_touch in [false, true] {
            for end in [false, true] {
                for drag in [false, true] {
                    let mut cache = Cache::default();
                    let point = Point::new(40.0, if end { 60.0 } else { 80.0 });
                    let finger = touch::Finger(17);
                    let press = if native_touch {
                        Event::Touch(touch::Event::FingerPressed {
                            id: finger,
                            position: point,
                        })
                    } else {
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                    };
                    let mut events = vec![press];
                    let movement = |position| {
                        if native_touch {
                            Event::Touch(touch::Event::FingerMoved {
                                id: finger,
                                position,
                            })
                        } else {
                            Event::Mouse(mouse::Event::CursorMoved { position })
                        }
                    };
                    if drag {
                        for step in 1..=10 {
                            events.push(movement(point + Vector::new(0.0, -6.0 * step as f32)));
                        }
                    } else {
                        events.push(movement(point + Vector::new(0.0, 1.0)));
                    }
                    events.push(Event::Window(iced::window::Event::RedrawRequested(
                        std::time::Instant::now()
                            + std::time::Duration::from_millis(crate::LONG_PRESS_MS + 1),
                    )));
                    let release = if native_touch {
                        Event::Touch(touch::Event::FingerLifted {
                            id: finger,
                            position: if drag {
                                point + Vector::new(0.0, -60.0)
                            } else {
                                point + Vector::new(0.0, 1.0)
                            },
                        })
                    } else {
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                    };
                    events.push(release);
                    let mut messages = vec![];
                    for (index, event) in events.into_iter().enumerate() {
                        let target = crate::hover::track_msg(
                            container(Space::new()).width(280).height(72),
                            51,
                            &crate::hover::HoverSlots::default(),
                            Message::MsgBubbleReleased(51),
                            Message::OpenMessageMenu(51),
                        );
                        let rows =
                            column![Space::new().height(1000), target, Space::new().height(200)];
                        let scroller = scrollable(content(rows)).height(300).direction(
                            iced::widget::scrollable::Direction::Vertical(
                                iced::widget::scrollable::Scrollbar::new().anchor(if end {
                                    iced::widget::scrollable::Anchor::End
                                } else {
                                    iced::widget::scrollable::Anchor::Start
                                }),
                            ),
                        );
                        let body = crate::kinetic::track("hold-test", scroller);
                        let mut ui = UserInterface::build(
                            body,
                            Size::new(300.0, 300.0),
                            cache,
                            &mut renderer,
                        );
                        if index == 0 && !end {
                            ui.operate(&renderer, &mut SetOffset);
                        }
                        let event = if matches!(event, Event::Window(iced::window::Event::RedrawRequested(_))) {
                        Event::Window(iced::window::Event::RedrawRequested(std::time::Instant::now() + std::time::Duration::from_millis(crate::LONG_PRESS_MS + 1)))
                    } else { event };
                    let pointer = match &event {
                            Event::Mouse(mouse::Event::CursorMoved { position }) => *position,
                            _ => point,
                        };
                        let _ = ui.update(
                            &[event],
                            if native_touch {
                                mouse::Cursor::Unavailable
                            } else {
                                mouse::Cursor::Available(pointer)
                            },
                            &mut renderer,
                            &mut iced::advanced::clipboard::Null,
                            &mut messages,
                        );
                        cache = ui.into_cache();
                    }
                    assert_eq!(
                        messages
                            .iter()
                            .filter(|m| matches!(m, Message::OpenMessageMenu(51)))
                            .count(),
                        if drag { 0 } else { 1 },
                        "end anchored={end}, drag={drag}"
                    );
                }
            }
        }
    }
}
