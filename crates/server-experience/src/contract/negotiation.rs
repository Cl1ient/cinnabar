// Generated from crates/server-experience/schema/extension.json; do not edit.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Marker {
    pub server_key: String,
    pub offer: SignedDocument,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_fragment_bytes: u32,
    pub max_message_bytes: u32,
    pub max_reassembly_bytes: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WireOffer {
    pub versions: BTreeSet<u16>,
    pub limits: Limits,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Wire {
    pub version: u16,
    pub limits: Limits,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub version: u16,
    pub api: u16,
    pub capabilities: BTreeSet<crate::manifest::Permission>,
    pub offer_digest: String,
    pub client_challenge: String,
    pub connection: String,
    pub subclient: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire: Option<WireOffer>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Accept {
    pub hello: Hello,
    pub server_challenge: String,
    pub session: String,
    pub audience: String,
    pub offer_digest: String,
    pub revision: u64,
    pub expires_unix: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire: Option<Wire>,
}

impl Limits {
    /// Checks that the selected ceilings are nonzero and ordered.
    fn ordered(&self) -> bool {
        0 < self.max_fragment_bytes
            && self.max_fragment_bytes <= self.max_message_bytes
            && self.max_message_bytes <= self.max_reassembly_bytes
    }
}
