use super::*;
use crate::fixtures::*;

#[test]
fn file_and_whole_bundle_swaps_keep_the_previous_copy_until_startup() {
    for bundle in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (target, candidate, backup) = (
            root.path().join("target"),
            root.path().join("candidate"),
            root.path().join("previous"),
        );
        if bundle {
            fs::create_dir(&target).unwrap();
            fs::create_dir(&candidate).unwrap();
            fs::write(target.join("signature"), "old").unwrap();
            fs::write(candidate.join("signature"), "new").unwrap();
        } else {
            fs::write(&target, "old").unwrap();
            fs::write(&candidate, "new").unwrap();
        }
        swap(&target, &candidate, &backup).unwrap();
        assert!(backup.exists());
        if bundle {
            assert_eq!(fs::read(backup.join("signature")).unwrap(), b"old");
        } else {
            assert_eq!(fs::read(&backup).unwrap(), b"old");
        }
        remove(&backup).unwrap();
        assert!(!backup.exists());
    }
}

#[test]
fn failed_bundle_or_appimage_start_restores_the_previous_payload() {
    for bundle in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (target, candidate, backup, failed) = (
            root.path().join("target"),
            root.path().join("candidate"),
            root.path().join("previous"),
            root.path().join("failed"),
        );
        if bundle {
            fs::create_dir(&target).unwrap();
            fs::create_dir(&candidate).unwrap();
            fs::write(target.join("signature"), "old").unwrap();
            fs::write(candidate.join("signature"), "new").unwrap();
        } else {
            fs::write(&target, "old").unwrap();
            fs::write(&candidate, "new").unwrap();
        }
        swap(&target, &candidate, &backup).unwrap();
        rollback(&target, &backup, &failed).unwrap();
        if bundle {
            assert_eq!(fs::read(target.join("signature")).unwrap(), b"old");
        } else {
            assert_eq!(fs::read(&target).unwrap(), b"old");
        }
        assert!(failed.exists());
    }
}

#[test]
fn a_failed_promotion_restores_the_old_file_without_discarding_a_retained_backup() {
    let root = tempfile::tempdir().unwrap();
    let (target, candidate, backup) = (
        root.path().join("target"),
        root.path().join("missing"),
        root.path().join("previous"),
    );
    fs::write(&target, "old").unwrap();
    assert!(swap(&target, &candidate, &backup).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"old");
    fs::write(&backup, "keep").unwrap();
    assert!(swap(&target, &candidate, &backup).is_err());
    assert_eq!(fs::read(&backup).unwrap(), b"keep");
}

#[test]
fn previous_downloaded_windows_installer_is_kept_for_manual_repair() {
    let root = tempfile::tempdir().unwrap();
    let artifact = root.path().join("payload");
    fs::write(&artifact, "first MSI").unwrap();
    remember_installer(root.path(), &artifact).unwrap();
    fs::write(&artifact, "second MSI").unwrap();
    remember_installer(root.path(), &artifact).unwrap();
    assert_eq!(
        fs::read(root.path().join("previous.msi")).unwrap(),
        b"first MSI"
    );
    assert_eq!(
        fs::read(root.path().join("installed.msi")).unwrap(),
        b"second MSI"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn apply_waits_for_parent_and_rolls_back_a_failed_new_start() {
    let root = tempfile::tempdir().unwrap();
    let (directory, keys) = staged(root.path(), "linux-x86_64");
    let target = root.path().join("client.AppImage");
    fs::write(&target, "old").unwrap();
    let request = || Request {
        stage_dir: &directory,
        target: &target,
        keys: &keys,
        check: check("linux-x86_64"),
        now: now(),
    };
    assert!(
        after_exit(
            request(),
            || anyhow::bail!("parent running"),
            |_| panic!("started before parent exit")
        )
        .is_err()
    );
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert!(after_exit(request(), || Ok(()), |_| anyhow::bail!("failed startup")).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"old");
    after_exit(
        request(),
        || Ok(()),
        |target| {
            assert_eq!(fs::read(target).unwrap(), PAYLOAD);
            assert!(crate::target::backup(target).unwrap().exists());
            Ok(())
        },
    )
    .unwrap();
    assert!(!crate::target::backup(&target).unwrap().exists());
}

#[cfg(target_os = "linux")]
#[test]
fn all_tampered_stages_are_refused_before_start_or_swap() {
    for case in [
        "signature",
        "hash",
        "truncated",
        "channel",
        "expired",
        "downgrade",
        "platform",
    ] {
        let root = tempfile::tempdir().unwrap();
        let (directory, keys) = staged(root.path(), "linux-x86_64");
        let target = root.path().join("client.AppImage");
        fs::write(&target, "old").unwrap();
        let mut policy = check("linux-x86_64");
        let mut clock = now();
        match case {
            "signature" => fs::write(directory.join(stage::ENVELOPE_FILE), "{}").unwrap(),
            "hash" => fs::write(
                directory.join(stage::ARTIFACT_FILE),
                vec![b'x'; PAYLOAD.len()],
            )
            .unwrap(),
            "truncated" => fs::write(directory.join(stage::ARTIFACT_FILE), &PAYLOAD[..2]).unwrap(),
            "channel" => policy.channel = "nightly",
            "expired" => clock += time::Duration::days(2),
            "downgrade" => policy.current = "3.0.0",
            "platform" => policy.platform = "windows-x86_64",
            _ => unreachable!(),
        }
        assert!(
            after_exit(
                Request {
                    stage_dir: &directory,
                    target: &target,
                    keys: &keys,
                    check: policy,
                    now: clock
                },
                || Ok(()),
                |_| panic!("unverified release executed")
            )
            .is_err()
        );
        assert_eq!(fs::read(&target).unwrap(), b"old");
    }
}
