//! Opening what other people send: links and attachments are
//! attacker-controlled, so every Cursus app checks them the same way before
//! handing them to the system.

/// Links from messages are attacker-controlled (markdown `[text](uri)` can
/// carry any scheme). Only web, mail and phone links may reach the system's URL handler;
/// `file:`, `smb:`, `javascript:`, app protocols (`vscode:`, `steam:`…) etc.
/// would hand the URL to whatever local handler is registered.
/// `mailto:` keeps only the address — query fields like `?attach=` let some
/// mail clients attach local files.
pub fn safe_external_url(raw: &str) -> Option<String> {
    let url = raw.trim();
    if url.is_empty() || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let (scheme, rest) = url.split_once(':')?;
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" if rest.starts_with("//") && rest.len() > 2 => Some(url.to_string()),
        "mailto" => {
            let addr = rest.split(['?', '#']).next().unwrap_or("");
            (addr.contains('@') && !addr.starts_with('-')).then(|| format!("mailto:{addr}"))
        }
        "tel" => {
            let digits: String = rest.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
            (!digits.is_empty()).then(|| format!("tel:{digits}"))
        }
        _ => None,
    }
}

/// Drop well-known tracking parameters (`utm_*`, `fbclid`, `gclid`, …) from
/// a web link before opening it — the official app offers the same "clean
/// link". The rest of the URL, including its fragment, is kept as-is.
pub fn strip_tracking_params(url: &str) -> String {
    const EXACT: &[&str] = &[
        "fbclid", "gclid", "dclid", "gbraid", "wbraid", "msclkid", "mc_eid", "mc_cid",
        "igshid", "igsh", "yclid", "twclid", "ttclid", "_hsenc", "_hsmi", "mkt_tok", "oly_enc_id",
        "oly_anon_id", "vero_id", "rb_clickid", "s_cid", "ref_src", "ref_url",
    ];
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return url.to_string();
    }
    let (before_frag, frag) = match url.split_once('#') {
        Some((a, f)) => (a, Some(f)),
        None => (url, None),
    };
    let Some((base, query)) = before_frag.split_once('?') else {
        return url.to_string();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .filter(|kv| {
            let key = kv.split('=').next().unwrap_or("").to_ascii_lowercase();
            !(key.starts_with("utm_") || EXACT.contains(&key.as_str()))
        })
        .collect();
    let mut out = base.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    if let Some(f) = frag {
        out.push('#');
        out.push_str(f);
    }
    out
}

/// File types whose default handler may *run* them (Wine `.exe`, Java
/// `.jar`, `.desktop` launchers, macOS apps, scripts, installers). The name is chosen by
/// the sender, so these are never opened with a tap — Save as… still works.
pub fn is_risky_to_open(path: &std::path::Path) -> bool {
    const RISKY: &[&str] = &[
        "exe", "msi", "bat", "cmd", "com", "scr", "pif", "cpl", "lnk", "reg", "vbs", "vbe",
        "js", "jse", "wsf", "ps1", "jar", "desktop", "sh", "bash", "zsh", "fish", "csh", "ksh",
        "command", "py", "pyw", "pl", "rb", "php", "lua", "appimage", "run", "bin", "elf", "so",
        "deb", "rpm", "apk", "flatpak", "flatpakref", "flatpakrepo", "snap", "xpi", "crx",
        "application", "msix", "dmg", "pkg", "mpkg", "app", "tool", "terminal", "scpt",
        "applescript", "workflow", "prefpane", "kext",
    ];
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| RISKY.contains(&e.as_str()))
}

/// Every web link in a message with its tracking parameters removed (see
/// [`strip_tracking_params`]): run on what you **send**, so codes like
/// `utm_source` never reach your contact — cleaning only when opening
/// protects you, not them. Text around the links is untouched.
pub fn clean_links_in_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = next_link_start(rest) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail.find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`')).unwrap_or(tail.len());
        // Trailing punctuation belongs to the sentence, not the link.
        let link_end = tail[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '\'']).len();
        let link = &tail[..link_end];
        out.push_str(&clean_one(link));
        out.push_str(&tail[link_end..end]);
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Byte offset of the next `http(s)://` or `www.` link starting a word.
fn next_link_start(s: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    let mut from = 0;
    while from < lower.len() {
        let hit = ["https://", "http://", "www."]
            .iter()
            .filter_map(|p| lower[from..].find(p).map(|i| from + i))
            .min()?;
        let at_word_start = hit == 0
            || s[..hit].chars().next_back().is_some_and(|c| c.is_whitespace() || matches!(c, '(' | '[' | '<' | '"' | '\''));
        if at_word_start {
            return Some(hit);
        }
        from = hit + 1;
        while !lower.is_char_boundary(from) {
            from += 1;
        }
    }
    None
}

fn clean_one(link: &str) -> String {
    if link.len() >= 4 && link[..4].eq_ignore_ascii_case("www.") {
        let cleaned = strip_tracking_params(&format!("https://{link}"));
        return cleaned.strip_prefix("https://").unwrap_or(&cleaned).to_string();
    }
    strip_tracking_params(link)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_mail_and_phone_links_open() {
        assert_eq!(safe_external_url("https://simplex.chat/docs").as_deref(), Some("https://simplex.chat/docs"));
        assert!(safe_external_url("http://example.com").is_some());
        for bad in [
            "file:///etc/passwd",
            "smb://evil/share",
            "javascript:alert(1)",
            "vscode://cmd",
            "steam://run/1",
            "data:text/html,x",
            "https://a.b/\nfile:///x",
            "--help",
            "https:no-slashes",
        ] {
            assert!(safe_external_url(bad).is_none(), "{bad}");
        }
        assert_eq!(
            safe_external_url("mailto:me@x.org?attach=/home/me/.ssh/id_rsa").as_deref(),
            Some("mailto:me@x.org")
        );
        assert_eq!(safe_external_url("tel:+39 123").as_deref(), None);
        assert_eq!(safe_external_url("tel:+39123").as_deref(), Some("tel:+39123"));
    }

    #[test]
    fn links_in_sent_text_are_cleaned() {
        assert_eq!(
            clean_links_in_text(
                "look https://www.example.com/page?utm_content=x&utm_medium=social&utm_source=snapchat.com&utm_campaign=buffer now"
            ),
            "look https://www.example.com/page now"
        );
        assert_eq!(
            clean_links_in_text("(see www.shop.it/a?id=3&fbclid=Z), ok?"),
            "(see www.shop.it/a?id=3), ok?"
        );
        assert_eq!(clean_links_in_text("two: http://a.b/?gclid=1 and https://c.d/x?q=1"), "two: http://a.b/ and https://c.d/x?q=1");
        assert_eq!(clean_links_in_text("no links, utm_source=x stays"), "no links, utm_source=x stays");
        assert_eq!(clean_links_in_text("emoji 😄 https://a.b/?utm_source=x 😄"), "emoji 😄 https://a.b/ 😄");
        assert_eq!(clean_links_in_text("mid-wordhttps://a.b/?utm_source=x"), "mid-wordhttps://a.b/?utm_source=x");
    }

    #[test]
    fn tracking_parameters_are_removed() {
        use super::strip_tracking_params as clean;
        assert_eq!(
            clean("https://shop.example/item?id=7&utm_source=nl&utm_medium=mail&fbclid=XYZ#top"),
            "https://shop.example/item?id=7#top"
        );
        assert_eq!(clean("https://a.b/?gclid=1"), "https://a.b/");
        assert_eq!(clean("https://a.b/search?q=utm_source"), "https://a.b/search?q=utm_source");
        assert_eq!(clean("mailto:x@y.z"), "mailto:x@y.z");
    }

    #[test]
    fn executable_attachments_are_not_opened() {
        use std::path::Path;
        assert!(super::is_risky_to_open(Path::new("/x/holiday.jpg.EXE")));
        assert!(super::is_risky_to_open(Path::new("/x/run.desktop")));
        assert!(super::is_risky_to_open(Path::new("/x/tool.AppImage")));
        assert!(!super::is_risky_to_open(Path::new("/x/photo.jpg")));
        assert!(!super::is_risky_to_open(Path::new("/x/notes.pdf")));
        assert!(super::is_risky_to_open(Path::new("/x/Free.app")));
    }
}
