//! Make official glibc-built `libsimplex` loadable on musl (Alpine / pmOS).
//!
//! Compiled automatically for musl targets (`target_env = "musl"`); glibc
//! builds contain none of this.
//!
//! `dlopen(RTLD_GLOBAL)` alone is not enough: musl relocates `libHS*.so`
//! before those globals are reliably visible. We therefore:
//! 1. Link shim symbols into the cursus binary (`-rdynamic` / `-u fcntl64`).
//! 2. Embed the `.so` at compile time and write it next to libsimplex on launch.
//! 3. Re-exec once with `LD_PRELOAD` of that shim (+ gcompat when present).

use std::env;
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::paths::default_libs_dir;

#[cfg(target_os = "linux")]
extern "C" {
    fn cursus_musl_glibc_compat_anchor() -> i32;
}

/// Shim bytes compiled into the musl build (see `core/build.rs`).
#[cfg(all(target_os = "linux", target_env = "musl"))]
const EMBEDDED_SHIM: &[u8] = include_bytes!(env!("CURSUS_MUSL_GLIBC_COMPAT"));

/// True when the process is running under musl's dynamic linker.
pub fn is_musl_host() -> bool {
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

fn shim_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.push(default_libs_dir().join("libcursus_musl_glibc_compat.so"));
    if let Some(p) = option_env!("CURSUS_MUSL_GLIBC_COMPAT") {
        out.push(PathBuf::from(p));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("libcursus_musl_glibc_compat.so"));
        }
    }
    out.push(env::temp_dir().join("libcursus_musl_glibc_compat.so"));
    out
}

fn shim_exports_fcntl64(path: &Path) -> bool {
    if let Ok(output) = std::process::Command::new("nm")
        .args(["-D", "--defined-only"])
        .arg(path)
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            return text
                .lines()
                .any(|l| l.split_whitespace().last() == Some("fcntl64"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        use libloading::os::unix::{Library, RTLD_NOW};
        let Ok(lib) = (unsafe { Library::open(Some(path.as_os_str()), RTLD_NOW) }) else {
            return false;
        };
        let ok = unsafe { lib.get::<unsafe extern "C" fn()>(b"fcntl64\0").is_ok() };
        drop(lib);
        ok
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        false
    }
}

/// Write the embedded musl shim next to libsimplex if it is missing / empty.
pub fn materialize_shim_so() -> Option<PathBuf> {
    for path in shim_candidates() {
        if path.is_file() && shim_exports_fcntl64(&path) {
            return Some(path);
        }
    }

    #[cfg(all(target_os = "linux", target_env = "musl"))]
    {
        if EMBEDDED_SHIM.is_empty() {
            return None;
        }
        let destinations = [
            default_libs_dir().join("libcursus_musl_glibc_compat.so"),
            env::temp_dir().join("libcursus_musl_glibc_compat.so"),
        ];
        for dest in destinations {
            if let Some(parent) = dest.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if fs::write(&dest, EMBEDDED_SHIM).is_err() {
                continue;
            }
            if shim_exports_fcntl64(&dest) {
                eprintln!("cursus: wrote musl glibc shim → {}", dest.display());
                return Some(dest);
            }
        }
    }

    None
}

/// Path to a non-empty musl shim `.so` (exports `fcntl64`), materializing if needed.
pub fn resolve_shim_so() -> Option<PathBuf> {
    materialize_shim_so()
}

/// If on musl, re-exec the current binary with `LD_PRELOAD` set so the shim
/// (and gcompat) are present for every subsequent `dlopen` of libsimplex.
///
/// Call this as the first line of `main`. Returns after the child path is
/// already preloaded, or immediately on glibc / when already re-exec'd.
pub fn ensure_ld_preload_reexec() {
    #[cfg(target_os = "linux")]
    {
        if !is_musl_host() {
            return;
        }
        if env::var_os("CURSUS_MUSL_COMPAT_READY").is_some() {
            return;
        }

        let mut preload: Vec<PathBuf> = Vec::new();
        if let Some(shim) = materialize_shim_so() {
            preload.push(shim);
        } else {
            eprintln!(
                "cursus: musl shim missing — `apk add build-base gcompat libucontext` \
                 and rebuild Cursus on this device"
            );
        }
        for p in [
            "/lib/libgcompat.so.0",
            "/usr/lib/libgcompat.so.0",
            "/lib/libucontext.so.1",
            "/usr/lib/libucontext.so.1",
            "/lib/libucontext.so.0",
            "/usr/lib/libucontext.so.0",
        ] {
            let path = PathBuf::from(p);
            if path.is_file() {
                preload.push(path);
            }
        }
        if preload.is_empty() {
            return;
        }

        let joined = env::join_paths(preload.iter()).unwrap_or_default();
        let mut new_preload = joined.into_string().unwrap_or_default();
        if let Ok(existing) = env::var("LD_PRELOAD") {
            if !existing.is_empty() {
                new_preload = format!("{new_preload}:{existing}");
            }
        }

        let Ok(exe) = env::current_exe() else {
            return;
        };
        let args: Vec<std::ffi::OsString> = env::args_os().skip(1).collect();

        let mut cmd = std::process::Command::new(&exe);
        cmd.args(&args)
            .env("LD_PRELOAD", &new_preload)
            .env("CURSUS_MUSL_COMPAT_READY", "1");

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = cmd.exec();
            eprintln!("cursus: musl LD_PRELOAD re-exec failed: {err}");
        }
    }
}

/// Touch the statically linked anchor and report whether `fcntl64` is visible
/// to the dynamic linker (needed before dlopening libsimplex on musl).
pub fn fcntl64_resolvable() -> bool {
    #[cfg(target_os = "linux")]
    {
        let _ = unsafe { cursus_musl_glibc_compat_anchor() };

        #[cfg(target_env = "musl")]
        {
            extern "C" {
                fn fcntl64();
            }
            let _keep: unsafe extern "C" fn() = fcntl64;
            std::mem::forget(_keep);
        }

        let name = CString::new("fcntl64").unwrap();
        extern "C" {
            fn dlsym(
                handle: *mut std::ffi::c_void,
                symbol: *const std::os::raw::c_char,
            ) -> *mut std::ffi::c_void;
        }
        let sym = unsafe { dlsym(std::ptr::null_mut(), name.as_ptr()) };
        !sym.is_null()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Human-readable status for load errors.
pub fn compat_status_line() -> String {
    let musl = is_musl_host();
    let shim = resolve_shim_so();
    let fcntl = fcntl64_resolvable();
    let anchor = {
        #[cfg(target_os = "linux")]
        {
            unsafe { cursus_musl_glibc_compat_anchor() }
        }
        #[cfg(not(target_os = "linux"))]
        {
            0
        }
    };
    format!(
        "musl={musl} shim={} fcntl64_visible={fcntl} static_anchor={anchor} preload={}",
        shim
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "none".into()),
        env::var("LD_PRELOAD").unwrap_or_default()
    )
}
