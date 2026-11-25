//! Reports a child's laid-out height whenever it changes. The floating
//! composer sits over the chat history, which reserves exactly this much
//! room at its end, so the newest message never hides under the field.
use crate::Message;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, tree, Tree, Widget};
use iced::advanced::{overlay, Clipboard, Shell};
use iced::{mouse, Element, Event, Length, Rectangle, Size, Theme, Vector};

pub(crate) struct HeightProbe<'a> {
    content: Element<'a, Message>,
    on_change: fn(f32) -> Message,
}

pub(crate) fn height_probe<'a>(
    content: impl Into<Element<'a, Message>>,
    on_change: fn(f32) -> Message,
) -> Element<'a, Message> {
    Element::new(HeightProbe { content: content.into(), on_change })
}

/// Last height reported, so only real changes publish a message.
#[derive(Default)]
struct Reported(Option<f32>);

impl Widget<Message, Theme, iced::Renderer> for HeightProbe<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Reported>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(Reported::default())
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
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
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
            self.content
                .as_widget()
                .draw(&tree.children[0], renderer, theme, style, child, cursor, viewport);
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
        if let Some(child) = layout.children().next() {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                child,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        let height = layout.bounds().height;
        let reported = tree.state.downcast_mut::<Reported>();
        if reported.0.is_none_or(|h| (h - height).abs() > 0.5) {
            reported.0 = Some(height);
            shell.publish((self.on_change)(height));
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
            self.content
                .as_widget()
                .mouse_interaction(&tree.children[0], child, cursor, viewport, renderer)
        })
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if let Some(child) = layout.children().next() {
            self.content
                .as_widget_mut()
                .operate(&mut tree.children[0], child, renderer, operation);
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
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            child,
            renderer,
            viewport,
            translation,
        )
    }
}
