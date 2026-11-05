//! Editable text field with a native-clipboard context menu.
//!
//! [`edit_field`] wraps an iced `text_input` in an [`iced_aw::ContextMenu`]
//! offering **Select all / Cut / Copy / Paste**. Right-click opens it on
//! desktop; on touch screens a long-press opens it too.
//!
//! `iced_aw::ContextMenu` only reacts to a right mouse button, so the outer
//! [`EditMenu`] widget adds what is missing:
//!
//! - **Long-press → menu.** A finger held still for [`LONG_PRESS`] is turned
//!   into the same right-button event the context menu listens for.
//! - **Disabled states.** Just before the menu opens, the field's selection
//!   and cursor are read from the inner `text_input` state and the system
//!   clipboard is read once; Copy/Cut are greyed out with no selection and
//!   Paste when the clipboard is empty (`on_press(None)`).
//! - **Touch dismissal.** The context-menu overlay closes on a mouse release
//!   but not on a finger lift; the overlay wrapper mirrors the lift as a
//!   release so tapping an action also closes the menu.
//!
//! Actions are applied by the app through [`crate::Message::FieldMenu`]:
//! Copy/Cut write the selection to the clipboard, Cut/Paste emit the field's
//! own `on_input` message with the new value, Select all runs the
//! `select_all` widget operation on the field's id.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, tree, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::time::Instant;
use iced::widget::{button, column, container, text, text_input};
use iced::{mouse, touch, Element, Event, Length, Padding, Point, Rectangle, Size, Theme, Vector};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme;
use crate::Message;

/// Hold time before a touch press opens the menu (matches chat long-press).
pub const LONG_PRESS: Duration = Duration::from_millis(520);
/// Finger travel (logical px) that turns a press into a drag / scroll.
const MOVE_SLOP: f32 = 14.0;

/// What a context-menu entry asks the app to do.
#[derive(Debug, Clone)]
pub enum FieldMenuAction {
    SelectAll(widget::Id),
    Copy(String),
    /// Copy `copied`, then apply the field's own `on_input(new value)`.
    Cut { copied: String, then: Box<Message> },
    /// Apply the field's own `on_input(new value)` with the clipboard inserted.
    Paste(Box<Message>),
}

/// Field state captured when the menu opens.
#[derive(Debug, Default, Clone)]
struct Snapshot {
    /// Selected grapheme range (start < end), if any.
    selection: Option<(usize, usize)>,
    /// Caret grapheme index when nothing is selected.
    caret: usize,
    clipboard: Option<String>,
}

type InputState = text_input::State<<iced::Renderer as iced::advanced::text::Renderer>::Paragraph>;

/// Build an editable field with the Select all / Cut / Copy / Paste menu.
///
/// `input` must already carry `.id(id)` and `.on_input(..)`; `on_input` is the
/// same constructor so menu edits flow through the app's normal input path.
pub fn edit_field<'a>(
    input: text_input::TextInput<'a, Message>,
    id: widget::Id,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
    hover_ok: bool,
) -> Element<'a, Message> {
    let snap = Rc::new(RefCell::new(Snapshot::default()));
    let value = value.to_string();
    let value_for_snapshot = value.clone();
    let on_input = Rc::new(on_input);

    let menu = {
        let snap = Rc::clone(&snap);
        move || menu_card(&snap.borrow(), &value, &id, on_input.as_ref(), hover_ok)
    };
    let context = iced_aw::ContextMenu::new(input, menu).style(|_, _| iced_aw::style::context_menu::Style {
        // No dimming backdrop — this is a small popup, not a modal.
        background: iced::Background::Color(iced::Color::TRANSPARENT),
    });

    Element::new(EditMenu {
        content: Element::new(context),
        snap,
        value: value_for_snapshot,
        opening_finger: Rc::new(Cell::new(None)),
    })
}

fn menu_card<'a>(
    snap: &Snapshot,
    value: &str,
    id: &widget::Id,
    on_input: &dyn Fn(String) -> Message,
    hover_ok: bool,
) -> Element<'a, Message> {
    let graphemes: Vec<&str> = value.graphemes(true).collect();
    let slice = |a: usize, b: usize| graphemes[a.min(graphemes.len())..b.min(graphemes.len())].concat();

    let selected = snap.selection.map(|(a, b)| slice(a, b)).filter(|s| !s.is_empty());
    let (cut_start, cut_end) = snap.selection.unwrap_or((snap.caret, snap.caret));

    let select_all = (!value.is_empty())
        .then(|| Message::FieldMenu(FieldMenuAction::SelectAll(id.clone())));
    let copy = selected
        .clone()
        .map(|s| Message::FieldMenu(FieldMenuAction::Copy(s)));
    let cut = selected.map(|s| {
        let rest = slice(0, cut_start) + &slice(cut_end, graphemes.len());
        Message::FieldMenu(FieldMenuAction::Cut {
            copied: s,
            then: Box::new(on_input(rest)),
        })
    });
    let paste = snap.clipboard.as_ref().filter(|c| !c.is_empty()).map(|clip| {
        // Single-line field: newlines from the clipboard become spaces.
        let clip = clip.replace(['\r', '\n'], " ");
        let new = slice(0, cut_start) + &clip + &slice(cut_end, graphemes.len());
        Message::FieldMenu(FieldMenuAction::Paste(Box::new(on_input(new))))
    });

    let item = |label: &'static str, msg: Option<Message>| {
        let enabled = msg.is_some();
        button(
            text(label)
                .size(14)
                .color(if enabled { theme::t().text_primary } else { theme::t().text_muted }),
        )
        .on_press_maybe(msg)
        .padding(Padding::from([8, 14]))
        .width(Length::Fill)
        .style(theme::menu_item(hover_ok))
    };

    container(
        column![
            item("Select all", select_all),
            item("Cut", cut),
            item("Copy", copy),
            item("Paste", paste),
        ]
        .spacing(2)
        .width(160),
    )
    .padding(4)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(theme::t().bg_elevated)),
        border: iced::Border {
            color: theme::t().border,
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: iced::Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 14.0,
        },
        ..container::Style::default()
    })
    .into()
}

/// Outer widget: long-press detection + selection/clipboard snapshot.
struct EditMenu<'a> {
    content: Element<'a, Message>,
    snap: Rc<RefCell<Snapshot>>,
    /// Current field text (cursor indices are graphemes into it).
    value: String,
    /// Finger whose long-press opened the menu: its lift must not close it.
    opening_finger: Rc<Cell<Option<touch::Finger>>>,
}

#[derive(Default)]
struct PressState {
    press: Option<Press>,
}

struct Press {
    finger: touch::Finger,
    origin: Point,
    at: Instant,
}

impl EditMenu<'_> {
    /// Capture selection / caret from the inner text_input and the clipboard.
    fn snapshot(&self, tree: &Tree, clipboard: &mut dyn Clipboard) {
        let mut snap = Snapshot {
            clipboard: clipboard.read(iced::advanced::clipboard::Kind::Standard),
            ..Snapshot::default()
        };
        // EditMenu → ContextMenu (children[0] = underlay) → text_input.
        if let Some(input) = tree
            .children
            .first()
            .and_then(|ctx| ctx.children.first())
            .filter(|t| t.tag == tree::Tag::of::<InputState>())
        {
            let state = input.state.downcast_ref::<InputState>();
            let value = text_input::Value::new(&self.value);
            match state.cursor().state(&value) {
                text_input::cursor::State::Index(i) => snap.caret = i,
                text_input::cursor::State::Selection { start, end } => {
                    let (a, b) = (start.min(end), start.max(end));
                    if a < b {
                        snap.selection = Some((a, b));
                    }
                    snap.caret = b;
                }
            }
        }
        *self.snap.borrow_mut() = snap;
    }
}

impl Widget<Message, Theme, iced::Renderer> for EditMenu<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<PressState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(PressState::default())
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
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
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
        self.content
            .as_widget()
            .draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
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
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let mut long_press_at: Option<Point> = None;
        {
            let st = tree.state.downcast_mut::<PressState>();
            match event {
                Event::Touch(touch::Event::FingerPressed { id, position }) if bounds.contains(*position) => {
                    let at = Instant::now();
                    st.press = Some(Press {
                        finger: *id,
                        origin: *position,
                        at,
                    });
                    shell.request_redraw_at(at + LONG_PRESS);
                }
                Event::Touch(touch::Event::FingerMoved { id, position }) => {
                    if st.press.as_ref().is_some_and(|p| {
                        p.finger == *id && p.origin.distance(*position) > MOVE_SLOP
                    }) {
                        st.press = None;
                    }
                }
                Event::Touch(touch::Event::FingerLifted { id, .. } | touch::Event::FingerLost { id, .. }) => {
                    if st.press.as_ref().is_some_and(|p| p.finger == *id) {
                        st.press = None;
                    }
                }
                Event::Window(iced::window::Event::RedrawRequested(now)) => {
                    if let Some(p) = st.press.as_ref() {
                        if now.saturating_duration_since(p.at) >= LONG_PRESS {
                            long_press_at = Some(p.origin);
                            self.opening_finger.set(Some(p.finger));
                            st.press = None;
                        } else {
                            shell.request_redraw_at(p.at + LONG_PRESS);
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(origin) = long_press_at {
            // Same trigger iced_aw's ContextMenu listens for, at the finger.
            self.snapshot(tree, clipboard);
            let right = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                &right,
                layout,
                mouse::Cursor::Available(origin),
                renderer,
                clipboard,
                shell,
                viewport,
            );
            shell.request_redraw();
        } else if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)))
            && cursor.is_over(bounds)
        {
            self.snapshot(tree, clipboard);
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

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
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
            .map(|inner| {
                overlay::Element::new(Box::new(TouchClose {
                    inner,
                    opening_finger: Rc::clone(&self.opening_finger),
                }))
            })
    }
}

/// Mirrors a finger lift as a left-button release so the context-menu
/// overlay closes after a tap on one of its actions (it only listens to mouse
/// releases for that).
struct TouchClose<'a> {
    inner: overlay::Element<'a, Message, Theme, iced::Renderer>,
    opening_finger: Rc<Cell<Option<touch::Finger>>>,
}

impl overlay::Overlay<Message, Theme, iced::Renderer> for TouchClose<'_> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        self.inner.as_overlay_mut().layout(renderer, bounds)
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.inner.as_overlay().draw(renderer, theme, style, layout, cursor);
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn widget::Operation) {
        self.inner.as_overlay_mut().operate(layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.inner
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
        if let Event::Touch(touch::Event::FingerLifted { id, position }) = event {
            // Lifting the finger that long-pressed just finishes the gesture.
            if self.opening_finger.get() == Some(*id) {
                self.opening_finger.set(None);
                return;
            }
            let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
            self.inner.as_overlay_mut().update(
                &release,
                layout,
                mouse::Cursor::Available(*position),
                renderer,
                clipboard,
                shell,
            );
        }
    }

    fn mouse_interaction(&self, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &iced::Renderer) -> mouse::Interaction {
        self.inner.as_overlay().mouse_interaction(layout, cursor, renderer)
    }

    fn overlay<'b>(
        &'b mut self,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.inner.as_overlay_mut().overlay(layout, renderer)
    }

    fn index(&self) -> f32 {
        self.inner.as_overlay().index()
    }
}
