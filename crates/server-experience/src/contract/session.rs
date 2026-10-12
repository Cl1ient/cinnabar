// Generated from crates/server-experience/schema/extension.json; do not edit.
#[derive(Debug, Deserialize, Serialize)]
#[serde(
    rename_all = "snake_case",
    tag = "kind",
    deny_unknown_fields,
    content = "body"
)]
pub enum Control {
    Hello(Hello),
    Accept(SignedDocument),
    Ready {
        session: String,
        packages: Vec<String>,
        generation: u64,
        permissions: std::collections::BTreeMap<
            String,
            std::collections::BTreeSet<crate::manifest::Permission>,
        >,
        world_epoch: u64,
    },
    Epoch {
        session: String,
        world_epoch: u64,
    },
    Disabled,
}
