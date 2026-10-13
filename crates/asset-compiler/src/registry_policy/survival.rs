//! Provisional support programs derived from admitted placement and render facts.

use assets::registry::{
    BlockFlags, BlockPlacementRule as Placement, BlockSupportKind as Kind,
    BlockSupportPolicy as Policy, BlockSurvivalFacts, BlockSurvivalRule as Rule, ModelFamily,
    RegistryRecord,
};

/// Converts a bounded support face to its typed survival disposition.
fn face(value: u32) -> Rule {
    match value {
        0 => Rule::Down,
        1 => Rule::Up,
        2 => Rule::North,
        3 => Rule::South,
        4 => Rule::West,
        5 => Rule::East,
        _ => Rule::Unresolved,
    }
}

/// Retains admitted support rules while leaving other identities unresolved.
pub(super) fn compile(record: &RegistryRecord) -> BlockSurvivalFacts {
    let name = record.name.as_ref();
    let support = match name {
        "minecraft:air" => Kind::Air,
        "minecraft:water"
        | "minecraft:flowing_water"
        | "minecraft:lava"
        | "minecraft:flowing_lava" => Kind::Fluid,
        "minecraft:powder_snow" => Kind::PowderSnow,
        "minecraft:stone" => Kind::Stone,
        "minecraft:snow_layer" => Kind::Snow,
        "minecraft:fire" | "minecraft:soul_fire" => Kind::Rejected,
        _ if record.model_family == ModelFamily::Leaves => Kind::Leaves,
        _ if record
            .flags
            .contains(BlockFlags::CUBE_GEOMETRY | BlockFlags::OCCLUDES_FULL_FACE) =>
        {
            Kind::FullCube
        }
        _ => Kind::Unresolved,
    };
    let facts = record.model_state.placement_facts();
    let key = facts.key;
    let (rule, policy) = match facts.rule {
        Placement::Axis
        | Placement::Loom
        | Placement::Glazed
        | Placement::Stair
        | Placement::Slab
        | Placement::DoubleSlab
        | Placement::Trapdoor
        | Placement::Hopper
        | Placement::Connections
        | Placement::Dispenser
        | Placement::Piston => (Rule::Independent, Policy::Unresolved),
        Placement::Torch => (
            if key == 1 {
                face(0)
            } else if key >= 2 {
                face(key)
            } else {
                Rule::Unresolved
            },
            Policy::FullSolid,
        ),
        Placement::Button => (face((key & 7) ^ 1), Policy::FullSolid),
        Placement::Lever => (
            face([1, 1, 0, 0, 3, 2, 5, 4][(key & 7) as usize]),
            Policy::FullSolid,
        ),
        Placement::Lantern => (face(key), Policy::FullSolid),
        Placement::Candle | Placement::SeaPickle | Placement::Door | Placement::StandingSign => {
            (Rule::Down, Policy::FullSolid)
        }
        Placement::WallSign => (face(key ^ 1), Policy::FullSolid),
        Placement::Snow => (Rule::Down, Policy::Snow),
        Placement::Vine => (
            [1, 4, 8, 2]
                .iter()
                .position(|bit| *bit == key)
                .map_or(Rule::Unresolved, |index| face((index as u32 + 2) ^ 1)),
            Policy::Vine,
        ),
        Placement::Multiface => (
            [1, 2, 16, 4, 8, 32]
                .iter()
                .position(|bit| *bit == key)
                .map_or(Rule::Unresolved, |index| face(index as u32)),
            Policy::FullSolid,
        ),
        Placement::Identity
            if record.model_family == ModelFamily::Carpet
                && !matches!(name, "minecraft:moss_carpet" | "minecraft:pale_moss_carpet") =>
        {
            (Rule::Down, Policy::NonAir)
        }
        Placement::Identity if name == "minecraft:stone" => (Rule::Independent, Policy::Unresolved),
        _ => (Rule::Unresolved, Policy::Unresolved),
    };
    BlockSurvivalFacts {
        rule,
        policy,
        support,
    }
}
