use super::*;

#[test]
fn layers_merge_fields_by_full_selector_and_keep_other_animations() {
    let mut entries = BTreeMap::new();
    merge(&mut entries, br#"[
        {"atlas_tile":"water","flipbook_texture":"textures/water","frames":[2,0,1],"ticks_per_frame":7,"replicate":2,"blend_frames":false},
        {"atlas_tile":"water","atlas_index":1,"atlas_tile_variant":2,"flipbook_texture":"textures/other"},
        {"atlas_tile":"fire","flipbook_texture":"textures/fire"}
    ]"#);
    merge(
        &mut entries,
        br#"[{"atlas_tile":"water","ticks_per_frame":3}]"#,
    );
    let water = &entries[&(Box::from("water"), 0, 0)];
    assert_eq!(water.texture_path.as_ref(), "textures/water");
    assert_eq!(water.frames.as_ref(), [2, 0, 1]);
    assert_eq!(water.ticks_per_frame, 3);
    assert_eq!(water.replicate, 2);
    assert!(!water.blend_frames);
    assert!(water.all_variants);
    assert_eq!(entries.len(), 3);
    let other = &entries[&(Box::from("water"), 1, 2)];
    assert!(!other.all_variants);
    assert_eq!(other.texture_path.as_ref(), "textures/other");
}

#[test]
fn malformed_patches_preserve_lower_descriptions_and_empty_lists_keep_them() {
    let mut entries = BTreeMap::new();
    merge(
        &mut entries,
        br#"[{"atlas_tile":"ore","flipbook_texture":"textures/ore","ticks_per_frame":2}]"#,
    );
    let expected = entries.clone();
    for layer in [
        br#"[{"atlas_tile":"ore","ticks_per_frame":0}]"#.as_slice(),
        br#"[{"atlas_tile":"ore","replicate":0}]"#,
        br#"[{"atlas_tile":"ore","atlas_index":-1}]"#,
        br#"[{"atlas_tile":"ore","frames":[-1]}]"#,
        br#"[{"atlas_tile":"ore","frames":[0,1],"ticks_per_frame":4294967295}]"#,
        b"[]",
        b"{}",
        b"invalid",
    ] {
        merge(&mut entries, layer);
        assert_eq!(entries, expected);
    }
}
