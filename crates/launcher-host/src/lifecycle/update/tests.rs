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
    assert_eq!(state.ready.unwrap().notice.action(), Action::Download);
}

#[test]
fn expired_ready_updates_never_authorize_exit_time_installation() {
    let mut state = State::new();
    state.ready = Some(ready(true));
    state.requested = true;
    assert!(!state.applies_on_exit(OffsetDateTime::UNIX_EPOCH + time::Duration::days(2)));
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
    let package =
        handoff::named_package(stage.path(), "Cinnabar-x86_64.AppImage", &artifact).unwrap();
    let mode = std::fs::metadata(package).unwrap().permissions().mode();
    assert_eq!(mode & 0o755, 0o755);
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
