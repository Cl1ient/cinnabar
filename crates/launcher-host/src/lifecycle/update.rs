//! Background signed downloads and an offline helper handoff after the client shuts down.

mod handoff;
#[cfg(test)]
mod tests;
mod worker;

use launcher::{
    install_layout::InstallLayout,
    menu::settings_options::{SETTINGS_FILE, SettingsOptions},
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use update_manifest::Verdict;
use update_manifest::platform_key;

const URL_ENV: &str = "CINNABAR_UPDATE_URL";
const DISABLE_ENV: &str = "CINNABAR_UPDATE_CHECK";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
static LAYOUT: OnceLock<InstallLayout> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(false);
static REVISION: AtomicU64 = AtomicU64::new(0);
static RUNNING: AtomicBool = AtomicBool::new(false);
static READY: Mutex<Option<Ready>> = Mutex::new(None);

#[derive(Clone)]
struct Ready {
    stage: PathBuf,
    version: String,
}

#[derive(Deserialize, Serialize)]
struct Stamp {
    checked_at: u64,
    #[serde(default)]
    current: String,
}

/// Configures the installed updater from persisted settings; development checkouts do nothing.
pub fn check_in_background(layout: &InstallLayout) {
    if !layout.is_installed() {
        return;
    }
    let _ = LAYOUT.set(layout.clone());
    let settings = SettingsOptions::load(&layout.user_config_root.join(SETTINGS_FILE));
    set_enabled(settings.automatic_updates());
}

/// Applies the user's live preference, including cancelling an incomplete download.
pub fn set_enabled(enabled: bool) {
    let enabled = enabled && !std::env::var(DISABLE_ENV).is_ok_and(|value| value == "0");
    let changed = ENABLED.swap(enabled, Ordering::SeqCst) != enabled;
    if changed {
        REVISION.fetch_add(1, Ordering::SeqCst);
    }
    if enabled
        && changed
        && let Some(layout) = LAYOUT.get()
    {
        worker::start(layout.clone());
    }
}

/// A ready notice comes only from the worker's verified stage, never an untrusted JSON notice.
pub fn ready_version() -> Option<String> {
    if !ENABLED.load(Ordering::SeqCst) {
        return None;
    }
    READY
        .lock()
        .ok()?
        .as_ref()
        .map(|ready| ready.version.clone())
}

/// Starts the detached offline helper after normal client and child-process shutdown.
pub fn apply_after_exit() {
    if !ENABLED.load(Ordering::SeqCst) {
        return;
    }
    let Some(layout) = LAYOUT.get() else {
        return;
    };
    let ready = READY.lock().ok().and_then(|ready| ready.clone());
    if let Some(ready) = ready
        && let Err(error) = handoff::spawn(layout, &ready)
    {
        tracing::warn!("could not start updater: {error:#}");
    }
}

/// Acknowledges the first successful client frame without doing disk work on the UI thread.
pub fn acknowledge_startup() {
    let Some(layout) = LAYOUT.get().cloned() else {
        return;
    };
    std::thread::spawn(move || {
        let result = handoff::installed_target().and_then(|target| {
            update_installer::apply::acknowledge(
                &layout.update_dir(),
                &target,
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
            )
        });
        if let Err(error) = result {
            tracing::debug!("update startup acknowledgement: {error:#}");
        }
    });
}

/// A future stamp is due too, so a corrected clock cannot suppress updates indefinitely.
fn due(last_checked: Option<u64>, now: u64) -> bool {
    last_checked.is_none_or(|last| now < last || now - last >= CHECK_INTERVAL.as_secs())
}

/// Refuses non-HTTPS manifest URLs before creating a request.
fn https(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some())
}
