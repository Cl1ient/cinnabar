use super::*;
use crate::fixtures::*;

#[test]
fn signed_manifest_download_verify_stage_and_reopen_without_network() {
    let root = tempfile::tempdir().unwrap();
    let (stage, keys) = staged(root.path(), "linux-x86_64");
    assert_eq!(fs::read(stage.join(ARTIFACT_FILE)).unwrap(), PAYLOAD);
    assert_eq!(
        verify(&stage, &keys, "linux-x86_64", CURRENT, now())
            .unwrap()
            .version,
        "2.0.0"
    );
}

#[test]
fn refuses_bad_signature_wrong_platform_downgrade_and_expiration() {
    let (envelope, keys) = signed("linux-x86_64", "2.0.0", PAYLOAD);
    let mut bad: update_manifest::Envelope = serde_json::from_slice(&envelope).unwrap();
    bad.signature = "AAAA".into();
    assert!(
        Selection::verify(
            &serde_json::to_vec(&bad).unwrap(),
            &keys,
            "linux-x86_64",
            CURRENT,
            now()
        )
        .is_err()
    );
    assert!(Selection::verify(&envelope, &keys, "windows-x86_64", CURRENT, now()).is_err());
    assert!(Selection::verify(&envelope, &keys, "linux-x86_64", "3.0.0", now()).is_err());
    assert!(Selection::verify(&envelope, &keys, "linux-x86_64", "2.0.0", now()).is_err());
    assert!(
        Selection::verify(
            &envelope,
            &keys,
            "linux-x86_64",
            CURRENT,
            now() + time::Duration::days(2)
        )
        .is_err()
    );
}

#[test]
fn tampered_short_and_oversized_downloads_never_become_ready() {
    for data in [
        vec![b'x'; PAYLOAD.len()],
        PAYLOAD[..3].to_vec(),
        vec![b'x'; PAYLOAD.len() + 1],
    ] {
        let root = tempfile::tempdir().unwrap();
        let (envelope, keys) = signed("linux-x86_64", "2.0.0", PAYLOAD);
        let selected = Selection::verify(&envelope, &keys, "linux-x86_64", CURRENT, now()).unwrap();
        let mut attempt = Download::begin(root.path(), selected).unwrap();
        let destination = root.path().join("ready");
        if attempt.write(&data).is_ok() {
            assert!(attempt.finish(&destination).is_err());
        } else {
            drop(attempt);
        }
        assert!(!destination.exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn interrupted_download_can_restart_and_staged_tampering_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let (envelope, keys) = signed("linux-x86_64", "2.0.0", PAYLOAD);
    let selected = Selection::verify(&envelope, &keys, "linux-x86_64", CURRENT, now()).unwrap();
    let mut attempt = Download::begin(root.path(), selected).unwrap();
    attempt.write(&PAYLOAD[..3]).unwrap();
    drop(attempt);
    let (stage, _) = staged(root.path(), "linux-x86_64");
    fs::write(stage.join(ARTIFACT_FILE), vec![b'x'; PAYLOAD.len()]).unwrap();
    assert!(verify(&stage, &keys, "linux-x86_64", CURRENT, now()).is_err());
}

#[test]
fn even_a_signed_manifest_cannot_exceed_the_download_bound() {
    let (envelope, keys) = signed("linux-x86_64", "2.0.0", PAYLOAD);
    let mut manifest = update_manifest::verify(&envelope, &keys, now()).unwrap();
    manifest.artifacts.get_mut("linux-x86_64").unwrap().size = (MAX_ARTIFACT_BYTES + 1) as i64;
    let envelope = update_manifest::sign(&manifest, "test", &crate::fixtures::SEED).unwrap();
    assert!(Selection::verify(&envelope, &keys, "linux-x86_64", CURRENT, now()).is_err());
}
