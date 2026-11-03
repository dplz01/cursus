//! Centralized responsive layout for Cursus.
//!
//! **Breakpoints use logical pixels only** (CSS/DIP-style units), never raw
//! framebuffer / physical pixels. On HiDPI Linux and Retina macOS, a 1200×800
//! logical window stays “desktop” whether the scale factor is 1.0 or 2.0.
//!
//! Iced’s `window::Event::Resized` already reports logical size; we also track
//! scale for diagnostics without letting it affect breakpoint math.

/// Logical width below which the mobile (full-screen push) layout is used.
pub const MOBILE_MAX_WIDTH: f32 = 650.0;

/// Logical width below which the compact (bottom nav + push) layout is used.
/// At or above this value the desktop three-column layout is preferred.
pub const COMPACT_MAX_WIDTH: f32 = 950.0;

/// When taller than wide by this factor (and still under the compact band),
/// prefer mobile even if width alone would allow compact.
const PORTRAIT_ASPECT: f32 = 0.85;

/// Extreme portrait on a wide window: fall back from desktop to compact.
const EXTREME_PORTRAIT_ASPECT: f32 = 0.70;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Narrow / portrait: full-screen destinations, bottom nav, push conversation.
    Mobile,
    /// Mid width: bottom nav, list ↔ content push, no left rail.
    Compact,
    /// Wide: nav rail + list + content.
    Desktop,
}

impl LayoutMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mobile => "mobile",
            Self::Compact => "compact",
            Self::Desktop => "desktop",
        }
    }
}

/// Density tokens derived from layout mode + window size.
#[derive(Debug, Clone, Copy)]
pub struct Density {
    pub title: u16,
    pub body: u16,
    pub meta: u16,
    pub pad: f32,
    pub pad_tight: f32,
    pub list_row: f32,
    pub composer: f32,
    pub rail_width: f32,
    pub list_width: f32,
    pub icon: f32,
    /// Square (rail) or min height (bottom bar) for nav destinations.
    pub nav_hit: f32,
    pub nav_bar_height: f32,
}

impl Density {
    pub fn for_window(mode: LayoutMode, logical_width: f32, _logical_height: f32) -> Self {
        match mode {
            LayoutMode::Mobile => {
                let scale = (logical_width / 390.0).clamp(1.0, 1.45);
                let icon = (26.0 * scale).round().clamp(26.0, 34.0);
                let bar = (60.0 * scale).round().clamp(60.0, 76.0);
                Self {
                    title: 17,
                    body: 16,
                    meta: 13,
                    pad: 16.0,
                    pad_tight: 10.0,
                    list_row: 14.0,
                    composer: 14.0,
                    rail_width: 0.0,
                    list_width: 0.0,
                    icon,
                    nav_hit: bar - 8.0,
                    nav_bar_height: bar,
                }
            }
            LayoutMode::Compact => {
                let scale = (logical_width / 800.0).clamp(1.0, 1.25);
                let icon = (24.0 * scale).round().clamp(24.0, 30.0);
                let bar = (56.0 * scale).round().clamp(56.0, 68.0);
                Self {
                    title: 16,
                    body: 14,
                    meta: 12,
                    pad: 14.0,
                    pad_tight: 8.0,
                    list_row: 12.0,
                    composer: 12.0,
                    rail_width: 0.0,
                    list_width: 0.0,
                    icon,
                    nav_hit: bar - 8.0,
                    nav_bar_height: bar,
                }
            }
            LayoutMode::Desktop => {
                let scale = (logical_width / 1200.0).clamp(1.0, 1.4);
                let rail = (80.0 * scale).round().clamp(72.0, 104.0);
                let icon = (26.0 * scale).round().clamp(24.0, 34.0);
                let hit = (rail - 16.0).max(icon + 20.0);
                Self {
                    title: 18,
                    body: 14,
                    meta: 12,
                    pad: 16.0,
                    pad_tight: 8.0,
                    list_row: 12.0,
                    composer: 12.0,
                    rail_width: rail,
                    list_width: (300.0 * scale.min(1.2)).round(),
                    icon,
                    nav_hit: hit,
                    nav_bar_height: 0.0,
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

fn scale_font(px: u16, scale: f32) -> u16 {
    ((f32::from(px) * scale).round() as u16).clamp(10, 36)
}

/// Resolve layout from **logical** window dimensions.
pub fn resolve(logical_width: f32, logical_height: f32) -> LayoutMode {
    let aspect = if logical_height > 0.0 {
        logical_width / logical_height
    } else {
        1.0
    };

    if logical_width < MOBILE_MAX_WIDTH {
        return LayoutMode::Mobile;
    }

    if logical_width < COMPACT_MAX_WIDTH {
        if aspect < PORTRAIT_ASPECT {
            return LayoutMode::Mobile;
        }
        return LayoutMode::Compact;
    }

    if aspect < EXTREME_PORTRAIT_ASPECT {
        LayoutMode::Compact
    } else {
        LayoutMode::Desktop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoints_match_spec_logical() {
        assert_eq!(resolve(400.0, 800.0), LayoutMode::Mobile);
        assert_eq!(resolve(800.0, 600.0), LayoutMode::Compact);
        assert_eq!(resolve(900.0, 900.0), LayoutMode::Compact);
        assert_eq!(resolve(1200.0, 800.0), LayoutMode::Desktop);
        assert_eq!(resolve(1600.0, 900.0), LayoutMode::Desktop);
    }

    #[test]
    fn hi_dpi_uses_logical_not_framebuffer() {
        assert_eq!(resolve(600.0, 800.0), LayoutMode::Mobile);
        assert_ne!(resolve(1200.0, 1600.0), LayoutMode::Mobile);
    }

    #[test]
    fn portrait_nudge_inside_compact_band() {
        assert_eq!(resolve(700.0, 900.0), LayoutMode::Mobile);
    }

    #[test]
    fn desktop_nav_grows_with_width() {
        let small = Density::for_window(LayoutMode::Desktop, 1200.0, 800.0);
        let large = Density::for_window(LayoutMode::Desktop, 1800.0, 1000.0);
        assert!(large.rail_width >= small.rail_width);
        assert!(large.nav_hit >= small.nav_hit);
        assert!(large.icon >= small.icon);
    }

    #[test]
    fn text_scale_grows_fonts_keeps_rail() {
        let base = Density::for_window(LayoutMode::Desktop, 1200.0, 800.0);
        let large = base.with_text_scale(1.3);
        assert!(large.body > base.body);
        assert!(large.title > base.title);
        assert!(large.meta > base.meta);
        assert_eq!(large.rail_width, base.rail_width);
        assert_eq!(large.icon, base.icon);
    }

    #[test]
    fn text_scale_default_is_identity() {
        let base = Density::for_window(LayoutMode::Compact, 800.0, 600.0);
        let same = base.with_text_scale(1.0);
        assert_eq!(same.body, base.body);
        assert_eq!(same.title, base.title);
    }

    #[test]
    fn text_scale_small_keeps_readable_meta() {
        let d = Density::for_window(LayoutMode::Desktop, 1200.0, 800.0).with_text_scale(0.875);
        assert!(d.meta >= 10, "meta must stay ≥10px so time glyphs do not crush");
        assert!(d.body >= 10);
    }
}
