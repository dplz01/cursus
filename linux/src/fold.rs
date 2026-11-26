//! A profile's "about" folded to two lines that end in "… Show more" (an
//! accent link right after the ellipsis); unfolded it ends in "Show less".
//!
//! Like [`crate::ellipsis`], the text is fitted at **layout** time, when the
//! real width is known, with the same font and shaping as the drawn text.
//! The formatting of the kept part (bold, links…) is kept.
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::widget::text::{Rich, Span, Wrapping};
use iced::{mouse, Color, Element, Event, Length, Rectangle, Size, Theme};

use crate::Message;
use cursus_core::TextSegment;

/// Lines shown while folded.
const LINES: usize = 2;
const MORE: &str = "Show more";
const LESS: &str = "Show less";

/// One span of the about text, styled like the rest of the profile.
pub(crate) type SpanOf = fn(&TextSegment, f32, Color) -> Span<'static, Message>;

pub(crate) fn fold<'a>(
    segments: Vec<TextSegment>,
    size: f32,
    color: Color,
    accent: Color,
    open: bool,
    span_of: SpanOf,
) -> Element<'a, Message> {
    let mut fold = Fold { segments, size, color, accent, open, span_of, inner: None, fitted_for: None };
    fold.inner = Some(fold.rich(None, false));
    crate::tap::tap_links(Element::new(fold))
}

struct Fold {
    segments: Vec<TextSegment>,
    size: f32,
    color: Color,
    accent: Color,
    open: bool,
    span_of: SpanOf,
    inner: Option<Rich<'static, Message, Message>>,
    fitted_for: Option<f32>,
}

impl Fold {
    fn plain(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }

    /// The text cut after `keep` bytes of its plain form (`None`: all of it),
    /// then "… Show more" / " Show less" when `toggle`.
    fn rich(&self, keep: Option<usize>, toggle: bool) -> Rich<'static, Message, Message> {
        let mut spans = Vec::new();
        let mut left = keep.unwrap_or(usize::MAX);
        for seg in &self.segments {
            if left == 0 {
                break;
            }
            let mut seg = seg.clone();
            if seg.text.len() > left {
                let mut end = left;
                while !seg.text.is_char_boundary(end) {
                    end -= 1;
                }
                seg.text.truncate(end);
                seg.text = seg.text.trim_end().to_string();
            }
            left -= seg.text.len().min(left);
            if !seg.text.is_empty() {
                spans.push((self.span_of)(&seg, self.size, self.color));
            }
        }
        if toggle {
            let (gap, label) = if keep.is_some() { ("… ", MORE) } else { (" ", LESS) };
            spans.push(iced::widget::span(gap).color(self.color));
            spans.push(
                iced::widget::span(label)
                    .color(self.accent)
                    .font(crate::ui::BOLD)
                    .link(Message::ToggleAbout),
            );
        }
        iced::widget::rich_text(spans)
            .size(self.size)
            .color(self.color)
            .wrapping(Wrapping::WordOrGlyph)
            .on_link_click(|msg: Message| msg)
    }

    /// Fit for `max_w`: everything if it fits in two lines, else the longest
    /// prefix that fits with "… Show more".
    fn fit(&mut self, max_w: f32) {
        let plain = self.plain();
        let line_px = self.size * 1.3;
        // Slack for bold / link spans measured as regular text.
        let w = (max_w - 12.0).max(20.0);
        let lines = |t: &str| crate::ellipsis::measured_lines_font(t, w, self.size, line_px, Wrapping::WordOrGlyph, false);
        let long = lines(&plain) > LINES;
        self.inner = Some(if !long {
            self.rich(None, false)
        } else if self.open {
            self.rich(None, true)
        } else {
            let ends: Vec<usize> = plain.char_indices().map(|(i, c)| i + c.len_utf8()).collect();
            let head = |n: usize| if n == 0 { "" } else { &plain[..ends[n - 1]] };
            let candidate = |n: usize| format!("{}… {MORE}", head(n).trim_end());
            let mut keep = crate::ellipsis::longest_fitting(ends.len(), |n| lines(&candidate(n)) <= LINES)
                .unwrap_or(0);
            // End on a whole word when one ends close by.
            let chars: Vec<char> = plain.chars().collect();
            if let Some(space) = (keep.saturating_sub(16)..keep).rev().find(|&i| chars[i].is_whitespace()) {
                keep = space;
            }
            self.rich(Some(if keep == 0 { 0 } else { ends[keep - 1] }), true)
        });
        self.fitted_for = Some(max_w);
    }

    fn inner(&self) -> &Rich<'static, Message, Message> {
        self.inner.as_ref().expect("built in fold()")
    }
}

impl Widget<Message, Theme, iced::Renderer> for Fold {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<Fold>()
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(self.inner() as &dyn Widget<Message, Theme, iced::Renderer>)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.inner() as &dyn Widget<Message, Theme, iced::Renderer>]);
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }
    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let max_w = limits.max().width;
        if self.fitted_for != Some(max_w) && max_w.is_finite() {
            self.fit(max_w);
            // The spans changed: their state (link hover…) starts afresh.
            tree.diff_children(&[self.inner() as &dyn Widget<Message, Theme, iced::Renderer>]);
        }
        let inner = self.inner.as_mut().expect("built in fold()");
        Widget::<Message, Theme, iced::Renderer>::layout(inner, &mut tree.children[0], renderer, limits)
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
        Widget::<Message, Theme, iced::Renderer>::draw(self.inner(), &tree.children[0], renderer, theme, style, layout, cursor, viewport);
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
        let inner = self.inner.as_mut().expect("built in fold()");
        Widget::<Message, Theme, iced::Renderer>::update(inner, &mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        Widget::<Message, Theme, iced::Renderer>::mouse_interaction(self.inner(), &tree.children[0], layout, cursor, viewport, renderer)
    }
}
