//! `[text][badge]` where the badge always keeps its size.
//!
//! iced's `Row` lays out children in order, so a long, wrapping text before a
//! fixed-size icon takes all the width and the icon is squeezed to zero when
//! the window narrows. [`TrailRow`] measures the badge first, gives the text
//! the rest, and places the badge right after the text (vertically centred on
//! the text's first line block).

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::{mouse, Element, Event, Length, Rectangle, Size, Theme, Vector};

use crate::Message;

pub struct TrailRow<'a> {
    content: Element<'a, Message>,
    badge: Element<'a, Message>,
    spacing: f32,
}

pub fn trail_row<'a>(
    content: impl Into<Element<'a, Message>>,
    badge: impl Into<Element<'a, Message>>,
    spacing: f32,
) -> Element<'a, Message> {
    Element::new(TrailRow {
        content: content.into(),
        badge: badge.into(),
        spacing,
    })
}

struct TrailRowTag;

impl Widget<Message, Theme, iced::Renderer> for TrailRow<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<TrailRowTag>()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content), Tree::new(&self.badge)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content, &self.badge]);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let max = limits.max();
        // Badge first, unconstrained by the text.
        let badge = self.badge.as_widget_mut().layout(
            &mut tree.children[1],
            renderer,
            &layout::Limits::new(Size::ZERO, max),
        );
        let bsize = badge.size();
        let text_max = (max.width - bsize.width - self.spacing).max(0.0);
        let content = self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::ZERO, Size::new(text_max, max.height)),
        );
        let csize = content.size();
        let height = csize.height.max(bsize.height);
        let content = content.move_to(iced::Point::new(0.0, (height - csize.height) / 2.0));
        let badge = badge.move_to(iced::Point::new(
            csize.width + self.spacing,
            (height - bsize.height) / 2.0,
        ));
        let size = limits.resolve(
            Length::Shrink,
            Length::Shrink,
            Size::new(csize.width + self.spacing + bsize.width, height),
        );
        layout::Node::with_children(size, vec![content, badge])
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
        for ((child, state), layout) in [&self.content, &self.badge]
            .into_iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            child
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
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
        for ((child, state), layout) in [&mut self.content, &mut self.badge]
            .into_iter()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child
                .as_widget_mut()
                .update(state, event, layout, cursor, renderer, clipboard, shell, viewport);
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
        [&self.content, &self.badge]
            .into_iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((c, s), l)| c.as_widget().mouse_interaction(s, l, cursor, viewport, renderer))
            .max()
            .unwrap_or_default()
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        for ((child, state), layout) in [&mut self.content, &mut self.badge]
            .into_iter()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().operate(state, layout, renderer, operation);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        _tree: &'b mut Tree,
        _layout: Layout<'b>,
        _renderer: &iced::Renderer,
        _viewport: &Rectangle,
        _translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, iced::Renderer>> {
        None
    }
}
