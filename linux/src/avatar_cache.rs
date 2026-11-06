//! Cached rounded profile avatars.
//!
//! Group history calls `profile_avatar` for many members on every `view()`.
//! Without a cache that re-decodes base64, resizes, and re-uploads GPU textures
//! on every mouse move (PointerMoved → view rebuild).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{LazyLock, Mutex};

use iced::widget::image::Handle;

use crate::media;

const MAX_ENTRIES: usize = 192;

static CACHE: LazyLock<Mutex<HashMap<(u64, u32), Handle>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Cache key for an avatar data URL. Runs for every visible avatar on every
/// frame, so it samples instead of hashing all ~12 KB of base64: the length
/// plus the head, middle and tail windows. Distinct images differ in their
/// compressed payload, so equal length *and* identical windows don't happen in
/// practice (~20x cheaper than hashing the whole string).
fn url_key(url: &str) -> u64 {
    const WINDOW: usize = 256;
    let bytes = url.as_bytes();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.len().hash(&mut hasher);
    if bytes.len() <= WINDOW * 3 {
        bytes.hash(&mut hasher);
    } else {
        let mid = bytes.len() / 2 - WINDOW / 2;
        bytes[..WINDOW].hash(&mut hasher);
        bytes[mid..mid + WINDOW].hash(&mut hasher);
        bytes[bytes.len() - WINDOW..].hash(&mut hasher);
    }
    hasher.finish()
}

/// Rounded avatar as a stable iced [`Handle`] (same id on cache hit).
pub fn handle(data_url: &str, size_px: u32) -> Option<Handle> {
    let size = size_px.max(16);
    let key = (url_key(data_url), size);
    let mut cache = CACHE.lock().ok()?;
    if let Some(hit) = cache.get(&key) {
        return Some(hit.clone());
    }
    let (w, h, pixels) = media::round_profile_avatar_rgba(data_url, size).ok()?;
    // from_rgba skips PNG encode here and PNG decode inside iced.
    let handle = Handle::from_rgba(w, h, pixels);
    if cache.len() >= MAX_ENTRIES {
        cache.clear();
    }
    cache.insert(key, handle.clone());
    Some(handle)
}
