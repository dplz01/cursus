//! Linux side of calls: the shared engine (`cursus-call`) with this
//! machine's sound devices ([`audio`]: sound-server tools, echo canceller,
//! call tones) and the keep-awake inhibitor. Signaling lives in
//! `call_flow.rs`, the screens in `ui.rs`.

pub mod audio;
pub mod inhibit;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use cursus_call::audio::{AudioDevice, AudioStream, JitterBuffer};
pub use cursus_call::{CallEngine, CallEvent};
use tokio::sync::mpsc;

/// Microphone and speaker through the sound server (see [`audio::AudioIo`]).
struct LinuxAudio;

impl AudioDevice for LinuxAudio {
    fn start(
        &self,
        muted: Arc<AtomicBool>,
        jitter: Arc<Mutex<JitterBuffer>>,
        on_packet: Box<dyn FnMut(Vec<u8>) + Send>,
    ) -> Result<Box<dyn AudioStream>, String> {
        Ok(Box::new(audio::AudioIo::start(muted, jitter, on_packet)?))
    }
}

impl AudioStream for audio::AudioIo {
    fn has_microphone(&self) -> bool {
        audio::AudioIo::has_microphone(self)
    }
}

/// A call's media session on this machine's sound devices.
pub async fn new_engine(
    shared_key: Option<&str>,
) -> Result<(CallEngine, mpsc::UnboundedReceiver<CallEvent>), String> {
    CallEngine::new(shared_key, Arc::new(LinuxAudio)).await
}
