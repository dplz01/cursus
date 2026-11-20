//! Centralized responsive layout for Cursus — the Telegram Desktop model.
//!
//! The window's **logical width** alone decides how many columns fit:
//!
//! | Width | Columns |
//! |---|---|
//! | under 600 | one: list, or the open chat / page (phone) |
//! | from 600 | two: list + chat; the four destinations sit in a bar under the list |
//! | from 900 | the destinations move to a rail on the left |
//! | chat has 700 to spare | three: list + chat + info (the open profile) |
//!
//! Logical pixels only (what iced reports), so HiDPI scale factors never
//! change the layout.

/// Below this width only one column fits.
pub const ONE_COLUMN_MAX: f32 = 600.0;

/// From this width the destinations get their own rail.
const RAIL_MIN_WIDTH: f32 = 900.0;

/// Narrowest a conversation may get next to the info column.
const CHAT_MIN_WIDTH: f32 = 380.0;

/// Width of the info (third) column.
const INFO_WIDTH: f32 = 340.0;

/// Widest a page column (profile, settings) gets; beyond it the content is
/// centred instead of stretched.
pub const CONTENT_MAX_WIDTH: f32 = 620.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutMode {
    /// One column: list ↔ conversation push, bottom bar.
    Mobile,
    /// List + content side by side (+ info column when it fits).
    Desktop,
}

/// Sizes derived from the layout and the window width.
#[derive(Debug, Clone, Copy)]
pub struct Density {
    pub title: f32,
    pub body: f32,
    pub meta: f32,
    pub pad: f32,
    pub pad_tight: f32,
    pub list_row: f32,
    pub composer: f32,
    /// Width of the destinations rail; 0 = they are a bar under the list.
    pub rail_width: f32,
    pub list_width: f32,
    /// Width of the info column; 0 = no room, profiles replace the chat.
    pub info_width: f32,
    pub icon: f32,
    /// Square (rail) or min height (bottom bar) for nav destinations.
    pub nav_hit: f32,
    pub nav_bar_height: f32,
}

impl Density {
    pub fn for_window(mode: LayoutMode, logical_width: f32, _logical_height: f32) -> Self {
        match mode {
            LayoutMode::Mobile => Self {
                title: 17.0,
                body: 17.0,
                meta: 14.0,
                pad: 16.0,
                pad_tight: 10.0,
                list_row: 6.0,
                composer: 14.0,
                rail_width: 0.0,
                list_width: 0.0,
                info_width: 0.0,
                icon: 26.0,
                nav_hit: 46.0,
                nav_bar_height: 54.0,
            },
            LayoutMode::Desktop => {
                let rail = if logical_width >= RAIL_MIN_WIDTH { 72.0 } else { 0.0 };
                // The list grows a little with the window, like Telegram's.
                let list = (logical_width * 0.27).round().clamp(260.0, 340.0);
                let chat = logical_width - rail - list;
                Self {
                    title: 18.0,
                    body: 14.0,
                    meta: 12.0,
                    pad: 16.0,
                    pad_tight: 8.0,
                    list_row: 6.0,
                    composer: 12.0,
                    rail_width: rail,
                    list_width: list,
                    info_width: if chat >= CHAT_MIN_WIDTH + INFO_WIDTH { INFO_WIDTH } else { 0.0 },
                    icon: if rail > 0.0 { 26.0 } else { 22.0 },
                    nav_hit: if rail > 0.0 { rail - 16.0 } else { 40.0 },
                    nav_bar_height: if rail > 0.0 { 0.0 } else { 54.0 },
                }
            }
        }
    }

    /// Scale title / body / meta (and related input padding) for accessibility.
    /// Layout chrome (rail, icons, nav hit) stays fixed so breakpoints stay stable.
    pub fn with_text_scale(mut self, scale: f32) -> Self {
        if (scale - 1.0).abs() < 0.01 {
            return self;
        }
        let scale = scale.clamp(0.75, 1.5);
        self.title = scale_font(self.title, scale);
        self.body = scale_font(self.body, scale);
        self.meta = scale_font(self.meta, scale);
        self.composer = (self.composer * scale).round().clamp(8.0, 28.0);
        self.list_row = (self.list_row * scale).round().clamp(8.0, 28.0);
        self
    }
}

fn scale_font(px: f32, scale: f32) -> f32 {
    (px * scale).round().clamp(10.0, 36.0)
}

/// Layout for a window `logical_width` wide.
pub fn resolve(logical_width: f32) -> LayoutMode {
    if logical_width < ONE_COLUMN_MAX {
        LayoutMode::Mobile
    } else {
        LayoutMode::Desktop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns(width: f32) -> (LayoutMode, bool, bool) {
        let mode = resolve(width);
        let d = Density::for_window(mode, width, 800.0);
        (mode, d.rail_width > 0.0, d.info_width > 0.0)
    }

    #[test]
    fn columns_follow_the_width() {
        // Narrow tiles and phones: one column.
        assert_eq!(columns(280.0), (LayoutMode::Mobile, false, false));
        assert_eq!(columns(599.0), (LayoutMode::Mobile, false, false));
        // Half of a 1280 px screen: list + chat, destinations under the list.
        assert_eq!(columns(640.0), (LayoutMode::Desktop, false, false));
        // Roomier: the rail appears.
        assert_eq!(columns(940.0), (LayoutMode::Desktop, true, false));
        // Wide: the info column fits next to the chat.
        assert_eq!(columns(1180.0), (LayoutMode::Desktop, true, true));
        assert_eq!(columns(1920.0), (LayoutMode::Desktop, true, true));
    }

    #[test]
    fn the_chat_keeps_a_usable_width() {
        for width in [600.0, 640.0, 760.0, 899.0, 900.0, 1100.0, 1132.0, 1400.0, 2560.0] {
            let d = Density::for_window(LayoutMode::Desktop, width, 800.0);
            let chat = width - d.rail_width - d.list_width - d.info_width;
            assert!(chat >= 330.0, "{width}: chat {chat}");
            assert!((260.0..=340.0).contains(&d.list_width), "{width}");
        }
    }

    #[test]
    fn text_scale_grows_fonts_keeps_rail() {
        let base = Density::for_window(LayoutMode::Desktop, 1200.0, 800.0);
        let large = base.with_text_scale(1.3);
        assert!(large.body > base.body && large.title > base.title && large.meta > base.meta);
        assert_eq!(large.rail_width, base.rail_width);
        assert_eq!(base.with_text_scale(1.0).body, base.body);
        assert!(base.with_text_scale(0.875).meta >= 10.0, "meta stays readable");
    }
}
