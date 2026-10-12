// Generated from crates/server-experience/schema/extension.json; do not edit.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedDocument {
    pub payload: String,
    pub signature: String,
}

/// Builds the domain-separated bytes consumed by native Ed25519 signing.
fn signing_bytes(domain: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(domain.len() + payload.len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(payload);
    bytes
}

pub const OFFER_DOMAIN: &[u8] = b"Cinnabar/experience/offer/v1\0";

pub const ACCEPT_DOMAIN: &[u8] = b"Cinnabar/experience/accept/v1\0";

pub const MANIFEST_DOMAIN: &[u8] = b"Cinnabar/experience/manifest/v1\0";
