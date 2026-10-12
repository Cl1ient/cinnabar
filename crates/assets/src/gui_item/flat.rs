//! Shared classification of block items that use a flat carried raster.

use crate::{ModelFamily, RegistryRecord, VisualKind};

/// Block families whose existing item routes use flat carried art.
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

/// Other block identifiers whose existing item routes use flat carried art.
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
