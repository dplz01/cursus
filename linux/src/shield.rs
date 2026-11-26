//! Floating bars (the glass header, composer, destinations bar) sit over
//! content in a `Stack`. iced lets an uncaptured event fall through to the
//! layers below, and its `opaque` only stops presses: a tap on the chat
//! header opened the profile *and*, on release, the picture under it.
//!
//! [`shield`] hands every event to its content first, then keeps the
//! clicks and touches that started on it from reaching anything below.
//! The scroll wheel still passes (scrolling the history under a bar is fine),
//! and a drag that began elsewhere is never taken over.
use std::collections::HashSet;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{overlay, Clipboard, Shell};
use iced::{mouse, touch, Element, Event, Length, Rectangle, Size, Theme, Vector};

use crate::Message;

pub(crate) fn shield<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Shield { content: content.into() })
}

struct Shield<'a> {
    content: Element<'a, Message>,
}

#[derive(Default)]
struct State {
    /// Mouse buttons pressed on the shield (their release is ours too).
    buttons: HashSet<mouse::Button>,
    /// Fingers that went down on the shield.
    fingers: HashSet<touch::Finger>,
}

impl Widget<Message, Theme, iced::Renderer> for Shield<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
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
        self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
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
        self.content.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
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
        self.content
            .as_widget_mut()
            .update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        let ours = match *event {
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                let over = cursor.is_over(bounds);
                if over {
                    state.buttons.insert(button);
                }
                over
            }
            Event::Mouse(mouse::Event::ButtonReleased(button)) => state.buttons.remove(&button),
            Event::Touch(touch::Event::FingerPressed { id, position }) => {
                let over = bounds.contains(position);
                if over {
                    state.fingers.insert(id);
                }
                over
            }
            Event::Touch(touch::Event::FingerMoved { id, .. }) => state.fingers.contains(&id),
            Event::Touch(touch::Event::FingerLifted { id, .. } | touch::Event::FingerLost { id, .. }) => {
                state.fingers.remove(&id)
            }
            _ => false,
        };
        if ours {
            shell.capture_event();
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
        self.content.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }
    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn widget::Operation) {
        self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::renderer::Headless;
    use iced::widget::{container, mouse_area, Space, Stack};
    use iced::{mouse, touch, Event, Length, Point, Size};
    use iced_runtime::user_interface::{Cache, UserInterface};

    use crate::Message;

    /// A picture (opens on release) under a floating bar (opens on press),
    /// like the chat header over the history.
    fn taps(events: Vec<Event>, cursor: Point) -> Vec<Message> {
        let size = Size::new(200.0, 200.0);
        let picture = mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
            .on_release(Message::OpenImage("picture.png".into()));
        let bar = super::shield(
            container(mouse_area(Space::new().width(Length::Fill).height(40)).on_press(Message::OpenContactProfile))
                .width(Length::Fill),
        );
        let view: iced::Element<'_, Message> =
            Stack::new().width(Length::Fill).height(Length::Fill).push(picture).push(bar).into();
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let mut ui = UserInterface::build(view, size, Cache::default(), &mut renderer);
        let mut messages = Vec::new();
        let mut clipboard = iced::advanced::clipboard::Null;
        let _ = ui.update(&events, mouse::Cursor::Available(cursor), &mut renderer, &mut clipboard, &mut messages);
        messages
    }

    fn opened_picture(messages: &[Message]) -> bool {
        messages.iter().any(|m| matches!(m, Message::OpenImage(_)))
    }

    #[test]
    fn a_click_on_the_bar_stays_on_the_bar() {
        let at = Point::new(100.0, 20.0);
        let m = taps(
            vec![
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            at,
        );
        assert!(m.iter().any(|m| matches!(m, Message::OpenContactProfile)), "the bar got it");
        assert!(!opened_picture(&m), "the picture under the bar did not");
    }

    #[test]
    fn a_tap_on_the_bar_stays_on_the_bar() {
        let at = Point::new(100.0, 20.0);
        let id = touch::Finger(1);
        let m = taps(
            vec![
                Event::Touch(touch::Event::FingerPressed { id, position: at }),
                Event::Touch(touch::Event::FingerLifted { id, position: at }),
            ],
            at,
        );
        assert!(!opened_picture(&m));
    }

    #[test]
    fn the_picture_still_opens_outside_the_bar() {
        let m = taps(
            vec![
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            Point::new(100.0, 150.0),
        );
        assert!(opened_picture(&m));
    }
}
