//! Stand-ins for picture bubbles while the file isn't ready yet.
//!
//! A SimpleX image message carries a small preview of the photo. Built off
//! the UI thread while the history loads ([`warm`]), it gives the bubble its
//! final size from the first frame: without it, a picture showed as a file
//! row, then a 160×120 box, then the real preview, and the history jumped
//! at each step.
use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};

use cursus_core::{ChatMessage, FileKind};
use iced::widget::image;

/// Chat-pane picture frame, the same as the decrypted file's preview.
pub(crate) const MAX_W: u32 = 220;
pub(crate) const MAX_H: u32 = 280;
pub(crate) const RADIUS: f32 = 12.0;

/// About 30 MB of RGBA at most.
const KEEP: usize = 120;

#[derive(Default)]
struct Cache {
    by_file: HashMap<i64, (image::Handle, u32, u32)>,
    order: VecDeque<i64>,
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);

/// The stand-in for this file, if its message embedded one.
pub(crate) fn get(file_id: i64) -> Option<(image::Handle, u32, u32)> {
    CACHE.lock().ok()?.by_file.get(&file_id).cloned()
}

/// Build the stand-ins for these messages' pictures (blocking: run it off
/// the UI thread). The newest come last and win when the cache is full.
pub(crate) fn warm(messages: &[ChatMessage]) {
    for msg in messages {
        let Some(file) = msg.file.as_ref() else { continue };
        let Some(url) = file.preview.as_deref() else { continue };
        let is_image = matches!(file.kind, FileKind::Image) || crate::media::is_image_name(&file.file_name);
        if !is_image || CACHE.lock().is_ok_and(|c| c.by_file.contains_key(&file.file_id)) {
            continue;
        }
        let Some((w, h, rgba)) = crate::media::embedded_preview_rgba(url, MAX_W, MAX_H, RADIUS) else {
            continue;
        };
        let handle = image::Handle::from_rgba(w, h, rgba);
        let Ok(mut cache) = CACHE.lock() else { return };
        cache.by_file.insert(file.file_id, (handle, w, h));
        cache.order.push_back(file.file_id);
        while cache.order.len() > KEEP {
            if let Some(old) = cache.order.pop_front() {
                cache.by_file.remove(&old);
            }
        }
    }
}

/// Forget them all (app lock: nothing of the chats stays in memory).
pub(crate) fn clear() {
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Cache::default();
    }
}
