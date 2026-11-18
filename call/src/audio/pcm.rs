//! PCM adapter for platforms that own their audio threads (Android AudioRecord/Track).
use super::{AudioDevice, AudioStream, JitterBuffer, Playout, FRAME, RATE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

struct Capture {
    encoder: opus::Encoder,
    send: Box<dyn FnMut(Vec<u8>) + Send>,
}
struct Stream {
    capture: Mutex<Capture>,
    decoder: Mutex<opus::Decoder>,
    jitter: Arc<Mutex<JitterBuffer>>,
    muted: Arc<AtomicBool>,
    live: AtomicBool,
}
impl AudioStream for Handle {}
struct Handle {
    stream: Arc<Stream>,
    slot: Weak<Mutex<Option<Arc<Stream>>>>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.stream.live.store(false, Ordering::Release);
        if let Some(slot) = self.slot.upgrade() {
            if let Ok(mut slot) = slot.lock() {
                if slot.as_ref().is_some_and(|s| Arc::ptr_eq(s, &self.stream)) {
                    slot.take();
                }
            }
        }
    }
}

#[derive(Default)]
pub struct PcmDevice(Arc<Mutex<Option<Arc<Stream>>>>);
impl PcmDevice {
    pub fn shared() -> Arc<Self> {
        static DEVICE: OnceLock<Arc<PcmDevice>> = OnceLock::new();
        DEVICE.get_or_init(Default::default).clone()
    }
    fn stream(&self) -> Option<Arc<Stream>> {
        self.0
            .lock()
            .ok()?
            .as_ref()
            .filter(|s| s.live.load(Ordering::Acquire))
            .cloned()
    }
    /// Exactly one 20 ms, 48 kHz mono frame. Muting happens before encoding.
    pub fn capture(&self, pcm: &[i16]) -> Result<(), String> {
        if pcm.len() != FRAME {
            return Err("Expected 960 PCM samples".into());
        }
        let Some(s) = self.stream() else {
            return Ok(());
        };
        let mut capture = s.capture.lock().map_err(|e| e.to_string())?;
        let mut packet = [0u8; 1500];
        let silent = [0i16; FRAME];
        let input = if s.muted.load(Ordering::Relaxed) {
            &silent
        } else {
            pcm
        };
        let n = capture
            .encoder
            .encode(input, &mut packet)
            .map_err(|e| e.to_string())?;
        (capture.send)(packet[..n].to_vec());
        Ok(())
    }
    /// Returns silence before connection and after teardown; no stale audio leaks.
    pub fn playback(&self, pcm: &mut [i16]) -> Result<(), String> {
        pcm.fill(0);
        if pcm.len() != FRAME {
            return Err("Expected 960 PCM samples".into());
        }
        let Some(s) = self.stream() else {
            return Ok(());
        };
        let frame = s.jitter.lock().map_err(|e| e.to_string())?.pop();
        let data = match &frame {
            Playout::Frame(data) => data.as_slice(),
            Playout::Conceal => &[],
            Playout::Silence => return Ok(()),
        };
        let n = s
            .decoder
            .lock()
            .map_err(|e| e.to_string())?
            .decode(data, pcm, false)
            .map_err(|e| e.to_string())?;
        pcm[n..].fill(0);
        Ok(())
    }
}
impl AudioDevice for PcmDevice {
    fn start(
        &self,
        muted: Arc<AtomicBool>,
        jitter: Arc<Mutex<JitterBuffer>>,
        on_packet: Box<dyn FnMut(Vec<u8>) + Send>,
    ) -> Result<Box<dyn AudioStream>, String> {
        let mut encoder = opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip)
            .map_err(|e| e.to_string())?;
        encoder
            .set_bitrate(opus::Bitrate::Bits(32_000))
            .map_err(|e| e.to_string())?;
        encoder.set_inband_fec(true).map_err(|e| e.to_string())?;
        encoder
            .set_packet_loss_perc(10)
            .map_err(|e| e.to_string())?;
        let decoder = opus::Decoder::new(RATE, opus::Channels::Mono).map_err(|e| e.to_string())?;
        let stream = Arc::new(Stream {
            capture: Mutex::new(Capture {
                encoder,
                send: on_packet,
            }),
            decoder: Mutex::new(decoder),
            jitter,
            muted,
            live: AtomicBool::new(true),
        });
        if let Some(old) = self
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .replace(stream.clone())
        {
            old.live.store(false, Ordering::Release);
        }
        Ok(Box::new(Handle {
            stream,
            slot: Arc::downgrade(&self.0),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pcm_encodes_decodes_and_silences_after_drop() {
        let device = PcmDevice::default();
        let jitter = Arc::new(Mutex::new(JitterBuffer::default()));
        let packets = Arc::new(Mutex::new(Vec::new()));
        let sent = packets.clone();
        let muted = Arc::new(AtomicBool::new(false));
        let handle = device
            .start(
                muted.clone(),
                jitter.clone(),
                Box::new(move |p| sent.lock().unwrap().push(p)),
            )
            .unwrap();
        let frame: Vec<i16> = (0..FRAME)
            .map(|n| ((n as f32 * 0.08).sin() * 8000.0) as i16)
            .collect();
        for _ in 0..6 {
            device.capture(&frame).unwrap();
        }
        for (seq, p) in packets.lock().unwrap().drain(..).enumerate() {
            jitter.lock().unwrap().push(seq as u16, p);
        }
        let mut pcm = [0; FRAME];
        device.playback(&mut pcm).unwrap();
        assert!(pcm.iter().any(|s| *s != 0));
        muted.store(true, Ordering::Relaxed);
        device.capture(&frame).unwrap();
        assert_eq!(packets.lock().unwrap().len(), 1);
        drop(handle);
        device.playback(&mut pcm).unwrap();
        assert!(pcm.iter().all(|s| *s == 0));
        assert!(device.capture(&[0]).is_err());
    }
    #[test]
    fn dropping_old_stream_cannot_stop_its_replacement() {
        let d = PcmDevice::default();
        let start = || {
            d.start(
                Arc::new(AtomicBool::new(false)),
                Arc::new(Mutex::new(JitterBuffer::default())),
                Box::new(|_| {}),
            )
            .unwrap()
        };
        let old = start();
        let current = start();
        drop(old);
        assert!(d.stream().is_some());
        drop(current);
        assert!(d.stream().is_none());
    }
}
