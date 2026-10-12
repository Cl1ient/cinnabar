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

/// Runs one worker at a time and retries when a preference changes during its request.
pub(super) fn start() {
    let Some(layout) = LAYOUT.get().cloned() else {
        return;
    };
    let (preferences, switch_now, generation) = {
        let Ok(mut state) = STATE.lock() else {
            return;
        };
        if state.running {
            return;
        }
        state.running = true;
        (state.preferences, state.switch_now, state.generation)
    };
    std::thread::spawn(move || {
        if let Err(error) = run(&layout, preferences, switch_now, generation) {
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
        .is_ok_and(|state| state.generation == generation)
}

/// Reopens a verified ready stage, or checks the selected channel and resumes its artifact.
fn run(
    layout: &InstallLayout,
    preferences: Preferences,
    switch_now: bool,
    generation: u64,
) -> Result<()> {
    let keys = update_manifest::parse_keys(update_installer::TRUSTED_KEYS)?;
    if keys.is_empty() {
        return Ok(());
    }
    let platform = update_manifest::platform_key(std::env::consts::OS, std::env::consts::ARCH)
        .context("unsupported platform")?;
    let root = layout.update_dir();
    fs::create_dir_all(&root)?;
    update_installer::helper::prune(&root);
    let lease = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("update.lock"))?;
    lease
        .try_lock()
        .context("another client is using the update directory")?;
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
    if read_cached(&stable_cache).is_some_and(|cached| {
        Selection::verify(&cached, &keys, stable, OffsetDateTime::now_utc()).is_ok()
    }) {
        if let Ok(mut state) = STATE.lock() {
            if state.generation == generation {
                state.stable_available = true;
            }
        }
    }
    let destination = root.join("ready");
    if destination.exists() {
        if let Ok(selected) = stage::verify(&destination, &keys, check, OffsetDateTime::now_utc()) {
            publish(
                destination,
                selected.manifest.version,
                selected.manifest.expires,
                preferences.channel,
                switch_now,
                generation,
            );
            return Ok(());
        }
        fs::remove_dir_all(&destination)?;
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
            if let Ok(bytes) = fetch(&client, &Channel::Stable.manifest_url()).await {
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
        ensure!(active(generation), "update channel changed");
        let envelope = fetch(&client, &preferences.channel.manifest_url()).await?;
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
        ensure!(active(generation), "update check cancelled");
        let artifact = selected.artifact.clone();
        let version = selected.manifest.version.clone();
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
        ensure!(active(generation), "update check cancelled");
        let directory = download.finish(&destination)?;
        ensure!(
            expires > OffsetDateTime::now_utc(),
            "release manifest expired during download"
        );
        publish(
            directory,
            version,
            expires,
            preferences.channel,
            switch_now,
            generation,
        );
        record(&stamp_path, check)
    })
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
    version: String,
    expires: OffsetDateTime,
    channel: Channel,
    switch_now: bool,
    generation: u64,
) {
    let writable = handoff::installed_target()
        .and_then(|target| update_installer::target::access(&target))
        .is_ok_and(|access| access == update_installer::target::Access::Writable);
    if let Ok(mut state) = STATE.lock() {
        if state.generation == generation {
            state.ready = Some(Ready {
                expires,
                stage,
                channel,
                switch_now,
                notice: Notice {
                    version,
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
    let mut response = client.get(url).timeout(FETCH_TIMEOUT).send().await?;
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
