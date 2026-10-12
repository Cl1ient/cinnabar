//! Shared classification of block items that use a flat carried raster.

use crate::{ModelFamily, RegistryRecord, VisualKind};

/// Registry families whose vanilla block shapes render as flat item icons.
const FLAT_FAMILIES: [ModelFamily; 11] = [
    ModelFamily::Cross,
    ModelFamily::Rail,
    ModelFamily::Torch,
    ModelFamily::Lever,
    ModelFamily::Vine,
    ModelFamily::GlowLichen,
    ModelFamily::SculkVein,
    ModelFamily::Pane,
    ModelFamily::Aquatic,
    ModelFamily::FlowerBed,
    ModelFamily::ResinClump,
];

/// Registry-unclassified blocks whose vanilla items show a flat icon (lanterns, candles,
/// chains, ladders, lily pads, end rods, sea pickles, spore blossoms, bamboo).
fn is_reviewed_flat(name: &str) -> bool {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    ["lantern", "candle", "chain"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
        || matches!(
            name,
            "ladder" | "waterlily" | "end_rod" | "sea_pickle" | "spore_blossom" | "bamboo" | "bell"
        )
}

/// Keeps accepted model items three-dimensional and uses flat art for the declared flat families.
pub fn block_item_is_flat(record: &RegistryRecord, kind: VisualKind) -> bool {
    FLAT_FAMILIES.contains(&record.model_family)
        || (kind == VisualKind::Cross && record.model_family != ModelFamily::Crop)
        || (record.model_family == ModelFamily::Unknown && is_reviewed_flat(&record.name))
}
