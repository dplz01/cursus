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
