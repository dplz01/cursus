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

/// What a SimpleX link connects to (official `SimplexLinkType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimplexLinkKind {
    Invitation,
    Address,
    Group,
    Channel,
    Relay,
}

impl SimplexLinkKind {
    fn from_link_type(raw: &str) -> Option<Self> {
        Some(match raw {
            "invitation" => Self::Invitation,
            "contact" => Self::Address,
            "group" => Self::Group,
            "channel" => Self::Channel,
            "relay" => Self::Relay,
            _ => return None,
        })
    }

    /// Classify a raw link: `simplex:/…` full links and `https://<host>/<t>#…`
    /// short links (`i` invitation, `a` address, `g` group, `c` channel, `r` relay).
    /// Returns `None` for ordinary web URLs.
    pub fn from_uri(uri: &str) -> Option<Self> {
        let uri = uri.trim();
        if let Some(rest) = uri.strip_prefix("simplex:") {
            let rest = rest.trim_start_matches('/');
            let lower = rest.to_ascii_lowercase();
            if lower.starts_with("invitation") {
                return Some(Self::Invitation);
            }
            if lower.starts_with("contact") {
                // Group links are contact links carrying a groupLinkId.
                return Some(if lower.contains("grouplinkid") {
                    Self::Group
                } else {
                    Self::Address
                });
            }
            return short_link_kind(rest);
        }
        let rest = uri.strip_prefix("https://")?;
        let (host, path) = rest.split_once('/')?;
        if host.is_empty() || !path.contains('#') {
            return None;
        }
        // Only trust the short-link shape on SimpleX-looking hosts; the full
        // https form (`/invitation#/?v=…`, `/contact#/?v=…`) is unambiguous.
        let lower = path.to_ascii_lowercase();
        if lower.starts_with("invitation#") {
            return Some(Self::Invitation);
        }
        if lower.starts_with("contact#") {
            return Some(if lower.contains("grouplinkid") {
                Self::Group
            } else {
                Self::Address
            });
        }
        if host.contains("simplex") {
            return short_link_kind(path);
        }
        None
    }

    /// Official wording ("SimpleX one-time invitation", …).
    pub fn description(self) -> &'static str {
        match self {
            Self::Invitation => "SimpleX one-time invitation",
            Self::Address => "SimpleX contact address",
            Self::Group => "SimpleX group link",
            Self::Channel => "SimpleX channel link",
            Self::Relay => "SimpleX relay link",
        }
    }

    /// Question for the join confirmation sheet.
    pub fn join_prompt(self) -> &'static str {
        match self {
            Self::Invitation => "Connect via one-time link?",
            Self::Address => "Connect via contact address?",
            Self::Group => "Join group?",
            Self::Channel => "Join channel?",
            Self::Relay => "Connect to relay?",
        }
    }

    pub fn join_action(self) -> &'static str {
        match self {
            Self::Group | Self::Channel => "Join",
            _ => "Connect",
        }
    }
}

fn short_link_kind(path: &str) -> Option<SimplexLinkKind> {
    let (head, frag) = path.split_once('#')?;
    if frag.is_empty() {
        return None;
    }
    Some(match head.trim_matches('/') {
        "i" => SimplexLinkKind::Invitation,
        "a" => SimplexLinkKind::Address,
        "g" => SimplexLinkKind::Group,
        "c" => SimplexLinkKind::Channel,
        "r" => SimplexLinkKind::Relay,
        _ => return None,
    })
}

/// Hostname of a link (`smp.simplex.im`), for "via …" captions.
pub fn link_host(uri: &str) -> Option<String> {
    let rest = uri.split_once("://").map(|(_, r)| r)?;
    let host = rest.split(['/', '#', '?']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_string())
}

/// Human-readable label for a SimpleX link: "SimpleX group link (via smp.simplex.im)".
pub fn simplex_link_label(kind: SimplexLinkKind, hosts: &[String]) -> String {
    if hosts.is_empty() {
        kind.description().to_string()
    } else {
        format!("{} (via {})", kind.description(), hosts.join(", "))
    }
}

/// Replace SimpleX link tokens in plain text with readable labels, for
/// one-line previews (chat list, quotes) that have no `formattedText`.
pub fn humanize_simplex_links(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains("simplex") && !s.contains('#') {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut changed = false;
    let mut last = 0usize;
    for (start, token) in s
        .split_whitespace()
        .map(|t| (t.as_ptr() as usize - s.as_ptr() as usize, t))
    {
        let trimmed = token.trim_end_matches(['.', ',', ')', '!', '?', ';']);
        if let Some(kind) = SimplexLinkKind::from_uri(trimmed) {
            out.push_str(&s[last..start]);
            let hosts: Vec<String> = link_host(trimmed)
                .filter(|_| !trimmed.starts_with("simplex:"))
                .into_iter()
                .collect();
            out.push_str(&simplex_link_label(kind, &hosts));
            last = start + trimmed.len();
            changed = true;
        }
    }
    if !changed {
        return std::borrow::Cow::Borrowed(s);
    }
    out.push_str(&s[last..]);
    std::borrow::Cow::Owned(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSegment {
    pub text: String,
    pub style: TextStyle,
    /// Absolute URL / URI to open when the segment is a link.
    pub link: Option<String>,
    /// Set when `link` is a SimpleX connection link (opens the join sheet).
    pub simplex: Option<SimplexLinkKind>,
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

/// Everyday emoji, most used first (gemoji shortcodes). Within a match tier
/// these outrank obscure ones so `:he` offers ❤️ before 🌿 herb.
const POPULAR: &[&str] = &[
    "joy", "heart", "rofl", "+1", "sob", "pray", "heart_eyes", "kissing_heart",
    "smile", "blush", "wink", "fire", "sparkles", "tada", "100", "ok_hand", "clap",
    "raised_hands", "thinking", "slightly_smiling_face", "smiley", "grin", "laughing",
    "sweat_smile", "sunglasses", "star_struck", "partying_face", "hugs", "relieved",
    "pleading_face", "face_holding_back_tears", "pensive", "cry", "sleeping", "rage",
    "angry", "scream", "flushed", "upside_down_face", "wave", "muscle", "eyes", "skull",
    "see_no_evil", "white_check_mark", "x", "warning", "rocket", "star", "sunny",
    "rainbow", "coffee", "beer", "beers", "pizza", "birthday", "gift", "balloon",
    "broken_heart", "two_hearts", "sparkling_heart", "purple_heart", "blue_heart",
    "green_heart", "yellow_heart", "orange_heart", "black_heart", "white_heart",
    "heart_hands", "-1", "v", "point_up", "point_right", "facepalm", "shrug",
    "grimacing", "neutral_face", "expressionless", "unamused", "roll_eyes", "smirk",
    "yum", "stuck_out_tongue_winking_eye", "zany_face", "melting_face",
    "saluting_face", "hot_face", "cold_face", "nauseated_face", "hankey",
    "kissing", "innocent", "nerd_face", "money_mouth_face", "zipper_mouth_face",
    "sleepy", "yawning_face", "dizzy_face", "exploding_head", "cowboy_hat_face",
    "clown_face", "ghost", "alien", "robot", "cat", "dog", "unicorn", "tulip", "rose",
    "sunflower", "cherry_blossom", "four_leaf_clover", "snowflake", "zap", "moon",
    "earth_africa", "christmas_tree", "jack_o_lantern", "champagne", "wine_glass",
    "cocktail", "hamburger", "fries", "taco", "sushi", "cookie", "doughnut", "apple",
    "avocado", "soccer", "basketball", "trophy", "medal_sports", "musical_note",
    "headphones", "camera", "phone", "computer", "bulb", "moneybag", "books",
    "pencil2", "lock", "key", "hourglass", "alarm_clock", "calendar", "pushpin",
    "airplane", "car", "house", "beach_umbrella", "world_map", "handshake",
    "crossed_fingers", "fist", "call_me_hand", "writing_hand", "nail_care",
    "kiss", "speech_balloon", "zzz", "boom", "dizzy", "question", "exclamation",
    "heavy_check_mark", "heavy_plus_sign", "recycle", "no_entry", "stop_sign",
];

fn popularity(shortcode: &str) -> usize {
    POPULAR
        .iter()
        .position(|p| *p == shortcode)
        .unwrap_or(POPULAR.len())
}

/// Emoji that read as plain text or UI chrome rather than a picture: letters
/// and number blocks, keycaps, arrows, geometric squares, ©®™ etc. They are
/// still rendered if typed, just never suggested.
fn is_dull_emoji(glyph: &str) -> bool {
    glyph.chars().any(|c| {
        let u = c as u32;
        c.is_ascii_alphanumeric()
            || matches!(c, '#' | '*' | '\u{20E3}')
            || matches!(u, 0x00A9 | 0x00AE | 0x2122 | 0x2139 | 0x203C | 0x2049 | 0x24C2)
            || (0x2190..=0x21FF).contains(&u)
            || matches!(u, 0x27A1 | 0x2934 | 0x2935 | 0x2B05..=0x2B07 | 0x2B1B | 0x2B1C)
            || (0x1F500..=0x1F504).contains(&u) // 🔀 🔁 🔂 🔃 🔄
            || (0x25AA..=0x25FE).contains(&u)
            || matches!(u, 0x3030 | 0x303D | 0x3297 | 0x3299)
            || (0x1F170..=0x1F19A).contains(&u) // 🅰 🆎 🆑 … squared letters
            || (0x1F201..=0x1F251).contains(&u) // 🈁 🉐 … squared CJK
            || (0x1F520..=0x1F524).contains(&u) // 🔠 🔡 🔢 🔣 🔤
            || (0x1F532..=0x1F53D).contains(&u) // 🔲 🔳 🔴 … buttons/triangles
            || (0x1F7E0..=0x1F7EB).contains(&u) // 🟠 🟦 coloured circles/squares
    })
}

/// How well `prefix` matches `shortcode`: 0 exact, 1 prefix, 2 word prefix
/// (`smil` → `slightly_smiling_face`), 4 substring (≥3 chars). `None` = no match.
fn shortcode_tier(shortcode: &str, prefix: &str) -> Option<u8> {
    if shortcode == prefix {
        Some(0)
    } else if shortcode.starts_with(prefix) {
        Some(1)
    } else if shortcode
        .split(['_', '-'])
        .skip(1)
        .any(|w| w.starts_with(prefix))
    {
        Some(2)
    } else if prefix.len() >= 3 && shortcode.contains(prefix) {
        Some(4)
    } else {
        None
    }
}

/// Suggest shortcodes for an incomplete `:prefix`.
///
/// Tiers: exact shortcode / alias, shortcode prefix, word prefix inside the
/// shortcode, word prefix in the CLDR name, substring. Inside a tier everyday
/// emoji come first, then shorter shortcodes. One entry per glyph; flags only
/// on prefix matches; text-like symbols ([`is_dull_emoji`]) are skipped.
pub fn suggest_emoji_shortcodes(prefix: &str, limit: usize) -> Vec<EmojiSuggestion> {
    if prefix.is_empty() || limit == 0 {
        return Vec::new();
    }
    let prefix = prefix.to_ascii_lowercase();
    // (tier, popularity, presentation, shortcode len, suggestion)
    let mut scored: Vec<(u8, usize, u8, usize, EmojiSuggestion)> = Vec::new();
    let mut push = |tier: u8, emoji: &'static emojis::Emoji, shortcode: &'static str| {
        let glyph = emoji.as_str();
        if is_dull_emoji(glyph) {
            return;
        }
        let pop = emoji.shortcodes().map(popularity).min().unwrap_or(POPULAR.len());
        scored.push((
            tier,
            pop,
            emoji_presentation_rank(glyph),
            shortcode.len(),
            EmojiSuggestion { shortcode, glyph },
        ));
    };

    for &(alias, target) in COLLOQUIAL_ALIASES {
        if let (Some(tier), Some(emoji)) =
            (shortcode_tier(alias, &prefix), emojis::get_by_shortcode(target))
        {
            push(tier, emoji, emoji.shortcode().unwrap_or(target));
        }
    }

    for emoji in emojis::iter().filter(|e| {
        matches!(e.skin_tone(), Some(emojis::SkinTone::Default) | None)
    }) {
        let flag = emoji.group() == emojis::Group::Flags;
        let mut best: Option<(u8, &'static str)> = None;
        for sc in emoji.shortcodes() {
            if let Some(tier) = shortcode_tier(sc, &prefix) {
                if flag && tier > 1 {
                    continue;
                }
                if best.is_none_or(|(t, b)| tier < t || (tier == t && sc.len() < b.len())) {
                    best = Some((tier, sc));
                }
            }
        }
        if best.is_none() && !flag && prefix.len() >= 2 {
            // CLDR name words ("grinning face with big eyes").
            let name = emoji.name().to_ascii_lowercase();
            if name.split([' ', '-']).any(|w| w.starts_with(&prefix)) {
                if let Some(sc) = emoji.shortcode() {
                    best = Some((3, sc));
                }
            }
        }
        if let Some((tier, sc)) = best {
            push(tier, emoji, sc);
        }
    }

    scored.sort_by(|a, b| {
        (a.0, a.1, a.2, a.3, a.4.shortcode).cmp(&(b.0, b.1, b.2, b.3, b.4.shortcode))
    });
    let mut seen = std::collections::HashSet::new();
    scored
        .into_iter()
        .filter(|s| seen.insert(s.4.glyph))
        .take(limit)
        .map(|s| s.4)
        .collect()
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

/// Add the emoji variation selector (U+FE0F) to emoji that default to text
/// presentation (❤ ☺ ☀ ✌ …) so every client — not just ours — draws them in
/// colour. Letter/arrow-like symbols, skin-tone and keycap sequences are left
/// alone.
pub fn emoji_presentation(s: &str) -> std::borrow::Cow<'_, str> {
    let needs = |c: char| {
        let mut b = [0u8; 4];
        let base: &str = c.encode_utf8(&mut b);
        !c.is_ascii()
            && emojis::get(&format!("{base}\u{FE0F}")).is_some_and(|e| e.as_str().contains('\u{FE0F}'))
            && !is_dull_emoji(base)
            && !emoji_default_presentation(c)
    };
    let chars: Vec<char> = s.chars().collect();
    let fix_at = |i: usize| {
        needs(chars[i])
            && !matches!(
                chars.get(i + 1),
                Some('\u{FE0F}' | '\u{FE0E}' | '\u{20E3}' | '\u{1F3FB}'..='\u{1F3FF}')
            )
    };
    if !(0..chars.len()).any(fix_at) {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        out.push(c);
        if fix_at(i) {
            out.push('\u{FE0F}');
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Code points that are already pictographic without a selector (😄 🚀 ⭐ ⚡).
fn emoji_default_presentation(c: char) -> bool {
    let mut b = [0u8; 4];
    emojis::get(c.encode_utf8(&mut b)).is_some_and(|e| e.as_str().chars().count() == 1)
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
                if let Some(emoji) = resolved.filter(|e| !is_dull_emoji(e.as_str())) {
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
    // Runs for every message on every frame: bail on the first non-emoji
    // grapheme or the (MAX+1)th one instead of segmenting the whole text.
    let mut n = 0;
    for g in s.graphemes(true) {
        n += 1;
        if n > SHORT_EMOJI_MAX || !is_emoji_grapheme(g) {
            return false;
        }
    }
    n > 0
}

fn is_emoji_grapheme(g: &str) -> bool {
    if emojis::get(g).is_some() {
        return true;
    }
    // Plain letters/digits/punctuation are never emoji on their own.
    if g.len() == 1 {
        return false;
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
    let show_text = format
        .and_then(|f| f.get("showText"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let ty = format
        .and_then(|f| f.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut simplex = None;
    let text = if ty == "simplexLink" {
        let kind = format
            .and_then(|f| f.get("linkType"))
            .and_then(|v| v.as_str())
            .and_then(SimplexLinkKind::from_link_type)
            .or_else(|| link.as_deref().and_then(SimplexLinkKind::from_uri))
            .unwrap_or(SimplexLinkKind::Address);
        simplex = Some(kind);
        match show_text {
            Some(t) => t.to_string(),
            None => {
                let mut hosts: Vec<String> = format
                    .and_then(|f| f.get("smpHosts"))
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|h| h.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                if hosts.is_empty() {
                    hosts.extend(link.as_deref().and_then(link_host));
                }
                simplex_link_label(kind, &hosts)
            }
        }
    } else if style == TextStyle::Uri {
        // A bare `simplex:` / short link the core tagged as a plain URI.
        simplex = link.as_deref().and_then(SimplexLinkKind::from_uri);
        match (show_text, simplex) {
            (Some(t), _) => t.to_string(),
            (None, Some(kind)) if raw.starts_with("simplex:") => {
                simplex_link_label(kind, &[])
            }
            _ => raw.to_string(),
        }
    } else {
        raw.to_string()
    };
    Some(TextSegment {
        text,
        style,
        link,
        simplex,
    })
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
                .filter(|s| !s.is_empty())
                .unwrap_or(raw_text)
                .trim()
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
        // Letter-block emoji are never produced from ordinary text.
        assert_eq!(expand_emoji_shortcodes("a:b:c"), "a:b:c");
        assert_eq!(expand_emoji_shortcodes("ratio 1:m:2 :heart:"), "ratio 1:m:2 ❤️");
        assert_eq!(expand_emoji_shortcodes("still :typing"), "still :typing");
        assert_eq!(expand_emoji_shortcodes("time 12:30 ok"), "time 12:30 ok");
        // Text-style emoji gain the colour selector; others stay untouched.
        assert_eq!(emoji_presentation("I \u{2764} it \u{263A}"), "I \u{2764}\u{FE0F} it \u{263A}\u{FE0F}");
        assert_eq!(emoji_presentation("\u{2764}\u{FE0F} 😄 ⭐"), "\u{2764}\u{FE0F} 😄 ⭐");
        assert_eq!(emoji_presentation("\u{270C}\u{1F3FD} ok"), "\u{270C}\u{1F3FD} ok");
        assert_eq!(emoji_presentation("a → b © 2026 #1 ™"), "a → b © 2026 #1 ™");
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
        let first = |p: &str| suggest_emoji_shortcodes(p, 8)[0].shortcode;
        assert_eq!(first("he"), "heart");
        assert_eq!(first("ok"), "ok_hand");
        assert_eq!(first("th"), "+1");
        let th = suggest_emoji_shortcodes("th", 8);
        let mut glyphs: Vec<_> = th.iter().map(|h| h.glyph).collect();
        glyphs.dedup();
        assert_eq!(glyphs.len(), th.len(), "one entry per emoji: {th:?}");
        assert!(suggest_emoji_shortcodes("a", 20).iter().all(|h| !h.glyph.contains('🔤')));
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

    #[test]
    fn simplex_links_get_readable_labels() {
        let v = json!([
            {"format": null, "text": "join "},
            {"format": {"type": "simplexLink", "linkType": "group",
                        "simplexUri": "https://smp9.simplex.im/g#abc", "smpHosts": ["smp9.simplex.im"]},
             "text": "https://smp9.simplex.im/g#abc"},
            {"format": {"type": "simplexLink", "linkType": "invitation", "showText": "my link",
                        "simplexUri": "simplex:/invitation#/?v=2-7&smp=x", "smpHosts": ["a", "b"]},
             "text": "[my link](simplex:/invitation#/?v=2-7&smp=x)"}
        ]);
        let segs = parse_formatted_text(Some(&v));
        assert_eq!(segs[0].simplex, None);
        assert_eq!(segs[1].text, "SimpleX group link (via smp9.simplex.im)");
        assert_eq!(segs[1].simplex, Some(SimplexLinkKind::Group));
        assert_eq!(segs[1].link.as_deref(), Some("https://smp9.simplex.im/g#abc"));
        assert_eq!(segs[2].text, "my link");
        assert_eq!(segs[2].simplex, Some(SimplexLinkKind::Invitation));
    }

    #[test]
    fn classify_simplex_uris() {
        use SimplexLinkKind::*;
        assert_eq!(SimplexLinkKind::from_uri("https://smp.simplex.im/a#xyz"), Some(Address));
        assert_eq!(SimplexLinkKind::from_uri("https://smp.simplex.im/c#xyz"), Some(Channel));
        assert_eq!(SimplexLinkKind::from_uri("https://simplex.chat/invitation#/?v=2"), Some(Invitation));
        assert_eq!(
            SimplexLinkKind::from_uri("simplex:/contact#/?v=2&data=%7B%22groupLinkId%22"),
            Some(Group)
        );
        assert_eq!(SimplexLinkKind::from_uri("simplex:/contact#/?v=2"), Some(Address));
        assert_eq!(SimplexLinkKind::from_uri("https://example.com/a#top"), None);
        assert_eq!(SimplexLinkKind::from_uri("https://example.com/page"), None);
        assert_eq!(link_host("https://smp.simplex.im/g#x").as_deref(), Some("smp.simplex.im"));
    }

    #[test]
    fn humanize_previews() {
        assert_eq!(
            humanize_simplex_links("join https://smp9.simplex.im/g#abc now"),
            "join SimpleX group link (via smp9.simplex.im) now"
        );
        assert_eq!(humanize_simplex_links("see https://example.com/#x"), "see https://example.com/#x");
        assert_eq!(humanize_simplex_links("plain"), "plain");
    }
}
