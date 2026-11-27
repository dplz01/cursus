//! Cached rounded profile avatars.
//!
//! Group history calls `profile_avatar` for many members on every `view()`.
//! Without a cache that re-decodes base64, resizes, and re-uploads GPU textures
//! on every mouse move (PointerMoved → view rebuild).
//!
//! The cache keeps the most recently used avatars within a memory budget.
//! It used to hold a fixed 192 and then drop everything: with more chats
//! than that, every chat-list rebuild decoded every avatar again.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{LazyLock, Mutex};

use iced::widget::image::Handle;

use crate::media;

/// Decoded RGBA kept around: ~1400 chat-list avatars (96 px) or ~5000
/// history ones (56 px).
const BUDGET_BYTES: usize = 48 * 1024 * 1024;

struct Entry {
    handle: Handle,
    bytes: usize,
    used: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<(u64, u32), Entry>,
    bytes: usize,
    clock: u64,
}

impl Cache {
    /// Drops the least recently used quarter.
    fn evict(&mut self) {
        let mut ages: Vec<u64> = self.entries.values().map(|e| e.used).collect();
        ages.sort_unstable();
        let drop = (ages.len() / 4).max(1);
        let cut = ages[drop - 1];
        self.entries.retain(|_, e| e.used > cut);
        self.bytes = self.entries.values().map(|e| e.bytes).sum();
    }
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(Cache::default()));

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
    {
        let mut cache = CACHE.lock().ok()?;
        cache.clock += 1;
        let now = cache.clock;
        if let Some(hit) = cache.entries.get_mut(&key) {
            hit.used = now;
            return Some(hit.handle.clone());
        }
    }
    // Decode outside the lock.
    let (w, h, pixels) = media::round_profile_avatar_rgba(data_url, size).ok()?;
    let bytes = pixels.len();
    // from_rgba skips PNG encode here and PNG decode inside iced.
    let handle = Handle::from_rgba(w, h, pixels);
    let mut cache = CACHE.lock().ok()?;
    if cache.bytes + bytes > BUDGET_BYTES && !cache.entries.is_empty() {
        cache.evict();
    }
    cache.clock += 1;
    let used = cache.clock;
    cache.bytes += bytes;
    if let Some(old) = cache.entries.insert(key, Entry { handle: handle.clone(), bytes, used }) {
        cache.bytes -= old.bytes;
    }
    Some(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eviction_keeps_recent_entries_and_the_budget() {
        let mut cache = Cache::default();
        for i in 0..100u64 {
            cache.entries.insert(
                (i, 96),
                Entry { handle: Handle::from_rgba(1, 1, vec![0u8; 4]), bytes: 10, used: i },
            );
            cache.bytes += 10;
        }
        cache.evict();
        assert_eq!(cache.entries.len(), 75);
        assert_eq!(cache.bytes, 750);
        assert!(!cache.entries.contains_key(&(0, 96)));
        assert!(cache.entries.contains_key(&(99, 96)));
    }
}
