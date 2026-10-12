//! Windows release builds run without a console window. Linker arguments do this instead of a
//! `windows_subsystem` crate attribute, so tests can still include `main.rs` as a module.

fn main() {
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let release = std::env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_none();
    if !(windows && release) {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/SUBSYSTEM:WINDOWS");
        println!("cargo:rustc-link-arg-bins=/ENTRY:mainCRTStartup");
    } else {
        println!("cargo:rustc-link-arg-bins=-Wl,--subsystem,windows");
    }
}
