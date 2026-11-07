//! Make widgets that only listen for mouse clicks also receive touch.
//!
//! iced's `text_editor` focuses on mouse button events; on phones the
//! compositor delivers touch. This wrapper rewrites finger press/move/lift
//! into the equivalent mouse events (cursor is already at the touch point).
//!
//! [`keep_focus_zone`] + [`touch_as_mouse_keeping_focus`] (the only entry point): iced editors drop
//! focus on any press outside them, which hides the on-screen keyboard. In a
//! chat, tapping a bubble, scrolling the history or pressing the composer's
//! buttons must keep it up; only presses outside the zone (the header, other
//! panes) still unfocus.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::mouse;
use iced::touch;
use iced::{Element, Event, Length, Rectangle, Size, Vector};

/// Touch → mouse (see the module docs); presses inside the [`keep_focus_zone`] and
/// outside this widget never reach it (so it keeps focus).
pub fn touch_as_mouse_keeping_focus<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(TouchAsMouse {
        content: content.into(),
        keep_focus: true,
    })
}

/// Area (in window coordinates) where presses don't unfocus the composer.
/// Written by [`KeepFocusZone`] on every event — it wraps the composer, so
/// it sees each event before the composer does.
static KEEP_ZONE: std::sync::Mutex<Option<Rectangle>> = std::sync::Mutex::new(None);

fn set_keep_zone(zone: Option<Rectangle>) {
    if let Ok(mut z) = KEEP_ZONE.lock() {
        *z = zone;
    }
}

fn in_keep_zone(p: iced::Point) -> bool {
    KEEP_ZONE
        .lock()
        .ok()
        .and_then(|z| *z)
        .is_some_and(|z| z.contains(p))
}

/// Should a press at `at` reach a keep-focus widget with `bounds`?
fn press_reaches(bounds: Rectangle, at: Option<iced::Point>) -> bool {
    match at {
        Some(p) => bounds.contains(p) || !in_keep_zone(p),
        None => true,
    }
}

/// Mark `content` (chat history + composer bar) as the keep-focus zone.
pub fn keep_focus_zone<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(KeepFocusZone {
        content: content.into(),
    })
}

pub struct KeepFocusZone<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for KeepFocusZone<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
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
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
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
            .operate(tree, layout, renderer, operation);
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
        set_keep_zone(Some(layout.bounds()));
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
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
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

pub struct TouchAsMouse<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    keep_focus: bool,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for TouchAsMouse<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
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
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
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
            .operate(tree, layout, renderer, operation);
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
        let mapped = match event {
            Event::Touch(touch::Event::FingerPressed { .. }) => {
                Some(Event::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Left,
                )))
            }
            Event::Touch(touch::Event::FingerLifted { .. })
            | Event::Touch(touch::Event::FingerLost { .. }) => Some(Event::Mouse(
                mouse::Event::ButtonReleased(mouse::Button::Left),
            )),
            Event::Touch(touch::Event::FingerMoved { position, .. }) => {
                Some(Event::Mouse(mouse::Event::CursorMoved {
                    position: *position,
                }))
            }
            _ => None,
        };

        if self.keep_focus {
            let press_at = match event {
                Event::Touch(touch::Event::FingerPressed { position, .. }) => Some(*position),
                Event::Mouse(mouse::Event::ButtonPressed(_)) => cursor.position(),
                _ => None,
            };
            let is_press = matches!(
                event,
                Event::Touch(touch::Event::FingerPressed { .. })
                    | Event::Mouse(mouse::Event::ButtonPressed(_))
            );
            if is_press && !press_reaches(layout.bounds(), press_at) {
                return;
            }
        }
        let event = mapped.as_ref().unwrap_or(event);
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
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
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Point;

    #[test]
    fn presses_in_the_chat_keep_the_composer_focused() {
        let composer = Rectangle::new(Point::new(50.0, 760.0), Size::new(300.0, 40.0));
        // Chat body (history + composer bar), below a 60px header.
        set_keep_zone(Some(Rectangle::new(Point::new(0.0, 60.0), Size::new(400.0, 760.0))));
        assert!(press_reaches(composer, Some(Point::new(100.0, 780.0))), "the field itself");
        assert!(!press_reaches(composer, Some(Point::new(100.0, 300.0))), "a bubble");
        assert!(!press_reaches(composer, Some(Point::new(20.0, 780.0))), "the attach button");
        assert!(press_reaches(composer, Some(Point::new(100.0, 30.0))), "the header");
        set_keep_zone(None);
        assert!(press_reaches(composer, Some(Point::new(100.0, 300.0))), "no zone: normal");
    }
}
