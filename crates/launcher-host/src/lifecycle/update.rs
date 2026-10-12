//! One background signed-release check and the launcher's explicit update decisions.

mod handoff;
#[cfg(test)]
mod tests;
mod worker;

use launcher::{
    install_layout::InstallLayout,
    menu::{
        settings_options::{SETTINGS_FILE, SettingsOptions},
        updates::{Action, Preferences, Ready as Notice, View},
    },
};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};
use time::OffsetDateTime;
use update_installer::startup;
use update_manifest::channel::Channel;

const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
static LAYOUT: OnceLock<InstallLayout> = OnceLock::new();
static STATE: Mutex<State> = Mutex::new(State::new());

#[derive(Clone)]
struct Ready {
    expires: OffsetDateTime,
    stage: PathBuf,
    notice: Notice,
    channel: Channel,
    switch_now: bool,
}

struct State {
    preferences: Preferences,
    ready: Option<Ready>,
    dismissed: bool,
    requested: bool,
    switch_now: bool,
    switch_available: bool,
    stable_available: bool,
    protocol_required: bool,
    message: Option<String>,
    generation: u64,
    running: bool,
}

impl State {
    /// Holds one session's preferences and consent, without a persisted transaction state.
    const fn new() -> Self {
        Self {
            preferences: Preferences::DEFAULT,
            ready: None,
            dismissed: false,
            requested: false,
            switch_now: false,
            switch_available: false,
            stable_available: false,
            protocol_required: false,
            message: None,
            generation: 0,
            running: false,
        }
    }

    /// An explicit Later choice suppresses the optional quit-time install for this session too.
    fn applies_on_exit(&self, now: OffsetDateTime) -> bool {
        self.ready
            .as_ref()
            .is_some_and(|ready| ready.notice.writable && ready.expires > now)
            && (self.requested || (self.preferences.install_on_quit && !self.dismissed))
    }
}

/// Configures only installed clients; cached stages are checked even when network checks are off.
pub fn check_in_background(layout: &InstallLayout) {
    if !layout.is_installed() {
        return;
    }
    let _ = LAYOUT.set(layout.clone());
    configure(
        SettingsOptions::load(&layout.user_config_root.join(SETTINGS_FILE)).update_preferences(),
    );
    worker::start();
}

/// Shares live preferences with the worker; channel changes always clear earlier consent.
pub fn configure(mut preferences: Preferences) {
    if std::env::var("CINNABAR_UPDATE_CHECK").is_ok_and(|value| value == "0") {
        preferences.checks = false;
    }
    let changed = if let Ok(mut state) = STATE.lock() {
        let changed = state.preferences != preferences;
        if changed {
            if state.preferences.channel != preferences.channel {
                state.ready = None;
                state.requested = false;
                state.dismissed = false;
                state.switch_now = false;
                state.switch_available = false;
            }
            state.preferences = preferences;
            state.generation += 1;
        }
        changed
    } else {
        false
    };
    if changed {
        worker::start();
    }
}

/// Presents a verified ready stage and channel availability through the launcher data contract.
pub fn view() -> View {
    let now = OffsetDateTime::now_utc();
    let Ok(state) = STATE.lock() else {
        return View::default();
    };
    View {
        ready: state
            .ready
            .as_ref()
            .filter(|ready| !state.dismissed && ready.expires > now)
            .map(|ready| {
                let mut notice = ready.notice.clone();
                notice.protocol_required = state.protocol_required;
                notice
            }),
        stable_available: state.stable_available,
        switch_available: state.switch_available,
        message: state.message.clone(),
    }
}

/// Raises the wording after a protocol refusal; it never changes update consent.
pub fn protocol_required() {
    if let Ok(mut state) = STATE.lock() {
        state.protocol_required = true;
    }
}

/// Returns true only when a verified, writable stage should trigger normal client shutdown.
pub fn act(action: Action) -> bool {
    let now = OffsetDateTime::now_utc();
    let mut download = None;
    let mut switch = false;
    let quit = if let Ok(mut state) = STATE.lock() {
        match action {
            Action::Restart
                if state
                    .ready
                    .as_ref()
                    .is_some_and(|ready| ready.notice.writable && ready.expires > now) =>
            {
                state.requested = true;
                true
            }
            Action::Later => {
                state.dismissed = true;
                state.requested = false;
                false
            }
            Action::Download => {
                download = state.ready.clone();
                false
            }
            Action::SwitchNow if state.switch_available => {
                state.switch_now = true;
                state.switch_available = false;
                state.message = Some("Downloading this channel's update…".into());
                state.dismissed = false;
                state.generation += 1;
                switch = true;
                false
            }
            _ => false,
        }
    } else {
        false
    };
    if let Some(ready) = download {
        handoff::open_download(ready);
    }
    if switch {
        worker::start();
    }
    quit
}

/// Starts the offline helper only for a click or the separately saved quit-time opt-in.
pub fn apply_after_exit() {
    let ready = STATE.lock().ok().and_then(|state| {
        state
            .applies_on_exit(OffsetDateTime::now_utc())
            .then(|| state.ready.clone())
            .flatten()
    });
    if let (Some(layout), Some(ready)) = (LAYOUT.get(), ready)
        && let Err(error) = handoff::spawn(layout, &ready)
    {
        tracing::warn!("could not start updater: {error:#}");
    }
}

/// Signals a complete first frame to the waiting helper.
pub fn acknowledge_startup() {
    let Some(marker) = LAYOUT.get().and_then(startup_marker) else {
        return;
    };
    std::thread::spawn(move || {
        let _ = std::fs::File::options()
            .write(true)
            .create_new(true)
            .open(marker);
    });
}

/// Runs `prepare`, relaying each change to its `progress` report to a waiting update helper;
/// `awaiting_user` marks reports during which the helper must not time out.
pub fn while_preparing<T>(
    layout: &InstallLayout,
    progress: &std::path::Path,
    awaiting_user: fn(&[u8]) -> bool,
    prepare: impl FnOnce() -> T,
) -> T {
    let Some(marker) = startup_marker(layout) else {
        return prepare();
    };
    let mut relay = startup::Relay::start(marker, progress.to_owned(), awaiting_user);
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let watcher = std::thread::spawn(move || {
        while stopped.recv_timeout(startup::PROGRESS_POLL)
            == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        {
            relay.poll();
        }
        relay
    });
    let result = prepare();
    drop(stop);
    if let Ok(relay) = watcher.join() {
        relay.finish();
    }
    result
}

/// Whether this process, or the client that started it, was launched by the update helper.
pub fn launched_by_updater() -> bool {
    std::env::var_os(startup::STARTUP_ENV).is_some()
}

/// The helper's marker, accepted only inside this install's update directory.
fn startup_marker(layout: &InstallLayout) -> Option<PathBuf> {
    let marker = PathBuf::from(std::env::var_os(startup::STARTUP_ENV)?);
    (marker.starts_with(layout.update_dir())
        && marker
            .file_name()
            .is_some_and(|name| name == startup::STARTED))
    .then_some(marker)
}

/// A corrected clock cannot suppress the next daily check.
fn due(last: Option<u64>, now: u64) -> bool {
    last.is_none_or(|last| now < last || now - last >= CHECK_INTERVAL.as_secs())
}
