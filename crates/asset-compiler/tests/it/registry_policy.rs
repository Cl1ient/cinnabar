use std::{path::PathBuf, process::Command};

/// Locates the raw Go source exporter and committed regeneration inputs.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn active_registry_regeneration_preserves_every_output_byte() {
    let root = root();
    for input in [
        "crates/protocol/data/retail_items_1_26_50.tsv",
        "crates/protocol/data/retail_biomes_1_26_50.txt",
        "crates/assets/data/block-registry-v1001.bin",
    ] {
        if !root.join(input).is_file() {
            eprintln!("missing fixture: {input}; skipping registry regeneration");
            return;
        }
    }
    let scratch = tempfile::tempdir().unwrap();
    let facts = scratch.path().join("facts.json");
    let export = Command::new("go")
        .args(["-C", "tools/registrygen", "run", ".", "-facts-out"])
        .arg(&facts)
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let compile = Command::new(env!("CARGO_BIN_EXE_registryc"))
        .arg("--root")
        .arg(&root)
        .arg("--facts")
        .arg(facts)
        .args(["--kind", "all", "--check", "--out"])
        .arg(scratch.path().join("regenerated"))
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
}
