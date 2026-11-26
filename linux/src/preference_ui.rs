//! Flat row groups shared by app, contact, group and channel settings.
use iced::widget::{column, container, text, Column};

use iced::{Element, Length, Padding};

use crate::{layout::Density, theme, Message};

/// Section heading above a group of rows: meta size, bold, `text.secondary`, inset
/// to the text inside the panel's rows (4 panel + 16 row = 20).
pub(crate) fn heading<'a>(title: impl text::IntoFragment<'a>, d: Density) -> Element<'a, Message> {
    container(text(title).size(d.meta).font(crate::ui::BOLD).color(theme::t().text_secondary))
        .padding(Padding::from([0.0, SECTION_INSET]))
        .width(Length::Fill)
        .into()
}

/// Quiet note under (or over) a panel, at the heading inset.
pub(crate) fn note<'a>(body: impl text::IntoFragment<'a>, d: Density) -> Element<'a, Message> {
    container(text(body).size(d.meta).color(theme::t().text_secondary))
        .padding(Padding::from([0.0, SECTION_INSET]))
        .width(Length::Fill)
        .into()
}

/// Inset of headings and notes; equals the text inset of rows in a panel.
pub(crate) const SECTION_INSET: f32 = PANEL_PAD + 16.0;
/// Padding between a panel's edge and its rows (rows round to
/// `INSET_CONTROL_RADIUS`, concentric with the panel).
pub(crate) const PANEL_PAD: f32 = 4.0;

/// A group of rows (settings, profile actions, member lists, preferences):
/// flat on the page — no surface, no border, no shadow; the heading above
/// and the spacing between groups do the grouping. Rows keep their 4px
/// inset and rounded hover, so text lines up with headings and notes.
pub(crate) fn panel<'a>(rows: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(rows)
        .width(Length::Fill)
        .padding(PANEL_PAD)
        .into()
}

pub(crate) fn group<'a>(
    title: Option<&'a str>,
    note_text: Option<&'a str>,
    rows: impl Into<Element<'a, Message>>,
    d: Density,
) -> Element<'a, Message> {
    let mut body = Column::new().spacing(8).width(Length::Fill);
    if let Some(title) = title {
        body = body.push(heading(title, d));
    }
    body = body.push(panel(rows));
    if let Some(n) = note_text {
        body = body.push(note(n, d));
    }
    body.into()
}

/// Build successive groups without repeating surface and spacing code in each page.
pub(crate) struct Sections<'a> {
    body: Column<'a, Message>,
    rows: Column<'a, Message>,
    title: Option<&'a str>,
    has_rows: bool,
    grouped: bool,
    density: Density,
}

impl<'a> Sections<'a> {
    pub(crate) fn new(grouped: bool, density: Density) -> Self {
        Self {
            body: column![].spacing(20),
            rows: column![],
            title: None,
            has_rows: false,
            grouped,
            density,
        }
    }

    pub(crate) fn push(mut self, row: impl Into<Element<'a, Message>>) -> Self {
        self.rows = self.rows.push(row);
        self.has_rows = true;
        self
    }

    pub(crate) fn section(mut self, title: &'a str) -> Self {
        self.flush();
        self.title = Some(title);
        self
    }

    fn flush(&mut self) {
        if !self.has_rows {
            return;
        }
        let rows = std::mem::replace(&mut self.rows, column![]);
        let content = if self.grouped {
            group(self.title.take(), None, rows, self.density)
        } else {
            let mut flat = column![].spacing(8);
            if let Some(title) = self.title.take() {
                flat = flat.push(heading(title, self.density));
            }
            flat.push(rows).into()
        };
        self.body = std::mem::replace(&mut self.body, column![].spacing(20)).push(content);
        self.has_rows = false;
    }

    pub(crate) fn finish(mut self) -> Element<'a, Message> {
        self.flush();
        self.body.width(Length::Fill).into()
    }
}
