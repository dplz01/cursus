//! A column for long lists (the chat history's bubbles, the chat list),
//! built to stay fast with thousands of rows.
//!
//! An iced `Column` is rebuilt, diffed and laid out in full whenever the view
//! changes, and hands every event to every child. The history can hold
//! thousands of bubbles (opening a chat with many unread loads them all) and
//! the chat list hundreds of rows. This column:
//!
//! - **keeps its rows between frames.** It is given a key of the whole list
//!   and a *plan*: each row's key and a function that builds the row. The
//!   plan runs only when the list's key changes, and then only rows whose
//!   key is new are built; every other row keeps its widget, its state and
//!   its layout. A new message builds one bubble, not the whole history.
//!   Rows whose key is new take over the state of the old row at their
//!   position (like a `Column` diff), so a change every row sees (theme,
//!   selection mode…) behaves as before;
//! - **caches its layout** while the list is unchanged, and re-lays out only
//!   new rows or rows that ask for it (`Shell::invalidate_layout`);
//! - **culls high-frequency events** (redraw ticks, pointer and finger
//!   motion) and popups to the rows on screen or within one screen of it.
//!   Presses, releases, keys and window events still reach every row, so
//!   no gesture or selection can be left half-finished off screen;
//! - **draws** only the rows that intersect the viewport.
//!
//! A row's key must change whenever anything that row shows changes, and the
//! list's key whenever any row key (or the padding) does. Reusing a row's
//! layout relies on the column living in a vertical scrollable.
use std::cell::RefCell;
use std::collections::HashMap;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{overlay, Clipboard, Shell};
use iced::{mouse, touch, window, Alignment, Element, Event, Length, Padding, Point, Rectangle, Size, Theme, Vector};

use crate::Message;

/// Builds one row; called only when the row's key is new.
pub(crate) type RowFn<'a> = Box<dyn FnOnce() -> Element<'static, Message> + 'a>;

/// The rows of a list: each one's key and how to build it.
pub(crate) type Plan<'a> = Vec<(u64, RowFn<'a>)>;

/// A vertical stack (no spacing, full width) of the rows `plan` describes.
/// `plan` runs only when `key` differs from the list last shown.
pub(crate) fn column<'a>(
    key: u64,
    padding: Padding,
    plan: impl FnOnce() -> Plan<'a> + 'a,
) -> Element<'a, Message> {
    Element::new(LongList { key, padding, plan: RefCell::new(Some(Box::new(plan))) })
}

struct LongList<'a> {
    key: u64,
    padding: Padding,
    plan: RefCell<Option<Box<dyn FnOnce() -> Plan<'a> + 'a>>>,
}

#[derive(Default)]
struct State {
    /// Key of the list the rows were built for.
    built: Option<u64>,
    /// Key of each row, in the order of the tree's children.
    keys: Vec<u64>,
    /// The row widgets, kept between frames.
    elements: Vec<Element<'static, Message>>,
    /// Each row's last layout (at the origin); `None` when it must be laid
    /// out again.
    rows: Vec<Option<layout::Node>>,
    /// Width the row layouts were made for.
    row_width: Option<f32>,
    /// The last whole layout and what it was computed for.
    cached: Option<Cached>,
}

struct Cached {
    key: u64,
    min: Size,
    max: Size,
    node: layout::Node,
}

/// Events that fire many times a second and only matter to what is near
/// the screen.
fn high_frequency(event: &Event) -> bool {
    matches!(
        event,
        Event::Window(window::Event::RedrawRequested(_))
            | Event::Mouse(
                mouse::Event::CursorMoved { .. }
                    | mouse::Event::WheelScrolled { .. }
                    | mouse::Event::CursorEntered
                    | mouse::Event::CursorLeft
            )
            | Event::Touch(touch::Event::FingerMoved { .. })
    )
}

/// The rows near the viewport: one screen above and below stays live, so a
/// row being pressed or hovered never stops hearing about the pointer.
fn near(viewport: &Rectangle) -> Rectangle {
    Rectangle { y: viewport.y - viewport.height, height: viewport.height * 3.0, ..*viewport }
}

impl LongList<'_> {
    /// Brings the kept rows up to date with this list, building only rows
    /// whose key is new.
    fn sync(&self, tree: &mut Tree) {
        let Tree { state, children, .. } = tree;
        let state = state.downcast_mut::<State>();
        if state.built == Some(self.key) && state.elements.len() == children.len() {
            return;
        }
        let Some(plan) = self.plan.borrow_mut().take() else {
            return;
        };
        let plan = plan();

        let old_keys = std::mem::take(&mut state.keys);
        let mut old_elements: Vec<Option<Element<'static, Message>>> =
            std::mem::take(&mut state.elements).into_iter().map(Some).collect();
        let mut old_trees: Vec<Option<Tree>> = std::mem::take(children).into_iter().map(Some).collect();
        let mut old_rows = std::mem::take(&mut state.rows);
        let len = old_elements.len().min(old_trees.len()).min(old_keys.len());
        old_rows.resize(len, None);

        // Unchanged rows, wherever they moved (first come, first served for
        // equal keys: equal keys show the same thing).
        // Old rows by key: the first index per key, chained to the next row
        // with the same key (no allocation per key).
        let mut by_key: HashMap<u64, usize> = HashMap::with_capacity(len);
        let mut next_same: Vec<usize> = vec![usize::MAX; len];
        for i in (0..len).rev() {
            if let Some(after) = by_key.insert(old_keys[i], i) {
                next_same[i] = after;
            }
        }
        enum Slot<'a> {
            Kept(Element<'static, Message>, Tree, Option<layout::Node>),
            New(RowFn<'a>),
        }
        let mut keys = Vec::with_capacity(plan.len());
        let mut slots = Vec::with_capacity(plan.len());
        for (k, build) in plan {
            keys.push(k);
            let first = by_key.get(&k).copied();
            if let Some(i) = first {
                match next_same[i] {
                    usize::MAX => by_key.remove(&k),
                    after => by_key.insert(k, after),
                };
            }
            let kept = first.and_then(|i| {
                match (old_elements[i].take(), old_trees[i].take()) {
                    (Some(e), Some(t)) => Some(Slot::Kept(e, t, old_rows[i].take())),
                    _ => None,
                }
            });
            slots.push(kept.unwrap_or(Slot::New(build)));
        }
        // New rows: built now, and given the state of the old row at their
        // position when nothing kept it, else a fresh one.
        let mut elements = Vec::with_capacity(slots.len());
        let mut trees = Vec::with_capacity(slots.len());
        let mut rows = Vec::with_capacity(slots.len());
        for (i, slot) in slots.into_iter().enumerate() {
            match slot {
                Slot::Kept(e, t, node) => {
                    elements.push(e);
                    trees.push(t);
                    rows.push(node);
                }
                Slot::New(build) => {
                    let e = build();
                    let t = match old_trees.get_mut(i).and_then(Option::take) {
                        Some(mut old) => {
                            old.diff(&e);
                            old
                        }
                        None => Tree::new(&e),
                    };
                    elements.push(e);
                    trees.push(t);
                    rows.push(None);
                }
            }
        }
        // Rows that are gone are dropped here, outside the hot path above.
        drop(old_elements);
        *children = trees;
        state.keys = keys;
        state.elements = elements;
        state.rows = rows;
        state.built = Some(self.key);
        state.cached = None;
    }
}

impl Widget<Message, Theme, iced::Renderer> for LongList<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        // Rows are built on first use (`sync`), where the tree is at hand.
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        Vec::new()
    }
    fn diff(&self, tree: &mut Tree) {
        self.sync(tree);
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        self.sync(tree);
        let (min, max) = (limits.min(), limits.max());
        let Tree { state, children, .. } = tree;
        let state = state.downcast_mut::<State>();
        if let Some(c) = &state.cached {
            if c.key == self.key && c.min == min && c.max == max && state.rows.iter().all(Option::is_some) {
                return c.node.clone();
            }
        }
        let inner = limits.width(Length::Fill).height(Length::Shrink).shrink(self.padding);
        if !inner.compression().height {
            // Not in a scrollable: rows may share a finite height, which
            // only the regular flex layout knows how to do.
            state.rows = vec![None; state.elements.len()];
            state.row_width = None;
            state.cached = None;
            return layout::flex::resolve(
                layout::flex::Axis::Vertical,
                renderer,
                limits,
                Length::Fill,
                Length::Shrink,
                self.padding,
                0.0,
                Alignment::Start,
                &mut state.elements,
                children,
            );
        }
        // What `flex::resolve` gives each child of a Fill-width, Shrink-
        // height column whose height is compressed (a scrollable's content).
        let width = inner.max().width;
        let row_limits = layout::Limits::with_compression(
            Size::ZERO,
            Size::new(width, inner.max().height),
            Size::new(false, true),
        );
        if state.row_width != Some(width) {
            state.rows = vec![None; state.elements.len()];
            state.row_width = Some(width);
        }
        state.rows.resize(state.elements.len(), None);

        let mut nodes = Vec::with_capacity(state.elements.len());
        let mut y = self.padding.top;
        let mut cross: f32 = 0.0;
        for ((child, child_tree), row) in state.elements.iter_mut().zip(children.iter_mut()).zip(&mut state.rows) {
            let node = match row {
                Some(node) => node.clone(),
                None => {
                    let node = child.as_widget_mut().layout(child_tree, renderer, &row_limits);
                    *row = Some(node.clone());
                    node
                }
            };
            let size = node.size();
            nodes.push(node.move_to(Point::new(self.padding.left, y)));
            y += size.height;
            cross = cross.max(size.width);
        }
        let intrinsic = Size::new(cross, y - self.padding.top);
        let size = inner.resolve(Length::Fill, Length::Shrink, intrinsic);
        let node = layout::Node::with_children(size.expand(self.padding), nodes);
        state.cached = Some(Cached { key: self.key, min, max, node: node.clone() });
        node
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn Operation) {
        let Tree { state, children, .. } = tree;
        let state = state.downcast_mut::<State>();
        // An operation may change a row's state in ways that move things;
        // rows hold no focusable or scrollable state, so only the assembled
        // column is redone (from the kept row layouts, cheap).
        state.cached = None;
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, tree), layout) in state.elements.iter_mut().zip(children.iter_mut()).zip(layout.children()) {
                child.as_widget_mut().operate(tree, layout, renderer, operation);
            }
        });
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
        let Tree { state, children, .. } = tree;
        let state = state.downcast_mut::<State>();
        let near = near(viewport);
        let cull = high_frequency(event);
        // Each row runs in a shell of its own so we know which one asked
        // for a new layout (a sibling such as the composer may have done so
        // earlier in this same pass). A row still sees an event as taken
        // when the glass above or an earlier row took it.
        let mut captured = shell.is_event_captured();
        let mut relayout: Vec<usize> = Vec::new();
        for (i, ((child, tree), layout)) in
            state.elements.iter_mut().zip(children.iter_mut()).zip(layout.children()).enumerate()
        {
            if cull && !layout.bounds().intersects(&near) {
                continue;
            }
            let mut messages = Vec::new();
            let mut inner = Shell::new(&mut messages);
            if captured {
                inner.capture_event();
            }
            child.as_widget_mut().update(tree, event, layout, cursor, renderer, clipboard, &mut inner, viewport);
            captured = inner.is_event_captured();
            if inner.is_layout_invalid() || inner.are_widgets_invalid() {
                relayout.push(i);
            }
            shell.merge(inner, std::convert::identity);
        }
        if !relayout.is_empty() {
            state.cached = None;
            for i in relayout {
                if let Some(row) = state.rows.get_mut(i) {
                    *row = None;
                }
            }
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
        let state = tree.state.downcast_ref::<State>();
        state
            .elements
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|(_, layout)| layout.bounds().intersects(viewport))
            .map(|((child, state), layout)| child.as_widget().mouse_interaction(state, layout, cursor, viewport, renderer))
            .max()
            .unwrap_or_default()
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
        let state = tree.state.downcast_ref::<State>();
        for ((child, state), layout) in state
            .elements
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|(_, layout)| layout.bounds().intersects(viewport))
        {
            child.as_widget().draw(state, renderer, theme, style, layout, cursor, viewport);
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
        // Only rows on (or right next to) the screen can have a popup open;
        // asking all of them walked every bubble's widget tree each frame.
        // `viewport` is in window space here and the layout is not yet
        // scrolled: `translation` moves one onto the other.
        let near = near(viewport);
        let Tree { state, children, .. } = tree;
        let state = state.downcast_mut::<State>();
        let overlays: Vec<_> = state
            .elements
            .iter_mut()
            .zip(children.iter_mut())
            .zip(layout.children())
            .filter(|(_, layout)| (layout.bounds() + translation).intersects(&near))
            .filter_map(|((child, tree), layout)| {
                child.as_widget_mut().overlay(tree, layout, renderer, viewport, translation)
            })
            .collect();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}
