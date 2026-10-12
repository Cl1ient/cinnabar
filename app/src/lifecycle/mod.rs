//! Process lifecycle around the client: crash capture, first-run asset preparation, update checks.

use anyhow::{Context, Result};
use launcher_host::lifecycle::{crash, update};

mod client_logs;

use launcher::install_layout::InstallLayout;
use std::{sync::OnceLock, time::Instant};

static LAUNCHED_AT: OnceLock<Instant> = OnceLock::new();

/// When client startup began; the first call records it.
pub(crate) fn launched_at() -> Instant {
    *LAUNCHED_AT.get_or_init(Instant::now)
}

/// Runs pre-window duties for a packaged install; a no-op for development checkouts.
/// `assets_overridden` skips asset preparation when the caller supplied its own carrier path.
/// Returns `false` when the user quit first-time setup, so the client should exit quietly.
pub fn before_run(assets_overridden: bool) -> Result<bool> {
    launched_at();
    let layout = InstallLayout::discover().context("resolve install layout")?;
    client_logs::capture(&layout);
    client_logs::install_panic_hook(&layout);
    crash::prune_reports(&layout);
    if !layout.is_installed() {
        return Ok(true);
    }
    update::check_in_background(&layout);

    // An update helper waiting on this start must not mistake a long asset download for a hang.
    if !assets_overridden
        && update::while_preparing(
            &layout,
            &first_run::progress_file(&layout),
            first_run::awaiting_user,
            || first_run::ensure_prepared(&layout),
        )? == first_run::Outcome::Quit
    {
        return Ok(false);
    }
    Ok(true)
}

/// Acknowledges startup only after an earlier frame completed and the app kept running.
pub fn acknowledge_startup(mut frames: bevy::prelude::Local<u8>) {
    if *frames < 2 {
        *frames += 1;
        if *frames == 2 {
            update::acknowledge_startup();
        }
    }
}
