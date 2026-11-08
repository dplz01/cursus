//! Call state machine: SimpleX signaling (chat core) ↔ [`crate::call`] media.
//!
//! Outgoing:  invite → (peer accepts) `callOffer` → our answer → connected.
//! Incoming:  `callInvitation` → accept → our offer → `callAnswer` → connected.
//! Both sides trickle late ICE candidates through `/_call extra`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::mpsc;

use super::{App, Message, NavSection};
use crate::call::audio::{Tone, TonePlayer};
use crate::call::{CallEngine, CallEvent};
use cursus_core::{CallMedia, CallRecord, ChatId, ChatType, Event};

/// Unanswered calls stop ringing after this.
const RING_TIMEOUT: Duration = Duration::from_secs(60);
/// Calls page: entries shown, and how far back through all chats to look.
const HISTORY_LIMIT: usize = 100;
const HISTORY_SCAN_ITEMS: u32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallPhase {
    /// Outgoing: invitation sent, waiting for the peer to accept.
    Ringing,
    /// Offer / answer exchanged, ICE + DTLS in progress.
    Connecting,
    Connected,
}

pub(crate) struct ActiveCall {
    pub contact_id: i64,
    pub peer_name: String,
    pub outgoing: bool,
    pub phase: CallPhase,
    pub connected_at: Option<Instant>,
    pub muted: bool,
    gen: u64,
    engine: Option<CallEngine>,
    /// Remote candidates that arrived before the engine existed.
    early_ice: Vec<String>,
}

pub(crate) struct IncomingCall {
    pub contact_id: i64,
    pub peer_name: String,
    pub media: CallMedia,
    shared_key: Option<String>,
    gen: u64,
}

#[derive(Default)]
pub(crate) struct CallState {
    active: Option<ActiveCall>,
    incoming: Option<IncomingCall>,
    tone: Option<TonePlayer>,
    events: Option<CallEvents>,
    gen: u64,
    /// Suspend / screen-blank inhibitor while a call rings or runs.
    awake: Option<crate::call::inhibit::AwakeGuard>,
    /// Recent calls, newest first (from the core's call chat items).
    history: Vec<CallRecord>,
    history_loading: bool,
    /// A change arrived while loading: load again when this one lands.
    history_stale: bool,
}

impl CallState {
    pub(crate) fn active(&self) -> Option<&ActiveCall> {
        self.active.as_ref()
    }

    pub(crate) fn incoming(&self) -> Option<&IncomingCall> {
        self.incoming.as_ref()
    }

    pub(crate) fn history(&self) -> &[CallRecord] {
        &self.history
    }

    #[cfg(test)]
    pub(crate) fn set_history_for_test(&mut self, history: Vec<CallRecord>) {
        self.history = history;
    }

    /// Hold the device awake exactly while a call is ringing or up.
    fn sync_awake(&mut self) {
        let wanted = self.active.is_some() || self.incoming.is_some();
        if !wanted {
            self.awake = None;
        } else if self.awake.is_none() {
            self.awake = Some(crate::call::inhibit::AwakeGuard::acquire());
        }
    }

    fn next_gen(&mut self) -> u64 {
        self.gen += 1;
        self.gen
    }
}

/// Engine event receiver, handed to the subscription once.
#[derive(Clone)]
pub struct CallEvents {
    gen: u64,
    rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<CallEvent>>>>,
}

impl std::fmt::Debug for CallEvents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CallEvents({})", self.gen)
    }
}

impl PartialEq for CallEvents {
    fn eq(&self, other: &Self) -> bool {
        self.gen == other.gen
    }
}

impl Eq for CallEvents {}

impl std::hash::Hash for CallEvents {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.gen.hash(state);
    }
}

#[derive(Debug, Clone)]
pub enum CallMsg {
    Start(i64),
    Accept,
    Reject,
    End,
    ToggleMute,
    /// Engine created and our offer/answer sent.
    Ready(u64, Result<(CallEngine, CallEvents), String>),
    Event(u64, CallEvent),
    /// A signaling / engine step finished.
    Done(Result<(), String>),
    RingTimeout(u64),
    /// Once a second while connected (duration label).
    Tick,
    ReloadHistory,
    HistoryLoaded(Result<Vec<CallRecord>, String>),
    /// Calls page row: open the chat with this contact.
    OpenChat(i64),
}

/// Contact id and display name from a core `contact` object.
fn contact_of(v: &Value) -> Option<(i64, String)> {
    let id = v.get("contactId")?.as_i64()?;
    let name = v
        .pointer("/profile/displayName")
        .or_else(|| v.get("localDisplayName"))
        .and_then(|n| n.as_str())
        .unwrap_or("Contact")
        .to_string();
    Some((id, name))
}

fn str_at<'a>(v: &'a Value, path: &str) -> &'a str {
    v.pointer(path).and_then(|s| s.as_str()).unwrap_or("")
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

fn events_handle(gen: u64, rx: mpsc::UnboundedReceiver<CallEvent>) -> CallEvents {
    CallEvents { gen, rx: Arc::new(Mutex::new(Some(rx))) }
}

fn done(r: Result<(), String>) -> Message {
    Message::Call(CallMsg::Done(r))
}

pub(crate) fn event_stream(
    events: &CallEvents,
) -> impl iced::futures::Stream<Item = Message> + Send + 'static {
    use iced::futures::SinkExt;
    let gen = events.gen;
    let rx = events.rx.lock().ok().and_then(|mut r| r.take());
    iced::stream::channel(16, move |mut out: iced::futures::channel::mpsc::Sender<Message>| async move {
        let Some(mut rx) = rx else { return };
        while let Some(ev) = rx.recv().await {
            if out.send(Message::Call(CallMsg::Event(gen, ev))).await.is_err() {
                break;
            }
        }
    })
}

impl App {
    pub(crate) fn call_subscription(&self) -> iced::Subscription<Message> {
        let events = match self.call.events.clone() {
            Some(e) => iced::Subscription::run_with(e, event_stream),
            None => iced::Subscription::none(),
        };
        let tick = if self.call.active.as_ref().is_some_and(|c| c.phase == CallPhase::Connected) {
            iced::time::every(Duration::from_secs(1)).map(|_| Message::Call(CallMsg::Tick))
        } else {
            iced::Subscription::none()
        };
        iced::Subscription::batch([events, tick])
    }

    fn ring_timeout(gen: u64) -> iced::Task<Message> {
        iced::Task::perform(
            async move {
                tokio::time::sleep(RING_TIMEOUT).await;
                gen
            },
            |g| Message::Call(CallMsg::RingTimeout(g)),
        )
    }

    /// Stop media and tones; the caller decides whether to tell the core.
    fn teardown_call(&mut self) -> iced::Task<Message> {
        self.call.tone = None;
        self.call.events = None;
        match self.call.active.take().and_then(|c| c.engine) {
            Some(engine) => iced::Task::perform(async move { engine.close().await }, |_| {
                Message::Call(CallMsg::Tick)
            }),
            None => iced::Task::none(),
        }
    }

    fn send_end(&self, contact_id: i64) -> iced::Task<Message> {
        let Some(session) = self.session.clone() else {
            return iced::Task::none();
        };
        iced::Task::perform(
            blocking(move || session.call_end(contact_id).map(|_| ()).map_err(|e| e.to_string())),
            |_| Message::Call(CallMsg::Tick),
        )
    }

    /// End the active call locally and tell the peer.
    fn hang_up(&mut self, status: String) -> iced::Task<Message> {
        let contact = self.call.active.as_ref().map(|c| c.contact_id);
        let close = self.teardown_call();
        self.status = status;
        match contact {
            Some(id) => iced::Task::batch([close, self.send_end(id)]),
            None => close,
        }
    }

    /// Before quit: end whatever call is up. Synchronous (the app exits
    /// right after; a queued task could lose that race). Dropping the engine
    /// stops the audio tools.
    pub(crate) fn end_calls_for_exit(&mut self) {
        self.call.tone = None;
        self.call.events = None;
        let active = self.call.active.take().map(|c| c.contact_id);
        let incoming = self.call.incoming.take().map(|c| c.contact_id);
        self.call.sync_awake();
        let Some(session) = self.session.clone() else { return };
        if let Some(id) = active {
            let _ = session.call_end(id);
        }
        if let Some(id) = incoming {
            let _ = session.call_reject(id);
        }
    }

    pub(crate) fn update_call(&mut self, msg: CallMsg) -> iced::Task<Message> {
        let task = self.update_call_inner(msg);
        self.call.sync_awake();
        task
    }

    /// Load the call history (coalesced: a request while loading reloads once
    /// more when the current load lands).
    pub(crate) fn reload_call_history(&mut self) -> iced::Task<Message> {
        if self.call.history_loading {
            self.call.history_stale = true;
            return iced::Task::none();
        }
        let Some(session) = self.session.clone() else {
            return iced::Task::none();
        };
        self.call.history_loading = true;
        self.call.history_stale = false;
        iced::Task::perform(
            blocking(move || {
                session.call_history(HISTORY_LIMIT, HISTORY_SCAN_ITEMS).map_err(|e| e.to_string())
            }),
            |r| Message::Call(CallMsg::HistoryLoaded(r)),
        )
    }

    /// A core event that adds / updates / deletes a call item: refresh the
    /// history if the Calls page shows it (opening the page always reloads).
    pub(crate) fn call_history_event(&mut self, event: &Event) -> iced::Task<Message> {
        let touches = match event {
            Event::NewChatItems(body) => CallRecord::touches_calls(body),
            Event::Other { type_name, body } => {
                matches!(type_name.as_str(), "callEnded" | "callInvitation")
                    || CallRecord::touches_calls(body)
            }
            _ => false,
        };
        if !touches {
            return iced::Task::none();
        }
        if self.nav == NavSection::Calls {
            self.reload_call_history()
        } else {
            iced::Task::none()
        }
    }

    fn update_call_inner(&mut self, msg: CallMsg) -> iced::Task<Message> {
        match msg {
            CallMsg::ReloadHistory => self.reload_call_history(),
            CallMsg::HistoryLoaded(result) => {
                self.call.history_loading = false;
                match result {
                    Ok(list) => self.call.history = list,
                    Err(err) => tracing::warn!(%err, "call history"),
                }
                if self.call.history_stale {
                    return self.reload_call_history();
                }
                iced::Task::none()
            }
            CallMsg::OpenChat(contact_id) => {
                let want = ChatId::new(ChatType::Direct, contact_id);
                let Some(index) = self.chats.iter().position(|c| c.id == want) else {
                    self.status = "That contact's chat is gone.".into();
                    return iced::Task::none();
                };
                self.nav = NavSection::Chats;
                self.update(Message::SelectChat(index))
            }
            CallMsg::Start(contact_id) => self.start_call(contact_id),
            CallMsg::Accept => self.accept_call(),
            CallMsg::Reject => {
                let Some(inv) = self.call.incoming.take() else {
                    return iced::Task::none();
                };
                self.call.tone = None;
                self.status = "Call declined.".into();
                let Some(session) = self.session.clone() else {
                    return iced::Task::none();
                };
                iced::Task::perform(
                    blocking(move || {
                        session.call_reject(inv.contact_id).map(|_| ()).map_err(|e| e.to_string())
                    }),
                    done,
                )
            }
            CallMsg::End => self.hang_up("Call ended.".into()),
            CallMsg::ToggleMute => {
                if let Some(call) = self.call.active.as_mut() {
                    call.muted = !call.muted;
                    if let Some(engine) = &call.engine {
                        engine.set_muted(call.muted);
                    }
                }
                iced::Task::none()
            }
            CallMsg::Ready(gen, result) => {
                let current = self.call.active.as_ref().is_some_and(|c| c.gen == gen);
                match result {
                    Ok((engine, events)) if current => {
                        let call = self.call.active.as_mut().expect("checked");
                        engine.set_muted(call.muted);
                        let early = std::mem::take(&mut call.early_ice);
                        call.engine = Some(engine.clone());
                        self.call.events = Some(events);
                        iced::Task::perform(
                            async move {
                                for ice in early {
                                    engine.add_remote_ice(&ice).await;
                                }
                            },
                            |_| Message::Call(CallMsg::Tick),
                        )
                    }
                    // Hung up while the engine was starting.
                    Ok((engine, _)) => iced::Task::perform(async move { engine.close().await }, |_| {
                        Message::Call(CallMsg::Tick)
                    }),
                    Err(err) if current => {
                        tracing::warn!(%err, "call setup failed");
                        self.hang_up(format!("Call failed: {err}"))
                    }
                    Err(_) => iced::Task::none(),
                }
            }
            CallMsg::Event(gen, ev) => {
                let Some(call) = self.call.active.as_mut().filter(|c| c.gen == gen) else {
                    return iced::Task::none();
                };
                match ev {
                    CallEvent::ExtraIce(ice) => {
                        let (Some(session), id) = (self.session.clone(), call.contact_id) else {
                            return iced::Task::none();
                        };
                        iced::Task::perform(
                            blocking(move || {
                                session.call_extra(id, &ice).map(|_| ()).map_err(|e| e.to_string())
                            }),
                            done,
                        )
                    }
                    CallEvent::Connected => {
                        call.phase = CallPhase::Connected;
                        call.connected_at = Some(Instant::now());
                        self.status = format!("In call with {}.", call.peer_name);
                        let (Some(session), id) = (self.session.clone(), call.contact_id) else {
                            return iced::Task::none();
                        };
                        iced::Task::perform(
                            blocking(move || {
                                session.call_status(id, "connected").map(|_| ()).map_err(|e| e.to_string())
                            }),
                            done,
                        )
                    }
                    CallEvent::Ended(reason) => self.hang_up(reason),
                }
            }
            CallMsg::Done(Ok(())) | CallMsg::Tick => iced::Task::none(),
            CallMsg::Done(Err(err)) => {
                tracing::warn!(%err, "call step failed");
                self.status = err;
                iced::Task::none()
            }
            CallMsg::RingTimeout(gen) => {
                if self
                    .call
                    .active
                    .as_ref()
                    .is_some_and(|c| c.gen == gen && c.phase == CallPhase::Ringing)
                {
                    return self.hang_up("No answer.".into());
                }
                if let Some(inv) = self.call.incoming.take_if(|i| i.gen == gen) {
                    self.call.tone = None;
                    self.status = format!("Missed call from {}.", inv.peer_name);
                }
                iced::Task::none()
            }
        }
    }

    fn start_call(&mut self, contact_id: i64) -> iced::Task<Message> {
        if self.call.active.is_some() || self.call.incoming.is_some() {
            self.status = "Already in a call.".into();
            return iced::Task::none();
        }
        let Some(session) = self.session.clone() else {
            return iced::Task::none();
        };
        let peer_name = self
            .chats
            .iter()
            .find(|c| matches!(c.id.chat_type, cursus_core::ChatType::Direct) && c.id.api_id == contact_id)
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "Contact".into());
        let gen = self.call.next_gen();
        self.call.active = Some(ActiveCall {
            contact_id,
            peer_name: peer_name.clone(),
            outgoing: true,
            phase: CallPhase::Ringing,
            connected_at: None,
            muted: false,
            gen,
            engine: None,
            early_ice: Vec::new(),
        });
        self.call.tone = TonePlayer::start(Tone::Ringback);
        self.status = format!("Calling {peer_name}…");
        self.nav = NavSection::Calls;
        iced::Task::batch([
            iced::Task::perform(
                blocking(move || {
                    session
                        .call_invite(contact_id, CallMedia::Audio)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                }),
                move |r| match r {
                    Ok(()) => Message::Call(CallMsg::Done(Ok(()))),
                    // e.g. the contact doesn't allow calls.
                    Err(e) => Message::Call(CallMsg::Ready(gen, Err(e))),
                },
            ),
            Self::ring_timeout(gen),
        ])
    }

    fn accept_call(&mut self) -> iced::Task<Message> {
        let Some(inv) = self.call.incoming.take() else {
            return iced::Task::none();
        };
        self.call.tone = None;
        let Some(session) = self.session.clone() else {
            return iced::Task::none();
        };
        let gen = inv.gen;
        self.call.active = Some(ActiveCall {
            contact_id: inv.contact_id,
            peer_name: inv.peer_name.clone(),
            outgoing: false,
            phase: CallPhase::Connecting,
            connected_at: None,
            muted: false,
            gen,
            engine: None,
            early_ice: Vec::new(),
        });
        self.status = format!("Connecting to {}…", inv.peer_name);
        self.nav = NavSection::Calls;
        let key = inv.shared_key;
        let contact_id = inv.contact_id;
        iced::Task::perform(
            async move {
                let (engine, rx) = crate::call::new_engine(key.as_deref()).await?;
                let sent = async {
                    let local = engine.create_offer().await?;
                    let encryption = key.is_some();
                    blocking(move || {
                        session
                            .call_offer(
                                contact_id,
                                CallMedia::Audio,
                                encryption,
                                &local.rtc_session,
                                &local.rtc_ice_candidates,
                            )
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    })
                    .await
                }
                .await;
                match sent {
                    Ok(()) => Ok((engine, events_handle(gen, rx))),
                    Err(e) => {
                        engine.close().await;
                        Err(e)
                    }
                }
            },
            move |r| Message::Call(CallMsg::Ready(gen, r)),
        )
    }

    /// Call events from the chat core (`callInvitation`, `callOffer`, …).
    pub(crate) fn on_call_core_event(&mut self, type_name: &str, body: &Value) -> iced::Task<Message> {
        let task = self.on_call_core_event_inner(type_name, body);
        self.call.sync_awake();
        task
    }

    fn on_call_core_event_inner(&mut self, type_name: &str, body: &Value) -> iced::Task<Message> {
        match type_name {
            "callInvitation" => {
                let inv = body.get("callInvitation").unwrap_or(body);
                let Some((contact_id, peer_name)) = inv.get("contact").and_then(contact_of) else {
                    return iced::Task::none();
                };
                let busy_with_other = self.call.active.as_ref().is_some_and(|c| c.contact_id != contact_id)
                    || self.call.incoming.as_ref().is_some_and(|c| c.contact_id != contact_id);
                if busy_with_other {
                    let Some(session) = self.session.clone() else {
                        return iced::Task::none();
                    };
                    return iced::Task::perform(
                        blocking(move || session.call_reject(contact_id).map(|_| ()).map_err(|e| e.to_string())),
                        done,
                    );
                }
                let media = match str_at(inv, "/callType/media") {
                    "video" => CallMedia::Video,
                    _ => CallMedia::Audio,
                };
                let shared_key = inv.get("sharedKey").and_then(|k| k.as_str()).map(str::to_string);
                let gen = self.call.next_gen();
                self.call.incoming = Some(IncomingCall {
                    contact_id,
                    peer_name: peer_name.clone(),
                    media,
                    shared_key,
                    gen,
                });
                self.call.tone = TonePlayer::start(Tone::Ring);
                self.status = format!("Incoming call from {peer_name}.");
                self.nav = NavSection::Calls;
                if self.locked.is_some() {
                    crate::desktop::notify_message("Cursus", "Incoming call");
                } else {
                    crate::desktop::notify_message(&peer_name, "Incoming call");
                }
                let show = if self.main_window.is_none() {
                    self.update(Message::ShowFromTray)
                } else {
                    iced::Task::none()
                };
                iced::Task::batch([show, Self::ring_timeout(gen)])
            }
            "callOffer" => {
                let Some((contact_id, _)) = body.get("contact").and_then(contact_of) else {
                    return iced::Task::none();
                };
                let Some(call) = self
                    .call
                    .active
                    .as_mut()
                    .filter(|c| c.contact_id == contact_id && c.outgoing && c.phase == CallPhase::Ringing)
                else {
                    return iced::Task::none();
                };
                call.phase = CallPhase::Connecting;
                let gen = call.gen;
                self.call.tone = None;
                self.status = format!("Connecting to {}…", call.peer_name);
                let Some(session) = self.session.clone() else {
                    return iced::Task::none();
                };
                let key = body.get("sharedKey").and_then(|k| k.as_str()).map(str::to_string);
                let offer = str_at(body, "/offer/rtcSession").to_string();
                let ice = str_at(body, "/offer/rtcIceCandidates").to_string();
                iced::Task::perform(
                    async move {
                        let (engine, rx) = crate::call::new_engine(key.as_deref()).await?;
                        let sent = async {
                            let local = engine.accept_offer(&offer, &ice).await?;
                            blocking(move || {
                                session
                                    .call_answer(contact_id, &local.rtc_session, &local.rtc_ice_candidates)
                                    .map(|_| ())
                                    .map_err(|e| e.to_string())
                            })
                            .await
                        }
                        .await;
                        match sent {
                            Ok(()) => Ok((engine, events_handle(gen, rx))),
                            Err(e) => {
                                engine.close().await;
                                Err(e)
                            }
                        }
                    },
                    move |r| Message::Call(CallMsg::Ready(gen, r)),
                )
            }
            "callAnswer" => {
                let Some((contact_id, _)) = body.get("contact").and_then(contact_of) else {
                    return iced::Task::none();
                };
                let Some(engine) = self
                    .call
                    .active
                    .as_ref()
                    .filter(|c| c.contact_id == contact_id && !c.outgoing)
                    .and_then(|c| c.engine.clone())
                else {
                    return iced::Task::none();
                };
                let answer = str_at(body, "/answer/rtcSession").to_string();
                let ice = str_at(body, "/answer/rtcIceCandidates").to_string();
                iced::Task::perform(async move { engine.apply_answer(&answer, &ice).await }, done)
            }
            "callExtraInfo" => {
                let Some((contact_id, _)) = body.get("contact").and_then(contact_of) else {
                    return iced::Task::none();
                };
                let ice = str_at(body, "/extraInfo/rtcIceCandidates").to_string();
                let Some(call) = self.call.active.as_mut().filter(|c| c.contact_id == contact_id) else {
                    return iced::Task::none();
                };
                match call.engine.clone() {
                    Some(engine) => iced::Task::perform(
                        async move { engine.add_remote_ice(&ice).await },
                        |_| Message::Call(CallMsg::Tick),
                    ),
                    None => {
                        call.early_ice.push(ice);
                        iced::Task::none()
                    }
                }
            }
            "callEnded" => {
                let Some((contact_id, _)) = body.get("contact").and_then(contact_of) else {
                    return iced::Task::none();
                };
                if let Some(inv) = self.call.incoming.take_if(|c| c.contact_id == contact_id) {
                    self.call.tone = None;
                    self.status = format!("Missed call from {}.", inv.peer_name);
                }
                if self.call.active.as_ref().is_some_and(|c| c.contact_id == contact_id) {
                    let task = self.teardown_call();
                    self.status = "Call ended.".into();
                    return task;
                }
                iced::Task::none()
            }
            _ => iced::Task::none(),
        }
    }
}
