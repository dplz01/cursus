use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "linux" {
        return;
    }

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let musl = target_env == "musl";
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest_dir.join("native/musl_glibc_compat.c");
    println!("cargo:rerun-if-changed={}", src.display());

    // Static lib linked into cursus so fcntl64 lives in the main link map
    // (musl can then relocate glibc libHS*.so against it). Also -u pulls the
    // symbol even if Rust never references it by name.
    let mut build = cc::Build::new();
    build.file(&src).warnings(true);
    if musl {
        build.define("CURSUS_MUSL_SHIM_IMPL", "1");
    } else {
        build.define("CURSUS_MUSL_SHIM_IMPL", "0");
    }
    build.compile("cursus_musl_glibc_compat");

    if musl {
        println!("cargo:rustc-link-arg=-Wl,--export-dynamic");
        println!("cargo:rustc-link-arg=-Wl,-u,fcntl64");
        println!("cargo:rustc-link-arg=-Wl,-u,stat64");
        println!("cargo:rustc-link-arg=-Wl,-u,__memcpy_chk");
    }

    // Shared object for LD_PRELOAD re-exec (most reliable on musl+gcompat).
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let so = out_dir.join("libcursus_musl_glibc_compat.so");
    let cc = env::var("CC").unwrap_or_else(|_| "cc".into());
    let mut cmd = Command::new(&cc);
    cmd.args(["-shared", "-fPIC", "-O2", "-Wall"])
        .arg(if musl {
            "-DCURSUS_MUSL_SHIM_IMPL=1"
        } else {
            "-DCURSUS_MUSL_SHIM_IMPL=0"
        })
        .arg("-o")
        .arg(&so)
        .arg(&src);

    match cmd.output() {
        Ok(out) if out.status.success() => {
            // Required by musl_compat.rs `include_bytes!(env!(...))` on musl.
            println!("cargo:rustc-env=CURSUS_MUSL_GLIBC_COMPAT={}", so.display());
            if musl {
                let beside = manifest_dir
                    .join("../third_party/libsimplex/libcursus_musl_glibc_compat.so");
                if let Some(parent) = beside.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::copy(&so, &beside) {
                    Ok(_) => println!(
                        "cargo:warning=musl glibc shim → {}",
                        beside.display()
                    ),
                    Err(err) => println!(
                        "cargo:warning=could not copy musl shim to {}: {err}",
                        beside.display()
                    ),
                }
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            println!("cargo:warning=shared musl glibc compat shim failed ({cc}): {stderr}");
            if musl {
                panic!("musl build requires libcursus_musl_glibc_compat.so: {stderr}");
            }
        }
        Err(err) => {
            println!("cargo:warning=could not run {cc} for shared musl shim: {err}");
            if musl {
                panic!("musl build requires a C compiler to build the glibc compat shim: {err}");
            }
        }
    }
}
