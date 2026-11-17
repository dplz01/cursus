//! Inset preference panels shared by app, contact, group and channel settings.
use iced::widget::{column, container, text, Column};
use iced::{Element, Length, Padding};

use crate::{layout::Density, theme, Message};

pub(crate) fn group<'a>(
    title: Option<&'a str>,
    note: Option<&'a str>,
    rows: impl Into<Element<'a, Message>>,
    d: Density,
) -> Element<'a, Message> {
    let mut body = Column::new().spacing(9).width(Length::Fill);
    if let Some(title) = title {
        body = body.push(
            container(text(title).size(d.meta).color(theme::t().text_secondary))
                .padding(Padding::from([0, 16])),
        );
    }
    body = body.push(
        container(rows)
            .width(Length::Fill)
            .padding(Padding::from([4, 0]))
            .style(|_| {
                let t = theme::t();
                container::Style {
                    background: Some(t.bg_elevated.into()),
                    border: iced::Border {
                        radius: 20.0.into(),
                        width: 1.0,
                        color: iced::Color { a: 0.5, ..t.border },
                    },
                    ..Default::default()
                }
            }),
    );
    if let Some(note) = note {
        body = body.push(
            container(text(note).size(d.meta).color(theme::t().text_secondary))
                .padding(Padding::from([0, 16])),
        );
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
            let mut flat = column![];
            if let Some(title) = self.title.take() {
                flat = flat.push(
                    text(title)
                        .size(self.density.meta)
                        .color(theme::t().text_secondary),
                );
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
