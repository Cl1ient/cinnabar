use super::MaterialKeys;

// The sidecar round-trips and refuses a carrier of a different size.
#[test]
fn json_round_trips_and_checks_the_material_count() {
    let keys = MaterialKeys::from_entries([(3, "stone"), (1, "stone"), (2, "dirt"), (1, "stone")]);
    assert_eq!(keys.materials("stone"), [1, 3]);
    assert_eq!(keys.materials("missing"), [] as [u32; 0]);
    let json = keys.to_json(4);
    assert_eq!(MaterialKeys::from_json(&json, 4), Some(keys));
    assert_eq!(MaterialKeys::from_json(&json, 5), None);
    assert_eq!(MaterialKeys::from_json(&json, 3), None);
}

// Packaged installs resolve vanilla aliases from the sidecar; a pre-alias one is stale.
#[test]
fn aliases_round_trip_and_alias_free_sidecars_are_rejected() {
    let keys = MaterialKeys::from_entries([(0, "stone")])
        .with_aliases([("stone", "textures/blocks/stone"), ("unused", "textures/x")])
        .with_fixed_tints([("stone", [20, 80, 30])]);
    let decoded = MaterialKeys::from_json(&keys.to_json(1), 1).unwrap();
    assert_eq!(
        decoded.fixed_tints().collect::<Vec<_>>(),
        [("stone", [20, 80, 30])]
    );
    assert_eq!(
        decoded.aliases().collect::<Vec<_>>(),
        [("stone", "textures/blocks/stone"), ("unused", "textures/x")]
    );
    assert_eq!(
        MaterialKeys::from_json(br#"{"schema":1,"materials":1,"keys":{"stone":[0]}}"#, 1),
        None
    );
}

#[test]
fn complete_sources_round_trip_with_variant_and_carried_binding_identity() {
    use super::{BlockTextureSource, MaterialTextureSource};
    let keys = MaterialKeys::from_entries([(1, "fixture")]).with_aliases([("fixture", "textures/a")])
        .with_sources(
            [("fixture".into(), serde_json::json!({"textures":["textures/a",{"variations":["textures/b","textures/c"]}]}))],
            [(1, MaterialTextureSource { key:"fixture".into(), variant:1, path:"textures/b".into(), raster: "textures/b.png".into(), baked_tint:Some([120,80,40]) })],
            [BlockTextureSource { visual:2, name:"minecraft:fixture".into(), binding_name:"fixture".into(),
                source_faces:[2,3,1,0,4,5], source_rotations:[false,false,true,true,true,true], cube_binding_eligible:true, carried_keys:std::array::from_fn(|_| Some("carried".into())),
                carried_fallback_keys:std::array::from_fn(|_| Some("fixture".into())) }],
        );
    let restored = MaterialKeys::from_json(&keys.to_json(3), 3).unwrap();
    assert_eq!(restored, keys);
    assert_eq!(restored.material_source(1).unwrap().variant, 1);
    assert_eq!(
        restored.material_source(1).unwrap().raster.as_ref(),
        "textures/b.png"
    );
    assert_eq!(
        restored.material_source(1).unwrap().baked_tint,
        Some([120, 80, 40])
    );
    assert_eq!(
        restored.block_sources()[0].source_rotations,
        [false, false, true, true, true, true]
    );
    assert_eq!(restored.block_sources()[0].source_faces, [2, 3, 1, 0, 4, 5]);
    let mut bad: serde_json::Value = serde_json::from_slice(&keys.to_json(3)).unwrap();
    bad["block_sources"][0]["source_faces"][0] = 6.into();
    assert!(MaterialKeys::from_json(&serde_json::to_vec(&bad).unwrap(), 3).is_none());
    bad["block_sources"][0]["source_faces"][0] = 3.into();
    assert!(MaterialKeys::from_json(&serde_json::to_vec(&bad).unwrap(), 3).is_none());
}
