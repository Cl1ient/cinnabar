use std::{fs, path::Path};

use architecture::check_repository;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// Returns read findings for `alpha`, whose source may read the `beta` crate's files.
fn findings(source: &str, alpha_metadata: &str) -> Vec<String> {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers=['crates/alpha','crates/beta']\n",
    );
    write(
        &root.join("crates/alpha/Cargo.toml"),
        &format!("[package]\nname='alpha'\nversion='0.1.0'\n{alpha_metadata}\n"),
    );
    write(&root.join("crates/alpha/src/lib.rs"), source);
    write(
        &root.join("crates/beta/Cargo.toml"),
        "[package]\nname='beta'\nversion='0.1.0'\n",
    );
    write(&root.join("crates/beta/data/table.tsv"), "id\n");
    write(
        &root.join("policy.toml"),
        "production_rust_max=1000\nmodule_root_max=300\npowershell_max=800\ntest_max=1200\n\
         [[crates]]\nname='alpha'\npath='crates/alpha'\n[[crates]]\nname='beta'\npath='crates/beta'\n",
    );
    check_repository(root, &root.join("policy.toml"))
        .unwrap()
        .into_iter()
        .filter(|line| line.contains("reads `") || line.contains("declared read"))
        .collect()
}

const MISSING: &str = "alpha: crates/alpha/src/lib.rs reads `crates/beta/data/table.tsv` from another crate; list it in `reads` under [package.metadata.cinnabar]";

#[test]
fn undeclared_reads_of_another_crate_fail() {
    for source in [
        "const TABLE: &str = include_str!(\"../../beta/data/table.tsv\");",
        "const TABLE: &[u8] = include_bytes!(\"../../beta/data/table.tsv\");",
        "const TABLE: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../beta/data/table.tsv\"));",
        "fn table() -> std::path::PathBuf { std::path::Path::new(env!(\"CARGO_MANIFEST_DIR\")).join(\"../beta/data/table.tsv\") }",
        "fn table() -> std::path::PathBuf { std::path::Path::new(env!(\"CARGO_MANIFEST_DIR\")).join(\"../../crates/beta/data/table.tsv\") }",
    ] {
        assert_eq!(findings(source, ""), [MISSING], "{source}");
        for declaration in [
            "crates/beta/data/table.tsv",
            "crates/beta/data/*",
            "crates/beta/data",
        ] {
            assert!(
                findings(
                    source,
                    &format!("[package.metadata.cinnabar]\nreads=['{declaration}']")
                )
                .is_empty(),
                "{source} with {declaration}"
            );
        }
    }
}

#[test]
fn directory_reads_need_a_covering_declaration() {
    let source = "fn data() -> std::path::PathBuf { std::path::Path::new(env!(\"CARGO_MANIFEST_DIR\")).join(\"../beta/data\") }";
    assert_eq!(
        findings(source, ""),
        [
            "alpha: crates/alpha/src/lib.rs reads `crates/beta/data` from another crate; list it in `reads` under [package.metadata.cinnabar]"
        ]
    );
    assert!(
        findings(
            source,
            "[package.metadata.cinnabar]\nreads=['crates/beta/data/*']"
        )
        .is_empty()
    );
}

#[test]
fn own_files_and_repository_paths_named_as_data_need_no_declaration() {
    for source in [
        "const OWN: &str = include_str!(\"../data/own.tsv\");",
        "const FIXTURE: &str = \"crates/beta/data/table.tsv\";",
        "const OUTSIDE: &str = \"../../../escape\";",
    ] {
        assert!(findings(source, "").is_empty(), "{source}");
    }
}

#[test]
fn stale_declarations_fail_unless_a_source_still_names_the_path() {
    let declared = "[package.metadata.cinnabar]\nreads=['crates/beta/data/table.tsv']";
    assert_eq!(
        findings("fn portable() {}", declared),
        ["alpha: declared read `crates/beta/data/table.tsv` matches nothing its sources read"]
    );
    assert!(
        findings(
            "fn root() -> &'static str { \"crates/beta/data/table.tsv\" }",
            declared
        )
        .is_empty()
    );
}
