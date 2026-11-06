//! Text that ends in "…" instead of wrapping past a line budget.
//!
//! [`ellipsis_text`] fits its string at **layout** time, when iced knows the
//! real width available, using the same font, shaping and wrap rules as the
//! drawn text — so names stay on one line and previews on two, ending with
//! "…", at any window size.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::widget::text::{LineHeight, Shaping, Wrapping};
use iced::{mouse, Color, Element, Event, Font, Length, Rectangle, Size, Theme};
use unicode_segmentation::UnicodeSegmentation;

use crate::Message;

/// Line height (× size) used for both measuring and drawing.
pub const LINE_HEIGHT: f32 = 1.25;

/// Glyph wrap for text with long unbroken runs (links, keys): word wrap would
/// push the whole run onto a fresh line and leave the first line half empty.
pub fn preview_wrapping(s: &str) -> Wrapping {
    let long_run = s
        .split_whitespace()
        .any(|w| w.graphemes(true).count() > 18);
    if long_run {
        Wrapping::Glyph
    } else {
        Wrapping::WordOrGlyph
    }
}

/// Number of lines `s` occupies when laid out exactly like the text widget
/// (Noto Sans, advanced shaping, given wrap) at width `max_w`.
pub fn measured_lines(s: &str, max_w: f32, size: f32, line_px: f32, wrapping: Wrapping) -> usize {
    use iced::advanced::text::Paragraph as _;
    let paragraph = iced::advanced::graphics::text::Paragraph::with_text(iced::advanced::Text {
        content: s,
        bounds: Size::new(max_w, f32::INFINITY),
        size: size.into(),
        line_height: iced::widget::text::LineHeight::Absolute(line_px.into()),
        font: Font::with_name("Noto Sans"),
        align_x: iced::advanced::text::Alignment::Default,
        align_y: iced::alignment::Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping,
    });
    (paragraph.min_height() / line_px).round().max(1.0) as usize
}

thread_local! {
    /// (text, width, size, lines) → fitted preview. Layout is not free and the
    /// chat list re-renders often; previews rarely change.
    static ELLIPSIS_CACHE: std::cell::RefCell<std::collections::HashMap<(String, u32, u32, usize), String>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Fit `s` into `lines` lines of `max_w` px at `size`, ending with "…" when
/// anything had to be cut. Uses the real text layout (same font, shaping and
/// wrap rules as the widget), so long links / unbroken strings get an ellipsis
/// instead of being clipped mid-glyph.
pub fn ellipsize_to_lines(s: &str, max_w: f32, size: f32, lines: usize) -> String {
    let line_px = size * LINE_HEIGHT;
    // Hair of slack for sub-pixel differences between measure and draw.
    let max_w = (max_w - 2.0).max(10.0);
    let key = (s.to_string(), max_w.to_bits(), size.to_bits(), lines);
    if let Some(hit) = ELLIPSIS_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }

    let wrapping = preview_wrapping(s);
    let fits = |t: &str| measured_lines(t, max_w, size, line_px, wrapping) <= lines;
    let out = if fits(s) {
        s.to_string()
    } else {
        // Longest grapheme prefix that still fits together with "…".
        let ends: Vec<usize> = s.grapheme_indices(true).map(|(i, g)| i + g.len()).collect();
        let candidate = |n: usize| -> String {
            let head = if n == 0 { "" } else { &s[..ends[n - 1]] };
            format!("{}…", head.trim_end())
        };
        // Fit is not monotonic in length under word wrap (a growing token
        // jumps to the next line, then glyph-wraps once it is longer than a
        // line), so scan down from a generous width-based upper bound and
        // take the longest prefix that fits.
        let budget = max_w * lines as f32 * 1.4;
        let mut acc = 0.0f32;
        let upper = s
            .graphemes(true)
            .take_while(|g| {
                acc += crate::ui::grapheme_em(g) * size;
                acc <= budget
            })
            .count()
            .min(ends.len());
        (0..=upper)
            .rev()
            .map(candidate)
            .find(|c| fits(c))
            .unwrap_or_else(|| "…".to_string())
    };

    ELLIPSIS_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 1024 {
            c.clear();
        }
        c.insert(key, out.clone());
    });
    out
}


/// Single- or multi-line text that ellipsizes to fit its width.
pub fn ellipsis_text<'a>(
    content: impl Into<String>,
    size: f32,
    color: Color,
    lines: usize,
) -> Element<'a, Message> {
    let full = content.into();
    let wrapping = preview_wrapping(&full);
    Element::new(Ellipsis {
        inner: make_text(full.clone(), size, color, wrapping),
        full,
        size,
        color,
        lines: lines.max(1),
        wrapping,
        fitted_for: None,
    })
}

fn make_text(s: String, size: f32, color: Color, wrapping: Wrapping) -> iced::widget::Text<'static, Theme, iced::Renderer> {
    iced::widget::text(s)
        .size(size)
        .color(color)
        .font(Font::with_name("Noto Sans"))
        .shaping(Shaping::Advanced)
        .line_height(LineHeight::Absolute((size * LINE_HEIGHT).into()))
        .wrapping(wrapping)
}

struct Ellipsis {
    full: String,
    size: f32,
    color: Color,
    lines: usize,
    wrapping: Wrapping,
    inner: iced::widget::Text<'static, Theme, iced::Renderer>,
    /// Width the inner text was last fitted for.
    fitted_for: Option<f32>,
}

impl Widget<Message, Theme, iced::Renderer> for Ellipsis {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.inner as &dyn Widget<Message, Theme, iced::Renderer>)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.children[0].diff(&self.inner as &dyn Widget<Message, Theme, iced::Renderer>);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let max_w = limits.max().width;
        if self.fitted_for != Some(max_w) && max_w.is_finite() {
            let fitted = ellipsize_to_lines(&self.full, max_w, self.size, self.lines);
            self.inner = make_text(fitted, self.size, self.color, self.wrapping);
            self.fitted_for = Some(max_w);
        }
        <iced::widget::Text<'static, Theme, iced::Renderer> as Widget<Message, Theme, iced::Renderer>>::layout(
            &mut self.inner,
            &mut tree.children[0],
            renderer,
            limits,
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
        <iced::widget::Text<'static, Theme, iced::Renderer> as Widget<Message, Theme, iced::Renderer>>::draw(
            &self.inner,
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        _event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
    }

    fn operate(
        &mut self,
        _tree: &mut Tree,
        _layout: Layout<'_>,
        _renderer: &iced::Renderer,
        _operation: &mut dyn widget::Operation,
    ) {
    }
}

#[cfg(test)]
mod ellipsize_tests {
    use super::{ellipsize_to_lines, measured_lines};

    fn load_fonts() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
        let mut fs = iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system");
        for f in ["NotoSans-Regular.ttf", "NotoColorEmoji.ttf"] {
            if let Ok(bytes) = std::fs::read(dir.join(f)) {
                fs.load_font(std::borrow::Cow::Owned(bytes));
            }
        }
    }

    #[test]
    fn short_text_is_untouched() {
        load_fonts();
        assert_eq!(ellipsize_to_lines("hello there", 200.0, 12.0, 2), "hello there");
    }

    #[test]
    fn long_link_ends_with_ellipsis_and_fits() {
        load_fonts();
        let link = "https://smp9.simplex.im/i#3iqO73eh1l09QmQfRwMyM2MplnycyJT7/HzD4YSRIOBK_Bvi1gZiXI1BY-JQivcBC5GizZ2qtBBM";
        for w in [120.0f32, 180.0, 230.0, 300.0] {
            let out = ellipsize_to_lines(link, w, 12.0, 2);
            assert!(out.ends_with('…'), "{w}: {out}");
            assert!(measured_lines(&out, w - 2.0, 12.0, 15.0, super::preview_wrapping(link)) <= 2, "{w}: {out}");
            // Uses the space: one more grapheme would overflow.
            assert!(out.chars().count() > 10, "{w}: {out}");
        }
    }

    #[test]
    fn long_words_get_ellipsis() {
        load_fonts();
        let long = "lorem ipsum dolor sit amet ".repeat(10);
        let out = ellipsize_to_lines(&long, 200.0, 12.0, 2);
        assert!(out.ends_with('…'));
        assert!(measured_lines(&out, 198.0, 12.0, 15.0, super::preview_wrapping(&long)) <= 2);
    }

    #[test]
    fn newlines_count_as_lines() {
        load_fonts();
        let out = ellipsize_to_lines("a\nb\nc", 200.0, 12.0, 2);
        assert!(out.starts_with('a') && out.ends_with('…'), "{out}");
    }
}
