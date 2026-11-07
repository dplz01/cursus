//! Bubble body column whose last child stretches to the widest sibling.
//!
//! A short reply under a long quote used to keep its time + receipt right
//! after the text, in the middle of the bubble. The stamped text block must
//! be as wide as the bubble (so the stamp reaches the right edge) without
//! making the bubble any wider. iced's `Column` can't express that: a `Fill`
//! child is either laid out at the siblings' width (and a long text wraps at
//! the quote's width) or takes all the available width.
//!
//! [`BubbleBody`] lays every child out at its natural width, then lays the
//! last child out again at exactly the widest width.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{overlay, Clipboard, Shell};
use iced::{mouse, Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

use crate::Message;

pub struct BubbleBody<'a> {
    children: Vec<Element<'a, Message>>,
    spacing: f32,
    stretch_last: bool,
}

impl<'a> BubbleBody<'a> {
    pub fn new(spacing: f32) -> Self {
        Self {
            children: Vec::new(),
            spacing,
            stretch_last: false,
        }
    }

    pub fn push(mut self, child: impl Into<Element<'a, Message>>) -> Self {
        self.children.push(child.into());
        self
    }

    /// Lay the last child out again at the width of the widest one.
    pub fn stretch_last(mut self, on: bool) -> Self {
        self.stretch_last = on;
        self
    }
}

impl<'a> From<BubbleBody<'a>> for Element<'a, Message> {
    fn from(body: BubbleBody<'a>) -> Self {
        Element::new(body)
    }
}

impl Widget<Message, Theme, iced::Renderer> for BubbleBody<'_> {
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let max = limits.max();
        let natural = layout::Limits::new(Size::ZERO, Size::new(max.width, f32::INFINITY));
        let mut nodes: Vec<layout::Node> = self
            .children
            .iter_mut()
            .zip(tree.children.iter_mut())
            .map(|(child, state)| child.as_widget_mut().layout(state, renderer, &natural))
            .collect();
        let width = nodes.iter().map(|n| n.size().width).fold(0.0, f32::max).min(max.width);
        if self.stretch_last && nodes.len() > 1 {
            let last = nodes.len() - 1;
            if nodes[last].size().width < width {
                let exact = layout::Limits::new(Size::new(width, 0.0), Size::new(width, f32::INFINITY));
                nodes[last] = self.children[last]
                    .as_widget_mut()
                    .layout(&mut tree.children[last], renderer, &exact);
            }
        }
        let mut y = 0.0;
        for (i, node) in nodes.iter_mut().enumerate() {
            if i > 0 {
                y += self.spacing;
            }
            *node = node.clone().move_to(Point::new(0.0, y));
            y += node.size().height;
        }
        let size = limits.resolve(Length::Shrink, Length::Shrink, Size::new(width, y));
        layout::Node::with_children(size, nodes)
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
        for ((child, state), layout) in self.children.iter().zip(&tree.children).zip(layout.children()) {
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
        for ((child, state), layout) in self
            .children
            .iter_mut()
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
        self.children
            .iter()
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
        for ((child, state), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().operate(state, layout, renderer, operation);
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
        overlay::from_children(&mut self.children, tree, layout, renderer, viewport, translation)
    }
}
