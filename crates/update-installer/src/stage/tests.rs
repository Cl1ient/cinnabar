use super::*;
use crate::fixtures::*;

#[test]
fn signed_manifest_download_verify_stage_and_reopen_without_network() {
    let root = tempfile::tempdir().unwrap();
    let (directory, keys) = staged(root.path(), "linux-x86_64");
    assert_eq!(fs::read(directory.join(ARTIFACT_FILE)).unwrap(), PAYLOAD);
    assert_eq!(
        verify(&directory, &keys, check("linux-x86_64"), now())
            .unwrap()
            .manifest
            .version,
        "2.0.0"
    );
}

#[test]
fn bad_signature_channel_expiry_platform_and_downgrade_are_refused() {
    let (envelope, keys) = signed("linux-x86_64", "2.0.0", PAYLOAD);
    let mut bad: update_manifest::Envelope = serde_json::from_slice(&envelope).unwrap();
    bad.signature = "AAAA".into();
    assert!(
        Selection::verify(
            &serde_json::to_vec(&bad).unwrap(),
            &keys,
            check("linux-x86_64"),
            now()
        )
        .is_err()
    );
    for policy in [
        Check {
            channel: "nightly",
            ..check("linux-x86_64")
        },
        check("windows-x86_64"),
        Check {
            current: "3.0.0",
            ..check("linux-x86_64")
        },
    ] {
        assert!(Selection::verify(&envelope, &keys, policy, now()).is_err());
    }
    assert!(
        Selection::verify(
            &envelope,
            &keys,
            check("linux-x86_64"),
            now() + time::Duration::days(2)
        )
        .is_err()
    );
}

#[test]
fn only_explicit_switch_now_permits_an_older_signed_release_on_the_chosen_channel() {
    let (envelope, keys) = signed("linux-x86_64", "0.9.0", PAYLOAD);
    assert!(Selection::verify(&envelope, &keys, check("linux-x86_64"), now()).is_err());
    assert!(
        Selection::verify(
            &envelope,
            &keys,
            Check {
                switch_now: true,
                ..check("linux-x86_64")
            },
            now()
        )
        .is_ok()
    );
    assert!(
        Selection::verify(
            &envelope,
            &keys,
            Check {
                channel: "nightly",
                switch_now: true,
                ..check("linux-x86_64")
            },
            now()
        )
        .is_err()
    );
}

#[test]
fn interrupted_transfer_resumes_its_digest_named_partial_file() {
    let root = tempfile::tempdir().unwrap();
    let (envelope, keys) = signed("linux-x86_64", "2.0.0", PAYLOAD);
    let selected = Selection::verify(&envelope, &keys, check("linux-x86_64"), now()).unwrap();
    let part = root
        .path()
        .join(format!("{}.part", selected.artifact.sha256));
    let mut first = Download::begin(root.path(), selected).unwrap();
    first.write(&PAYLOAD[..7]).unwrap();
    drop(first);
    assert_eq!(fs::read(&part).unwrap(), &PAYLOAD[..7]);
    let selected = Selection::verify(&envelope, &keys, check("linux-x86_64"), now()).unwrap();
    let mut second = Download::begin(root.path(), selected).unwrap();
    assert_eq!(second.offset(), 7);
    second.write(&PAYLOAD[7..]).unwrap();
    let directory = second.finish(&root.path().join("ready")).unwrap();
    assert!(!part.exists());
    assert_eq!(fs::read(directory.join(ARTIFACT_FILE)).unwrap(), PAYLOAD);
}

#[test]
fn full_response_can_safely_restart_an_unsupported_range() {
    let root = tempfile::tempdir().unwrap();
    let (envelope, keys) = signed("linux-x86_64", "2.0.0", PAYLOAD);
    let selected = Selection::verify(&envelope, &keys, check("linux-x86_64"), now()).unwrap();
    let mut download = Download::begin(root.path(), selected).unwrap();
    download.write(&PAYLOAD[..7]).unwrap();
    download.restart().unwrap();
    assert_eq!(download.offset(), 0);
    download.write(PAYLOAD).unwrap();
    assert!(download.finish(&root.path().join("ready")).is_ok());
}

#[test]
fn short_oversized_and_corrupt_transfers_never_publish_ready_artifacts() {
    for payload in [
        PAYLOAD[..3].to_vec(),
        vec![b'x'; PAYLOAD.len()],
        vec![b'x'; PAYLOAD.len() + 1],
    ] {
        let root = tempfile::tempdir().unwrap();
        let (envelope, keys) = signed("linux-x86_64", "2.0.0", PAYLOAD);
        let selected = Selection::verify(&envelope, &keys, check("linux-x86_64"), now()).unwrap();
        let mut download = Download::begin(root.path(), selected).unwrap();
        if download.write(&payload).is_ok() {
            assert!(download.finish(&root.path().join("ready")).is_err());
        }
        assert!(!root.path().join("ready").exists());
    }
}

#[test]
fn signed_artifacts_still_obey_the_absolute_size_bound() {
    let (envelope, keys) = signed("linux-x86_64", "2.0.0", PAYLOAD);
    let mut manifest = update_manifest::verify(&envelope, &keys, now()).unwrap();
    manifest.artifacts.get_mut("linux-x86_64").unwrap().size = (MAX_ARTIFACT_BYTES + 1) as i64;
    let envelope = update_manifest::sign(&manifest, "test", &SEED).unwrap();
    assert!(Selection::verify(&envelope, &keys, check("linux-x86_64"), now()).is_err());
}
