//! Validation of the pinned source manifest shared by entity, language and audio carriers.

use std::{borrow::Cow, path::Path};

use assets::AssetError;
use sha2::{Digest, Sha256};

use super::{invalid, json::parse_unique_json};

const MAX_SOURCE_MANIFEST_BYTES: usize = 1024 * 1024;

/// Requires the exact pinned manifest while accepting canonical CRLF transport.
pub(crate) fn validate(source: &[u8]) -> Result<[u8; 32], AssetError> {
    if source.len() > MAX_SOURCE_MANIFEST_BYTES {
        return Err(invalid("entity source manifest exceeds bound"));
    }
    let canonical = canonical_manifest_line_endings(source)?;
    let value = parse_unique_json(Path::new("assets/vanilla-source.json"), &canonical)?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("entity source manifest must be an object"))?;
    let expected_fields = [
        "schema",
        "tag",
        "commit",
        "archive",
        "url",
        "sha256",
        "artifact_policy",
        "cache_dir",
    ];
    if object.len() != expected_fields.len()
        || expected_fields
            .iter()
            .any(|field| !object.contains_key(*field))
    {
        return Err(invalid(
            "entity source manifest fields do not match the pin",
        ));
    }
    let digest: [u8; 32] = Sha256::digest(&canonical).into();
    if digest != assets::vanilla_source_manifest_sha256() {
        return Err(invalid(
            "manifest bytes and fields must exactly match the reviewed Mojang Bedrock Samples pin",
        ));
    }
    Ok(digest)
}

fn canonical_manifest_line_endings(source: &[u8]) -> Result<Cow<'_, [u8]>, AssetError> {
    if !source.contains(&b'\r') {
        return Ok(Cow::Borrowed(source));
    }
    let mut canonical = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        match source[index] {
            b'\r' if source.get(index + 1) == Some(&b'\n') => {
                canonical.push(b'\n');
                index += 2;
            }
            b'\r' | b'\n' => return Err(invalid("manifest line endings are not canonical")),
            byte => {
                canonical.push(byte);
                index += 1;
            }
        }
    }
    Ok(Cow::Owned(canonical))
}
