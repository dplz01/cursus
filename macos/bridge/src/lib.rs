//! C ABI over `cursus-core` for the native macOS app.
//!
//! Deliberately tiny so the Swift side needs no generated bindings (and builds
//! with the Swift 5.7 toolchain of the last Xcode that runs on Monterey):
//!
//! ```c
//! void  cursus_init(void);                    // first thing in main()
//! char *cursus_call(const char *request);     // {"method": …, "params": {…}}
//! char *cursus_next_event(int32_t timeout_ms); // NULL on timeout / no session
//! void  cursus_free(char *s);
//! ```
//!
//! Every reply is `{"ok": <value>}` or `{"error": "<message>"}`. Calls block
//! (they go through libsimplex), so Swift makes them off the main thread;
//! the session is shared and thread-safe. Chats are addressed by their core
//! command reference (`@3`, `#9`, `#9(_support:4)`, `*1`) — see [`dto`].

mod dto;
mod methods;
mod prefs;

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use cursus_core::{Event, Session};
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;

/// Runtime for the core's receive worker (`spawn_blocking`) and timeouts.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("cursus-core")
            .enable_time()
            .build()
            .expect("tokio runtime")
    })
}

/// The open session (None until unlocked, and again after `close`).
static SESSION: Mutex<Option<Arc<Session>>> = Mutex::new(None);
/// Core events, drained by Swift's event thread. Separate lock: waiting for
/// an event must never hold up a command.
static EVENTS: Mutex<Option<UnboundedReceiver<Event>>> = Mutex::new(None);

pub(crate) fn session() -> Result<Arc<Session>, String> {
    SESSION
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "Not unlocked".to_string())
}

pub(crate) fn install_session(session: Session) -> Result<Arc<Session>, String> {
    let session = Arc::new(session);
    let rx = {
        let _rt = runtime().enter();
        session.spawn_receiver()
    };
    *EVENTS.lock().map_err(|e| e.to_string())? = Some(rx);
    *SESSION.lock().map_err(|e| e.to_string())? = Some(Arc::clone(&session));
    // Subscribe transports only once the receive loop runs (doing it before
    // has hung startup when the agent blocked with nobody reading events).
    let net = Arc::clone(&session);
    runtime().spawn_blocking(move || {
        if let Err(e) = net.activate_network() {
            tracing::warn!(%e, "activate network");
        }
    });
    Ok(session)
}

pub(crate) fn drop_session() {
    if let Ok(mut events) = EVENTS.lock() {
        *events = None;
    }
    if let Ok(mut s) = SESSION.lock() {
        *s = None;
    }
}

fn to_c(value: Value) -> *mut c_char {
    let text = value.to_string();
    // serde_json never emits interior NULs (they are escaped as \u0000).
    CString::new(text).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

/// Set up logging and the process environment libsimplex needs. Call first
/// in `main`, before other threads exist (it sets environment variables).
#[no_mangle]
pub extern "C" fn cursus_init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = std::fs::create_dir_all(cursus_core::temp_dir());
        let _ = std::fs::create_dir_all(cursus_core::files_dir());
        cursus_core::init_process_env();
        let filter = tracing_subscriber::EnvFilter::try_from_env("CURSUS_LOG")
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
        let _ = tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).try_init();
    });
}

/// Run one request. Never returns NULL for a valid request string.
///
/// # Safety
/// `request` must be a valid NUL-terminated UTF-8 string (or NULL).
#[no_mangle]
pub unsafe extern "C" fn cursus_call(request: *const c_char) -> *mut c_char {
    let reply = catch_unwind(AssertUnwindSafe(|| {
        if request.is_null() {
            return Err("null request".to_string());
        }
        // SAFETY: caller contract above.
        let raw = unsafe { CStr::from_ptr(request) }.to_str().map_err(|e| e.to_string())?;
        let req: Value = serde_json::from_str(raw).map_err(|e| format!("bad request: {e}"))?;
        let method = req.get("method").and_then(Value::as_str).ok_or("missing method")?;
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        methods::dispatch(method, &params)
    }));
    let value = match reply {
        Ok(Ok(v)) => json!({ "ok": v }),
        Ok(Err(e)) => json!({ "error": e }),
        Err(_) => json!({ "error": "internal error (panic in cursus core)" }),
    };
    to_c(value)
}

/// Wait up to `timeout_ms` for the next core event. Returns NULL when none
/// arrived (or no session is open); the caller simply asks again.
#[no_mangle]
pub extern "C" fn cursus_next_event(timeout_ms: i32) -> *mut c_char {
    let wait = Duration::from_millis(timeout_ms.max(0) as u64);
    let got = catch_unwind(AssertUnwindSafe(|| {
        let mut guard = EVENTS.lock().ok()?;
        let rx = guard.as_mut()?;
        runtime().block_on(async { tokio::time::timeout(wait, rx.recv()).await.ok().flatten() })
    }));
    match got {
        Ok(Some(event)) => {
            // Mirror the official clients: small images / voice / video
            // download without a tap.
            if let Ok(session) = session() {
                for (file_id, _) in cursus_core::auto_receive_candidates(&event) {
                    let s = Arc::clone(&session);
                    runtime().spawn_blocking(move || {
                        if let Err(e) = s.receive_file(file_id, false) {
                            tracing::warn!(%e, file_id, "auto-receive");
                        }
                    });
                }
            }
            to_c(dto::event(&event))
        }
        Ok(None) => {
            if timeout_ms > 0 && EVENTS.lock().map(|g| g.is_none()).unwrap_or(true) {
                // No session: don't let the caller spin.
                std::thread::sleep(wait.min(Duration::from_millis(250)));
            }
            std::ptr::null_mut()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a string returned by this library.
///
/// # Safety
/// `s` must come from `cursus_call` / `cursus_next_event` and be freed once.
#[no_mangle]
pub unsafe extern "C" fn cursus_free(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: caller contract above; allocated by CString::into_raw.
        drop(unsafe { CString::from_raw(s) });
    }
}
