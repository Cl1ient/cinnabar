#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod entry;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Runs the client through its shared startup and exit dispatch.
fn main() {
    entry::dispatch();
}
