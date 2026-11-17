//! Preference text reflows below its control row instead of becoming a narrow column.
use crate::Message;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{overlay, Clipboard, Shell};
use iced::{mouse, Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

pub(crate) struct PreferenceRow<'a> {
    // Stable child slots: icon, title, supporting text/help, control, peer status.
    children: Vec<Element<'a, Message>>,
    has_icon: bool,
    has_support: bool,
    body_size: f32,
}

impl<'a> PreferenceRow<'a> {
    pub(crate) fn row(
        icon: Option<Element<'a, Message>>,
        title: Element<'a, Message>,
        support: Option<Element<'a, Message>>,
        control: Element<'a, Message>,
        trail: Element<'a, Message>,
        body_size: f32,
    ) -> Element<'a, Message> {
        let has_icon = icon.is_some();
        let has_support = support.is_some();
        let empty = || iced::widget::Space::new().width(0).height(0).into();
        Element::new(Self {
            children: vec![
                icon.unwrap_or_else(empty),
                title,
                support.unwrap_or_else(empty),
                control,
                trail,
            ],
            has_icon,
            has_support,
            body_size,
        })
    }
}

impl Widget<Message, Theme, iced::Renderer> for PreferenceRow<'_> {
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width.min(620.0);
        let inset = if width < 360.0 { 12.0 } else { 16.0 };
        let available = (width - inset * 2.0).max(0.0);
        let icon_visible = self.has_icon && width >= 360.0 && self.body_size <= 20.0;
        let mut measure = |index: usize, width: f32| {
            self.children[index].as_widget_mut().layout(
                &mut tree.children[index],
                renderer,
                &layout::Limits::new(Size::ZERO, Size::new(width.max(0.0), f32::INFINITY)),
            )
        };
        // Fixed controls get their width before the flexible text is measured.
        let control = measure(3, available);
        let trail = measure(4, (available - control.size().width - 8.0).max(0.0));
        let icon = if icon_visible {
            measure(0, 32.0)
        } else {
            layout::Node::new(Size::ZERO)
        };
        let leading = if icon_visible { 44.0 } else { 0.0 };
        let trailing = if trail.size().width > 0.0 {
            trail.size().width + 8.0
        } else {
            0.0
        };
        let title = measure(
            1,
            available - leading - trailing - control.size().width - 12.0,
        );
        let height = title
            .size()
            .height
            .max(control.size().height)
            .max(icon.size().height);
        let support = if self.has_support {
            measure(2, available - leading)
        } else {
            layout::Node::new(Size::ZERO)
        };
        let support_height = if self.has_support {
            support.size().height + 4.0
        } else {
            0.0
        };
        let pad = if self.has_support { 12.0 } else { 8.0 };
        let center = |h: f32| pad + (height - h) / 2.0;
        let icon_y = center(icon.size().height);
        let title_y = center(title.size().height);
        let control_y = center(control.size().height);
        let trail_y = center(trail.size().height);
        let control_x = width - inset - trailing - control.size().width;
        let trail_x = width - inset - trail.size().width;
        let nodes = vec![
            icon.move_to(Point::new(inset, icon_y)),
            title.move_to(Point::new(inset + leading, title_y)),
            support.move_to(Point::new(inset + leading, pad + height + 4.0)),
            control.move_to(Point::new(control_x, control_y)),
            trail.move_to(Point::new(trail_x, trail_y)),
        ];
        layout::Node::with_children(
            limits.resolve(
                Length::Fill,
                Length::Shrink,
                Size::new(width, height + support_height + pad * 2.0),
            ),
            nodes,
        )
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
        for ((child, state), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            if layout.bounds().width <= 0.0 || layout.bounds().height <= 0.0 {
                continue;
            }
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
            if layout.bounds().width <= 0.0 || layout.bounds().height <= 0.0 {
                continue;
            }
            child.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
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
            .filter(|(_, l)| l.bounds().width > 0.0 && l.bounds().height > 0.0)
            .map(|((c, s), l)| {
                c.as_widget()
                    .mouse_interaction(s, l, cursor, viewport, renderer)
            })
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
            if layout.bounds().width > 0.0 && layout.bounds().height > 0.0 {
                child
                    .as_widget_mut()
                    .operate(state, layout, renderer, operation);
            }
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
        let overlays = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .filter(|(_, layout)| layout.bounds().width > 0.0 && layout.bounds().height > 0.0)
            .filter_map(|((child, state), layout)| {
                child
                    .as_widget_mut()
                    .overlay(state, layout, renderer, viewport, translation)
            })
            .collect::<Vec<_>>();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}
