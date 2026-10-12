use super::*;

fn helper_copies(update_dir: &Path) -> usize {
    fs::read_dir(update_dir)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(PREFIX))
        .count()
}

#[test]
fn a_copy_survives_until_its_helper_takes_the_lock() {
    let update_dir = tempfile::tempdir().unwrap();
    let source = update_dir.path().join("packaged-helper");
    fs::write(&source, "helper").unwrap();
    let helper = copy(update_dir.path(), &source).unwrap();
    // A concurrent prune between spawning and the helper's own lock keeps the copy.
    prune(update_dir.path(), SystemTime::now());
    assert!(helper.exists());
    let running = hold(&helper).unwrap();
    let later = SystemTime::now() + HANDOFF_WINDOW * 2;
    prune(update_dir.path(), later);
    assert!(helper.exists());
    drop(running);
    prune(update_dir.path(), later);
    assert!(!helper.exists());
}

#[test]
fn finished_attempts_are_removed_after_the_handoff_window() {
    let update_dir = tempfile::tempdir().unwrap();
    let source = update_dir.path().join("packaged-helper");
    fs::write(&source, "helper").unwrap();
    fs::create_dir(update_dir.path().join("ready")).unwrap();
    let finished = copy(update_dir.path(), &source).unwrap();
    drop(hold(&finished).unwrap());
    fs::create_dir(update_dir.path().join("helper-crashed")).unwrap();
    assert_eq!(helper_copies(update_dir.path()), 2);
    prune(update_dir.path(), SystemTime::now() + HANDOFF_WINDOW * 2);
    assert_eq!(helper_copies(update_dir.path()), 0);
    assert!(update_dir.path().join("ready").is_dir());
}

#[test]
fn the_install_itself_is_never_held() {
    let update_dir = tempfile::tempdir().unwrap();
    let install = update_dir.path().join("Cinnabar").join("cinnabar-updater");
    assert!(hold(&install).is_err());
}
