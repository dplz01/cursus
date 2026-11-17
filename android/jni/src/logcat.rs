//! `tracing` → logcat (tag "Cursus"), so `adb logcat -s Cursus` shows the
//! core's logs. Installed before the bridge's stderr logger, which then
//! stays out of the way (stderr goes nowhere on Android).

use std::ffi::{c_char, c_int, CString};
use std::io::{self, Write};

#[link(name = "log")]
extern "C" {
    fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

const INFO: c_int = 4;

/// One log line per write; tracing's fmt layer writes each event whole.
pub struct Logcat(Vec<u8>);

impl Write for Logcat {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Logcat {
    fn drop(&mut self) {
        let text = String::from_utf8_lossy(&self.0).trim_end().replace('\0', " ");
        if text.is_empty() {
            return;
        }
        if let Ok(c) = CString::new(text) {
            // SAFETY: valid C strings for the duration of the call.
            unsafe { __android_log_write(INFO, c"Cursus".as_ptr(), c.as_ptr()) };
        }
    }
}

pub fn install() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("CURSUS_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .without_time()
        .with_writer(|| Logcat(Vec::new()))
        .try_init();
}
