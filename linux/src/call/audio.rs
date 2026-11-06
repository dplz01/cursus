//! Call audio: microphone → Opus frames, Opus frames → speaker, call tones.
//!
//! Capture and playback go through the same sound-server tools Cursus already
//! uses for voice messages (`pw-record` / `pw-play`, `parecord` / `pacat`,
//! ALSA `arecord` / `aplay`), streaming raw 48 kHz mono s16le over pipes. On
//! phones the mic/speaker routing lives in the sound server's UCM profile, so
//! server-native tools are the most reliable path, and they need nothing extra
//! inside the Flatpak sandbox. Streams carry `media.role = Communication` so
//! the policy manager can route them like a phone call (earpiece, echo
//! cancellation when the system has it configured).
//!
//! Playback is clocked by the sound device: the player pulls from its stdin
//! pipe at the hardware rate, and we top the pipe up only while it holds less
//! than [`TARGET_BUFFERED`] — so latency stays bounded and never drifts.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const RATE: u32 = 48_000;
/// 20 ms of mono audio.
pub const FRAME: usize = 960;
const FRAME_BYTES: usize = FRAME * 2;
/// Audio queued in the player's pipe (~60 ms).
const TARGET_BUFFERED: usize = FRAME_BYTES * 3;
/// Frames held before playout starts (absorbs network jitter).
const PREBUFFER: usize = 3;
/// More than this queued → drop the oldest (latency would only grow).
const MAX_QUEUED: usize = 12;
/// Consecutive missing frames concealed before falling back to silence.
const MAX_CONCEALED: u32 = 5;

fn sound_server_running() -> bool {
    if std::env::var_os("PULSE_SERVER").is_some() {
        return true;
    }
    let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from) else {
        return false;
    };
    rt.join("pulse/native").exists() || rt.join("pipewire-0").exists()
}

struct Recipe {
    program: &'static str,
    args: &'static [&'static str],
}

fn capture_recipes() -> Vec<Recipe> {
    let mut out = Vec::new();
    if sound_server_running() {
        out.push(Recipe {
            program: "pw-record",
            args: &[
                "--raw", "--rate", "48000", "--channels", "1", "--format", "s16",
                "--latency", "20ms", "--media-role", "Communication", "-",
            ],
        });
        out.push(Recipe {
            program: "parecord",
            args: &[
                "--raw", "--format=s16le", "--rate=48000", "--channels=1",
                "--latency-msec=20", "--property=media.role=phone",
            ],
        });
    }
    out.push(Recipe {
        program: "arecord",
        args: &["-q", "-t", "raw", "-f", "S16_LE", "-c", "1", "-r", "48000", "-"],
    });
    out
}

fn playback_recipes() -> Vec<Recipe> {
    let mut out = Vec::new();
    if sound_server_running() {
        out.push(Recipe {
            program: "pw-play",
            args: &[
                "--raw", "--rate", "48000", "--channels", "1", "--format", "s16",
                "--latency", "20ms", "--media-role", "Communication", "-",
            ],
        });
        out.push(Recipe {
            program: "pacat",
            args: &[
                "--playback", "--raw", "--format=s16le", "--rate=48000", "--channels=1",
                "--latency-msec=40", "--property=media.role=phone",
            ],
        });
    }
    out.push(Recipe {
        program: "aplay",
        args: &["-q", "-t", "raw", "-f", "S16_LE", "-c", "1", "-r", "48000", "-"],
    });
    out
}

/// Spawn the first tool that starts and is still alive shortly after (a tool
/// that can't open its device exits right away).
fn spawn_first(recipes: Vec<Recipe>, capture: bool) -> Result<Child, String> {
    let mut errors = Vec::new();
    for r in recipes {
        let mut cmd = Command::new(r.program);
        cmd.args(r.args).stderr(Stdio::null());
        if capture {
            cmd.stdin(Stdio::null()).stdout(Stdio::piped());
        } else {
            cmd.stdin(Stdio::piped()).stdout(Stdio::null());
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    errors.push(format!("{}: {e}", r.program));
                }
                continue;
            }
        };
        std::thread::sleep(Duration::from_millis(120));
        match child.try_wait() {
            Ok(None) => {
                tracing::info!(tool = r.program, capture, "call audio started");
                return Ok(child);
            }
            _ => {
                let _ = child.wait();
                errors.push(format!("{}: exited", r.program));
            }
        }
    }
    Err(if errors.is_empty() {
        "no audio tool found (install pipewire or pulseaudio utilities)".into()
    } else {
        errors.join("; ")
    })
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Bytes still queued in a pipe (`FIONREAD`); `None` if unsupported.
fn pipe_queued(stdin: &ChildStdin) -> Option<usize> {
    let mut n: libc::c_int = 0;
    // SAFETY: valid fd and out-pointer; FIONREAD only writes an int.
    let r = unsafe { libc::ioctl(stdin.as_raw_fd(), libc::FIONREAD, &mut n) };
    (r == 0 && n >= 0).then_some(n as usize)
}

fn to_bytes(pcm: &[i16; FRAME]) -> [u8; FRAME_BYTES] {
    let mut out = [0u8; FRAME_BYTES];
    for (i, s) in pcm.iter().enumerate() {
        out[2 * i..2 * i + 2].copy_from_slice(&s.to_le_bytes());
    }
    out
}

/// Feed `next` into a player process until `stop`, keeping ~60 ms queued.
fn pump(mut stdin: ChildStdin, stop: &AtomicBool, mut next: impl FnMut(&mut [i16; FRAME])) {
    let started = Instant::now();
    let mut written: u64 = 0;
    let mut frame = [0i16; FRAME];
    while !stop.load(Ordering::Relaxed) {
        let queued = match pipe_queued(&stdin) {
            Some(q) => q,
            // No FIONREAD: pace by the wall clock instead.
            None => {
                let played = started.elapsed().as_micros() as u64 * RATE as u64 / 1_000_000 * 2;
                (written.saturating_sub(played)) as usize
            }
        };
        if queued < TARGET_BUFFERED {
            next(&mut frame);
            if stdin.write_all(&to_bytes(&frame)).is_err() {
                tracing::warn!("call audio: player stopped");
                return;
            }
            written += FRAME_BYTES as u64;
            continue;
        }
        std::thread::sleep(Duration::from_millis(5));
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

// ---------------------------------------------------------------------------
// Capture / playback for a connected call

pub struct AudioIo {
    stop: Arc<AtomicBool>,
    capture: Arc<Mutex<Option<Child>>>,
    playback: Arc<Mutex<Option<Child>>>,
}

impl AudioIo {
    /// Start microphone and speaker. `on_packet` receives each encoded Opus
    /// frame (20 ms); `jitter` is fed by the network side.
    pub fn start(
        muted: Arc<AtomicBool>,
        jitter: Arc<Mutex<JitterBuffer>>,
        mut on_packet: impl FnMut(Vec<u8>) + Send + 'static,
    ) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));

        // Speaker first: hearing the peer matters even when the mic fails.
        let mut player = spawn_first(playback_recipes(), false)?;
        let stdin = player.stdin.take().ok_or("player has no stdin")?;
        let playback = Arc::new(Mutex::new(Some(player)));
        {
            let stop = Arc::clone(&stop);
            let mut decoder = opus::Decoder::new(RATE, opus::Channels::Mono)
                .map_err(|e| format!("opus decoder: {e}"))?;
            std::thread::Builder::new()
                .name("call-playback".into())
                .spawn(move || {
                    pump(stdin, &stop, |frame| {
                        let item = jitter.lock().map(|mut j| j.pop()).unwrap_or(Playout::Silence);
                        let decoded = match item {
                            Playout::Frame(data) => decoder.decode(&data, frame, false),
                            Playout::Conceal => decoder.decode(&[], frame, false),
                            Playout::Silence => Ok(0),
                        };
                        match decoded {
                            Ok(n) if n >= FRAME => {}
                            Ok(n) => frame[n..].fill(0),
                            Err(_) => frame.fill(0),
                        }
                    })
                })
                .map_err(|e| e.to_string())?;
        }

        let capture = Arc::new(Mutex::new(None));
        match spawn_first(capture_recipes(), true) {
            Ok(mut rec) => {
                let mut stdout = rec.stdout.take().ok_or("recorder has no stdout")?;
                *capture.lock().map_err(|e| e.to_string())? = Some(rec);
                let stop = Arc::clone(&stop);
                let mut encoder = opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip)
                    .map_err(|e| format!("opus encoder: {e}"))?;
                let _ = encoder.set_bitrate(opus::Bitrate::Bits(32_000));
                let _ = encoder.set_inband_fec(true);
                let _ = encoder.set_packet_loss_perc(10);
                std::thread::Builder::new()
                    .name("call-capture".into())
                    .spawn(move || {
                        let mut raw = [0u8; FRAME_BYTES];
                        let mut pcm = [0i16; FRAME];
                        let mut out = [0u8; 1500];
                        while !stop.load(Ordering::Relaxed) {
                            if stdout.read_exact(&mut raw).is_err() {
                                tracing::warn!("call audio: recorder stopped");
                                break;
                            }
                            if muted.load(Ordering::Relaxed) {
                                pcm.fill(0);
                            } else {
                                for (i, s) in pcm.iter_mut().enumerate() {
                                    *s = i16::from_le_bytes([raw[2 * i], raw[2 * i + 1]]);
                                }
                            }
                            match encoder.encode(&pcm, &mut out) {
                                Ok(n) => on_packet(out[..n].to_vec()),
                                Err(e) => tracing::warn!(%e, "opus encode"),
                            }
                        }
                    })
                    .map_err(|e| e.to_string())?;
            }
            Err(err) => tracing::warn!(%err, "call: no microphone, listening only"),
        }

        Ok(Self { stop, capture, playback })
    }

    pub fn has_microphone(&self) -> bool {
        self.capture.lock().is_ok_and(|c| c.is_some())
    }
}

impl Drop for AudioIo {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for slot in [&self.capture, &self.playback] {
            if let Ok(mut guard) = slot.lock() {
                if let Some(mut child) = guard.take() {
                    stop_child(&mut child);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tones

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Incoming call.
    Ring,
    /// Outgoing call, waiting for the peer.
    Ringback,
}

/// A looping call tone; stops when dropped.
pub struct TonePlayer {
    stop: Arc<AtomicBool>,
    child: Option<Child>,
}

impl TonePlayer {
    pub fn start(tone: Tone) -> Option<Self> {
        let mut child = match spawn_first(playback_recipes(), false) {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(%err, "call tone");
                return None;
            }
        };
        let stdin = child.stdin.take()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = Arc::clone(&stop);
        let spawned = std::thread::Builder::new().name("call-tone".into()).spawn(move || {
            let mut n: u64 = 0;
            pump(stdin, &stop2, |frame| {
                for s in frame.iter_mut() {
                    *s = tone_sample(tone, n);
                    n += 1;
                }
            })
        });
        if spawned.is_err() {
            stop_child(&mut child);
            return None;
        }
        Some(Self { stop, child: Some(child) })
    }
}

impl Drop for TonePlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.child.take() {
            stop_child(&mut child);
        }
    }
}

/// Sample `n` of a tone (48 kHz).
fn tone_sample(tone: Tone, n: u64) -> i16 {
    let t = n as f32 / RATE as f32;
    let (on, freqs, amp): (bool, &[f32], f32) = match tone {
        // Two short rings, then a pause (2.6 s cycle).
        Tone::Ring => {
            let c = t % 2.6;
            (c < 0.4 || (0.6..1.0).contains(&c), &[440.0, 480.0], 0.22)
        }
        // 1 s tone every 4 s, quieter.
        Tone::Ringback => (t % 4.0 < 1.0, &[425.0], 0.12),
    };
    if !on {
        return 0;
    }
    let v: f32 = freqs
        .iter()
        .map(|f| (2.0 * std::f32::consts::PI * f * t).sin())
        .sum::<f32>()
        / freqs.len() as f32;
    (v * amp * i16::MAX as f32) as i16
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

    #[test]
    fn pipe_fill_is_measurable() {
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        assert!(pipe_queued(&stdin).is_some());
        drop(stdin);
        let _ = child.wait();
    }
}
