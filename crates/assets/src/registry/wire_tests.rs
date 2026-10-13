use super::*;

/// Decode the synthetic carrier checked against the Go encoder.
fn fixture() -> Vec<u8> {
    let hex = include_str!("../../tests/fixtures/registry-wire.hex")
        .split_whitespace()
        .collect::<String>();
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn reads_the_go_wire_contract_without_reinterpreting_fields() {
    let records = read_registry_for_protocol(&fixture(), 2193).unwrap();
    assert_eq!(records.len(), 2);
    let leaf = &records[0];
    assert_eq!(leaf.sequential_id, 7);
    assert_eq!(leaf.beacon_tint, BeaconTint::Red);
    assert_eq!(leaf.network_hash, 0x1234_5678);
    assert_eq!(leaf.name.as_ref(), "fixture:leaf");
    assert_eq!(
        leaf.canonical_state.as_ref(),
        r#"{"color":{"type":"int","value":3}}"#
    );
    assert_eq!(
        leaf.flags,
        BlockFlags::CUBE_GEOMETRY | BlockFlags::LEAF_MODEL
    );
    assert_eq!(leaf.model_family, ModelFamily::Leaves);
    assert_eq!(leaf.contributor_role, ContributorRole::Primary);
    assert_eq!(leaf.model_state.mask(), 0xff);
    assert_eq!(
        leaf.model_state.survival_facts(),
        BlockSurvivalFacts {
            rule: BlockSurvivalRule::Unresolved,
            policy: BlockSupportPolicy::Unresolved,
            support: BlockSupportKind::Leaves,
        }
    );
    for (field, value) in [
        (ModelStateField::Orientation, 1),
        (ModelStateField::Half, 2),
        (ModelStateField::Open, 3),
        (ModelStateField::Hinge, 4),
        (ModelStateField::Connections, 5),
        (ModelStateField::Growth, 6),
        (ModelStateField::LiquidDepth, 7),
        (ModelStateField::Flags, 8),
    ] {
        assert_eq!(leaf.model_state.get(field), Some(value));
    }
    assert_eq!(leaf.face_coverage, 0x3f);
    assert_eq!(leaf.provenance, RegistryProvenance::all());
    assert_eq!(leaf.collision_seed.shape_id, 0x1234);
    assert_eq!(
        leaf.collision_seed.confidence,
        CollisionConfidence::ReviewedVisibleBounds
    );
    assert_eq!(
        leaf.collision_seed.boxes.as_ref(),
        &[CollisionBox {
            min_x: -1,
            min_y: 0,
            min_z: 1,
            max_x: 100_000_000,
            max_y: 99_999_999,
            max_z: 50_000_000,
        }]
    );
    let resin = &records[1];
    assert_eq!(resin.sequential_id, 8);
    assert_eq!(resin.beacon_tint, BeaconTint::NoTinting);
    assert_eq!(resin.network_hash, 0x8765_4321);
    assert_eq!(resin.name.as_ref(), "fixture:resin");
    assert_eq!(resin.canonical_state.as_ref(), "{}");
    assert_eq!(resin.model_family, ModelFamily::ResinClump);
    assert_eq!(resin.contributor_role, ContributorRole::LiquidAdditional);
    assert_eq!(resin.model_state.get(ModelStateField::Orientation), None);
    assert_eq!(resin.model_state.get(ModelStateField::Flags), Some(0xfeed));
    assert!(resin.collision_seed.boxes.is_empty());
}

#[test]
fn rejects_reserved_discriminants_bits_and_inconsistent_lengths() {
    // Fixture offsets are independent of the generated decoder.
    for (offset, value) in [
        (44, 0x80),
        (45, 0xff),
        (46, 0xff),
        (49, 0xff),
        (50, 0x80),
        (51, 8),
        (92, 0xff),
        (93, 0xff),
        (94, 0xff),
        (95, 0xff),
        (108, 0x80),
        (109, 0xff),
    ] {
        let mut bytes = fixture();
        bytes[offset] = value;
        assert!(
            read_registry_for_protocol(&bytes, 2193).is_err(),
            "offset {offset}"
        );
    }
    let mut bytes = fixture();
    bytes[56..60].copy_from_slice(&1_048_577u32.to_le_bytes());
    assert!(matches!(
        read_registry_for_protocol(&bytes, 2193),
        Err(AssetError::RegistryStateTooLarge { .. })
    ));
    let bytes = fixture();
    assert!(read_registry_for_protocol(&bytes[..bytes.len() - 1], 2193).is_err());
    let mut bytes = fixture();
    bytes.push(0);
    assert!(matches!(
        read_registry_for_protocol(&bytes, 2193),
        Err(AssetError::TrailingRegistryBytes { .. })
    ));
}

#[test]
fn accepts_compiled_enrichment_flags_without_using_source_only_mask() {
    let mut bytes = fixture();
    bytes[44] |= BlockFlags::SEASONAL_REPLACEABLE.bits()
        | BlockFlags::FIRE_FLAMMABLE.bits()
        | BlockFlags::FIRE_TOP_SUPPORT.bits();
    let records = read_registry_for_protocol(&bytes, 2193).unwrap();
    assert!(records[0].flags.contains(BlockFlags::SEASONAL_REPLACEABLE));
    assert!(records[0].flags.contains(BlockFlags::FIRE_FLAMMABLE));
    assert!(records[0].flags.contains(BlockFlags::FIRE_TOP_SUPPORT));
}

#[test]
fn placement_inventory_rejects_missing_identity_out_of_range_keys_and_wrong_alternates() {
    let mut bytes = fixture();
    // Fixture:leaf is independently the first sorted identity; Identity has only key zero.
    bytes[95] = BlockPlacementRule::Identity as u8;
    bytes[96..100].copy_from_slice(&1_u32.to_le_bytes());
    let admitted = read_registry_for_protocol(&bytes, 2193).unwrap();
    assert_eq!(admitted[0].model_state.placement_facts().set, 1);
    for (offset, value) in [(96, 0_u32), (96, 2), (100, 1), (104, 3)] {
        let mut forged = bytes.clone();
        forged[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(read_registry_for_protocol(&forged, 2193).is_err());
    }
    bytes[95] = BlockPlacementRule::Unresolved as u8;
    assert!(read_registry_for_protocol(&bytes, 2193).is_err());
}

#[test]
fn stale_active_registry_requires_rebuilding_and_legacy_facts_stay_unresolved() {
    for magic in [PREVIOUS_REGISTRY_MAGIC, PLACEMENT_REGISTRY_MAGIC] {
        let mut stale = fixture();
        stale[..8].copy_from_slice(magic);
        let error = read_registry_for_protocol(&stale, 2193).unwrap_err();
        assert!(error.to_string().contains("make assets"));
    }
    let legacy = read_registry(include_bytes!("../../data/block-registry-v1001.bin")).unwrap();
    assert!(
        legacy
            .iter()
            .all(|record| record.beacon_tint == BeaconTint::Unresolved)
    );
}
