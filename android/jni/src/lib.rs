//! JNI wrapper of `cursus-bridge` for the Android app.
//!
//! The bridge's JSON API is the same one the macOS app uses
//! (`bridge/src/lib.rs`); this crate only moves strings between the JVM and
//! it. Kotlin side: `android/app/src/main/java/chat/cursus/core/Native.kt`.
//!
//! ```kotlin
//! object Native {
//!     external fun init(dataDir: String): Unit
//!     external fun call(request: String): String       // {"ok": …} | {"error": …}
//!     external fun nextEvent(timeoutMs: Int): String?  // null: nothing yet
//! }
//! ```

use std::ffi::{c_char, CStr, CString};
use std::path::PathBuf;

use jni::objects::{JClass, JString};
use jni::sys::{jint, jstring};
use jni::JNIEnv;

#[cfg(target_os = "android")]
mod logcat;

/// Point the core at the app's private storage, route logs to logcat and
/// set up the bridge. Called once from `Application.onCreate`, before any
/// other call (`cursus_init` sets TMPDIR for libsimplex's XFTP downloads).
#[no_mangle]
pub extern "system" fn Java_chat_cursus_core_Native_init(mut env: JNIEnv, _class: JClass, data_dir: JString) {
    let dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    cursus_core::set_data_dir(PathBuf::from(dir));
    #[cfg(target_os = "android")]
    logcat::install();
    cursus_bridge::cursus_init();
}

/// One request `{"method": …, "params": {…}}` → one reply. Blocks inside
/// libsimplex: Kotlin calls it on `Dispatchers.IO`.
#[no_mangle]
pub extern "system" fn Java_chat_cursus_core_Native_call(
    mut env: JNIEnv,
    _class: JClass,
    request: JString,
) -> jstring {
    let reply = match env.get_string(&request).map(String::from) {
        Ok(text) => match CString::new(text) {
            // SAFETY: valid NUL-terminated string; the reply is freed below.
            Ok(c) => take(unsafe { cursus_bridge::cursus_call(c.as_ptr()) }),
            Err(_) => None,
        },
        Err(_) => None,
    }
    .unwrap_or_else(|| r#"{"error":"bad request"}"#.to_string());
    env.new_string(reply).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// The next core event as JSON, or null after `timeoutMs` without one.
#[no_mangle]
pub extern "system" fn Java_chat_cursus_core_Native_nextEvent(env: JNIEnv, _class: JClass, timeout_ms: jint) -> jstring {
    match take(cursus_bridge::cursus_next_event(timeout_ms)) {
        Some(event) => env.new_string(event).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// Copy a bridge string and free it.
fn take(raw: *mut c_char) -> Option<String> {
    if raw.is_null() {
        return None;
    }
    // SAFETY: the bridge returns NUL-terminated UTF-8 from CString::into_raw.
    let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
    // SAFETY: freed exactly once, by the library that allocated it.
    unsafe { cursus_bridge::cursus_free(raw) };
    Some(text)
}
