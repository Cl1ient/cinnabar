#[cfg(test)]
use super::*;

#[test]
fn platform_keys_match_the_release_artifact_names() {
    assert_eq!(
        platform_key("macos", "aarch64").as_deref(),
        Some("macos-arm64")
    );
    assert_eq!(
        platform_key("windows", "x86_64").as_deref(),
        Some("windows-x86_64")
    );
    assert_eq!(
        platform_key("linux", "x86_64").as_deref(),
        Some("linux-x86_64")
    );
    assert_eq!(platform_key("linux", "riscv64"), None);
    assert_eq!(platform_key("freebsd", "x86_64"), None);
}

#[test]
fn checks_run_at_most_once_per_interval() {
    assert!(due(None, 10));
    assert!(!due(Some(1_000), 1_000 + 3_600));
    assert!(due(Some(1_000), 1_000 + 24 * 3_600));
    assert!(due(Some(5_000), 100));
}

#[test]
fn recorded_verdicts_parse_with_and_without_an_artifact() {
    let with = br#"{"available":true,"current":"0.1.0","latest":"0.2.0","artifact":{"url":"https://x/y","sha256":"ab","size":9}}"#;
    let notice: Verdict = serde_json::from_slice(with).unwrap();
    assert_eq!(notice.artifact.unwrap().size, 9);
    let without = br#"{"available":false,"current":"0.2.0","latest":"0.2.0"}"#;
    assert!(
        !serde_json::from_slice::<Verdict>(without)
            .unwrap()
            .available
    );
    assert!(serde_json::from_slice::<Verdict>(b"nope").is_err());
}

#[test]
fn manifests_are_fetched_only_over_https() {
    assert!(https("https://example.test/update-stable.json"));
    for url in ["http://example.test/m", "file:///m", "not a url", ""] {
        assert!(!https(url), "{url} accepted");
    }
}
