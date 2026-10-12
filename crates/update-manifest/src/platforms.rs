//! The release pipeline and updater read the same supported platform and artifact names.
use serde::Deserialize;
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Deserialize)]
struct ReleaseAssets {
    repository: String,
    assets: BTreeMap<String, BTreeMap<String, String>>,
    update_assets: BTreeMap<String, BTreeMap<String, String>>,
}

static RELEASE: LazyLock<ReleaseAssets> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../packaging/release-assets.json"))
        .expect("release asset manifest must be valid")
});

/// Returns a key only for an architecture actually published by the release pipeline.
pub fn platform_key(os: &str, arch: &str) -> Option<String> {
    let arch = if arch == "aarch64" { "arm64" } else { arch };
    RELEASE
        .assets
        .get(os)?
        .contains_key(arch)
        .then(|| format!("{os}-{arch}"))
}

/// The native package filename selected by a manifest publisher for this platform.
pub fn artifact_name(os: &str, arch: &str) -> Option<&'static str> {
    let arch = if arch == "aarch64" { "arm64" } else { arch };
    RELEASE
        .update_assets
        .get(os)
        .and_then(|arches| arches.get(arch))
        .or_else(|| RELEASE.assets.get(os)?.get(arch))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_published_platform_has_one_native_update_artifact() {
        for (os, arches) in &RELEASE.assets {
            for arch in arches.keys() {
                assert_eq!(platform_key(os, arch), Some(format!("{os}-{arch}")));
                let name = artifact_name(os, arch).unwrap();
                let extension = match os.as_str() {
                    "windows" => ".msi",
                    "macos" => ".dmg",
                    "linux" => ".AppImage",
                    _ => panic!("unsupported platform"),
                };
                assert!(name.ends_with(extension));
            }
        }
        for (os, arches) in &RELEASE.update_assets {
            for arch in arches.keys() {
                assert!(platform_key(os, arch).is_some());
            }
        }
        assert!(platform_key("windows", "aarch64").is_none());
        assert!(platform_key("freebsd", "x86_64").is_none());
    }
}

/// Repository that publishes the installer's channel manifests and native artifacts.
pub fn repository() -> &'static str {
    &RELEASE.repository
}
