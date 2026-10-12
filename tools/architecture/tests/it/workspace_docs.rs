use std::{fs, path::Path};

use architecture::{ArchitectureError, check_repository};

/// Builds a two-crate repository whose ownership document is checked against its policy.
fn fixture(root: &Path, document: Option<&str>) {
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=['alpha','beta']\n",
    )
    .expect("write workspace");
    fs::write(
        root.join("policy.toml"),
        r#"
production_rust_max = 1000
module_root_max = 300
powershell_max = 800
test_max = 1200
workspace_document = 'workspace.md'

[[crates]]
name = 'alpha'
path = 'alpha'

[[crates]]
name = 'beta'
path = 'beta'
"#,
    )
    .expect("write policy");
    for name in ["alpha", "beta"] {
        fs::create_dir(root.join(name)).expect("create crate");
        fs::write(
            root.join(name).join("Cargo.toml"),
            format!("[package]\nname='{name}'\nversion='0.1.0'\n"),
        )
        .expect("write crate manifest");
    }
    if let Some(document) = document {
        fs::write(root.join("workspace.md"), document).expect("write ownership document");
    }
}

#[test]
fn accepts_complete_ownership_rows_in_any_order() {
    let temp = tempfile::tempdir().expect("fixture root");
    let root = temp.path();
    fixture(
        root,
        Some(
            " | `beta` | `beta` | Call beta for decoding. | Alpha owns input. |\n\
            | `alpha` | `alpha` | Call alpha for input. | Beta owns decoding. |\n",
        ),
    );
    assert_eq!(
        check_repository(root, &root.join("policy.toml")).expect("check complete guide"),
        Vec::<String>::new(),
    );
}

#[test]
fn rejects_a_crate_mentioned_only_outside_its_ownership_row() {
    let temp = tempfile::tempdir().expect("fixture root");
    let root = temp.path();
    fixture(
        root,
        Some(
            "Beta lives in `beta`.\n\
            | `alpha` | `alpha` | Call alpha for input. | `beta` owns decoding. |\n",
        ),
    );
    assert_eq!(
        check_repository(root, &root.join("policy.toml")).expect("check incomplete guide"),
        ["workspace.md: workspace crate `beta` has no complete ownership row for `beta`"],
    );
}

#[test]
fn rejects_an_ownership_row_without_caller_guidance() {
    let temp = tempfile::tempdir().expect("fixture root");
    let root = temp.path();
    fixture(
        root,
        Some(
            "| `alpha` | `alpha` | | Beta owns decoding. |\n\
            | `beta` | `beta` | Call beta for decoding. | Alpha owns input. |\n",
        ),
    );
    assert_eq!(
        check_repository(root, &root.join("policy.toml")).expect("check incomplete row"),
        ["workspace.md: workspace crate `alpha` has no complete ownership row for `alpha`"],
    );
}

#[test]
fn rejects_a_stale_package_name_after_a_rename() {
    let temp = tempfile::tempdir().expect("fixture root");
    let root = temp.path();
    fixture(
        root,
        Some(
            "| `alpha` | `alpha` | Call alpha for input. | Beta owns decoding. |\n\
            | `beta` | `beta` | Call beta for decoding. | Alpha owns input. |\n",
        ),
    );
    fs::write(
        root.join("alpha/Cargo.toml"),
        "[package]\nname='alpha-renamed'\nversion='0.1.0'\n",
    )
    .expect("rename fixture package");
    assert_eq!(
        check_repository(root, &root.join("policy.toml")).expect("check renamed package"),
        [
            "workspace.md: ownership row for `alpha` names package `alpha` instead of `alpha-renamed`"
        ],
    );
}

#[test]
fn rejects_a_missing_ownership_document() {
    let temp = tempfile::tempdir().expect("fixture root");
    let root = temp.path();
    fixture(root, None);
    let result = check_repository(root, &root.join("policy.toml"));
    assert!(
        matches!(result, Err(ArchitectureError::Read { path, .. }) if path == root.join("workspace.md"))
    );
}
