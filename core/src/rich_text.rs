//! SimpleX markdown / emoji helpers for chat bubbles.
//!
//! `formattedText` comes from libsimplex chat items (same shape as the official
//! clients). Short emoji-only messages get the large/medium emoji treatment.

use serde_json::Value;
use unicode_segmentation::UnicodeSegmentation;

/// Max grapheme clusters for the large-emoji bubble (matches official clients).
pub const SHORT_EMOJI_MAX: usize = 5;

/// Under this count → 48px; otherwise medium 36px (official `EmojiItemView`).
pub const LARGE_EMOJI_MAX: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    Plain,
    Bold,
    Italic,
    Strike,
    Snippet,
    Secret,
    Small,
    Colored { color: FormatColor },
    Uri,
    Email,
    Phone,
    Mention,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatColor {
    Red,
    Green,
    Blue,
    Yellow,
    Cyan,
    Magenta,
    Black,
    White,
}

impl FormatColor {
    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "red" => Self::Red,
            "green" => Self::Green,
            "blue" => Self::Blue,
            "yellow" => Self::Yellow,
            "cyan" => Self::Cyan,
            "magenta" => Self::Magenta,
            "black" => Self::Black,
            "white" => Self::White,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSegment {
    pub text: String,
    pub style: TextStyle,
    /// Absolute URL / URI to open when the segment is a link.
    pub link: Option<String>,
}

/// One autocomplete hit for an incomplete `:prefix` in the composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmojiSuggestion {
    pub shortcode: &'static str,
    pub glyph: &'static str,
}

/// Incomplete `:prefix` at the end of `s` (no closing colon yet).
///
/// Returns the prefix after the colon (`"ca"` for `"hi :ca"`).
pub fn pending_shortcode_query(s: &str) -> Option<&str> {
    let start = s.rfind(':')?;
    let after = &s[start + 1..];
    if after.is_empty() {
        return None;
    }
    if after.contains(':') || after.contains(char::is_whitespace) {
        return None;
    }
    if !is_shortcode_name(after) {
        return None;
    }
    Some(after)
}

/// Common nicknames → gemoji shortcode (gemoji has no `:pumpkin:`, only `:jack_o_lantern:`).
const COLLOQUIAL_ALIASES: &[(&str, &str)] = &[
    ("pumpkin", "jack_o_lantern"),
    ("halloween", "jack_o_lantern"),
    ("thumbsup", "+1"),
    ("thumbs_up", "+1"),
    ("thumbsdown", "-1"),
    ("thumbs_down", "-1"),
    ("poop", "hankey"),
    ("shit", "hankey"),
    ("lol", "joy"),
    ("haha", "joy"),
    ("kiss", "kissing_heart"),
    ("heart", "heart"),
    ("fire", "fire"),
    ("check", "white_check_mark"),
    ("checkmark", "white_check_mark"),
];

/// Suggest shortcodes for an incomplete `:prefix`.
///
/// Matches (in order): shortcode prefix, colloquial aliases, shortcode substring,
/// CLDR name substring.
pub fn suggest_emoji_shortcodes(prefix: &str, limit: usize) -> Vec<EmojiSuggestion> {
    if prefix.is_empty() || limit == 0 {
        return Vec::new();
    }
    let prefix = prefix.to_ascii_lowercase();
    let mut scored: Vec<(u8, EmojiSuggestion)> = Vec::new();

    for &(alias, target) in COLLOQUIAL_ALIASES {
        if alias.starts_with(&prefix) || alias.contains(&prefix) {
            if let Some(emoji) = emojis::get_by_shortcode(target) {
                scored.push((
                    if alias.starts_with(&prefix) { 0 } else { 1 },
                    EmojiSuggestion {
                        shortcode: emoji.shortcode().unwrap_or(target),
                        glyph: emoji.as_str(),
                    },
                ));
            }
        }
    }

    for emoji in emojis::iter().filter(|emoji| {
        matches!(
            emoji.skin_tone(),
            Some(emojis::SkinTone::Default) | None
        )
    }) {
        let name = emoji.name().to_ascii_lowercase();
        let name_hit = name.contains(&prefix);
        for sc in emoji.shortcodes() {
            let rank = if sc.starts_with(&prefix) {
                Some(0)
            } else if sc.contains(&prefix) {
                Some(2)
            } else if name_hit {
                Some(3)
            } else {
                None
            };
            if let Some(rank) = rank {
                scored.push((
                    rank,
                    EmojiSuggestion {
                        shortcode: sc,
                        glyph: emoji.as_str(),
                    },
                ));
            }
        }
        // Name hit but no shortcodes (rare) — still surface via primary shortcode.
        if name_hit && emoji.shortcode().is_none() {
            // skip — can't insert without a shortcode/glyph still works:
            scored.push((
                3,
                EmojiSuggestion {
                    shortcode: emoji.name(),
                    glyph: emoji.as_str(),
                },
            ));
        }
    }

    scored.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| {
                emoji_presentation_rank(a.1.glyph).cmp(&emoji_presentation_rank(b.1.glyph))
            })
            .then_with(|| a.1.shortcode.len().cmp(&b.1.shortcode.len()))
            .then_with(|| a.1.shortcode.cmp(b.1.shortcode))
    });
    scored.dedup_by(|a, b| a.1.shortcode == b.1.shortcode || a.1.glyph == b.1.glyph);
    scored.truncate(limit);
    scored.into_iter().map(|(_, s)| s).collect()
}

/// Prefer colorful emoji (🚀 / ❤️) over dingbat/text hearts (❤ / ♥ without color).
fn emoji_presentation_rank(glyph: &str) -> u8 {
    if glyph.chars().any(|c| {
        let u = c as u32;
        (0x1F300..=0x1FAFF).contains(&u)
    }) {
        0
    } else if glyph.contains('\u{FE0F}') {
        1
    } else {
        2
    }
}

/// Expand GitHub-style `:shortcode:` tokens to emoji (unknown codes left as-is).
pub fn expand_emoji_shortcodes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find(':') {
            let code = &after[..end];
            if is_shortcode_name(code) {
                let resolved = emojis::get_by_shortcode(code).or_else(|| {
                    COLLOQUIAL_ALIASES
                        .iter()
                        .find(|(alias, _)| *alias == code)
                        .and_then(|(_, target)| emojis::get_by_shortcode(target))
                });
                if let Some(emoji) = resolved {
                    out.push_str(emoji.as_str());
                    rest = &after[end + 1..];
                    continue;
                }
            }
            // Not a known shortcode — keep the opening colon and keep scanning.
            out.push(':');
            rest = after;
        } else {
            out.push_str(&rest[start..]);
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn is_shortcode_name(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 40
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '+' || c == '-')
}

/// True when trimmed text is 1..=5 emoji grapheme clusters (no other chars).
pub fn is_short_emoji(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let graphemes: Vec<&str> = s.graphemes(true).collect();
    if graphemes.is_empty() || graphemes.len() > SHORT_EMOJI_MAX {
        return false;
    }
    graphemes.iter().all(|g| is_emoji_grapheme(g))
}

fn is_emoji_grapheme(g: &str) -> bool {
    if emojis::get(g).is_some() {
        return true;
    }
    // Keycaps: 1️⃣ / 1⃣ / #️⃣
    let chars: Vec<char> = g.chars().collect();
    match chars.as_slice() {
        [c, '\u{FE0F}', '\u{20E3}'] | [c, '\u{20E3}']
            if c.is_ascii_digit() || *c == '#' || *c == '*' =>
        {
            true
        }
        // Lone emoji-ish symbol (dingbats / misc symbols) used as emoji.
        [c] if (*c as u32) >= 0x2600 && !c.is_ascii() => true,
        _ => g.contains('\u{200D}') && g.chars().any(|c| (c as u32) >= 0x1F300),
    }
}

/// Parse SimpleX `formattedText` JSON array into display segments.
/// Returns empty when absent / unusable (caller falls back to plain text).
pub fn parse_formatted_text(value: Option<&Value>) -> Vec<TextSegment> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items.iter().filter_map(parse_one_segment).collect()
}

fn parse_one_segment(item: &Value) -> Option<TextSegment> {
    let raw = item.get("text")?.as_str()?;
    if raw.is_empty() {
        return None;
    }
    let format = item.get("format");
    let (style, link) = parse_format(format, raw);
    let text = match style {
        TextStyle::Uri => format
            .and_then(|f| f.get("showText"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(raw)
            .to_string(),
        _ => raw.to_string(),
    };
    Some(TextSegment { text, style, link })
}

fn parse_format(format: Option<&Value>, raw_text: &str) -> (TextStyle, Option<String>) {
    let Some(format) = format else {
        return (TextStyle::Plain, None);
    };
    if format.is_null() {
        return (TextStyle::Plain, None);
    }

    // Unit variants may be a bare string ("bold") or {"type":"bold"}.
    let ty = format
        .as_str()
        .or_else(|| format.get("type").and_then(|v| v.as_str()))
        .unwrap_or("");

    match ty {
        "bold" => (TextStyle::Bold, None),
        "italic" => (TextStyle::Italic, None),
        "strikeThrough" | "strike" => (TextStyle::Strike, None),
        "snippet" => (TextStyle::Snippet, None),
        "secret" => (TextStyle::Secret, None),
        "small" => (TextStyle::Small, None),
        "colored" => {
            let color = format
                .get("color")
                .and_then(|v| v.as_str())
                .and_then(FormatColor::parse)
                .unwrap_or(FormatColor::Blue);
            (TextStyle::Colored { color }, None)
        }
        "uri" => {
            let link = normalize_web_url(raw_text);
            (TextStyle::Uri, Some(link))
        }
        "hyperLink" => {
            let link = format
                .get("linkUri")
                .and_then(|v| v.as_str())
                .unwrap_or(raw_text)
                .to_string();
            (TextStyle::Uri, Some(link))
        }
        "simplexLink" => {
            let link = format
                .get("simplexUri")
                .and_then(|v| v.as_str())
                .unwrap_or(raw_text)
                .to_string();
            (TextStyle::Uri, Some(link))
        }
        "email" => (TextStyle::Email, Some(format!("mailto:{raw_text}"))),
        "phone" => {
            let digits: String = raw_text
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '+')
                .collect();
            (TextStyle::Phone, Some(format!("tel:{digits}")))
        }
        "mention" => (TextStyle::Mention, None),
        "command" => (TextStyle::Command, None),
        _ => (TextStyle::Plain, None),
    }
}

fn normalize_web_url(s: &str) -> String {
    if s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("simplex:")
        || s.starts_with("mailto:")
        || s.starts_with("tel:")
    {
        s.to_string()
    } else {
        format!("https://{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn short_emoji_detection() {
        assert!(is_short_emoji("🙂"));
        assert!(is_short_emoji("👍👍"));
        assert!(is_short_emoji("  🔥🎉  "));
        assert!(!is_short_emoji("hello"));
        assert!(!is_short_emoji("🙂 hi"));
        assert!(!is_short_emoji(""));
    }

    #[test]
    fn colon_shortcodes_expand() {
        assert_eq!(expand_emoji_shortcodes("hi :rocket:"), "hi 🚀");
        assert_eq!(expand_emoji_shortcodes(":+1:"), "👍");
        assert_eq!(expand_emoji_shortcodes(":not_a_real_emoji:"), ":not_a_real_emoji:");
        // `:b:` is a real gemoji shortcode (B button).
        assert!(expand_emoji_shortcodes("a:b:c").contains('🅱') || expand_emoji_shortcodes("a:b:c") == "a🅱️c");
        assert_eq!(expand_emoji_shortcodes("still :typing"), "still :typing");
        assert_eq!(expand_emoji_shortcodes("time 12:30 ok"), "time 12:30 ok");
    }

    #[test]
    fn shortcode_suggestions() {
        assert_eq!(pending_shortcode_query("hi :ca"), Some("ca"));
        assert_eq!(pending_shortcode_query("hi :cat:"), None);
        assert_eq!(pending_shortcode_query("hi"), None);
        let hits = suggest_emoji_shortcodes("ca", 20);
        let names: Vec<_> = hits.iter().map(|h| h.shortcode).collect();
        assert!(names.iter().any(|n| *n == "cat" || n.starts_with("cat")));
        assert!(names.iter().any(|n| n.starts_with("car")));
        let pumpkin = suggest_emoji_shortcodes("pumpkin", 5);
        assert!(
            pumpkin.iter().any(|h| h.glyph.contains('🎃')),
            "pumpkin should resolve via colloquial alias to jack_o_lantern, got {pumpkin:?}"
        );
    }

    #[test]
    fn parse_bold_and_uri() {
        let v = json!([
            {"format": {"type": "bold"}, "text": "Hi"},
            {"format": null, "text": " — "},
            {"format": "uri", "text": "example.com"}
        ]);
        let segs = parse_formatted_text(Some(&v));
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].style, TextStyle::Bold);
        assert_eq!(segs[2].style, TextStyle::Uri);
        assert_eq!(
            segs[2].link.as_deref(),
            Some("https://example.com")
        );
    }
}
