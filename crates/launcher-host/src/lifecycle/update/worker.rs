//! The existing HTTPS check streams one digest-named partial file on its background thread.
use super::*;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use time::OffsetDateTime;
use update_installer::stage::{self, Download, Selection};
use update_manifest::{TrustedKeys, channel::Channel, select::Check};

#[derive(Deserialize, Serialize)]
struct Stamp {
    checked_at: u64,
    current: String,
    #[serde(default)]
    build: u64,
}

/// A signed manifest can outlive the retained download it names; fetch the channel again.
#[derive(Debug, thiserror::Error)]
#[error("the update download is no longer published")]
pub(super) struct MissingArtifact;

/// A missing served document or payload warrants one refresh of the signed channel manifest.
pub(super) fn check_missing(status: reqwest::StatusCode) -> Result<()> {
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(MissingArtifact.into());
    }
    Ok(())
}

/// Refreshes a missing payload's manifest once, leaving every other failure for the next check.
pub(super) fn retry_missing(
    mut check: impl FnMut() -> Result<()>,
    active: impl FnOnce() -> bool,
) -> Result<()> {
    let result = check();
    if result
        .as_ref()
        .is_err_and(|error| error.is::<MissingArtifact>())
        && active()
    {
        check()
    } else {
        result
    }
}

/// Runs one worker at a time and retries when a preference changes during its request.
pub(super) fn start() {
    let Some(layout) = LAYOUT.get().cloned() else {
        return;
    };
    let (preferences, switch_now, generation) = {
        let Ok(mut state) = STATE.lock() else {
            return;
        };
        if state.running || state.requested || state.startup != StartupPhase::Complete {
            return;
        }
        state.running = true;
        (state.preferences, state.switch_now, state.generation)
    };
    std::thread::spawn(move || {
        if let Err(error) = retry_missing(
            || run(&layout, preferences, switch_now, generation),
            || active(generation),
        ) {
            tracing::debug!("update check: {error:#}");
            if let Ok(mut state) = STATE.lock() {
                if state.generation == generation && switch_now {
                    state.message =
                        Some("Could not download this channel's update. Try again.".into());
                    state.switch_available = true;
                }
            }
        }
        let changed = if let Ok(mut state) = STATE.lock() {
            state.running = false;
            state.generation != generation
        } else {
            false
        };
        if changed {
            start();
        }
    });
}

/// Rejects stale requests before publishing a stage or consuming another response chunk.
fn active(generation: u64) -> bool {
    STATE
        .lock()
        .is_ok_and(|state| state.accepts_worker(generation))
}

/// Reopens a verified ready stage, or checks the selected channel and resumes its artifact.
fn run(
    layout: &InstallLayout,
    preferences: Preferences,
    switch_now: bool,
    generation: u64,
) -> Result<()> {
    let Some(manifest_url) = preferences.channel.manifest_url() else {
        return Ok(());
    };
    let keys = update_manifest::parse_keys(update_installer::TRUSTED_KEYS)?;
    if keys.is_empty() {
        return Ok(());
    }
    let platform = update_manifest::platform_key(std::env::consts::OS, std::env::consts::ARCH)
        .context("unsupported platform")?;
    let root = layout.update_dir();
    fs::create_dir_all(&root)?;
    update_installer::helper::prune(&root, SystemTime::now());
    // Other clients and offline helpers share this lease with stage writers.
    let lease =
        update_installer::lease::Lease::acquire(&root).context("lock the update directory")?;
    let check = Check {
        channel: preferences.channel.name(),
        platform: &platform,
        current: env!("CARGO_PKG_VERSION"),
        current_build: update_manifest::build_number(),
        switch_now,
    };
    let stable = Check {
        channel: Channel::Stable.name(),
        switch_now: true,
        ..check
    };
    let stable_cache = cache_path(&root, Channel::Stable);
    let channel_cache = cache_path(&root, preferences.channel);
    let stable_url = Channel::Stable.manifest_url();
    if stable_url.is_some()
        && read_cached(&stable_cache).is_some_and(|cached| {
            Selection::verify(&cached, &keys, stable, OffsetDateTime::now_utc()).is_ok()
        })
    {
        if let Ok(mut state) = STATE.lock() {
            if state.generation == generation {
                state.stable_available = true;
            }
        }
    }
    let destination = root.join("ready");
    stage::recover(&destination, &keys, check, OffsetDateTime::now_utc())?;
    // A postponed stage stays offered while daily checks continue, and a newer release replaces it.
    let mut staged = None;
    if destination.exists() {
        match stage::verify(&destination, &keys, check, OffsetDateTime::now_utc()) {
            Ok(selected) => {
                staged = Some(selected.manifest.clone());
                let (directory, reader, selected) =
                    stage::Snapshot::capture(&destination, &lease, selected)?.into_parts();
                publish(
                    directory,
                    reader,
                    selected,
                    preferences.channel,
                    switch_now,
                    generation,
                );
            }
            Err(_) => {
                if let Ok(mut state) = STATE.lock() {
                    if state.generation == generation {
                        state.ready = None;
                        state.requested = false;
                    }
                }
                fs::remove_dir_all(&destination)?;
            }
        }
    }
    if (!preferences.checks && !switch_now) || !active(generation) {
        return Ok(());
    }
    let stamp_path = root.join(format!("last-check-{}.json", preferences.channel.name()));
    let last = fs::read(&stamp_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Stamp>(&bytes).ok())
        .filter(|stamp| stamp.current == check.current && stamp.build == check.current_build)
        .map(|stamp| stamp.checked_at);
    if !switch_now && !due(last, now_secs()) {
        // The skipped check would have found the same release, so restore what it offered.
        if cached_switch_offer(&channel_cache, &keys, check, OffsetDateTime::now_utc()) {
            if let Ok(mut state) = STATE.lock() {
                if state.generation == generation {
                    offer_switch(&mut state);
                }
            }
        }
        return Ok(());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let client = http_client()?;
    runtime.block_on(async {
        if preferences.channel != Channel::Stable {
            if let Some(url) = &stable_url {
                if let Ok(bytes) = fetch(&client, url).await {
                    if Selection::verify(&bytes, &keys, stable, OffsetDateTime::now_utc()).is_ok() {
                        let _ = fs::write(&stable_cache, &bytes);
                        if let Ok(mut state) = STATE.lock() {
                            if state.generation == generation {
                                state.stable_available = true;
                            }
                        }
                    }
                }
            }
        }
        ensure!(active(generation), "update channel changed");
        let envelope = fetch(&client, &manifest_url).await?;
        let selected = Selection::verify(
            &envelope,
            &keys,
            Check {
                switch_now: true,
                ..check
            },
            OffsetDateTime::now_utc(),
        )?;
        let _ = fs::write(&channel_cache, &envelope);
        if !switch_now
            && !update_manifest::select::newer_build(
                &selected.manifest.version,
                selected.manifest.build_number,
                check.current,
                check.current_build,
            )?
        {
            if older_release(&selected.manifest, check)? {
                if let Ok(mut state) = STATE.lock() {
                    if state.generation == generation {
                        offer_switch(&mut state);
                    }
                }
            }
            return record(&stamp_path, check);
        }
        if !supersedes(&selected.manifest, staged.as_ref())? {
            return record(&stamp_path, check);
        }
        ensure!(active(generation), "update check cancelled");
        let artifact = selected.artifact.clone();
        let expires = selected.manifest.expires;
        let mut download = Download::begin(&root, selected)?;
        let offset = download.offset();
        // Retain only the current partial transfer. A channel change can restart another digest safely.
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(digest) = name.strip_suffix(".part") {
                if digest.len() == 64
                    && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && !digest.eq_ignore_ascii_case(&artifact.sha256)
                    && entry.file_type()?.is_file()
                {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        if offset < artifact.size as u64 {
            let mut request = client.get(&artifact.url).timeout(DOWNLOAD_TIMEOUT);
            if offset != 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
            }
            let mut response = request.send().await?;
            check_missing(response.status())?;
            if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                let expected = format!("bytes {offset}-{}/{}", artifact.size - 1, artifact.size);
                ensure!(
                    response
                        .headers()
                        .get(reqwest::header::CONTENT_RANGE)
                        .and_then(|value| value.to_str().ok())
                        == Some(expected.as_str()),
                    "invalid resumed range"
                );
            } else {
                ensure!(
                    response.status() == reqwest::StatusCode::OK,
                    "artifact download failed: {}",
                    response.status()
                );
                if offset != 0 {
                    download.restart()?;
                }
            }
            if let Some(length) = response.content_length() {
                ensure!(
                    length == artifact.size as u64 - download.offset(),
                    "artifact content length mismatch"
                );
            }
            while let Some(chunk) = response.chunk().await? {
                ensure!(active(generation), "update check cancelled");
                download.write(&chunk)?;
            }
        }
        let previous = {
            let mut state = STATE
                .lock()
                .map_err(|_| anyhow::anyhow!("update state unavailable"))?;
            begin_promotion(&mut state, generation)?
        };
        let promotion = download.finish(&destination);
        let directory = {
            let mut state = STATE
                .lock()
                .map_err(|_| anyhow::anyhow!("update state unavailable"))?;
            if state.generation == generation {
                finish_promotion(&mut state, previous, promotion)?
            } else {
                promotion?
            }
        };
        ensure!(
            expires > OffsetDateTime::now_utc(),
            "release manifest expired during download"
        );
        let selected = stage::verify(&directory, &keys, check, OffsetDateTime::now_utc())?;
        let (directory, reader, selected) =
            stage::Snapshot::capture(&directory, &lease, selected)?.into_parts();
        publish(
            directory,
            reader,
            selected,
            preferences.channel,
            switch_now,
            generation,
        );
        record(&stamp_path, check)
    })
}

/// Withdraws the current notice only if no accepted handoff has frozen its stage.
pub(super) fn begin_promotion(state: &mut State, generation: u64) -> Result<Option<Ready>> {
    ensure!(state.accepts_worker(generation), "update check cancelled");
    Ok(state.ready.take())
}

/// Restores the postponed notice only after failed promotion leaves its files ready on disk.
pub(super) fn finish_promotion(
    state: &mut State,
    previous: Option<Ready>,
    result: Result<PathBuf>,
) -> Result<PathBuf> {
    state.ready = previous.filter(|ready| {
        result.is_err()
            && ready.stage.join(stage::ENVELOPE_FILE).is_file()
            && ready.stage.join(stage::ARTIFACT_FILE).is_file()
    });
    if state.ready.is_none() {
        state.requested = false;
    }
    result
}

/// The last verified envelope fetched for `channel`.
fn cache_path(root: &Path, channel: Channel) -> PathBuf {
    root.join(format!("manifest-{}.json", channel.name()))
}

/// Reads a cached envelope no larger than a fetched one may be.
fn read_cached(path: &Path) -> Option<Vec<u8>> {
    let mut cached = Vec::new();
    File::open(path)
        .and_then(|file| {
            file.take(update_manifest::MAX_ENVELOPE_BYTES as u64 + 1)
                .read_to_end(&mut cached)
        })
        .ok()?;
    Some(cached)
}

/// True when the channel's release is strictly older than the running build.
fn older_release(manifest: &update_manifest::Manifest, check: Check<'_>) -> Result<bool> {
    Ok(update_manifest::select::newer_build(
        check.current,
        check.current_build,
        &manifest.version,
        manifest.build_number,
    )?)
}

/// Whether a fetched release should replace the postponed stage, if there is one.
pub(super) fn supersedes(
    fetched: &update_manifest::Manifest,
    staged: Option<&update_manifest::Manifest>,
) -> Result<bool> {
    let Some(staged) = staged else {
        return Ok(true);
    };
    Ok(update_manifest::select::newer_build(
        &fetched.version,
        fetched.build_number,
        &staged.version,
        staged.build_number,
    )?)
}

/// Whether the cached, still-valid manifest at `path` warrants the Switch now offer.
pub(super) fn cached_switch_offer(
    path: &Path,
    keys: &TrustedKeys,
    check: Check<'_>,
    now: OffsetDateTime,
) -> bool {
    read_cached(path).is_some_and(|envelope| {
        Selection::verify(
            &envelope,
            keys,
            Check {
                switch_now: true,
                ..check
            },
            now,
        )
        .and_then(|selected| older_release(&selected.manifest, check))
        .unwrap_or(false)
    })
}

/// Offers an explicit switch to the selected channel's older release.
pub(super) fn offer_switch(state: &mut State) {
    state.switch_available = true;
    state.message = Some(
        "This channel has an older version. Wait for its next release, or choose Switch now."
            .into(),
    );
}

/// Publishes only a verified artifact and the non-elevating permission decision.
fn publish(
    stage: PathBuf,
    reader: File,
    selected: Selection,
    channel: Channel,
    switch_now: bool,
    generation: u64,
) {
    let writable = handoff::installed_target()
        .and_then(|target| update_installer::target::access(&target))
        .is_ok_and(|access| access == update_installer::target::Access::Writable);
    if let Ok(mut state) = STATE.lock() {
        if state.accepts_worker(generation) {
            state.ready = Some(Ready {
                expires: selected.manifest.expires,
                stage,
                channel,
                switch_now,
                lease: Some(std::sync::Arc::new(reader)),
                notice: Notice {
                    version: selected.manifest.version,
                    writable,
                    protocol_required: false,
                },
            });
            state.switch_available = false;
            state.message = None;
            if channel == Channel::Stable {
                state.stable_available = true;
            }
        }
    }
}

/// Bounds the signed document separately from the much larger release artifact.
pub(super) async fn fetch(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let mut response = client
        .get(url)
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .timeout(FETCH_TIMEOUT)
        .send()
        .await?;
    check_missing(response.status())?;
    ensure!(
        response.status() == reqwest::StatusCode::OK,
        "manifest download failed: {}",
        response.status()
    );
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            body.len() + chunk.len() <= update_manifest::MAX_ENVELOPE_BYTES,
            "manifest exceeds size limit"
        );
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Shares the HTTPS-only policy and idle deadlines between checks and range requests.
pub(super) fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(FETCH_TIMEOUT)
        .read_timeout(FETCH_TIMEOUT)
        .build()?)
}

/// Reads the edge clock for the daily check stamp.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Records successful channel checks; failed or interrupted downloads remain retryable.
fn record(path: &std::path::Path, check: Check<'_>) -> Result<()> {
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().context("missing check directory")?)?;
    file.write_all(&serde_json::to_vec(&Stamp {
        checked_at: now_secs(),
        current: check.current.into(),
        build: check.current_build,
    })?)?;
    file.persist(path)?;
    Ok(())
}
