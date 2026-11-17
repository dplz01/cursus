//! Suspend an opaque page without rebuilding it; retain only its widget state for Back.
use crate::Message;
use iced::advanced::{layout, overlay, renderer, widget, Clipboard, Layout, Shell, Widget};
use iced::{mouse, Element, Event, Length, Rectangle, Size, Vector};

pub(crate) fn view<'a>(content: Option<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Retained { content })
}
struct Retained<'a> {
    content: Option<Element<'a, Message>>,
}
impl Widget<Message, iced::Theme, iced::Renderer> for Retained<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<Retained<'static>>()
    }
    fn children(&self) -> Vec<widget::Tree> {
        self.content.iter().map(widget::Tree::new).collect()
    }
    fn diff(&self, tree: &mut widget::Tree) {
        if let Some(content) = &self.content {
            tree.diff_children(std::slice::from_ref(content));
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
        if let Some(content) = &mut self.content {
            let child = content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, limits);
            layout::Node::with_children(child.size(), vec![child])
        } else {
            layout::Node::new(limits.resolve(Length::Fill, Length::Fill, Size::ZERO))
        }
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if let Some(content) = &mut self.content {
            content.as_widget_mut().operate(
                &mut tree.children[0],
                layout.children().next().unwrap(),
                renderer,
                operation,
            );
        }
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
        if let Some(content) = &mut self.content {
            content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout.children().next().unwrap(),
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
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
        if let Some(content) = &self.content {
            content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout.children().next().unwrap(),
                cursor,
                viewport,
            );
        }
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
            .as_ref()
            .map_or(mouse::Interaction::None, |content| {
                content.as_widget().mouse_interaction(
                    &tree.children[0],
                    layout.children().next().unwrap(),
                    cursor,
                    viewport,
                    renderer,
                )
            })
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_mut()?.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::{operation::Scrollable, Operation};
    use iced::widget::{column, scrollable, Space};
    use iced_runtime::user_interface::{Cache, UserInterface};
    struct Probe {
        set: bool,
        offset: Option<f32>,
    }
    impl Operation for Probe {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn scrollable(
            &mut self,
            _: Option<&widget::Id>,
            _: Rectangle,
            _: Rectangle,
            translation: Vector,
            state: &mut dyn Scrollable,
        ) {
            self.offset = Some(translation.y);
            if self.set {
                state.scroll_to(iced::widget::scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(250.0),
                });
            }
        }
    }
    #[test]
    fn suspended_pages_are_not_traversed_and_restore_their_scroll_state() {
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let mut cache = Cache::default();
        for (index, visible) in [true, false, true].into_iter().enumerate() {
            let page = visible.then(|| {
                scrollable(column![Space::new().height(1500)])
                    .height(Length::Fill)
                    .into()
            });
            let mut ui =
                UserInterface::build(view(page), Size::new(400.0, 300.0), cache, &mut renderer);
            let mut probe = Probe {
                set: index == 0,
                offset: None,
            };
            ui.operate(&renderer, &mut probe);
            if index == 1 {
                assert!(probe.offset.is_none());
            }
            if index == 2 {
                assert_eq!(probe.offset, Some(250.0));
            }
            cache = ui.into_cache();
        }
    }
}
