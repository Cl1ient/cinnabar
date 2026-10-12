use super::*;

fn helper_copies(update_dir: &Path) -> usize {
    fs::read_dir(update_dir)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(PREFIX))
        .count()
}

#[test]
fn finished_attempts_are_removed_and_a_running_helper_keeps_its_copy() {
    let update_dir = tempfile::tempdir().unwrap();
    let source = update_dir.path().join("packaged-helper");
    fs::write(&source, "helper").unwrap();
    fs::create_dir(update_dir.path().join("ready")).unwrap();
    let (first, spawning) = copy(update_dir.path(), &source).unwrap();
    drop(spawning);
    let running = hold(&first).unwrap();
    let (second, spawning) = copy(update_dir.path(), &source).unwrap();
    drop(spawning);
    assert_eq!(helper_copies(update_dir.path()), 2);
    // The second attempt finished; only the copy whose helper still runs survives.
    copy(update_dir.path(), &source).unwrap();
    assert!(first.exists() && !second.exists());
    drop(running);
    prune(update_dir.path());
    assert!(!first.exists());
    assert!(update_dir.path().join("ready").is_dir());
}

#[test]
fn a_copy_that_never_locked_is_stale_and_the_install_is_never_held() {
    let update_dir = tempfile::tempdir().unwrap();
    fs::create_dir(update_dir.path().join("helper-crashed")).unwrap();
    prune(update_dir.path());
    assert_eq!(helper_copies(update_dir.path()), 0);
    let install = update_dir.path().join("Cinnabar").join("cinnabar-updater");
    assert!(hold(&install).is_err());
}
