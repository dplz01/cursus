//! Colour emoji everywhere, not only where a widget forces the emoji font.
//!
//! cosmic-text's Unix fallback order tries DejaVu Sans / FreeSans / Noto Sans
//! Symbols *before* Noto Color Emoji. Any emoji typed into ordinary text (a
//! message, the composer, a chat preview) is missing from Noto Sans, so the
//! shaper picked DejaVu's monochrome outline glyphs (❤ 😄 rendered as black
//! line art). This swaps in a fallback list that asks Noto Color Emoji right
//! after the UI font, keeping the platform's per-script lists untouched.

use iced::advanced::graphics::text::cosmic_text::{self, Fallback, PlatformFallback};
use unicode_script::Script;

const EMOJI_FIRST: &[&str] = &[
    "Noto Sans",
    "Noto Color Emoji",
    "DejaVu Sans",
    "FreeSans",
    "Noto Sans Mono",
    "DejaVu Sans Mono",
    "FreeMono",
    "Noto Sans Symbols",
    "Noto Sans Symbols2",
];

struct EmojiFirst;

impl Fallback for EmojiFirst {
    fn common_fallback(&self) -> &[&'static str] {
        EMOJI_FIRST
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        &[]
    }

    fn script_fallback(&self, script: Script, locale: &str) -> &[&'static str] {
        PlatformFallback.script_fallback(script, locale)
    }
}

/// Install the emoji-first fallback on iced's global font system. Call once,
/// before the first frame (fonts loaded later keep working — the database is
/// carried over).
pub fn prefer_color_emoji() {
    let Ok(mut fs) = iced::advanced::graphics::text::font_system().write() else {
        return;
    };
    let raw = fs.raw();
    let placeholder = cosmic_text::FontSystem::new_with_locale_and_db_and_fallback(
        String::new(),
        cosmic_text::fontdb::Database::new(),
        PlatformFallback,
    );
    let (locale, db) = std::mem::replace(raw, placeholder).into_locale_and_db();
    *raw = cosmic_text::FontSystem::new_with_locale_and_db_and_fallback(locale, db, EmojiFirst);
}

#[cfg(test)]
mod tests {
    use super::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

    /// Lists emoji the bundled Noto Color Emoji can't draw as one glyph.
    #[test]
    #[ignore]
    fn bundled_font_coverage() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/fonts/NotoColorEmoji.ttf");
        let bytes = std::fs::read(path).expect("font");
        let mut fs = FontSystem::new_with_locale_and_db_and_fallback(
            "en-US".into(),
            {
                let mut db = super::cosmic_text::fontdb::Database::new();
                db.load_font_data(bytes);
                db
            },
            super::EmojiFirst,
        );
        let mut missing = std::collections::BTreeMap::<String, Vec<String>>::new();
        let mut total = 0;
        for e in emojis::iter() {
            total += 1;
            let mut buf = Buffer::new(&mut fs, Metrics::new(20.0, 24.0));
            let attrs = Attrs::new().family(Family::Name("Noto Color Emoji"));
            buf.set_text(&mut fs, e.as_str(), &attrs, Shaping::Advanced, None);
            buf.shape_until_scroll(&mut fs, false);
            let glyphs: Vec<u16> = buf
                .layout_runs()
                .flat_map(|r| r.glyphs.iter().map(|g| g.glyph_id))
                .collect();
            if glyphs.len() != 1 || glyphs[0] == 0 {
                let v = e.unicode_version();
                missing
                    .entry(format!("{}.{}", v.major(), v.minor()))
                    .or_default()
                    .push(format!("{} {}", e.as_str(), e.name()));
            }
        }
        for (v, list) in &missing {
            println!("{v}: {} missing, e.g. {:?}", list.len(), &list[..list.len().min(4)]);
        }
        println!("total {total}");
    }
}
