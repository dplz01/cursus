//! The open chat's messages, with a fingerprint of their content.
//!
//! The history view is cached and rebuilt only when what it shows changes.
//! Comparing meant hashing every message — text, segments, senders' profile
//! pictures — on every frame; the fingerprint is computed once per change
//! instead (when a history loads, or an [`edit`](History::edit) ends).

use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};

use cursus_core::ChatMessage;

#[derive(Default)]
pub(crate) struct History {
    messages: Vec<ChatMessage>,
    fingerprint: u64,
}

impl History {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Hash of every message; equal fingerprints mean equal histories.
    pub(crate) fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    #[cfg(test)]
    pub(crate) fn push(&mut self, message: ChatMessage) {
        let mut messages = std::mem::take(&mut self.messages);
        messages.push(message);
        *self = messages.into();
    }

    /// Change messages in place (delivery status, transfer progress); the
    /// fingerprint follows when the returned guard is dropped.
    pub(crate) fn edit(&mut self) -> HistoryEdit<'_> {
        HistoryEdit(self)
    }
}

impl From<Vec<ChatMessage>> for History {
    fn from(messages: Vec<ChatMessage>) -> Self {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        messages.hash(&mut h);
        Self { fingerprint: h.finish(), messages }
    }
}

impl Deref for History {
    type Target = [ChatMessage];

    fn deref(&self) -> &[ChatMessage] {
        &self.messages
    }
}

pub(crate) struct HistoryEdit<'a>(&'a mut History);

impl Deref for HistoryEdit<'_> {
    type Target = [ChatMessage];

    fn deref(&self) -> &[ChatMessage] {
        &self.0.messages
    }
}

impl DerefMut for HistoryEdit<'_> {
    fn deref_mut(&mut self) -> &mut [ChatMessage] {
        &mut self.0.messages
    }
}

impl Drop for HistoryEdit<'_> {
    fn drop(&mut self) {
        *self.0 = History::from(std::mem::take(&mut self.0.messages));
    }
}
