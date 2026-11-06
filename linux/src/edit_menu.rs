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

use std::cell::RefCell;
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

/// Composer (multi-line `text_editor`) menu actions — the app owns the
/// editor content, so it applies them directly.
#[derive(Debug, Clone)]
pub enum ComposerMenuAction {
    /// The menu just opened; `by_touch` = long-press (restore the selection
    /// the finger-down cleared).
    Opened { by_touch: bool },
    SelectAll,
    Cut,
    Copy,
    Paste(String),
}

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

thread_local! {
    /// Snapshot per field (keyed by widget id), written when the menu opens.
    /// It must outlive the view: the app rebuilds `view()` after nearly every
    /// event, so anything stored in the widget value itself is gone by the time
    /// the menu's contents are built.
    static SNAPSHOTS: RefCell<std::collections::HashMap<String, Snapshot>> =
        RefCell::new(std::collections::HashMap::new());
}

fn snapshot_key(id: &widget::Id) -> String {
    format!("{id:?}")
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
    let key = snapshot_key(&id);
    let value = value.to_string();
    let value_for_snapshot = value.clone();
    let on_input = Rc::new(on_input);

    let menu = {
        let key = key.clone();
        move || {
            let snap = SNAPSHOTS.with(|m| m.borrow().get(&key).cloned().unwrap_or_default());
            menu_card(&snap, &value, &id, on_input.as_ref(), hover_ok)
        }
    };
    let context = iced_aw::ContextMenu::new(input, menu).style(|_, _| iced_aw::style::context_menu::Style {
        // No dimming backdrop — this is a small popup, not a modal.
        background: iced::Background::Color(iced::Color::TRANSPARENT),
    });

    Element::new(EditMenu {
        content: Element::new(context),
        key,
        value: value_for_snapshot,
        on_open: None,
    })
}

/// Composer key in [`SNAPSHOTS`].
const COMPOSER_KEY: &str = "composer";

/// Wrap the chat composer (`text_editor`) with the same menu. `has_selection`
/// and `is_empty` come from the app's editor content at view time.
pub fn composer_menu<'a>(
    editor: impl Into<Element<'a, Message>>,
    has_selection: bool,
    is_empty: bool,
    hover_ok: bool,
) -> Element<'a, Message> {
    let menu = move || {
        let snap = SNAPSHOTS.with(|m| m.borrow().get(COMPOSER_KEY).cloned().unwrap_or_default());
        let act = |a: ComposerMenuAction| Some(Message::ComposerMenu(a));
        menu_items(
            [
                ("Select all", if is_empty { None } else { act(ComposerMenuAction::SelectAll) }),
                ("Cut", if has_selection { act(ComposerMenuAction::Cut) } else { None }),
                ("Copy", if has_selection { act(ComposerMenuAction::Copy) } else { None }),
                (
                    "Paste",
                    snap.clipboard
                        .filter(|c| !c.is_empty())
                        .and_then(|c| act(ComposerMenuAction::Paste(c))),
                ),
            ],
            hover_ok,
        )
    };
    let context = iced_aw::ContextMenu::new(editor, menu).style(|_, _| iced_aw::style::context_menu::Style {
        background: iced::Background::Color(iced::Color::TRANSPARENT),
    });
    Element::new(EditMenu {
        content: Element::new(context),
        key: COMPOSER_KEY.to_string(),
        value: String::new(),
        on_open: Some(|by_touch| Message::ComposerMenu(ComposerMenuAction::Opened { by_touch })),
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

    menu_items(
        [
            ("Select all", select_all),
            ("Cut", cut),
            ("Copy", copy),
            ("Paste", paste),
        ],
        hover_ok,
    )
}

/// The popup card: four items, disabled ones greyed out (`on_press(None)`).
fn menu_items<'a>(items: [(&'static str, Option<Message>); 4], hover_ok: bool) -> Element<'a, Message> {
    let mut col = column![].spacing(2).width(160);
    for (label, msg) in items {
        let enabled = msg.is_some();
        col = col.push(
            button(
                text(label)
                    .size(14)
                    .color(if enabled { theme::t().text_primary } else { theme::t().text_muted }),
            )
            .on_press_maybe(msg)
            .padding(Padding::from([8, 14]))
            .width(Length::Fill)
            .style(theme::menu_item(hover_ok)),
        );
    }
    container(col)
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
    /// Key into [`SNAPSHOTS`].
    key: String,
    /// Current field text (cursor indices are graphemes into it).
    value: String,
    /// Published when the menu opens (composer: restore selection).
    on_open: Option<fn(bool) -> Message>,
}

#[derive(Default)]
struct PressState {
    press: Option<Press>,
    /// Finger whose long-press opened the menu: its lift must not close it.
    /// Lives in the widget tree so it survives view rebuilds.
    opening_finger: Option<touch::Finger>,
    /// Same finger, for this widget: its remaining moves / lift are not passed
    /// to the text field (a held finger must not drag-change the selection).
    swallow_finger: Option<touch::Finger>,
    /// Text-input selection at finger-down: the press itself moves the caret,
    /// so this is restored when the long-press opens the menu.
    pre_selection: Option<(usize, usize)>,
}

struct Press {
    finger: touch::Finger,
    origin: Point,
    at: Instant,
}

fn input_tree(tree: &mut Tree) -> Option<&mut Tree> {
    tree.children
        .first_mut()
        .and_then(|ctx| ctx.children.first_mut())
        .filter(|t| t.tag == tree::Tag::of::<InputState>())
}

impl EditMenu<'_> {
    /// Current text-input selection (grapheme range), if any.
    fn input_selection(&self, tree: &mut Tree) -> Option<(usize, usize)> {
        let value = text_input::Value::new(&self.value);
        let input = input_tree(tree)?;
        match input.state.downcast_ref::<InputState>().cursor().state(&value) {
            text_input::cursor::State::Selection { start, end } if start != end => {
                Some((start.min(end), start.max(end)))
            }
            _ => None,
        }
    }

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
        SNAPSHOTS.with(|m| m.borrow_mut().insert(self.key.clone(), snap));
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
        let mut long_press_at: Option<(Point, touch::Finger)> = None;
        let mut pre_press = false;
        // Remaining events of the finger that opened the menu stay out of
        // the text field.
        {
            let st = tree.state.downcast_mut::<PressState>();
            if let Event::Touch(
                touch::Event::FingerMoved { id, .. }
                | touch::Event::FingerLifted { id, .. }
                | touch::Event::FingerLost { id, .. },
            ) = event
            {
                if st.swallow_finger == Some(*id) {
                    if !matches!(event, Event::Touch(touch::Event::FingerMoved { .. })) {
                        st.swallow_finger = None;
                    }
                    return;
                }
            }
        }
        {
            let st = tree.state.downcast_mut::<PressState>();
            match event {
                Event::Touch(touch::Event::FingerPressed { id, position }) if bounds.contains(*position) => {
                    let at = Instant::now();
                    pre_press = true;
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
                            long_press_at = Some((p.origin, p.finger));
                            st.opening_finger = Some(p.finger);
                            st.swallow_finger = Some(p.finger);
                            st.press = None;
                        } else {
                            shell.request_redraw_at(p.at + LONG_PRESS);
                        }
                    }
                }
                _ => {}
            }
        }

        if pre_press {
            let sel = self.input_selection(tree);
            tree.state.downcast_mut::<PressState>().pre_selection = sel;
        }

        if let Some((origin, finger)) = long_press_at {
            // End the field's own press/drag for this finger…
            let lift = Event::Touch(touch::Event::FingerLifted { id: finger, position: origin });
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                &lift,
                layout,
                mouse::Cursor::Available(origin),
                renderer,
                clipboard,
                shell,
                viewport,
            );
            // …and put back the selection the finger-down cleared.
            if let Some((a, b)) = tree.state.downcast_mut::<PressState>().pre_selection.take() {
                if let Some(input) = input_tree(tree) {
                    input.state.downcast_mut::<InputState>().select_range(a, b);
                }
            }
            if let Some(on_open) = self.on_open {
                shell.publish(on_open(true));
            }
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
            if let Some(on_open) = self.on_open {
                shell.publish(on_open(false));
            }
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
        let Tree { state, children, .. } = tree;
        let press = state.downcast_mut::<PressState>();
        self.content
            .as_widget_mut()
            .overlay(&mut children[0], layout, renderer, viewport, translation)
            .map(|inner| {
                overlay::Element::new(Box::new(TouchClose {
                    inner,
                    opening_finger: &mut press.opening_finger,
                }))
            })
    }
}

/// Mirrors a finger lift as a left-button release so the context-menu
/// overlay closes after a tap on one of its actions (it only listens to mouse
/// releases for that).
struct TouchClose<'a> {
    inner: overlay::Element<'a, Message, Theme, iced::Renderer>,
    opening_finger: &'a mut Option<touch::Finger>,
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
            if *self.opening_finger == Some(*id) {
                *self.opening_finger = None;
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

#[cfg(test)]
mod tests {
    //! Drives the real widget through iced's `UserInterface`, **rebuilding the
    //! view between events** like the app does (most events produce a message
    //! and a fresh `view()`), with a fake clipboard.
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced::keyboard;
    use iced::widget::container;
    use iced_runtime::user_interface::{Cache, UserInterface};

    struct FakeClipboard(Option<String>);
    impl Clipboard for FakeClipboard {
        fn read(&self, _: iced::advanced::clipboard::Kind) -> Option<String> {
            self.0.clone()
        }
        fn write(&mut self, _: iced::advanced::clipboard::Kind, contents: String) {
            self.0 = Some(contents);
        }
    }

    struct Harness {
        renderer: iced::Renderer,
        cache: Option<Cache>,
        value: String,
        clipboard: FakeClipboard,
        messages: Vec<Message>,
    }

    const FIELD: &str = "test-field";

    impl Harness {
        fn new(value: &str) -> Self {
            let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
                iced::Font::DEFAULT,
                iced::Pixels(16.0),
                Some("tiny-skia"),
            ))
            .expect("headless renderer");
            Self {
                renderer,
                cache: Some(Cache::default()),
                value: value.into(),
                clipboard: FakeClipboard(Some("PASTED".into())),
                messages: Vec::new(),
            }
        }

        fn view(value: &str) -> Element<'_, Message> {
            container(edit_field(
                text_input("placeholder", value)
                    .id(FIELD)
                    .on_input(Message::InviteInputChanged)
                    .width(300),
                widget::Id::new(FIELD),
                value,
                Message::InviteInputChanged,
                true,
            ))
            .padding(20)
            .into()
        }

        /// One frame: build a fresh view from cache, feed events, draw, cache.
        fn frame(&mut self, events: &[Event], at: Point) {
            let value = self.value.clone();
            let mut ui = UserInterface::build(
                Self::view(&value),
                Size::new(800.0, 600.0),
                self.cache.take().unwrap(),
                &mut self.renderer,
            );
            let _ = ui.update(
                events,
                mouse::Cursor::Available(at),
                &mut self.renderer,
                &mut self.clipboard,
                &mut self.messages,
            );
            ui.draw(
                &mut self.renderer,
                &Theme::Dark,
                &renderer::Style::default(),
                mouse::Cursor::Available(at),
            );
            self.cache = Some(ui.into_cache());
        }

        fn click(&mut self, button: mouse::Button, at: Point) {
            self.frame(&[Event::Mouse(mouse::Event::CursorMoved { position: at })], at);
            self.frame(&[Event::Mouse(mouse::Event::ButtonPressed(button))], at);
            self.frame(&[Event::Mouse(mouse::Event::ButtonReleased(button))], at);
        }

        fn select_all_keys(&mut self, at: Point) {
            let key = keyboard::Key::Character("a".into());
            self.frame(
                &[Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL))],
                at,
            );
            self.frame(
                &[Event::Keyboard(keyboard::Event::KeyPressed {
                    key: key.clone(),
                    modified_key: key.clone(),
                    physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyA),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::CTRL,
                    text: None,
                    repeat: false,
                })],
                at,
            );
            self.frame(
                &[Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::empty()))],
                at,
            );
        }

        fn take_field_actions(&mut self) -> Vec<FieldMenuAction> {
            std::mem::take(&mut self.messages)
                .into_iter()
                .filter_map(|m| match m {
                    Message::FieldMenu(a) => Some(a),
                    _ => None,
                })
                .collect()
        }
    }

    /// Menu item centres relative to where the menu opened (see `menu_card`).
    fn item(at: Point, index: usize) -> Point {
        Point::new(at.x + 40.0, at.y + 4.0 + 17.0 + index as f32 * 36.0)
    }

    const FIELD_POINT: Point = Point::new(60.0, 35.0);

    #[test]
    fn cut_copies_and_clears_selection() {
        let mut h = Harness::new("hello world");
        h.click(mouse::Button::Left, FIELD_POINT);
        h.select_all_keys(FIELD_POINT);
        h.click(mouse::Button::Right, FIELD_POINT);
        h.click(mouse::Button::Left, item(FIELD_POINT, 1)); // Cut
        let actions = h.take_field_actions();
        let [FieldMenuAction::Cut { copied, then }] = actions.as_slice() else {
            panic!("{actions:?}");
        };
        assert_eq!(copied, "hello world");
        assert!(matches!(then.as_ref(), Message::InviteInputChanged(v) if v.is_empty()));
    }

    #[test]
    fn right_click_copy_selection_survives_rebuilds() {
        let mut h = Harness::new("hello world");
        h.click(mouse::Button::Left, FIELD_POINT);
        h.select_all_keys(FIELD_POINT);
        h.click(mouse::Button::Right, FIELD_POINT);
        h.click(mouse::Button::Left, item(FIELD_POINT, 2)); // Copy
        let actions = h.take_field_actions();
        assert!(
            matches!(actions.as_slice(), [FieldMenuAction::Copy(s)] if s == "hello world"),
            "{actions:?}"
        );
    }

    #[test]
    fn paste_inserts_clipboard_at_caret() {
        let mut h = Harness::new("ab");
        h.click(mouse::Button::Left, FIELD_POINT);
        h.click(mouse::Button::Right, FIELD_POINT);
        h.click(mouse::Button::Left, item(FIELD_POINT, 3)); // Paste
        let actions = h.take_field_actions();
        let [FieldMenuAction::Paste(then)] = actions.as_slice() else {
            panic!("{actions:?}");
        };
        assert!(matches!(then.as_ref(), Message::InviteInputChanged(v) if v.contains("PASTED")), "{then:?}");
    }

    #[test]
    fn copy_disabled_without_selection() {
        let mut h = Harness::new("hello");
        h.click(mouse::Button::Left, FIELD_POINT);
        h.click(mouse::Button::Right, FIELD_POINT);
        h.click(mouse::Button::Left, item(FIELD_POINT, 2)); // Copy (disabled)
        assert!(h.take_field_actions().is_empty());
    }

    #[test]
    fn long_press_keeps_existing_selection() {
        let mut h = Harness::new("hello world");
        h.click(mouse::Button::Left, FIELD_POINT);
        h.select_all_keys(FIELD_POINT);
        // Touch the selected text and hold: the finger-down moves the caret,
        // the long-press must bring the selection back.
        let finger = touch::Finger(7);
        h.frame(
            &[Event::Touch(touch::Event::FingerPressed { id: finger, position: FIELD_POINT })],
            FIELD_POINT,
        );
        // A tiny wobble while holding must not drag-change the selection.
        let wobble = Point::new(FIELD_POINT.x + 3.0, FIELD_POINT.y);
        h.frame(&[Event::Touch(touch::Event::FingerMoved { id: finger, position: wobble })], wobble);
        std::thread::sleep(LONG_PRESS + Duration::from_millis(30));
        h.frame(
            &[Event::Window(iced::window::Event::RedrawRequested(Instant::now()))],
            FIELD_POINT,
        );
        h.frame(&[Event::Touch(touch::Event::FingerLifted { id: finger, position: wobble })], wobble);
        let at = item(FIELD_POINT, 2); // Copy
        h.frame(&[Event::Touch(touch::Event::FingerPressed { id: touch::Finger(8), position: at })], at);
        h.frame(&[Event::Touch(touch::Event::FingerLifted { id: touch::Finger(8), position: at })], at);
        let actions = h.take_field_actions();
        assert!(
            matches!(actions.as_slice(), [FieldMenuAction::Copy(s)] if s == "hello world"),
            "{actions:?}"
        );
    }

    #[test]
    fn long_press_opens_menu() {
        let mut h = Harness::new("hello world");
        let finger = touch::Finger(1);
        h.frame(
            &[Event::Touch(touch::Event::FingerPressed { id: finger, position: FIELD_POINT })],
            FIELD_POINT,
        );
        std::thread::sleep(LONG_PRESS + Duration::from_millis(30));
        h.frame(
            &[Event::Window(iced::window::Event::RedrawRequested(Instant::now()))],
            FIELD_POINT,
        );
        h.frame(
            &[Event::Touch(touch::Event::FingerLifted { id: finger, position: FIELD_POINT })],
            FIELD_POINT,
        );
        // Menu must still be open after lifting the long-press finger: tap Select all.
        let at = item(FIELD_POINT, 0);
        h.frame(&[Event::Touch(touch::Event::FingerPressed { id: touch::Finger(2), position: at })], at);
        h.frame(&[Event::Touch(touch::Event::FingerLifted { id: touch::Finger(2), position: at })], at);
        let actions = h.take_field_actions();
        assert!(matches!(actions.as_slice(), [FieldMenuAction::SelectAll(_)]), "{actions:?}");
    }
}
