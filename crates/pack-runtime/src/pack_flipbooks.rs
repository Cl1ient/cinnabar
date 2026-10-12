//! Layered flipbook descriptions, keyed by the complete atlas destination.

use std::collections::BTreeMap;

use assets::flipbook::FlipbookSource;
use resource_pack::LayeredPackView;
use serde_json::Value;

use crate::pack_textures::{MAX_CATALOG_ENTRIES, parse_pack_json};

type Selector = (Box<str>, u32, u32);

/// Overlays only supplied fields of matching selectors and retains unrelated base animations.
pub fn flipbooks(view: &LayeredPackView, base: &[FlipbookSource]) -> Vec<FlipbookSource> {
    let mut entries = base
        .iter()
        .take(MAX_CATALOG_ENTRIES)
        .map(|source| {
            let selector = (
                source.atlas_tile.clone(),
                source.atlas_index,
                source.atlas_tile_variant,
            );
            (selector, source.clone())
        })
        .collect::<BTreeMap<_, _>>();
    for bytes in view.read_layers("textures/flipbook_textures.json") {
        merge(&mut entries, &bytes);
    }
    entries.into_values().collect()
}

/// Applies a bounded list of valid descriptions; malformed patches leave lower entries usable.
fn merge(entries: &mut BTreeMap<Selector, FlipbookSource>, bytes: &[u8]) {
    let Some(Value::Array(layer)) = parse_pack_json(bytes) else {
        return;
    };
    for value in layer.into_iter().take(MAX_CATALOG_ENTRIES) {
        let Value::Object(mut fields) = value else {
            continue;
        };
        let Some(tile) = fields.get("atlas_tile").and_then(Value::as_str) else {
            continue;
        };
        let Some(index) = selector_index(fields.get("atlas_index")) else {
            continue;
        };
        let Some(variant) = selector_index(fields.get("atlas_tile_variant")) else {
            continue;
        };
        let selector = (Box::<str>::from(tile), index, variant);
        let lower = entries.get(&selector);
        if entries.len() >= MAX_CATALOG_ENTRIES && lower.is_none() {
            continue;
        }
        let all_variants = if fields.contains_key("atlas_tile_variant") {
            false
        } else {
            lower.is_none_or(|source| source.all_variants)
        };
        fields.insert("all_variants".into(), Value::Bool(all_variants));
        let mut merged = lower.map_or_else(serde_json::Map::new, |source| {
            serde_json::to_value(source)
                .expect("flipbook serializes")
                .as_object()
                .expect("flipbook object")
                .clone()
        });
        merged.extend(fields);
        if let Ok(source) = serde_json::from_value::<FlipbookSource>(Value::Object(merged))
            && source.is_valid()
        {
            entries.insert(selector, source);
        }
    }
}

/// Reads a selector index with zero as its omitted value, rejecting malformed integers.
fn selector_index(value: Option<&Value>) -> Option<u32> {
    value.map_or(Some(0), |value| {
        value.as_u64().and_then(|value| u32::try_from(value).ok())
    })
}

#[cfg(test)]
mod tests;
