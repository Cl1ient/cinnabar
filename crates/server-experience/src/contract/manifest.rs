// Generated from crates/server-experience/schema/extension.json; do not edit.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub permissions: BTreeSet<Permission>,
    pub origins: BTreeSet<String>,
    pub memory_bytes: u64,
    pub gpu_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageOffer {
    pub id: String,
    pub publisher_key: String,
    pub digest: String,
    pub bytes: u64,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub version: u16,
    pub audience: String,
    pub server_key: String,
    pub revision: u64,
    pub expires_unix: u64,
    pub scope: Scope,
    pub packages: Vec<PackageOffer>,
    pub fallback: String,
    pub carrier: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u16,
    pub api: u16,
    pub id: String,
    pub publisher_key: String,
    pub package_version: String,
    pub permissions: BTreeSet<Permission>,
    pub component: Option<String>,
    pub channels: Vec<crate::wire::Channel>,
    pub actions: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub templates: BTreeSet<String>,
    pub files: Vec<ContentFile>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Ui,
    ModalUi,
    Input,
    Messaging,
    Scene,
    Media,
}

/// Restricts identifiers to the shared portable namespace.
pub fn identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_IDENTIFIER_BYTES
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b":_-.".contains(&b))
}
/// Lists permissions with application adapters in the developer preview.
pub fn implemented_permissions() -> BTreeSet<Permission> {
    BTreeSet::from([
        Permission::Ui,
        Permission::ModalUi,
        Permission::Input,
        Permission::Messaging,
    ])
}
