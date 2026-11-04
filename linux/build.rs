use std::env;

fn main() {
    if env::var_os("CARGO_FEATURE_MUSL_GLIBC_COMPAT").is_none() {
        return;
    }
    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "linux" {
        return;
    }
    if env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() != "musl" {
        return;
    }
    // Export statically linked musl→glibc shim symbols for dlopen'd libHS*.so.
    println!("cargo:rustc-link-arg=-Wl,--export-dynamic");
    println!("cargo:rustc-link-arg=-Wl,-u,fcntl64");
    println!("cargo:rustc-link-arg=-Wl,-u,stat64");
    println!("cargo:rustc-link-arg=-Wl,-u,fstat64");
    println!("cargo:rustc-link-arg=-Wl,-u,lstat64");
    println!("cargo:rustc-link-arg=-Wl,-u,__memcpy_chk");
}
