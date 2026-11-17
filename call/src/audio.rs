//! Call audio shared by every app: the format (48 kHz mono, 20 ms Opus
//! frames), the jitter buffer that orders incoming frames, and the device
//! interface each platform implements — Linux with its sound-server tools,
//! macOS with AVAudioEngine (both with echo cancellation).

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub const RATE: u32 = 48_000;
/// 20 ms of mono audio.
pub const FRAME: usize = 960;
/// Frames held before playout starts (absorbs network jitter).
const PREBUFFER: usize = 3;
/// More than this queued → drop the oldest (latency would only grow).
const MAX_QUEUED: usize = 12;
/// Consecutive missing frames concealed before falling back to silence.
const MAX_CONCEALED: u32 = 5;

/// A platform's microphone and speaker for a connected call.
///
/// `start` begins capture and playback: every 20 ms of (echo-cancelled)
/// microphone audio goes to `on_packet` as one Opus frame — silence while
/// `muted` — and playback takes frames from `jitter` ([`JitterBuffer::pop`],
/// Opus packet-loss concealment for [`Playout::Conceal`]).
pub trait AudioDevice: Send + Sync + 'static {
    fn start(
        &self,
        muted: Arc<AtomicBool>,
        jitter: Arc<Mutex<JitterBuffer>>,
        on_packet: Box<dyn FnMut(Vec<u8>) + Send>,
    ) -> Result<Box<dyn AudioStream>, String>;
}

/// A running call's audio; dropping it stops capture and playback.
pub trait AudioStream: Send {
    /// False when only the speaker could be opened.
    fn has_microphone(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Jitter buffer

/// Reorders incoming frames by RTP sequence number and paces playout.
#[derive(Default)]
pub struct JitterBuffer {
    frames: BTreeMap<u64, Vec<u8>>,
    /// Extended (wrap-free) sequence of the last pushed packet.
    last_ext: Option<u64>,
    /// Next extended sequence to play; `None` while (re)buffering.
    next: Option<u64>,
    concealed: u32,
}

pub enum Playout {
    Frame(Vec<u8>),
    /// A frame is missing: conceal it (Opus PLC).
    Conceal,
    Silence,
}

impl JitterBuffer {
    fn extend(&mut self, seq: u16) -> u64 {
        let ext = match self.last_ext {
            None => (1u64 << 16) + seq as u64,
            Some(last) => {
                let base = last & !0xFFFF;
                let candidates = [base.wrapping_sub(1 << 16), base, base + (1 << 16)];
                candidates
                    .into_iter()
                    .map(|b| b + seq as u64)
                    .min_by_key(|c| c.abs_diff(last))
                    .unwrap_or(base + seq as u64)
            }
        };
        if self.last_ext.is_none_or(|l| ext > l) {
            self.last_ext = Some(ext);
        }
        ext
    }

    pub fn push(&mut self, seq: u16, payload: Vec<u8>) {
        let ext = self.extend(seq);
        if self.next.is_some_and(|n| ext < n) {
            return; // too late
        }
        self.frames.insert(ext, payload);
        if self.frames.len() > MAX_QUEUED {
            // Catch up: keep only the newest PREBUFFER frames.
            while self.frames.len() > PREBUFFER {
                self.frames.pop_first();
            }
            self.next = self.frames.keys().next().copied();
        }
    }

    /// Tests: take every frame that is ready, without concealing gaps.
    #[cfg(test)]
    pub fn drain_ready(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let ready = match self.next {
                Some(n) => self.frames.contains_key(&n),
                None => self.frames.len() >= PREBUFFER,
            };
            if !ready {
                return out;
            }
            if let Playout::Frame(f) = self.pop() {
                out.push(f);
            }
        }
    }

    pub fn pop(&mut self) -> Playout {
        let Some(next) = self.next else {
            if self.frames.len() >= PREBUFFER {
                self.next = self.frames.keys().next().copied();
                return self.pop();
            }
            return Playout::Silence;
        };
        if let Some(frame) = self.frames.remove(&next) {
            self.next = Some(next + 1);
            self.concealed = 0;
            return Playout::Frame(frame);
        }
        self.next = Some(next + 1);
        self.concealed += 1;
        if self.frames.is_empty() && self.concealed > MAX_CONCEALED {
            // Stream paused or stalled: rebuffer before playing again.
            self.next = None;
            return Playout::Silence;
        }
        Playout::Conceal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(jb: &mut JitterBuffer) -> Option<u8> {
        match jb.pop() {
            Playout::Frame(f) => Some(f[0]),
            _ => None,
        }
    }

    #[test]
    fn prebuffers_then_plays_in_order() {
        let mut jb = JitterBuffer::default();
        jb.push(10, vec![10]);
        assert!(matches!(jb.pop(), Playout::Silence));
        jb.push(12, vec![12]);
        jb.push(11, vec![11]);
        assert_eq!(frame(&mut jb), Some(10));
        assert_eq!(frame(&mut jb), Some(11));
        assert_eq!(frame(&mut jb), Some(12));
    }

    #[test]
    fn conceals_gaps_and_drops_late() {
        let mut jb = JitterBuffer::default();
        for s in [1u16, 2, 4] {
            jb.push(s, vec![s as u8]);
        }
        assert_eq!(frame(&mut jb), Some(1));
        assert_eq!(frame(&mut jb), Some(2));
        assert!(matches!(jb.pop(), Playout::Conceal));
        jb.push(3, vec![3]); // late: already concealed
        assert_eq!(frame(&mut jb), Some(4));
    }

    #[test]
    fn survives_sequence_wrap() {
        let mut jb = JitterBuffer::default();
        for s in [65534u16, 65535, 0, 1] {
            jb.push(s, vec![s as u8]);
        }
        assert_eq!(frame(&mut jb), Some(254));
        assert_eq!(frame(&mut jb), Some(255));
        assert_eq!(frame(&mut jb), Some(0));
        assert_eq!(frame(&mut jb), Some(1));
    }

    #[test]
    fn caps_latency() {
        let mut jb = JitterBuffer::default();
        for s in 0..40u16 {
            jb.push(s, vec![s as u8]);
        }
        assert!(jb.frames.len() <= MAX_QUEUED);
        // Plays recent audio, not the backlog from the start.
        assert!(frame(&mut jb).unwrap() >= 25);
    }

    #[test]
    fn rebuffers_after_long_stall() {
        let mut jb = JitterBuffer::default();
        for s in 0..3u16 {
            jb.push(s, vec![0]);
        }
        for _ in 0..3 {
            assert!(matches!(jb.pop(), Playout::Frame(_)));
        }
        for _ in 0..MAX_CONCEALED {
            assert!(matches!(jb.pop(), Playout::Conceal));
        }
        assert!(matches!(jb.pop(), Playout::Silence));
        assert!(jb.next.is_none());
    }
}
