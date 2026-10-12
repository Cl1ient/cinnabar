use super::*;
use std::io::{Cursor, Write};
use ui_engine::atlas::ServerAtlas;

/// Admits an archive containing the supplied artwork as a server pack.
pub(in crate::ui_runtime::presentation::forms) fn view(
    files: &[(String, Vec<u8>)],
) -> resource_pack::LayeredPackView {
    layered_view(&[files])
}

/// Admits fixture layers in exact lowest-to-highest precedence order.
pub(in crate::ui_runtime::presentation::forms) fn layered_view(
    layers: &[&[(String, Vec<u8>)]],
) -> resource_pack::LayeredPackView {
    let archives = layers.iter().enumerate().map(|(index, files)| {
        let id = format!("00000000-0000-0000-0000-{:012x}", index + 17);
        let manifest = format!(
            r#"{{"format_version":2,"header":{{"uuid":"{id}","version":[1,0,0]}},"modules":[{{"type":"resources"}}]}}"#
        );
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (path, bytes) in std::iter::once(("manifest.json", manifest.as_bytes())).chain(
            files.iter().map(|(path, bytes)| (path.as_str(), bytes.as_slice())),
        ) {
            zip.start_file(path, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        protocol::ResourcePackArchive::unencrypted(
            id.parse().unwrap(), "1.0.0".into(), String::new(), zip.finish().unwrap().into_inner(),
        )
    }).collect();
    resource_pack::LayeredPackView::new(resource_pack::validate_handoff(
        protocol::ResourcePackHandoff::from_archives(archives),
    ))
}

#[test]
fn pack_image_path_uses_supported_extensions_without_directory_restriction() {
    assert!(ServerUiPack::is_image_path("assets/gui/inventory.png"));
    assert!(ServerUiPack::is_image_path("custom/menu.tga"));
    assert!(!ServerUiPack::is_image_path("assets/gui/inventory.json"));
}

#[test]
fn animated_sidecars_survive_lazy_pack_reads_and_image_independence() {
    let files = vec![(
        "custom/gui/noise.json".into(),
        br#"{"frames":[{"frame":{"x":8,"y":0},"duration":50}]}"#.to_vec(),
    )];
    for atlas in [
        ServerAtlas::new(&files, None, 1),
        ServerAtlas::new(&[], Some(view(&files)), 1),
    ] {
        let first = atlas.aseprite_frames("custom/gui/noise").unwrap();
        assert_eq!(first[0].x, 8);
        assert!(Arc::ptr_eq(
            &first,
            &atlas.aseprite_frames("custom/gui/noise").unwrap()
        ));
        assert!(!atlas.has_image("custom/gui/noise"));
    }
}
