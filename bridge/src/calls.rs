//! Native-app call controller. One actor owns signaling/media, so stale engine
//! callbacks cannot end a newer call and core events never block the receive loop.
use cursus_call::{audio::pcm::PcmDevice, CallEngine, CallEvent};
use cursus_core::{CallMedia, Event, Session};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

struct Controller {
    tx: mpsc::UnboundedSender<Command>,
    view: Arc<Mutex<View>>,
}
#[derive(Default)]
struct View {
    value: Value,
    connected: Option<Instant>,
}
enum Command {
    Request(Action, oneshot::Sender<Result<(), String>>),
    Core(String, Value),
    Media(u64, CallEvent),
    Timeout(u64, &'static str),
}
enum Action {
    Start(i64, String),
    Accept,
    Reject,
    End,
    Mute(bool),
}
struct Active {
    id: i64,
    name: String,
    outgoing: bool,
    phase: &'static str,
    generation: u64,
    key: Option<String>,
    engine: Option<CallEngine>,
    early_ice: Vec<String>,
    muted: bool,
    connected: Option<Instant>,
    session: Arc<Session>,
}
struct Actor {
    active: Option<Active>,
    generation: u64,
    view: Arc<Mutex<View>>,
    tx: mpsc::UnboundedSender<Command>,
    error: Option<String>,
}
fn controller() -> &'static Controller {
    static CONTROL: OnceLock<Controller> = OnceLock::new();
    CONTROL.get_or_init(|| {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let view = Arc::new(Mutex::new(View::default()));
        let mut actor = Actor {
            active: None,
            generation: 0,
            view: view.clone(),
            tx: tx.clone(),
            error: None,
        };
        super::runtime().spawn(async move {
            while let Some(command) = rx.recv().await {
                let result = match command {
                    Command::Request(action, reply) => {
                        let result = actor.action(action).await;
                        if let Err(e) = &result {
                            actor.error = Some(e.clone());
                        }
                        actor.publish();
                        let _ = reply.send(result);
                        continue;
                    }
                    Command::Core(kind, body) => actor.core(&kind, &body).await,
                    Command::Media(generation, event) => actor.media(generation, event).await,
                    Command::Timeout(generation, phase) => {
                        if actor
                            .active
                            .as_ref()
                            .is_some_and(|a| a.generation == generation && a.phase == phase)
                        {
                            actor.finish(true).await;
                            actor.error = Some(
                                if phase == "connecting" {
                                    "Could not connect the call."
                                } else {
                                    "No answer."
                                }
                                .into(),
                            );
                        }
                        Ok(())
                    }
                };
                if let Err(error) = result {
                    actor.finish(true).await;
                    actor.error = Some(error);
                }
                actor.publish();
            }
        });
        Controller { tx, view }
    })
}
fn snapshot() -> Value {
    let view = controller().view.lock().unwrap_or_else(|e| e.into_inner());
    let mut value = view.value.clone();
    if value.is_null() {
        return json!({"active": null});
    }
    if let Some(at) = view.connected {
        value["active"]["durationSecs"] = json!(at.elapsed().as_secs());
    }
    value
}
fn request(action: Action) -> Result<Value, String> {
    let (tx, rx) = oneshot::channel();
    controller()
        .tx
        .send(Command::Request(action, tx))
        .map_err(|e| e.to_string())?;
    super::runtime().block_on(rx).map_err(|e| e.to_string())??;
    Ok(snapshot())
}
pub(crate) fn dispatch(method: &str, p: &Value) -> Option<Result<Value, String>> {
    Some(match method {
        "callState" => Ok(snapshot()),
        "callStart" => match p
            .get("contactId")
            .and_then(Value::as_i64)
            .filter(|id| *id > 0)
        {
            Some(id) => request(Action::Start(
                id,
                p.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Contact")
                    .into(),
            )),
            None => Err("Missing contact id".into()),
        },
        "callAccept" => request(Action::Accept),
        "callReject" => request(Action::Reject),
        "callEnd" => request(Action::End),
        "callMute" => match p.get("muted").and_then(Value::as_bool) {
            Some(value) => request(Action::Mute(value)),
            None => Err("Missing mute state".into()),
        },
        _ => return None,
    })
}
pub(crate) fn on_event(event: &Event) {
    if let Event::Other { type_name, body } = event {
        if matches!(
            type_name.as_str(),
            "callInvitation" | "callOffer" | "callAnswer" | "callExtraInfo" | "callEnded"
        ) {
            let _ = controller()
                .tx
                .send(Command::Core(type_name.clone(), body.clone()));
        }
    }
}
pub(crate) fn close() {
    let _ = request(Action::End);
}
pub fn capture(pcm: &[i16]) -> Result<(), String> {
    PcmDevice::shared().capture(pcm)
}
pub fn playback(pcm: &mut [i16]) -> Result<(), String> {
    PcmDevice::shared().playback(pcm)
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> cursus_core::Result<T> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
fn text<'a>(v: &'a Value, path: &str) -> &'a str {
    v.pointer(path).and_then(Value::as_str).unwrap_or("")
}
fn contact(v: &Value) -> Option<(i64, String)> {
    Some((
        v.get("contactId")?.as_i64()?,
        v.pointer("/profile/displayName")
            .or_else(|| v.get("localDisplayName"))
            .and_then(Value::as_str)
            .unwrap_or("Contact")
            .into(),
    ))
}
impl Actor {
    fn publish(&self) {
        let mut view = self.view.lock().unwrap_or_else(|e| e.into_inner());
        view.connected = self.active.as_ref().and_then(|a| a.connected);
        view.value = json!({"active": self.active.as_ref().map(|a| json!({
            "contactId": a.id, "name": a.name, "phase": a.phase, "outgoing": a.outgoing,
            "muted": a.muted, "encrypted": a.key.is_some(), "durationSecs": 0,
        })), "error": self.error});
    }
    fn timeout(&self, phase: &'static str, seconds: u64) {
        let generation = self.active.as_ref().unwrap().generation;
        let tx = self.tx.clone();
        super::runtime().spawn(async move {
            tokio::time::sleep(Duration::from_secs(seconds)).await;
            let _ = tx.send(Command::Timeout(generation, phase));
        });
    }
    async fn finish(&mut self, notify: bool) {
        if let Some(a) = self.active.take() {
            self.publish();
            if let Some(engine) = a.engine {
                engine.close().await;
            }
            if notify {
                let _ = blocking(move || {
                    if a.phase == "incoming" {
                        a.session.call_reject(a.id)
                    } else {
                        a.session.call_end(a.id)
                    }
                })
                .await;
            }
        }
    }
    async fn action(&mut self, action: Action) -> Result<(), String> {
        match action {
            Action::Start(id, name) => {
                if self.active.is_some() {
                    return Err("Already in a call.".into());
                }
                let session = super::session()?;
                self.generation += 1;
                self.error = None;
                self.active = Some(Active {
                    id,
                    name,
                    outgoing: true,
                    phase: "ringing",
                    generation: self.generation,
                    key: None,
                    engine: None,
                    early_ice: Vec::new(),
                    muted: false,
                    connected: None,
                    session: session.clone(),
                });
                self.publish();
                self.timeout("ringing", 60);
                if let Err(e) = blocking(move || session.call_invite(id, CallMedia::Audio)).await {
                    self.finish(false).await;
                    return Err(e);
                }
            }
            Action::Accept => {
                let a = self
                    .active
                    .as_mut()
                    .filter(|a| a.phase == "incoming")
                    .ok_or("No incoming call")?;
                a.phase = "connecting";
                self.publish();
                self.timeout("connecting", 45);
                if let Err(e) = self.negotiate(None).await {
                    self.finish(true).await;
                    return Err(e);
                }
            }
            Action::Reject => {
                if self.active.as_ref().is_some_and(|a| a.phase == "incoming") {
                    self.finish(true).await;
                }
            }
            Action::End => {
                self.finish(true).await;
                self.error = None;
            }
            Action::Mute(muted) => {
                if let Some(a) = self.active.as_mut() {
                    a.muted = muted;
                    if let Some(e) = &a.engine {
                        e.set_muted(muted);
                    }
                }
            }
        }
        Ok(())
    }
    async fn negotiate(&mut self, remote: Option<(String, String)>) -> Result<(), String> {
        let a = self.active.as_ref().ok_or("Call ended")?;
        let (engine, mut rx) = CallEngine::new(a.key.as_deref(), PcmDevice::shared()).await?;
        let (session, id, generation, encrypted) =
            (a.session.clone(), a.id, a.generation, a.key.is_some());
        let local = match remote {
            Some((offer, ice)) => engine.accept_offer(&offer, &ice).await,
            None => engine.create_offer().await,
        };
        let outgoing = a.outgoing;
        let result = match local {
            Ok(local) => blocking(move || {
                if outgoing {
                    session.call_answer(id, &local.rtc_session, &local.rtc_ice_candidates)
                } else {
                    session.call_offer(
                        id,
                        CallMedia::Audio,
                        encrypted,
                        &local.rtc_session,
                        &local.rtc_ice_candidates,
                    )
                }
            })
            .await
            .map(|_| ()),
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            engine.close().await;
            return Err(e);
        }
        let a = self.active.as_mut().ok_or("Call ended")?;
        engine.set_muted(a.muted);
        for ice in a.early_ice.drain(..) {
            engine.add_remote_ice(&ice).await;
        }
        a.engine = Some(engine);
        let tx = self.tx.clone();
        super::runtime().spawn(async move {
            while let Some(e) = rx.recv().await {
                if tx.send(Command::Media(generation, e)).is_err() {
                    break;
                }
            }
        });
        Ok(())
    }
    async fn core(&mut self, kind: &str, body: &Value) -> Result<(), String> {
        if kind == "callInvitation" {
            let inv = body.get("callInvitation").unwrap_or(body);
            let Some((id, name)) = inv.get("contact").and_then(contact) else {
                return Ok(());
            };
            // Never replace an ongoing call, including repeated invitations from its peer.
            if self.active.as_ref().is_some_and(|a| a.id == id) {
                return Ok(());
            }
            let session = super::session()?;
            if self.active.is_some() || text(inv, "/callType/media") == "video" {
                let _ = blocking(move || session.call_reject(id)).await;
                return Ok(());
            }
            self.generation += 1;
            self.error = None;
            self.active = Some(Active {
                id,
                name,
                outgoing: false,
                phase: "incoming",
                generation: self.generation,
                key: inv
                    .get("sharedKey")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                engine: None,
                early_ice: Vec::new(),
                muted: false,
                connected: None,
                session,
            });
            self.timeout("incoming", 60);
            return Ok(());
        }
        let Some((id, _)) = body.get("contact").and_then(contact) else {
            return Ok(());
        };
        let Some(a) = self.active.as_mut().filter(|a| a.id == id) else {
            return Ok(());
        };
        match kind {
            "callOffer" if a.outgoing && a.phase == "ringing" => {
                a.phase = "connecting";
                a.key = body
                    .get("sharedKey")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let remote = (
                    text(body, "/offer/rtcSession").into(),
                    text(body, "/offer/rtcIceCandidates").into(),
                );
                self.publish();
                self.timeout("connecting", 45);
                self.negotiate(Some(remote)).await?;
            }
            "callAnswer" if !a.outgoing && a.phase == "connecting" => {
                if let Some(e) = &a.engine {
                    e.apply_answer(
                        text(body, "/answer/rtcSession"),
                        text(body, "/answer/rtcIceCandidates"),
                    )
                    .await?;
                }
            }
            "callExtraInfo" => {
                let ice = text(body, "/extraInfo/rtcIceCandidates");
                if let Some(e) = &a.engine {
                    e.add_remote_ice(ice).await;
                } else if a.early_ice.len() < 32 {
                    a.early_ice.push(ice.into());
                }
            }
            "callEnded" => self.finish(false).await,
            _ => {}
        }
        Ok(())
    }
    async fn media(&mut self, generation: u64, event: CallEvent) -> Result<(), String> {
        let Some(a) = self.active.as_mut().filter(|a| a.generation == generation) else {
            return Ok(());
        };
        let (session, id) = (a.session.clone(), a.id);
        match event {
            CallEvent::ExtraIce(ice) => {
                blocking(move || session.call_extra(id, &ice)).await?;
            }
            CallEvent::Connected => {
                a.phase = "connected";
                a.connected.get_or_insert_with(Instant::now);
                self.publish();
                blocking(move || session.call_status(id, "connected")).await?;
            }
            CallEvent::Ended(reason) => {
                self.finish(true).await;
                self.error = Some(reason);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incoming_busy_stale_callbacks_and_failed_start_leave_consistent_state() {
        // An isolated database with no contacts: signaling errors cannot reach
        // anyone, and opening it cannot clear the user's plaintext cache.
        let dir = std::env::temp_dir().join(format!("cursus-native-calls-{}", std::process::id()));
        assert!(cursus_core::set_data_dir(dir.clone()));
        let session = Session::open(cursus_core::SessionConfig {
            db_prefix: dir.join("test"),
            db_key: zeroize::Zeroizing::new("test-calls".into()),
            ..Default::default()
        })
        .unwrap();
        *crate::SESSION.lock().unwrap() = Some(Arc::new(session));
        let invite = |id| Event::Other {
            type_name: "callInvitation".into(),
            body: json!({
                "callInvitation": { "contact": {"contactId": id, "profile": {"displayName": "Test"}},
                    "callType": {"media": "audio"} }
            }),
        };
        let barrier = || request(Action::Mute(false)).unwrap();
        on_event(&invite(77));
        assert_eq!(barrier()["active"]["phase"], "incoming");
        // A repeated invitation and another caller never replace the first one.
        on_event(&invite(77));
        on_event(&invite(88));
        controller()
            .tx
            .send(Command::Media(0, CallEvent::Ended("stale".into())))
            .unwrap();
        controller()
            .tx
            .send(Command::Timeout(0, "incoming"))
            .unwrap();
        controller()
            .tx
            .send(Command::Timeout(1, "ringing"))
            .unwrap();
        assert_eq!(barrier()["active"]["contactId"], 77);
        request(Action::Reject).unwrap();
        assert!(snapshot()["active"].is_null());
        assert!(request(Action::Start(99, "Missing contact".into())).is_err());
        assert!(
            snapshot()["active"].is_null(),
            "failed invite must not leave a ringing call"
        );
        on_event(&invite(77));
        barrier();
        request(Action::End).unwrap();
        assert!(snapshot()["active"].is_null());
        crate::drop_session();
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// A fixed 20 ms mono PCM frame from a native app. Returns 0 on success.
///
/// # Safety
/// `samples` must point to `count` readable i16 values and remain valid for the call.
#[no_mangle]
pub unsafe extern "C" fn cursus_call_capture(samples: *const i16, count: i32) -> i32 {
    if samples.is_null() || count != 960 {
        return -1;
    }
    // SAFETY: caller supplies one readable PCM frame, checked for null above.
    let pcm = unsafe { std::slice::from_raw_parts(samples, count as usize) };
    capture(pcm).map(|_| 0).unwrap_or(-1)
}

/// Fills one 20 ms PCM frame, or silence when no call is connected.
///
/// # Safety
/// `samples` must point to `count` writable i16 values, exclusively borrowed for the call.
#[no_mangle]
pub unsafe extern "C" fn cursus_call_playback(samples: *mut i16, count: i32) -> i32 {
    if samples.is_null() || count != 960 {
        return -1;
    }
    // SAFETY: caller supplies one exclusively writable PCM frame.
    let pcm = unsafe { std::slice::from_raw_parts_mut(samples, count as usize) };
    playback(pcm).map(|_| 0).unwrap_or(-1)
}

#[cfg(test)]
mod pcm_abi_tests {
    use super::*;
    #[test]
    fn fixed_pcm_abi_validates_buffers_and_silences_idle_playback() {
        let mut pcm = [123i16; 960];
        // SAFETY: the non-null pointers belong to the live local array. Invalid
        // lengths/null pointers must be rejected before creating a Rust slice.
        unsafe {
            assert_eq!(cursus_call_playback(std::ptr::null_mut(), 960), -1);
            assert_eq!(cursus_call_capture(std::ptr::null(), 960), -1);
            assert_eq!(cursus_call_capture(pcm.as_ptr(), 959), -1);
            assert_eq!(cursus_call_playback(pcm.as_mut_ptr(), -1), -1);
            assert_eq!(cursus_call_playback(pcm.as_mut_ptr(), 960), 0);
            assert!(pcm.iter().all(|s| *s == 0));
            assert_eq!(cursus_call_capture(pcm.as_ptr(), 960), 0);
        }
    }
}
