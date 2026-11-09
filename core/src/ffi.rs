//! Runtime FFI to official `libsimplex`.
//!
//! Symbols match `packages/simplex-chat-nodejs/cpp/simplex.h` /
//! `apps/ios/SimpleXChat/SimpleX.h`.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::ptr;
use std::sync::Once;
#[cfg(target_env = "musl")]
use std::sync::OnceLock;

use libloading::{Library, Symbol};

use crate::error::{Error, Result};
use crate::paths::{default_libs_dir, lib_filename};

type HsInit = unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char);
type ChatMigrateInit = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    *const c_char,
    *mut *mut std::ffi::c_void,
) -> *mut c_char;
type ChatSendCmd = unsafe extern "C" fn(*mut std::ffi::c_void, *const c_char) -> *mut c_char;
type ChatRecvMsgWait = unsafe extern "C" fn(*mut std::ffi::c_void, c_int) -> *mut c_char;
type ChatCloseStore = unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut c_char;
/// Decrypt CryptoFile on disk → plaintext path. Empty string = success.
type ChatDecryptFile = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
) -> *mut c_char;
/// Encrypt plaintext → CryptoFile; returns JSON WriteFileResult (needs chat ctrl).
type ChatEncryptFile = unsafe extern "C" fn(
    *mut std::ffi::c_void,
    *const c_char,
    *const c_char,
) -> *mut c_char;

static HS_INIT: Once = Once::new();

/// The one loaded libsimplex, never unloaded. `hs_init` starts the GHC
/// runtime once per process and its threads (network, timers, GC) keep
/// running code from the library after a session is closed; dropping the
/// last handle `dlclose`d it underneath them → SIGSEGV (reproduced by
/// opening sessions again after closing networked ones, see
/// `call::simplex_e2e_tests`; in the app: on quit / after a failed unlock).
static LOADED: std::sync::Mutex<Option<std::sync::Arc<LibSimplex>>> = std::sync::Mutex::new(None);

pub struct LibSimplex {
    _library: Library,
    chat_migrate_init: ChatMigrateInit,
    chat_send_cmd: ChatSendCmd,
    chat_recv_msg_wait: ChatRecvMsgWait,
    chat_close_store: ChatCloseStore,
    chat_decrypt_file: ChatDecryptFile,
    chat_encrypt_file: ChatEncryptFile,
}

// Shared across the UI and receive threads via Arc.
unsafe impl Send for LibSimplex {}
unsafe impl Sync for LibSimplex {}

impl LibSimplex {
    /// Load once per process and share (see [`LOADED`]).
    pub fn shared(libs_dir: Option<&Path>) -> Result<std::sync::Arc<Self>> {
        let mut slot = LOADED.lock().map_err(|_| Error::LibraryLoad("libsimplex lock poisoned".into()))?;
        if let Some(lib) = slot.as_ref() {
            return Ok(std::sync::Arc::clone(lib));
        }
        let lib = std::sync::Arc::new(Self::load(libs_dir)?);
        *slot = Some(std::sync::Arc::clone(&lib));
        Ok(lib)
    }

    fn load(libs_dir: Option<&Path>) -> Result<Self> {
        let dir = libs_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(default_libs_dir);
        let path = dir.join(lib_filename());
        if !path.exists() {
            return Err(Error::LibraryNotFound {
                searched: path.display().to_string(),
            });
        }

        // Dependent libHS*.so resolve via libsimplex's RUNPATH ($ORIGIN).
        // (Setting LD_LIBRARY_PATH here would be useless — ld.so reads it only
        // at process start — and racy: setenv while other threads run is UB.)

        #[cfg(target_env = "musl")]
        {
            // RTLD_GLOBAL + statically linked fcntl64 (main() already
            // re-exec'd with LD_PRELOAD — see musl_compat.rs).
            load_musl_glibc_compat();
            if is_musl_host() && !crate::musl_compat::fcntl64_resolvable() {
                return Err(Error::LibraryLoad(format!(
                    "fcntl64 is still not visible to the dynamic linker on musl. \
                     Install `apk add gcompat libucontext` and rebuild Cursus \
                     on the device. ({})",
                    crate::musl_compat::compat_status_line()
                )));
            }
        }

        if let Some(hint) = glibc_loader_hint() {
            return Err(Error::LibraryLoad(hint));
        }

        // SAFETY: we only call C ABI symbols exported by libsimplex.
        let library = unsafe {
            Library::new(&path).map_err(|err| Error::LibraryLoad(explain_load_error(&err)))?
        };
        let hs_init: Symbol<HsInit> = unsafe { library.get(b"hs_init_with_rtsopts\0") }
            .map_err(|_| Error::MissingSymbol("hs_init_with_rtsopts".into()))?;
        let chat_migrate_init: Symbol<ChatMigrateInit> =
            unsafe { library.get(b"chat_migrate_init\0") }
                .map_err(|_| Error::MissingSymbol("chat_migrate_init".into()))?;
        let chat_send_cmd: Symbol<ChatSendCmd> = unsafe { library.get(b"chat_send_cmd\0") }
            .map_err(|_| Error::MissingSymbol("chat_send_cmd".into()))?;
        let chat_recv_msg_wait: Symbol<ChatRecvMsgWait> =
            unsafe { library.get(b"chat_recv_msg_wait\0") }
                .map_err(|_| Error::MissingSymbol("chat_recv_msg_wait".into()))?;
        let chat_close_store: Symbol<ChatCloseStore> =
            unsafe { library.get(b"chat_close_store\0") }
                .map_err(|_| Error::MissingSymbol("chat_close_store".into()))?;
        let chat_decrypt_file: Symbol<ChatDecryptFile> =
            unsafe { library.get(b"chat_decrypt_file\0") }
                .map_err(|_| Error::MissingSymbol("chat_decrypt_file".into()))?;
        let chat_encrypt_file: Symbol<ChatEncryptFile> =
            unsafe { library.get(b"chat_encrypt_file\0") }
                .map_err(|_| Error::MissingSymbol("chat_encrypt_file".into()))?;

        HS_INIT.call_once(|| unsafe {
            init_haskell(*hs_init);
        });

        Ok(Self {
            chat_migrate_init: *chat_migrate_init,
            chat_send_cmd: *chat_send_cmd,
            chat_recv_msg_wait: *chat_recv_msg_wait,
            chat_close_store: *chat_close_store,
            chat_decrypt_file: *chat_decrypt_file,
            chat_encrypt_file: *chat_encrypt_file,
            _library: library,
        })
    }

    pub fn migrate_init(&self, db_prefix: &Path, key: &str, confirm: &str) -> Result<*mut std::ffi::c_void> {
        let prefix = CString::new(db_prefix.to_string_lossy().as_bytes())?;
        let key = CString::new(key)?;
        let confirm = CString::new(confirm)?;
        let mut ctrl: *mut std::ffi::c_void = ptr::null_mut();
        // SAFETY: pointers are valid CStrings; ctrl out-param is written by Haskell.
        let raw = unsafe {
            (self.chat_migrate_init)(prefix.as_ptr(), key.as_ptr(), confirm.as_ptr(), &mut ctrl)
        };
        // Our copy of the key is no longer needed: wipe it before freeing.
        zeroize::Zeroize::zeroize(&mut key.into_bytes_with_nul());
        let json = take_c_string(raw)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let ty = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty != "ok" {
            return Err(Error::Migration(json));
        }
        if ctrl.is_null() {
            return Err(Error::NoController);
        }
        Ok(ctrl)
    }

    pub fn send_cmd(&self, ctrl: *mut std::ffi::c_void, cmd: &str) -> Result<String> {
        let cmd = CString::new(cmd)?;
        // SAFETY: ctrl must come from migrate_init; cmd is a valid C string.
        let raw = unsafe { (self.chat_send_cmd)(ctrl, cmd.as_ptr()) };
        take_c_string(raw)
    }

    pub fn recv_msg_wait(&self, ctrl: *mut std::ffi::c_void, wait_us: i32) -> Result<Option<String>> {
        // SAFETY: ctrl must come from migrate_init.
        let raw = unsafe { (self.chat_recv_msg_wait)(ctrl, wait_us) };
        let s = take_c_string(raw)?;
        if s.is_empty() {
            Ok(None)
        } else {
            Ok(Some(s))
        }
    }

    pub fn close_store(&self, ctrl: *mut std::ffi::c_void) -> Result<String> {
        let raw = unsafe { (self.chat_close_store)(ctrl) };
        take_c_string(raw)
    }

    /// Decrypt a CryptoFile (`chat_decrypt_file`). Empty error string means success.
    pub fn decrypt_file(
        &self,
        from: &Path,
        key: &str,
        nonce: &str,
        to: &Path,
    ) -> Result<()> {
        let from = CString::new(from.to_string_lossy().as_bytes())?;
        let key = CString::new(key)?;
        let nonce = CString::new(nonce)?;
        let to = CString::new(to.to_string_lossy().as_bytes())?;
        // SAFETY: all pointers are valid CStrings; Haskell writes a malloc'd error or "".
        let raw = unsafe {
            (self.chat_decrypt_file)(from.as_ptr(), key.as_ptr(), nonce.as_ptr(), to.as_ptr())
        };
        let err = take_c_string(raw)?;
        if err.is_empty() {
            Ok(())
        } else {
            Err(Error::Command(format!("decrypt file: {err}")))
        }
    }

    /// Encrypt plaintext → CryptoFile (`chat_encrypt_file`). Returns fileKey / fileNonce.
    pub fn encrypt_file(
        &self,
        ctrl: *mut std::ffi::c_void,
        from: &Path,
        to: &Path,
    ) -> Result<(String, String)> {
        let from = CString::new(from.to_string_lossy().as_bytes())?;
        let to = CString::new(to.to_string_lossy().as_bytes())?;
        // SAFETY: ctrl from migrate_init; paths are valid CStrings.
        let raw = unsafe { (self.chat_encrypt_file)(ctrl, from.as_ptr(), to.as_ptr()) };
        let json = take_c_string(raw)?;
        let value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|e| Error::Command(format!("encrypt file JSON: {e}; raw={json}")))?;
        let ty = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty == "error" {
            let err = value
                .get("writeError")
                .and_then(|v| v.as_str())
                .unwrap_or(json.as_str());
            return Err(Error::Command(format!("encrypt file: {err}")));
        }
        let args = value
            .get("cryptoArgs")
            .ok_or_else(|| Error::Command(format!("encrypt file: missing cryptoArgs: {json}")))?;
        let key = args
            .get("fileKey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Command("encrypt file: missing fileKey".into()))?
            .to_string();
        let nonce = args
            .get("fileNonce")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Command("encrypt file: missing fileNonce".into()))?
            .to_string();
        Ok((key, nonce))
    }
}

fn take_c_string(ptr: *mut c_char) -> Result<String> {
    if ptr.is_null() {
        return Err(Error::Command("null pointer from libsimplex".into()));
    }
    // SAFETY: libsimplex returns malloc'd NUL-terminated UTF-8.
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { libc_free(ptr as *mut std::ffi::c_void) };
    Ok(s)
}

unsafe fn libc_free(ptr: *mut std::ffi::c_void) {
    extern "C" {
        fn free(ptr: *mut std::ffi::c_void);
    }
    free(ptr);
}

unsafe fn init_haskell(hs_init: HsInit) {
    // Mirrors packages/simplex-chat-nodejs/cpp/simplex.cc
    let mut args = [
        CString::new("cursus").unwrap(),
        CString::new("+RTS").unwrap(),
        // Phones: a smaller allocation area (less resident memory).
        CString::new(if cfg!(target_os = "android") { "-A16m" } else { "-A64m" }).unwrap(),
        CString::new("-H64m").unwrap(),
        CString::new("-xn").unwrap(),
        CString::new("--install-signal-handlers=no").unwrap(),
    ];
    let mut ptrs: Vec<*mut c_char> = args
        .iter_mut()
        .map(|s| s.as_ptr() as *mut c_char)
        .collect();
    ptrs.push(ptr::null_mut());
    let mut argc = (ptrs.len() - 1) as c_int;
    let mut argv = ptrs.as_mut_ptr();
    hs_init(&mut argc, &mut argv);
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_musl_host() -> bool {
    #[cfg(target_os = "linux")]
    {
        [
            "/lib/ld-musl-aarch64.so.1",
            "/usr/lib/ld-musl-aarch64.so.1",
            "/lib/ld-musl-x86_64.so.1",
            "/usr/lib/ld-musl-x86_64.so.1",
        ]
        .iter()
        .any(|p| Path::new(p).exists())
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Kept alive so RTLD_GLOBAL symbols from gcompat / our shim stay mapped.
#[cfg(all(target_env = "musl", target_os = "linux"))]
static MUSL_COMPAT_LIBS: OnceLock<Vec<libloading::os::unix::Library>> = OnceLock::new();

/// Preload gcompat, libucontext, and our fcntl64/stat64/... shim so musl can
/// relocate official glibc-built libHS*.so objects.
#[cfg(all(target_env = "musl", target_os = "linux"))]
fn load_musl_glibc_compat() {
    if !is_musl_host() {
        return;
    }
    let _ = crate::musl_compat::fcntl64_resolvable();

    let _ = MUSL_COMPAT_LIBS.get_or_init(|| {
        use libloading::os::unix::{Library, RTLD_GLOBAL, RTLD_NOW};

        let mut libs = Vec::new();

        if let Some(path) = crate::musl_compat::resolve_shim_so() {
            // SAFETY: our build-time shim; RTLD_GLOBAL for later libsimplex relocs.
            if let Ok(lib) = unsafe {
                Library::open(Some(path.as_os_str()), RTLD_NOW | RTLD_GLOBAL)
            } {
                libs.push(lib);
            }
        }

        let candidates = [
            "/lib/libgcompat.so.0",
            "/usr/lib/libgcompat.so.0",
            "/lib/libucontext.so.1",
            "/usr/lib/libucontext.so.1",
            "/lib/libucontext.so.0",
            "/usr/lib/libucontext.so.0",
        ];
        for path in candidates {
            if !Path::new(path).exists() {
                continue;
            }
            // SAFETY: system libs; RTLD_GLOBAL so later libsimplex relocations see them.
            if let Ok(lib) = unsafe { Library::open(Some(path), RTLD_NOW | RTLD_GLOBAL) } {
                libs.push(lib);
            }
        }

        libs
    });
}

/// Official linux-aarch64/x86_64 libsimplex is a **glibc** build. On musl
/// (Alpine, many postmarketOS images) `ld-linux-*.so.1` is missing unless
/// `gcompat` is installed.
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn glibc_loader_hint() -> Option<String> {
    const LOADER: &str = "/lib/ld-linux-aarch64.so.1";
    const LOADER_USR: &str = "/lib64/ld-linux-aarch64.so.1";
    if Path::new(LOADER).exists() || Path::new(LOADER_USR).exists() {
        return None;
    }
    if !is_musl_host() {
        return Some(format!(
            "missing {LOADER} (glibc dynamic linker needed by libsimplex)."
        ));
    }
    Some(format!(
        "missing {LOADER} (glibc dynamic linker). This looks like musl \
         (Alpine / musl postmarketOS). Install `apk add gcompat libucontext` and \
         build Cursus on the device (musl builds include the compat layer)."
    ))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn glibc_loader_hint() -> Option<String> {
    const LOADER: &str = "/lib64/ld-linux-x86-64.so.2";
    const LOADER_ALT: &str = "/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2";
    if Path::new(LOADER).exists() || Path::new(LOADER_ALT).exists() {
        return None;
    }
    if !is_musl_host() {
        return Some(format!(
            "missing {LOADER} (glibc dynamic linker needed by libsimplex)."
        ));
    }
    Some(format!(
        "missing {LOADER} (glibc dynamic linker). On musl: `apk add gcompat libucontext` \
         and build Cursus on the device."
    ))
}

#[cfg(not(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "x86_64"))))]
fn glibc_loader_hint() -> Option<String> {
    None
}

fn explain_load_error(err: &libloading::Error) -> String {
    let msg = err.to_string();
    if msg.contains("ld-linux-aarch64.so.1") || msg.contains("ld-linux-x86-64.so.2") {
        format!(
            "{msg}. Official libsimplex needs the glibc loader. On musl: \
             `apk add gcompat libucontext`, then build Cursus on the device \
             (`cargo run -p cursus-linux`)."
        )
    } else if msg.contains("fcntl64")
        || msg.contains("stat64")
        || msg.contains("Error relocating")
        || msg.contains("symbol not found")
    {
        #[cfg(target_env = "musl")]
        {
            format!(
                "{msg}. ({}). Rebuild Cursus on the musl device after \
                 `apk add gcompat libucontext`.",
                crate::musl_compat::compat_status_line()
            )
        }
        #[cfg(not(target_env = "musl"))]
        {
            format!(
                "{msg}. This is a glibc build of Cursus; on musl (Alpine / \
                 postmarketOS) build Cursus on the device instead."
            )
        }
    } else {
        msg
    }
}

// Allow CString::new errors to map into our Error via From.
impl From<std::ffi::NulError> for Error {
    fn from(value: std::ffi::NulError) -> Self {
        Error::Other(anyhow::anyhow!("CString contained NUL: {value}"))
    }
}
