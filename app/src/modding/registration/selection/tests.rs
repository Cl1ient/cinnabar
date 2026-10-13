use super::super::Status as Written;
use super::*;

/// A receipt binds deletion to all registration fields, including edits that retain the request ID.
#[test]
fn removal_receipts_reject_changes_reusing_the_request_id() {
    let directory = tempfile::tempdir().unwrap();
    let component = directory.path().join("zoom.wasm");
    let receipt = forget(directory.path(), &component, "remove".into()).unwrap();
    receipt.confirm(directory.path()).unwrap();
    select(directory.path(), &component, true, "remove".into()).unwrap();
    assert!(receipt.confirm(directory.path()).is_err());
}

/// Authorization survives disable/re-enable but never survives removal at the same path.
#[test]
fn removing_enabled_or_disabled_mods_forgets_all_retained_authorization() {
    for enabled in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let component = root.join("zoom.wasm");
        select(root, &component, true, "selected".into()).unwrap();
        let mut granted = observe(&root.join(REGISTRATION_FILE))
            .result
            .unwrap()
            .unwrap();
        granted.grants.controls = true;
        granted.grants.settings = true;
        granted.font = Some(root.join("font.ttf"));
        granted.ui = Some(root.join("ui.json"));
        write_selection(&root.join(REGISTRATION_FILE), granted.clone()).unwrap();
        for enabled in [false, true, enabled] {
            select(root, &component, enabled, "toggle".into()).unwrap();
            let kept = observe(&root.join(REGISTRATION_FILE))
                .result
                .unwrap()
                .unwrap();
            assert_eq!(kept.grants, granted.grants);
            assert_eq!(kept.font, granted.font);
            assert_eq!(kept.ui, granted.ui);
        }
        forget(root, &component, "removed".into()).unwrap();
        let removed = observe(&root.join(REGISTRATION_FILE))
            .result
            .unwrap()
            .unwrap();
        assert!(!removed.enabled);
        assert_eq!(removed.grants, ModGrants::default());
        assert!(removed.font.is_none() && removed.ui.is_none());
        select(root, &component, true, "replacement".into()).unwrap();
        let replacement = observe(&root.join(REGISTRATION_FILE))
            .result
            .unwrap()
            .unwrap();
        assert_eq!(replacement.grants, ModGrants::default());
        assert!(replacement.font.is_none() && replacement.ui.is_none());
    }
}

/// Removing an unrelated library file keeps the selected mod's state and authorization.
#[test]
fn forgetting_another_mod_preserves_the_current_selection() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let selected = root.join("selected.wasm");
    select(root, &selected, true, "selected".into()).unwrap();
    let mut granted = observe(&root.join(REGISTRATION_FILE))
        .result
        .unwrap()
        .unwrap();
    granted.grants.settings = true;
    granted.font = Some(root.join("font.ttf"));
    granted.ui = Some(root.join("ui.json"));
    write_selection(&root.join(REGISTRATION_FILE), granted.clone()).unwrap();
    forget(root, &root.join("other.wasm"), "remove-other".into()).unwrap();
    let kept = observe(&root.join(REGISTRATION_FILE))
        .result
        .unwrap()
        .unwrap();
    assert!(kept.enabled);
    assert_eq!(kept.component, selected);
    assert_eq!(kept.grants, granted.grants);
    assert_eq!(kept.font, granted.font);
    assert_eq!(kept.ui, granted.ui);
    assert_eq!(kept.request_id, "remove-other");
}

/// A Settings write reads back through the watcher's own reader, with its recorded status.
#[test]
fn selection_round_trips_through_the_watcher_reader() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let zoom = root.join("zoom.wasm");
    select(root, &zoom, true, "settings-1".into()).unwrap();
    let read = observe(&root.join(REGISTRATION_FILE))
        .result
        .unwrap()
        .unwrap();
    assert_eq!(read.component, zoom);
    assert!(read.enabled);
    assert_eq!(read.request_id, "settings-1");
    assert_eq!(read.grants, ModGrants::default());
    assert_eq!(registered(root).unwrap().status, Status::Starting);
    let status = root.join(STATUS_FILE);
    write_json(&status, &Written::new("settings-1".into(), "loaded", None)).unwrap();
    assert_eq!(registered(root).unwrap().status, Status::Running);
    write_json(
        &status,
        &Written::new("settings-1".into(), "error", Some("trap".into())),
    )
    .unwrap();
    assert_eq!(
        registered(root).unwrap().status,
        Status::Failed("trap".into())
    );

    let mut granted = serde_json::from_slice::<serde_json::Value>(
        &std::fs::read(root.join(REGISTRATION_FILE)).unwrap(),
    )
    .unwrap();
    granted["grants"]["hud"] = true.into();
    std::fs::write(
        root.join(REGISTRATION_FILE),
        serde_json::to_vec(&granted).unwrap(),
    )
    .unwrap();
    select(root, &zoom, false, "settings-2".into()).unwrap();
    let disabled = registered(root).unwrap();
    assert!(!disabled.enabled && disabled.component == zoom);
    assert_eq!(disabled.status, Status::Starting);
    let kept = observe(&root.join(REGISTRATION_FILE))
        .result
        .unwrap()
        .unwrap();
    assert!(kept.grants.hud);

    let minimap = root.join("minimap.wasm");
    select(root, &minimap, true, "settings-3".into()).unwrap();
    let switched = observe(&root.join(REGISTRATION_FILE))
        .result
        .unwrap()
        .unwrap();
    assert_eq!(switched.component, minimap);
    assert_eq!(switched.grants, ModGrants::default());
}
