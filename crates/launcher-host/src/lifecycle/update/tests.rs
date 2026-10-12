use super::*;

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
