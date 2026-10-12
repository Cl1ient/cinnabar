use super::*;
use crate::fixtures::*;

#[test]
fn complete_app_bundle_and_appimage_are_retained_until_matching_startup() {
    for os in ["macos", "linux"] {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join(if os == "macos" {
            "Client.app"
        } else {
            "Client.AppImage"
        });
        let candidate = root.path().join("candidate");
        if os == "macos" {
            fs::create_dir(&target).unwrap();
            fs::create_dir(&candidate).unwrap();
            fs::write(target.join("signature"), "old").unwrap();
            fs::write(candidate.join("signature"), "new").unwrap();
        } else {
            fs::write(&target, "old").unwrap();
            fs::write(&candidate, "new").unwrap();
        }
        let backup = backup_path(root.path(), &target, os).unwrap();
        write_receipt(
            root.path(),
            &Pending {
                target: target.clone(),
                version: "2.0.0".into(),
            },
        )
        .unwrap();
        swap(&target, &candidate, &backup).unwrap();
        assert!(backup.exists());
        assert!(acknowledge(root.path(), &target, CURRENT, os).is_err());
        assert!(
            acknowledge(
                root.path(),
                &root.path().join("another-install"),
                "2.0.0",
                os
            )
            .is_err()
        );
        assert!(backup.exists());
        acknowledge(root.path(), &target, "2.0.0", os).unwrap();
        assert!(!backup.exists());
        assert!(!root.path().join(RECEIPT).exists());
    }
}

#[test]
fn failed_file_or_whole_bundle_promotion_restores_previous_version() {
    for bundle in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("installed");
        let candidate = root.path().join("candidate");
        let backup = root.path().join("previous");
        if bundle {
            fs::create_dir(&target).unwrap();
            fs::write(target.join("old"), "signed bundle").unwrap();
        } else {
            fs::write(&target, "old AppImage").unwrap();
        }
        let mut calls = 0;
        assert!(
            swap_with(&target, &candidate, &backup, |from, to| {
                calls += 1;
                if calls == 2 {
                    return Err(std::io::Error::other("simulated promotion failure"));
                }
                fs::rename(from, to)
            })
            .is_err()
        );
        assert!(target.exists());
        assert!(!backup.exists());
        if bundle {
            assert!(target.join("old").exists());
        } else {
            assert_eq!(fs::read(&target).unwrap(), b"old AppImage");
        }
    }
}

#[test]
fn existing_backup_is_never_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");
    let candidate = root.path().join("candidate");
    let backup = root.path().join("previous");
    for path in [&target, &candidate, &backup] {
        fs::write(path, "keep").unwrap();
    }
    assert!(swap(&target, &candidate, &backup).is_err());
    assert_eq!(fs::read(&backup).unwrap(), b"keep");
}

#[cfg(target_os = "linux")]
#[test]
fn apply_waits_reverifies_and_replaces_appimage_with_no_network() {
    let root = tempfile::tempdir().unwrap();
    let (stage, keys) = staged(root.path(), "linux-x86_64");
    let target = root.path().join("client.AppImage");
    fs::write(&target, b"old").unwrap();
    assert!(
        after_exit(
            Request {
                stage_dir: &stage,
                target: &target,
                keys: &keys,
                platform_key: "linux-x86_64",
                current: CURRENT,
                now: now()
            },
            || anyhow::bail!("parent still running")
        )
        .is_err()
    );
    assert_eq!(fs::read(&target).unwrap(), b"old");
    after_exit(
        Request {
            stage_dir: &stage,
            target: &target,
            keys: &keys,
            platform_key: "linux-x86_64",
            current: CURRENT,
            now: now(),
        },
        || Ok(()),
    )
    .unwrap();
    assert_eq!(fs::read(&target).unwrap(), PAYLOAD);
    assert_eq!(fs::read(target::backup(&target).unwrap()).unwrap(), b"old");
}

#[cfg(target_os = "linux")]
#[test]
fn helper_refuses_every_tampered_stage_before_touching_target() {
    for case in ["signature", "hash", "platform", "downgrade"] {
        let root = tempfile::tempdir().unwrap();
        let (stage, keys) = staged(root.path(), "linux-x86_64");
        let target = root.path().join("client.AppImage");
        fs::write(&target, "old").unwrap();
        let mut platform = "linux-x86_64";
        let mut current = CURRENT;
        match case {
            "signature" => {
                fs::write(stage.join(stage::ENVELOPE_FILE), b"{}").unwrap();
            }
            "hash" => {
                fs::write(stage.join(stage::ARTIFACT_FILE), vec![b'x'; PAYLOAD.len()]).unwrap();
            }
            "platform" => {
                platform = "linux-arm64";
            }
            "downgrade" => {
                current = "3.0.0";
            }
            _ => unreachable!(),
        }
        assert!(
            after_exit(
                Request {
                    stage_dir: &stage,
                    target: &target,
                    keys: &keys,
                    platform_key: platform,
                    current,
                    now: now()
                },
                || Ok(())
            )
            .is_err()
        );
        assert_eq!(fs::read(&target).unwrap(), b"old");
        assert!(!root.path().join(RECEIPT).exists());
    }
}

#[test]
fn failed_rollback_never_discards_the_only_previous_copy() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");
    let candidate = root.path().join("candidate");
    let backup = root.path().join("previous");
    fs::write(&target, "old").unwrap();
    let mut calls = 0;
    assert!(
        swap_with(&target, &candidate, &backup, |from, to| {
            calls += 1;
            if calls > 1 {
                return Err(std::io::Error::other("filesystem failure"));
            }
            fs::rename(from, to)
        })
        .is_err()
    );
    assert_eq!(fs::read(&backup).unwrap(), b"old");
}
