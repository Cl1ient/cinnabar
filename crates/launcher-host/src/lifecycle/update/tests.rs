use super::*;
use update_manifest::TrustedKeys;

#[test]
fn update_http_client_refuses_plaintext_before_connecting() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/update.json", listener.local_addr().unwrap());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(
        runtime
            .block_on(worker::fetch(&worker::http_client().unwrap(), &url))
            .is_err()
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn checks_remain_daily_when_the_clock_moves_back() {
    assert!(due(None, 10));
    assert!(!due(Some(1_000), 1_000 + 3_600));
    assert!(due(Some(1_000), 1_000 + 24 * 3_600));
    assert!(due(Some(5_000), 100));
}

/// A verified notice model lets consent tests avoid any network or platform installer.
fn ready(writable: bool) -> Ready {
    Ready {
        expires: OffsetDateTime::UNIX_EPOCH + time::Duration::days(1),
        stage: PathBuf::from("ready"),
        channel: Channel::Beta,
        switch_now: false,
        notice: Notice {
            version: "2.0.0".into(),
            writable,
            protocol_required: false,
        },
    }
}

#[test]
fn checking_does_not_authorize_installing_and_later_defers_an_opt_in() {
    let mut state = State::new();
    state.ready = Some(ready(true));
    assert!(!state.applies_on_exit(OffsetDateTime::UNIX_EPOCH));
    state.preferences.install_on_quit = true;
    assert!(state.applies_on_exit(OffsetDateTime::UNIX_EPOCH));
    state.dismissed = true;
    assert!(!state.applies_on_exit(OffsetDateTime::UNIX_EPOCH));
    state.requested = true;
    assert!(state.applies_on_exit(OffsetDateTime::UNIX_EPOCH));
}

#[test]
fn a_not_writable_install_cannot_launch_an_apply_helper() {
    let mut state = State::new();
    state.ready = Some(ready(false));
    state.preferences.install_on_quit = true;
    state.requested = true;
    assert!(!state.applies_on_exit(OffsetDateTime::UNIX_EPOCH));
    assert!(
        state
            .handoff_after_exit(OffsetDateTime::UNIX_EPOCH)
            .is_none()
    );
    assert_eq!(state.ready.unwrap().notice.action(), Action::Download);
}

#[test]
fn expired_ready_updates_never_authorize_exit_time_installation() {
    let mut state = State::new();
    state.ready = Some(ready(true));
    state.requested = true;
    assert!(!state.applies_on_exit(OffsetDateTime::UNIX_EPOCH + time::Duration::days(2)));
}

#[test]
fn accepted_restart_freezes_the_stage_while_a_newer_download_finishes() {
    let root = tempfile::tempdir().unwrap();
    let stage = root.path().join("ready");
    std::fs::create_dir(&stage).unwrap();
    std::fs::write(
        stage.join(update_installer::stage::ARTIFACT_FILE),
        "accepted release",
    )
    .unwrap();
    let mut state = State::new();
    state.ready = Some(Ready {
        stage: stage.clone(),
        ..ready(true)
    });
    let downloading_generation = state.generation;
    assert!(state.request_restart(OffsetDateTime::UNIX_EPOCH));
    assert!(!state.preferences.install_on_quit);
    assert!(worker::begin_promotion(&mut state, downloading_generation).is_err());
    let frozen_generation = state.generation;
    assert!(worker::begin_promotion(&mut state, frozen_generation).is_err());
    assert!(state.applies_on_exit(OffsetDateTime::UNIX_EPOCH));
    assert_eq!(state.ready.as_ref().unwrap().stage, stage);
    assert_eq!(
        std::fs::read(stage.join(update_installer::stage::ARTIFACT_FILE)).unwrap(),
        b"accepted release"
    );
}

#[test]
fn expiry_during_shutdown_keeps_the_accepted_handoff_but_never_authorizes_an_expired_install() {
    let mut state = State::new();
    state.ready = Some(ready(true));
    assert!(state.request_restart(OffsetDateTime::UNIX_EPOCH));
    let expired = OffsetDateTime::UNIX_EPOCH + time::Duration::days(2);
    assert!(!state.applies_on_exit(expired));
    assert!(state.handoff_after_exit(expired).is_some());
    let mut unaccepted = State::new();
    unaccepted.ready = Some(ready(true));
    unaccepted.preferences.install_on_quit = true;
    assert!(unaccepted.handoff_after_exit(expired).is_none());
}

#[test]
fn a_missing_payload_refreshes_the_channel_once_and_keeps_failures_retryable() {
    let mut fetched = 0;
    worker::retry_missing(
        || {
            fetched += 1;
            worker::check_missing(if fetched == 1 {
                reqwest::StatusCode::NOT_FOUND
            } else {
                reqwest::StatusCode::OK
            })
        },
        || true,
    )
    .unwrap();
    assert_eq!(fetched, 2);
    fetched = 0;
    assert!(
        worker::retry_missing(
            || {
                fetched += 1;
                worker::check_missing(reqwest::StatusCode::NOT_FOUND)
            },
            || true
        )
        .is_err()
    );
    assert_eq!(fetched, 2);
    assert!(due(None, 0));
}

#[test]
fn cancellation_and_other_download_failures_do_not_trigger_manifest_retry() {
    for missing in [false, true] {
        let mut fetched = 0;
        assert!(
            worker::retry_missing(
                || {
                    fetched += 1;
                    if missing {
                        Err(worker::MissingArtifact.into())
                    } else {
                        anyhow::bail!("connection failed")
                    }
                },
                || false
            )
            .is_err()
        );
        assert_eq!(fetched, 1);
    }
}

#[test]
fn a_failed_promotion_keeps_restart_available_only_when_the_previous_stage_survives() {
    let root = tempfile::tempdir().unwrap();
    let stage = root.path().join("ready");
    std::fs::create_dir(&stage).unwrap();
    std::fs::write(
        stage.join(update_installer::stage::ENVELOPE_FILE),
        "envelope",
    )
    .unwrap();
    std::fs::write(
        stage.join(update_installer::stage::ARTIFACT_FILE),
        "payload",
    )
    .unwrap();
    let mut state = State::new();
    state.ready = Some(Ready {
        stage: stage.clone(),
        ..ready(true)
    });
    state.preferences.install_on_quit = true;
    let previous = state.ready.take();
    assert!(!state.applies_on_exit(OffsetDateTime::UNIX_EPOCH));
    assert!(
        worker::finish_promotion(
            &mut state,
            previous,
            Err(anyhow::anyhow!("promotion failed; stage restored"))
        )
        .is_err()
    );
    assert!(state.ready.is_some());
    assert!(state.applies_on_exit(OffsetDateTime::UNIX_EPOCH));
    let previous = state.ready.take();
    std::fs::remove_dir_all(&stage).unwrap();
    assert!(
        worker::finish_promotion(
            &mut state,
            previous,
            Err(anyhow::anyhow!("promotion and restoration failed"))
        )
        .is_err()
    );
    assert!(state.ready.is_none());
    assert!(!state.requested);
    assert!(!state.applies_on_exit(OffsetDateTime::UNIX_EPOCH));
}

#[test]
fn a_successful_background_promotion_withdraws_the_previous_notice_until_reverified() {
    let mut state = State::new();
    state.ready = Some(ready(true));
    let previous = state.ready.take();
    let stage =
        worker::finish_promotion(&mut state, previous, Ok(PathBuf::from("new-ready"))).unwrap();
    assert_eq!(stage, PathBuf::from("new-ready"));
    assert!(state.ready.is_none());
    assert!(!state.requested);
    assert!(!state.applies_on_exit(OffsetDateTime::UNIX_EPOCH));
}

/// A signed envelope for `version` on the stable channel, as a manifest fetch would cache it.
fn cached_release(path: &std::path::Path, version: &str, build_number: u64) -> TrustedKeys {
    let seed = [7; update_manifest::KEY_BYTES];
    let manifest = update_manifest::Manifest {
        schema: update_manifest::SCHEMA,
        channel: "stable".into(),
        version: version.into(),
        build_number,
        expires: OffsetDateTime::now_utc() + time::Duration::days(1),
        notes_url: String::new(),
        artifacts: [(
            "macos-arm64".to_owned(),
            update_manifest::Artifact {
                url: "https://example.test/c.dmg".into(),
                sha256: "a".repeat(64),
                size: 10,
            },
        )]
        .into(),
    };
    std::fs::write(path, update_manifest::sign(&manifest, "k", &seed).unwrap()).unwrap();
    TrustedKeys::from([("k".to_owned(), update_manifest::public_key(&seed).unwrap())])
}

#[cfg(target_os = "linux")]
#[test]
fn a_manually_downloaded_appimage_is_executable() {
    use sha2::Digest;
    use std::os::unix::fs::PermissionsExt;
    let stage = tempfile::tempdir().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let payload = b"verified AppImage";
    std::fs::write(
        stage.path().join(update_installer::stage::ARTIFACT_FILE),
        payload,
    )
    .unwrap();
    let artifact = update_manifest::Artifact {
        url: "https://example.test/Cinnabar-x86_64.AppImage".into(),
        sha256: format!("{:x}", sha2::Sha256::digest(payload)),
        size: payload.len() as i64,
    };
    let package = handoff::named_package(
        stage.path(),
        directory.path(),
        update_manifest::platforms::artifact_name("linux", "x86_64").unwrap(),
        &artifact,
    )
    .unwrap();
    let mode = std::fs::metadata(package).unwrap().permissions().mode();
    assert_eq!(mode & 0o755, 0o755);
}

/// Publishes a signed fixture through the real stage writer without network or installer processes.
fn manual_stage(root: &std::path::Path, version: &str, payload: &[u8]) -> (PathBuf, TrustedKeys) {
    use sha2::Digest;
    let seed = [7; update_manifest::KEY_BYTES];
    let platform = update_manifest::platform_key("macos", "arm64").unwrap();
    let keys = TrustedKeys::from([("test".into(), update_manifest::public_key(&seed).unwrap())]);
    let manifest = update_manifest::Manifest {
        schema: update_manifest::SCHEMA,
        channel: Channel::Beta.name().into(),
        version: version.into(),
        build_number: 2,
        expires: OffsetDateTime::UNIX_EPOCH + time::Duration::days(1),
        notes_url: String::new(),
        artifacts: [(
            platform.clone(),
            update_manifest::Artifact {
                url: "https://example.test/manual.dmg".into(),
                size: payload.len() as i64,
                sha256: format!("{:x}", sha2::Sha256::digest(payload)),
            },
        )]
        .into(),
    };
    let envelope = update_manifest::sign(&manifest, "test", &seed).unwrap();
    let selection = update_installer::stage::Selection::verify(
        &envelope,
        &keys,
        update_manifest::select::Check {
            channel: Channel::Beta.name(),
            platform: &platform,
            current: "1.0.0",
            current_build: 1,
            switch_now: false,
        },
        OffsetDateTime::UNIX_EPOCH,
    )
    .unwrap();
    let mut download = update_installer::stage::Download::begin(root, selection).unwrap();
    download.write(payload).unwrap();
    (download.finish(&root.join("ready")).unwrap(), keys)
}

#[test]
fn replacing_ready_during_manual_handoff_keeps_the_handed_off_installer() {
    let root = tempfile::tempdir().unwrap();
    let (stage, keys) = manual_stage(root.path(), "2.0.0", b"accepted manual installer");
    let platform = update_manifest::platform_key("macos", "arm64").unwrap();
    let package = handoff::snapshot_package(
        &stage,
        update_manifest::platforms::artifact_name("macos", "arm64").unwrap(),
        &keys,
        update_manifest::select::Check {
            channel: Channel::Beta.name(),
            platform: &platform,
            current: "1.0.0",
            current_build: 1,
            switch_now: false,
        },
        OffsetDateTime::UNIX_EPOCH,
    )
    .unwrap();
    let path = package.path.clone();
    assert!(!path.starts_with(&stage));
    // The worker can take its lease again before the desktop opens the manual installer.
    let writer = std::fs::File::options()
        .write(true)
        .open(root.path().join(update_installer::LOCK_FILE))
        .unwrap();
    writer.try_lock().unwrap();
    manual_stage(root.path(), "3.0.0", b"replacement installer");
    assert_eq!(
        std::fs::read(stage.join(update_installer::stage::ARTIFACT_FILE)).unwrap(),
        b"replacement installer"
    );
    std::fs::remove_dir_all(stage).unwrap();
    let later = std::time::SystemTime::now() + update_installer::helper::HANDOFF_WINDOW * 2;
    update_installer::helper::prune(root.path(), later);
    assert_eq!(std::fs::read(&path).unwrap(), b"accepted manual installer");
    drop(package);
    update_installer::helper::prune(root.path(), later);
    assert!(!path.exists());
}

#[test]
fn a_skipped_daily_check_restores_the_older_channel_switch_offer() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("manifest-stable.json");
    let check = update_manifest::select::Check {
        channel: "stable",
        platform: "macos-arm64",
        current: "2.0.0-nightly",
        current_build: 50,
        switch_now: false,
    };
    assert!(!worker::cached_switch_offer(
        &path,
        &TrustedKeys::new(),
        check,
        OffsetDateTime::now_utc()
    ));
    let keys = cached_release(&path, "1.0.0", 40);
    assert!(worker::cached_switch_offer(
        &path,
        &keys,
        check,
        OffsetDateTime::now_utc()
    ));
    let mut state = State::new();
    worker::offer_switch(&mut state);
    assert!(state.switch_available && state.message.is_some());
    // A newer release is a download, not a switch.
    let keys = cached_release(&path, "3.0.0", 60);
    assert!(!worker::cached_switch_offer(
        &path,
        &keys,
        check,
        OffsetDateTime::now_utc()
    ));
}

#[test]
fn a_postponed_stage_is_replaced_only_by_a_newer_release() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("manifest.json");
    let release = |version, build| {
        let keys = cached_release(&path, version, build);
        update_manifest::verify(
            &std::fs::read(&path).unwrap(),
            &keys,
            OffsetDateTime::now_utc(),
        )
        .unwrap()
    };
    let staged = release("2.0.0", 10);
    assert!(worker::supersedes(&staged, None).unwrap());
    assert!(!worker::supersedes(&release("2.0.0", 10), Some(&staged)).unwrap());
    assert!(worker::supersedes(&release("2.0.0", 11), Some(&staged)).unwrap());
    assert!(worker::supersedes(&release("2.1.0", 1), Some(&staged)).unwrap());
}
