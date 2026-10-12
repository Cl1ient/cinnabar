//! One background worker shares the existing HTTPS client for checks and bounded downloads.
use super::*;
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
    sync::atomic::Ordering,
    time::{SystemTime, UNIX_EPOCH},
};
use update_installer::{
    CHANNEL, OffsetDateTime, TRUSTED_KEYS,
    stage::{self, Download, Selection},
};

/// Spawns at most one worker; its process lock also excludes other running client instances.
pub(super) fn start(layout: InstallLayout) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let revision = REVISION.load(Ordering::SeqCst);
    if let Ok(mut ready) = READY.lock() {
        *ready = None;
    }
    std::thread::spawn(move || {
        if let Err(error) = run(&layout) {
            tracing::debug!("update download: {error:#}");
        }
        RUNNING.store(false, Ordering::SeqCst);
        if ENABLED.load(Ordering::SeqCst) && REVISION.load(Ordering::SeqCst) != revision {
            start(layout);
        }
    });
}

/// Checks a retained stage first, then performs the daily manifest check when due.
fn run(layout: &InstallLayout) -> Result<()> {
    let keys = update_manifest::parse_keys(TRUSTED_KEYS)?;
    if keys.is_empty() {
        return Ok(());
    }
    // Unsupported layouts can still be upgraded manually, but never download unusable packages.
    handoff::installed_target()?;
    let platform = platform_key(std::env::consts::OS, std::env::consts::ARCH)
        .context("unsupported platform")?;
    let root = layout.update_dir();
    fs::create_dir_all(&root)?;
    let lease = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("download.lock"))?;
    lease.try_lock().context("another client is downloading")?;
    // Clean only partial downloads owned by this updater, after acquiring its exclusive lease.
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with("download-")
            && entry.file_type()?.is_dir()
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    let destination = root.join("ready");
    let current = env!("CARGO_PKG_VERSION");
    if destination.exists() {
        if let Ok(selected) = stage::verify(
            &destination,
            &keys,
            &platform,
            current,
            OffsetDateTime::now_utc(),
        ) {
            publish(destination, selected.version);
            return Ok(());
        }
        fs::remove_dir_all(&destination)?;
    }
    let stamp = fs::read(root.join("last-check.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Stamp>(&bytes).ok())
        .filter(|stamp| stamp.current == current);
    if !due(stamp.map(|stamp| stamp.checked_at), now_secs()) {
        return Ok(());
    }
    let url = std::env::var(URL_ENV)
        .ok()
        .or_else(|| fs::read_to_string(layout.resource_root.join("update-url")).ok())
        .map(|url| url.trim().to_owned())
        .filter(|url| https(url))
        .context("no HTTPS update URL")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let client = http_client()?;
    runtime.block_on(async {
        let envelope = fetch(&client, &url).await?;
        let manifest = update_manifest::verify(&envelope, &keys, OffsetDateTime::now_utc())?;
        let verdict = update_manifest::evaluate(&manifest, CHANNEL, &platform, current)?;
        if verdict.available {
            let selected = Selection::verify(
                &envelope,
                &keys,
                &platform,
                current,
                OffsetDateTime::now_utc(),
            )?;
            let version = selected.version.clone();
            let mut response = client
                .get(&selected.artifact.url)
                .timeout(DOWNLOAD_TIMEOUT)
                .send()
                .await?;
            ensure!(
                response.status() == reqwest::StatusCode::OK,
                "artifact download failed: {}",
                response.status()
            );
            if let Some(length) = response.content_length() {
                ensure!(
                    length == selected.artifact.size as u64,
                    "artifact content length mismatch"
                );
            }
            let mut download = Download::begin(&root, selected)?;
            while let Some(chunk) = response.chunk().await? {
                ensure!(ENABLED.load(Ordering::SeqCst), "automatic updates disabled");
                download.write(&chunk)?;
            }
            ensure!(ENABLED.load(Ordering::SeqCst), "automatic updates disabled");
            publish(download.finish(&destination)?, version);
        }
        record(&root, &verdict)
    })
}

/// Reads a bounded envelope with the existing fifteen-second request deadline.
async fn fetch(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
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

/// Publishes only a fully verified, atomically staged download to the menu.
fn publish(stage: PathBuf, version: String) {
    if let Ok(mut ready) = READY.lock() {
        *ready = Some(Ready { stage, version });
    }
}

/// Reads the edge clock for the daily-check stamp.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Records successful checks only; interrupted transfers can be retried on the next launch.
fn record(root: &Path, verdict: &Verdict) -> Result<()> {
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    temporary.write_all(&serde_json::to_vec(&Stamp {
        checked_at: now_secs(),
        current: verdict.current.clone(),
    })?)?;
    temporary.persist(root.join("last-check.json"))?;
    Ok(())
}

/// Uses the same HTTPS-only policy and idle deadlines for manifests and release payloads.
fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(FETCH_TIMEOUT)
        .read_timeout(FETCH_TIMEOUT)
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetching_never_opens_plain_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/update-stable.json",
            listener.local_addr().unwrap()
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(
            runtime
                .block_on(fetch(&http_client().unwrap(), &url))
                .is_err()
        );
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err(), "connected over plain HTTP");
    }
}
