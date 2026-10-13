use super::*;
use assets::registry::{
    BeaconTint, BlockFlags, BlockSurvivalRule, CollisionSeed, ContributorRole, ModelState,
    RegistryProvenance,
};

/// Keeps canonical input independent from its compiled numerical program.
fn record(name: &str, state: &str) -> RegistryRecord {
    RegistryRecord {
        sequential_id: 0,
        network_hash: 0,
        name: name.into(),
        canonical_state: state.into(),
        flags: BlockFlags::empty(),
        model_family: ModelFamily::Unknown,
        contributor_role: ContributorRole::Primary,
        model_state: ModelState::default(),
        face_coverage: 0,
        collision_seed: CollisionSeed::default(),
        provenance: RegistryProvenance::DRAGONFLY,
        beacon_tint: BeaconTint::Unresolved,
    }
}

#[test]
fn independent_identities_join_slabs_without_manufacturing_missing_alternates() {
    let mut records = [
        record("fixture:custom", "{}"),
        record(
            "minecraft:oak_slab",
            r#"{"minecraft:vertical_half":{"type":"string","value":"bottom"}}"#,
        ),
        record(
            "minecraft:oak_slab",
            r#"{"minecraft:vertical_half":{"type":"string","value":"top"}}"#,
        ),
        record(
            "minecraft:oak_double_slab",
            r#"{"minecraft:vertical_half":{"type":"string","value":"bottom"}}"#,
        ),
        record("minecraft:stone", "{}"),
        record(
            "minecraft:unadmitted",
            r#"{"pillar_axis":{"type":"string","value":"x"},"fixture:effect":{"type":"byte","value":0}}"#,
        ),
    ];
    compile(&mut records).unwrap();
    assert_eq!(
        records[0].model_state.placement_facts().rule,
        Rule::Unresolved
    );
    for (record, key) in records[1..3].iter().zip([0, 1]) {
        let facts = record.model_state.placement_facts();
        assert_eq!(facts.rule, Rule::Slab);
        assert_eq!(facts.set, 3);
        assert_eq!(facts.key, key);
        assert_eq!(facts.alternate, Some(2));
    }
    assert_eq!(
        records[5].model_state.placement_facts().rule,
        Rule::Unresolved
    );
    let mut reduced = [records[1].clone()];
    compile(&mut reduced).unwrap();
    assert_eq!(reduced[0].model_state.placement_facts().set, 1);
    assert_eq!(reduced[0].model_state.placement_facts().alternate, None);
    let mut duplicates = [records[1].clone(), records[1].clone()];
    assert!(compile(&mut duplicates).is_err());
}

#[test]
fn invalid_types_and_unadmitted_survival_stay_unresolved() {
    let mut records = [
        record(
            "minecraft:fixture",
            r#"{"pillar_axis":{"type":"int","value":0}}"#,
        ),
        record(
            "minecraft:fixture",
            r#"{"pillar_axis":{"type":"string","value":"w"}}"#,
        ),
        record("minecraft:fixture", "null"),
        record("minecraft:unknown_survival", "{}"),
    ];
    compile(&mut records).unwrap();
    for record in &records[..3] {
        assert_eq!(record.model_state.placement_facts().rule, Rule::Unresolved);
    }
    assert_eq!(
        records[3].model_state.survival_facts().rule,
        BlockSurvivalRule::Unresolved
    );
}

#[test]
fn iron_trapdoors_remain_outside_hand_toggle_admission() {
    let state = r#"{"direction":{"type":"int","value":2},"open_bit":{"type":"byte","value":1},"upside_down_bit":{"type":"byte","value":0}}"#;
    let mut records = [
        record("minecraft:iron_trapdoor", state),
        record("minecraft:trapdoor", state),
    ];
    compile(&mut records).unwrap();
    assert!(
        !records[0]
            .model_state
            .placement_facts()
            .flags
            .contains(Flags::HAND_TOGGLE)
    );
    assert!(
        records[1]
            .model_state
            .placement_facts()
            .flags
            .contains(Flags::HAND_TOGGLE)
    );
}
