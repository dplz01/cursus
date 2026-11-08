//! Audio calls for every Cursus app, entirely in-process.
//!
//! Platform-independent: WebRTC, SimpleX signaling and frame encryption,
//! Opus framing. Each app plugs in its microphone / speaker through
//! [`audio::AudioDevice`] and keeps its own call screens.
//!
//! The official SimpleX clients run WebRTC inside a WebView (`call.ts`). We
//! run a native WebRTC stack (webrtc-rs) instead and speak the same wire
//! format, so calls interoperate with them:
//!
//! - signaling goes through the chat core (`/_call invite|offer|answer|extra|
//!   end`), payloads packed as in [`signal`];
//! - the **callee** creates the WebRTC offer when accepting, the caller
//!   answers;
//! - four m-lines in the official order — mic audio, camera video, screen
//!   audio, screen video (`call.ts` indexes transceivers by mid); we only send
//!   and play the mic audio (mid 0);
//! - Opus frames are end-to-end encrypted with the call's shared key
//!   ([`crypto`]) on top of DTLS-SRTP when both sides support it;
//! - ICE: host and server-reflexive (STUN) candidates only — no TURN relay of
//!   our own. A peer behind a NAT that needs one can still offer its relay
//!   candidates.

pub mod audio;
pub mod crypto;
#[cfg(test)]
mod interop_tests;
#[cfg(test)]
mod simplex_e2e_tests;
pub mod signal;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, Notify};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::APIBuilder;
use webrtc::ice::mdns::MulticastDnsMode;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::rtp_transceiver::RTCRtpTransceiverInit;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;

use audio::{AudioDevice, AudioStream, JitterBuffer};
use crypto::FrameCrypto;
use signal::{IceCandidate, SessionDescription};

/// STUN only (address discovery, no media relay) — same host the official
/// clients use.
const STUN_SERVERS: &[&str] = &["stun:stun.simplex.im:443"];

/// Initial ICE batch goes out with the offer/answer after this (as `call.ts`).
const ICE_INITIAL_DELAY: Duration = Duration::from_millis(750);
/// Later candidates are sent in batches every …
const ICE_EXTRAS_INTERVAL: Duration = Duration::from_millis(1500);
/// … until.
const ICE_EXTRAS_TIMEOUT: Duration = Duration::from_secs(12);
/// A dropped connection gets this long to recover before the call ends.
const DISCONNECT_GRACE: Duration = Duration::from_secs(6);

/// Engine → app notifications.
#[derive(Debug, Clone)]
pub enum CallEvent {
    /// More local ICE candidates (packed) for `/_call extra`.
    ExtraIce(String),
    Connected,
    /// The media connection is gone (or never came up).
    Ended(String),
}

/// Our session description + initial candidates, packed for the core.
#[derive(Debug, Clone)]
pub struct LocalSession {
    pub rtc_session: String,
    pub rtc_ice_candidates: String,
}

#[derive(Default)]
struct IceState {
    /// Initial batch handed out with the session description.
    resolved: bool,
    pending: Vec<IceCandidate>,
    complete: bool,
}

struct Inner {
    pc: Arc<RTCPeerConnection>,
    track: Arc<TrackLocalStaticSample>,
    crypto: Option<FrameCrypto>,
    muted: Arc<AtomicBool>,
    jitter: Arc<Mutex<JitterBuffer>>,
    audio: Mutex<Option<Box<dyn AudioStream>>>,
    ice: Arc<Mutex<IceState>>,
    ice_changed: Arc<Notify>,
    /// Remote candidates that arrived before the remote description.
    early_remote: Mutex<Vec<IceCandidate>>,
    events: mpsc::UnboundedSender<CallEvent>,
    closed: AtomicBool,
    /// The platform's microphone / speaker; `None` in tests (frames go
    /// through `outgoing`).
    device: Option<Arc<dyn AudioDevice>>,
    /// Encoded Opus frames to send (fed by the microphone).
    outgoing: Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>,
}

/// One call's media session. Cheap to clone.
#[derive(Clone)]
pub struct CallEngine {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for CallEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CallEngine")
    }
}

impl CallEngine {
    /// `shared_key`: the core's `sharedKey` for this call (None = the peer
    /// doesn't encrypt frames).
    pub async fn new(
        shared_key: Option<&str>,
        device: Arc<dyn AudioDevice>,
    ) -> Result<(Self, mpsc::UnboundedReceiver<CallEvent>), String> {
        Self::with_devices(shared_key, Some(device)).await
    }

    async fn with_devices(
        shared_key: Option<&str>,
        device: Option<Arc<dyn AudioDevice>>,
    ) -> Result<(Self, mpsc::UnboundedReceiver<CallEvent>), String> {
        let crypto = shared_key.map(FrameCrypto::from_shared_key).transpose()?;

        let mut media = MediaEngine::default();
        media.register_default_codecs().map_err(err)?;
        let registry = register_default_interceptors(Registry::new(), &mut media).map_err(err)?;
        let mut settings = SettingEngine::default();
        // Browsers hide host addresses behind `<uuid>.local` names.
        settings.set_ice_multicast_dns_mode(MulticastDnsMode::QueryOnly);
        let api = APIBuilder::new()
            .with_media_engine(media)
            .with_interceptor_registry(registry)
            .with_setting_engine(settings)
            .build();
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: STUN_SERVERS.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let pc = Arc::new(api.new_peer_connection(config).await.map_err(err)?);

        let track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: audio::RATE,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                rtcp_feedback: vec![],
            },
            "audio".to_owned(),
            "cursus".to_owned(),
        ));

        let (events, rx) = mpsc::unbounded_channel();
        let inner = Arc::new(Inner {
            pc,
            track,
            crypto,
            muted: Arc::new(AtomicBool::new(false)),
            jitter: Arc::new(Mutex::new(JitterBuffer::default())),
            audio: Mutex::new(None),
            ice: Arc::new(Mutex::new(IceState::default())),
            ice_changed: Arc::new(Notify::new()),
            early_remote: Mutex::new(Vec::new()),
            events,
            closed: AtomicBool::new(false),
            device,
            outgoing: Mutex::new(None),
        });
        let engine = Self { inner };
        engine.install_handlers();
        Ok((engine, rx))
    }

    fn install_handlers(&self) {
        let pc = &self.inner.pc;

        let ice = Arc::clone(&self.inner.ice);
        let changed = Arc::clone(&self.inner.ice_changed);
        pc.on_ice_candidate(Box::new(move |candidate| {
            if let Some(c) = candidate.and_then(|c| c.to_json().ok()) {
                if let Ok(mut st) = ice.lock() {
                    st.pending.push(browser_candidate(c));
                }
                changed.notify_waiters();
            }
            Box::pin(async {})
        }));

        let ice = Arc::clone(&self.inner.ice);
        let changed = Arc::clone(&self.inner.ice_changed);
        pc.on_ice_gathering_state_change(Box::new(move |state| {
            if state == RTCIceGathererState::Complete {
                if let Ok(mut st) = ice.lock() {
                    st.complete = true;
                }
                changed.notify_waiters();
            }
            Box::pin(async {})
        }));

        let weak = Arc::downgrade(&self.inner);
        pc.on_peer_connection_state_change(Box::new(move |state| {
            let weak = weak.clone();
            Box::pin(async move {
                let Some(inner) = weak.upgrade() else { return };
                tracing::info!(?state, "call: connection state");
                match state {
                    RTCPeerConnectionState::Connected => {
                        CallEngine { inner }.on_connected();
                    }
                    RTCPeerConnectionState::Failed => {
                        let _ = inner.events.send(CallEvent::Ended("Connection failed.".into()));
                    }
                    RTCPeerConnectionState::Disconnected => {
                        let weak = Arc::downgrade(&inner);
                        tokio::spawn(async move {
                            tokio::time::sleep(DISCONNECT_GRACE).await;
                            if let Some(inner) = weak.upgrade() {
                                if inner.pc.connection_state() != RTCPeerConnectionState::Connected
                                    && !inner.closed.load(Ordering::Relaxed)
                                {
                                    let _ = inner.events.send(CallEvent::Ended("Connection lost.".into()));
                                }
                            }
                        });
                    }
                    _ => {}
                }
            })
        }));

        let weak = Arc::downgrade(&self.inner);
        pc.on_track(Box::new(move |track, _receiver, transceiver| {
            let weak = weak.clone();
            Box::pin(async move {
                let mid = transceiver.mid();
                tracing::info!(?mid, kind = %track.kind(), "call: remote track");
                // Only the peer's microphone (mid 0); camera/screen are unused.
                if track.kind() != RTPCodecType::Audio || mid.as_deref().is_some_and(|m| m != "0") {
                    return;
                }
                tokio::spawn(async move {
                    let mut warned = false;
                    while let Ok((packet, _)) = track.read_rtp().await {
                        let Some(inner) = weak.upgrade() else { break };
                        if packet.payload.is_empty() {
                            continue;
                        }
                        let payload = match &inner.crypto {
                            Some(c) => match c.decrypt_audio(&packet.payload) {
                                Some(p) => p,
                                None => {
                                    if !warned {
                                        tracing::warn!("call: undecryptable audio frame");
                                        warned = true;
                                    }
                                    continue;
                                }
                            },
                            None => packet.payload.to_vec(),
                        };
                        if let Ok(mut j) = inner.jitter.lock() {
                            j.push(packet.header.sequence_number, payload);
                        };
                    }
                });
            })
        }));
    }

    /// Callee: create the offer (mic + the three unused official m-lines).
    pub async fn create_offer(&self) -> Result<LocalSession, String> {
        let pc = &self.inner.pc;
        let send = RTCRtpTransceiverInit {
            direction: RTCRtpTransceiverDirection::Sendrecv,
            send_encodings: vec![],
        };
        let sender_track: Arc<dyn TrackLocal + Send + Sync> = self.inner.track.clone();
        let mic = pc.add_transceiver_from_track(sender_track, Some(send)).await.map_err(err)?;
        self.drain_rtcp(&mic.sender().await);
        for kind in [RTPCodecType::Video, RTPCodecType::Audio, RTPCodecType::Video] {
            let recv = RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                send_encodings: vec![],
            };
            pc.add_transceiver_from_kind(kind, Some(recv)).await.map_err(err)?;
        }
        let offer = pc.create_offer(None).await.map_err(err)?;
        let sdp = offer.sdp.clone();
        pc.set_local_description(offer).await.map_err(err)?;
        let ice = self.initial_candidates().await;
        Ok(LocalSession {
            rtc_session: signal::pack(&SessionDescription { kind: "offer".into(), sdp }),
            rtc_ice_candidates: ice,
        })
    }

    /// Caller: take the callee's offer and produce our answer.
    pub async fn accept_offer(&self, offer: &str, remote_ice: &str) -> Result<LocalSession, String> {
        let desc: SessionDescription = signal::unpack(offer)?;
        let pc = &self.inner.pc;
        // Added before the offer is applied, so JSEP associates it with the
        // first audio m-line — the peer's mic slot (mid 0). Transceivers made
        // from the offer have no sender slot and can't take a track later.
        let sender_track: Arc<dyn TrackLocal + Send + Sync> = self.inner.track.clone();
        let mic = pc
            .add_transceiver_from_track(
                sender_track,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendrecv,
                    send_encodings: vec![],
                }),
            )
            .await
            .map_err(err)?;
        self.drain_rtcp(&mic.sender().await);
        pc.set_remote_description(RTCSessionDescription::offer(desc.sdp).map_err(err)?)
            .await
            .map_err(err)?;
        tracing::info!(mid = ?mic.mid(), "call: microphone slot");
        // Like a current official client: every slot sendrecv (the unused
        // ones just carry no track). Otherwise `call.ts` takes us for an old
        // version (`adaptToOldVersion`, checks slot 2 for "sendonly").
        for t in pc.get_transceivers().await {
            if !Arc::ptr_eq(&t, &mic) {
                t.set_direction(RTCRtpTransceiverDirection::Sendrecv).await;
            }
        }
        let answer = pc.create_answer(None).await.map_err(err)?;
        let sdp = answer.sdp.clone();
        pc.set_local_description(answer).await.map_err(err)?;
        self.add_remote_ice(remote_ice).await;
        self.flush_early_remote().await;
        let ice = self.initial_candidates().await;
        Ok(LocalSession {
            rtc_session: signal::pack(&SessionDescription { kind: "answer".into(), sdp }),
            rtc_ice_candidates: ice,
        })
    }

    /// Callee: the caller's answer arrived.
    pub async fn apply_answer(&self, answer: &str, remote_ice: &str) -> Result<(), String> {
        let desc: SessionDescription = signal::unpack(answer)?;
        self.inner
            .pc
            .set_remote_description(RTCSessionDescription::answer(desc.sdp).map_err(err)?)
            .await
            .map_err(err)?;
        self.add_remote_ice(remote_ice).await;
        self.flush_early_remote().await;
        Ok(())
    }

    /// Remote candidates (packed list), from the session or `callExtraInfo`.
    pub async fn add_remote_ice(&self, packed: &str) {
        let list: Vec<IceCandidate> = match signal::unpack(packed) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(%e, "call: remote candidates");
                return;
            }
        };
        if self.inner.pc.remote_description().await.is_none() {
            if let Ok(mut early) = self.inner.early_remote.lock() {
                early.extend(list);
            }
            return;
        }
        for c in list {
            if c.candidate.is_empty() {
                continue;
            }
            let init = RTCIceCandidateInit {
                candidate: c.candidate,
                sdp_mid: c.sdp_mid,
                sdp_mline_index: c.sdp_mline_index,
                username_fragment: c.username_fragment,
            };
            // TCP / unsupported candidate types are skipped, not fatal.
            if let Err(e) = self.inner.pc.add_ice_candidate(init).await {
                tracing::debug!(%e, "call: remote candidate skipped");
            }
        }
    }

    async fn flush_early_remote(&self) {
        let early = self
            .inner
            .early_remote
            .lock()
            .map(|mut e| std::mem::take(&mut *e))
            .unwrap_or_default();
        if !early.is_empty() {
            self.add_remote_ice(&signal::pack(&early)).await;
        }
    }

    pub fn set_muted(&self, muted: bool) {
        self.inner.muted.store(muted, Ordering::Relaxed);
    }

    pub async fn close(&self) {
        if self.inner.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        if let Ok(mut audio) = self.inner.audio.lock() {
            audio.take();
        }
        if let Ok(mut out) = self.inner.outgoing.lock() {
            out.take();
        }
        let _ = self.inner.pc.close().await;
    }

    /// Tests: send one encoded Opus frame as if from the microphone.
    #[cfg(test)]
    fn test_send(&self, opus: Vec<u8>) {
        if let Some(tx) = self.inner.outgoing.lock().ok().and_then(|o| o.clone()) {
            let _ = tx.send(opus);
        }
    }

    /// Tests: frames received (decrypted) so far, in arrival order.
    #[cfg(test)]
    fn test_received(&self) -> Vec<Vec<u8>> {
        self.inner.jitter.lock().map(|mut j| j.drain_ready()).unwrap_or_default()
    }

    /// Wait for the first candidates (or gathering end), pack them, and keep
    /// sending later ones as `ExtraIce` — mirrors `getIceCandidates` in
    /// `call.ts`.
    async fn initial_candidates(&self) -> String {
        let deadline = tokio::time::Instant::now() + ICE_INITIAL_DELAY;
        loop {
            let notified = self.inner.ice_changed.notified();
            if self.inner.ice.lock().map(|s| s.complete).unwrap_or(true) {
                break;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                break;
            }
        }
        let initial = self
            .inner
            .ice
            .lock()
            .map(|mut s| {
                s.resolved = true;
                std::mem::take(&mut s.pending)
            })
            .unwrap_or_default();

        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let end = tokio::time::Instant::now() + ICE_EXTRAS_TIMEOUT;
            loop {
                let wait = tokio::time::sleep(ICE_EXTRAS_INTERVAL);
                let Some(inner) = weak.upgrade() else { return };
                let changed = Arc::clone(&inner.ice_changed);
                drop(inner);
                tokio::select! {
                    _ = wait => {}
                    _ = changed.notified() => {}
                }
                let Some(inner) = weak.upgrade() else { return };
                let (batch, complete) = inner
                    .ice
                    .lock()
                    .map(|mut s| (std::mem::take(&mut s.pending), s.complete))
                    .unwrap_or_default();
                if !batch.is_empty() {
                    let _ = inner.events.send(CallEvent::ExtraIce(signal::pack(&batch)));
                }
                if complete || tokio::time::Instant::now() >= end {
                    return;
                }
            }
        });
        signal::pack(&initial)
    }

    /// Senders must have their RTCP read for the interceptors (NACK, reports).
    fn drain_rtcp(&self, sender: &Arc<webrtc::rtp_transceiver::rtp_sender::RTCRtpSender>) {
        let sender = Arc::clone(sender);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 1500];
            while sender.read(&mut buf).await.is_ok() {}
        });
    }

    fn on_connected(&self) {
        let inner = &self.inner;
        let started = inner.audio.lock().map(|a| a.is_some()).unwrap_or(true);
        if started || inner.closed.load(Ordering::Relaxed) {
            return;
        }
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        if let Ok(mut slot) = inner.outgoing.lock() {
            *slot = Some(tx.clone());
        }
        if let Some(device) = &inner.device {
            match device.start(
                Arc::clone(&inner.muted),
                Arc::clone(&inner.jitter),
                Box::new(move |p| {
                    let _ = tx.send(p);
                }),
            ) {
                Ok(io) => {
                    if !io.has_microphone() {
                        tracing::warn!("call: microphone unavailable");
                    }
                    if let Ok(mut slot) = inner.audio.lock() {
                        *slot = Some(io);
                    }
                }
                Err(e) => {
                    let _ = inner.events.send(CallEvent::Ended(format!("Audio: {e}")));
                    return;
                }
            }
        }
        let track = Arc::clone(&inner.track);
        let crypto = inner.crypto.clone();
        tokio::spawn(async move {
            while let Some(packet) = rx.recv().await {
                let data = match &crypto {
                    Some(c) => c.encrypt_audio(&packet),
                    None => packet,
                };
                let sample = Sample {
                    data: data.into(),
                    duration: Duration::from_millis(20),
                    ..Default::default()
                };
                if track.write_sample(&sample).await.is_err() {
                    break;
                }
            }
        });
        let _ = inner.events.send(CallEvent::Connected);
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Ok(mut audio) = self.audio.lock() {
            audio.take();
        }
    }
}

/// webrtc-rs stamps candidates with an empty mid, which browsers reject;
/// everything is bundled on the first m-line.
fn browser_candidate(c: RTCIceCandidateInit) -> IceCandidate {
    IceCandidate {
        candidate: c.candidate,
        sdp_mid: Some("0".into()),
        sdp_mline_index: Some(0),
        username_fragment: c.username_fragment,
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn key() -> String {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([3u8; 32])
    }

    /// `n` Opus frames of a 440 Hz tone.
    pub(crate) fn tone_packets(n: usize) -> Vec<Vec<u8>> {
        let mut enc =
            opus::Encoder::new(audio::RATE, opus::Channels::Mono, opus::Application::Voip).unwrap();
        let mut out = Vec::new();
        let mut t = 0u64;
        for _ in 0..n {
            let pcm: Vec<i16> = (0..audio::FRAME)
                .map(|_| {
                    t += 1;
                    let x = t as f32 / audio::RATE as f32;
                    ((2.0 * std::f32::consts::PI * 440.0 * x).sin() * 8000.0) as i16
                })
                .collect();
            let mut buf = [0u8; 1500];
            let len = enc.encode(&pcm, &mut buf).unwrap();
            out.push(buf[..len].to_vec());
        }
        out
    }

    /// RMS of the decoded frames (0 for silence / garbage that won't decode).
    pub(crate) fn rms(frames: &[Vec<u8>]) -> f64 {
        let mut dec = opus::Decoder::new(audio::RATE, opus::Channels::Mono).unwrap();
        let (mut sum, mut n) = (0f64, 0usize);
        for f in frames {
            let mut pcm = [0i16; audio::FRAME];
            if let Ok(len) = dec.decode(f, &mut pcm, false) {
                for s in &pcm[..len] {
                    sum += (*s as f64).powi(2);
                    n += 1;
                }
            }
        }
        if n == 0 { 0.0 } else { (sum / n as f64).sqrt() }
    }

    pub(crate) async fn wait_connected(rx: &mut mpsc::UnboundedReceiver<CallEvent>) -> Result<(), String> {
        loop {
            match rx.recv().await {
                Some(CallEvent::Connected) => return Ok(()),
                Some(CallEvent::Ended(e)) => return Err(e),
                Some(CallEvent::ExtraIce(_)) => continue,
                None => return Err("events closed".into()),
            }
        }
    }

    /// Two engines negotiate exactly like a callee (offer) and caller
    /// (answer) through packed signaling, connect over loopback, and carry
    /// end-to-end encrypted Opus both ways.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn engines_negotiate_connect_and_carry_audio() {
        let key = key();
        let (callee, mut callee_rx) = CallEngine::with_devices(Some(&key), None).await.unwrap();
        let (caller, mut caller_rx) = CallEngine::with_devices(Some(&key), None).await.unwrap();

        let offer = callee.create_offer().await.unwrap();
        let sd: SessionDescription = signal::unpack(&offer.rtc_session).unwrap();
        assert_eq!(sd.kind, "offer");
        assert_eq!(sd.sdp.matches("m=audio").count(), 2);
        assert_eq!(sd.sdp.matches("m=video").count(), 2);

        let answer = caller.accept_offer(&offer.rtc_session, &offer.rtc_ice_candidates).await.unwrap();
        let asd: SessionDescription = signal::unpack(&answer.rtc_session).unwrap();
        let first_mline = asd.sdp.split("m=").nth(1).unwrap_or_default();
        assert!(first_mline.starts_with("audio") && first_mline.contains("a=mid:0"));
        assert!(first_mline.contains("a=sendrecv"), "mic slot must send and receive");
        callee.apply_answer(&answer.rtc_session, &answer.rtc_ice_candidates).await.unwrap();

        let both = async { tokio::join!(wait_connected(&mut callee_rx), wait_connected(&mut caller_rx)) };
        let (a, b) = tokio::time::timeout(Duration::from_secs(20), both).await.expect("connect timeout");
        a.unwrap();
        b.unwrap();

        // Drain like playback does (the jitter buffer caps its backlog).
        let (mut at_caller, mut at_callee) = (Vec::new(), Vec::new());
        for p in tone_packets(50) {
            callee.test_send(p.clone());
            caller.test_send(p);
            tokio::time::sleep(Duration::from_millis(20)).await;
            at_caller.extend(caller.test_received());
            at_callee.extend(callee.test_received());
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        at_caller.extend(caller.test_received());
        at_callee.extend(callee.test_received());
        assert!(at_caller.len() >= 40, "caller got {} frames", at_caller.len());
        assert!(at_callee.len() >= 40, "callee got {} frames", at_callee.len());
        assert!(rms(&at_caller) > 1000.0, "caller audio silent");
        assert!(rms(&at_callee) > 1000.0, "callee audio silent");

        callee.close().await;
        caller.close().await;
    }

    /// A peer with a different key can't decode our frames.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wrong_key_yields_no_audio() {
        use base64::Engine as _;
        let other = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([9u8; 32]);
        let (callee, mut callee_rx) = CallEngine::with_devices(Some(&key()), None).await.unwrap();
        let (caller, mut caller_rx) = CallEngine::with_devices(Some(&other), None).await.unwrap();
        let offer = callee.create_offer().await.unwrap();
        let answer = caller.accept_offer(&offer.rtc_session, &offer.rtc_ice_candidates).await.unwrap();
        callee.apply_answer(&answer.rtc_session, &answer.rtc_ice_candidates).await.unwrap();
        let both = async { tokio::join!(wait_connected(&mut callee_rx), wait_connected(&mut caller_rx)) };
        let _ = tokio::time::timeout(Duration::from_secs(20), both).await.expect("connect timeout");
        for p in tone_packets(20) {
            callee.test_send(p);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(caller.test_received().is_empty());
        callee.close().await;
        caller.close().await;
    }
}
