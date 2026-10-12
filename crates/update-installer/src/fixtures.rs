//! Deterministic signed payloads for offline updater tests.
use crate::OffsetDateTime;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use update_manifest::{Artifact, Manifest, TrustedKeys};

pub const SEED: [u8; update_manifest::KEY_BYTES] = [7; update_manifest::KEY_BYTES];
pub const CURRENT: &str = "1.0.0";
pub const PAYLOAD: &[u8] = b"a verified release payload";

/// Fixed clock used by every test, independent of the machine date.
pub fn now() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH
}

/// Builds a signed test release whose only trusted key is an in-memory test seed.
pub fn signed(platform: &str, version: &str, payload: &[u8]) -> (Vec<u8>, TrustedKeys) {
    let seed = SEED;
    let keys = BTreeMap::from([(
        "test".to_owned(),
        update_manifest::public_key(&seed).unwrap(),
    )]);
    let manifest = Manifest {
        schema: update_manifest::SCHEMA,
        channel: crate::CHANNEL.into(),
        version: version.into(),
        expires: now() + time::Duration::days(1),
        notes_url: String::new(),
        artifacts: BTreeMap::from([(
            platform.to_owned(),
            Artifact {
                url: "https://example.invalid/release".into(),
                sha256: format!("{:x}", Sha256::digest(payload)),
                size: payload.len() as i64,
            },
        )]),
    };
    (
        update_manifest::sign(&manifest, "test", &seed).unwrap(),
        keys,
    )
}

/// Stages fixture bytes through the same streaming writer used by the HTTP worker.
pub fn staged(root: &std::path::Path, platform: &str) -> (std::path::PathBuf, TrustedKeys) {
    let (envelope, keys) = signed(platform, "2.0.0", PAYLOAD);
    let selected =
        crate::stage::Selection::verify(&envelope, &keys, platform, CURRENT, now()).unwrap();
    let mut download = crate::stage::Download::begin(root, selected).unwrap();
    for chunk in PAYLOAD.chunks(3) {
        download.write(chunk).unwrap();
    }
    let stage = download.finish(&root.join("ready")).unwrap();
    (stage, keys)
}
