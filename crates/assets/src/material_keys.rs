//! Terrain texture key index over a compiled carrier's materials, so a session
//! can retexture vanilla blocks from a server pack's `terrain_texture.json`.
//! It also carries the vanilla key-to-image aliases, since installs keep no unpacked pack.

use std::collections::BTreeMap;

use crate::texture::flipbook::FlipbookSource;
use serde::{Deserialize, Serialize};

/// Sidecar wire version; part of the world carrier's prepared identity.
pub const MATERIAL_KEYS_SCHEMA: u32 = 4;
/// Largest sidecar file the runtime reads.
pub const MAX_MATERIAL_KEYS_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    schema: u32,
    materials: u32,
    keys: BTreeMap<String, Vec<u32>>,
    aliases: BTreeMap<String, String>,
    fixed_tints: BTreeMap<String, [u8; 3]>,
    flipbooks: Vec<FlipbookSource>,
    flipbook_selectors: BTreeMap<u32, (u32, u32)>,
    flipbook_destinations: BTreeMap<String, Vec<u32>>,
}

/// Material ids by the terrain texture key they were compiled from.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MaterialKeys {
    keys: BTreeMap<Box<str>, Box<[u32]>>,
    /// Every vanilla terrain key's variant-zero image path.
    aliases: BTreeMap<Box<str>, Box<str>>,
    fixed_tints: BTreeMap<Box<str>, [u8; 3]>,
    flipbooks: Box<[FlipbookSource]>,
    flipbook_selectors: BTreeMap<u32, (u32, u32)>,
    flipbook_destinations: BTreeMap<Box<str>, Box<[u32]>>,
}

impl MaterialKeys {
    /// Groups `(material id, texture key)` pairs by key, ids sorted and unique.
    pub fn from_entries<K: AsRef<str>>(entries: impl IntoIterator<Item = (u32, K)>) -> Self {
        let mut grouped = BTreeMap::<Box<str>, Vec<u32>>::new();
        for (material, key) in entries {
            grouped
                .entry(key.as_ref().into())
                .or_default()
                .push(material);
        }
        let keys = grouped
            .into_iter()
            .map(|(key, mut ids)| {
                ids.sort_unstable();
                ids.dedup();
                (key, ids.into_boxed_slice())
            })
            .collect();
        Self {
            keys,
            aliases: BTreeMap::new(),
            fixed_tints: BTreeMap::new(),
            flipbooks: Box::default(),
            flipbook_selectors: BTreeMap::new(),
            flipbook_destinations: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_aliases<K: AsRef<str>, P: AsRef<str>>(
        mut self,
        aliases: impl IntoIterator<Item = (K, P)>,
    ) -> Self {
        self.aliases = aliases
            .into_iter()
            .map(|(key, path)| (key.as_ref().into(), path.as_ref().into()))
            .collect();
        for key in self.aliases.keys() {
            self.flipbook_destinations
                .entry(key.clone())
                .or_insert_with(|| vec![1].into());
        }
        self
    }

    #[must_use]
    pub fn with_fixed_tints<K: AsRef<str>>(
        mut self,
        tints: impl IntoIterator<Item = (K, [u8; 3])>,
    ) -> Self {
        self.fixed_tints = tints
            .into_iter()
            .map(|(key, tint)| (key.as_ref().into(), tint))
            .collect();
        self
    }

    /// Literal atlas multipliers already baked into the base material textures.
    pub fn fixed_tints(&self) -> impl Iterator<Item = (&str, [u8; 3])> {
        self.fixed_tints
            .iter()
            .map(|(key, tint)| (key.as_ref(), *tint))
    }

    /// Retains the base pack's animation descriptions beneath optional pack layers.
    #[must_use]
    pub fn with_flipbooks(mut self, flipbooks: &[FlipbookSource]) -> Self {
        self.flipbooks = flipbooks.into();
        self
    }

    /// Records each material's atlas index and UV variant for animation replacement.
    #[must_use]
    pub fn with_flipbook_selectors(
        mut self,
        selectors: impl IntoIterator<Item = (u32, (u32, u32))>,
    ) -> Self {
        self.flipbook_selectors = selectors.into_iter().collect();
        self
    }

    /// Base animation descriptions with their original image routes and timelines.
    pub fn flipbooks(&self) -> &[FlipbookSource] {
        &self.flipbooks
    }

    /// Atlas destination for a material; ordinary unindexed tiles use `(0, 0)`.
    pub fn flipbook_selector(&self, material: u32) -> (u32, u32) {
        self.flipbook_selectors
            .get(&material)
            .copied()
            .unwrap_or_default()
    }

    /// Recorded destinations, used when copying carrier materials.
    pub fn flipbook_selectors(&self) -> impl Iterator<Item = (u32, (u32, u32))> + '_ {
        self.flipbook_selectors
            .iter()
            .map(|(&material, &selector)| (material, selector))
    }

    /// Records the number of UV variants at each atlas index, including unused destinations.
    #[must_use]
    pub fn with_flipbook_destinations<K: AsRef<str>, C: AsRef<[u32]>>(
        mut self,
        destinations: impl IntoIterator<Item = (K, C)>,
    ) -> Self {
        self.flipbook_destinations = destinations
            .into_iter()
            .map(|(key, counts)| (key.as_ref().into(), counts.as_ref().into()))
            .collect();
        self
    }

    /// Atlas index and UV-variant bounds derived from the complete base terrain catalog.
    pub fn flipbook_destinations(&self) -> impl Iterator<Item = (&str, &[u32])> {
        self.flipbook_destinations
            .iter()
            .map(|(key, counts)| (key.as_ref(), counts.as_ref()))
    }

    /// Serializes the sidecar for a carrier with `material_count` materials.
    #[must_use]
    pub fn to_json(&self, material_count: u32) -> Vec<u8> {
        let wire = Wire {
            schema: MATERIAL_KEYS_SCHEMA,
            materials: material_count,
            keys: self
                .keys
                .iter()
                .map(|(key, ids)| (key.to_string(), ids.to_vec()))
                .collect(),
            aliases: self
                .aliases
                .iter()
                .map(|(key, path)| (key.to_string(), path.to_string()))
                .collect(),
            fixed_tints: self
                .fixed_tints
                .iter()
                .map(|(key, tint)| (key.to_string(), *tint))
                .collect(),
            flipbooks: self.flipbooks.to_vec(),
            flipbook_selectors: self.flipbook_selectors.clone(),
            flipbook_destinations: self
                .flipbook_destinations
                .iter()
                .map(|(key, counts)| (key.to_string(), counts.to_vec()))
                .collect(),
        };
        serde_json::to_vec(&wire).expect("material key sidecar serializes")
    }

    /// Parses a sidecar; `None` unless it matches a carrier of `material_count`
    /// materials exactly.
    #[must_use]
    pub fn from_json(bytes: &[u8], material_count: usize) -> Option<Self> {
        let wire: Wire = serde_json::from_slice(bytes).ok()?;
        if wire.schema != MATERIAL_KEYS_SCHEMA || wire.materials as usize != material_count {
            return None;
        }
        if wire.flipbooks.iter().any(|flipbook| !flipbook.is_valid())
            || wire
                .flipbook_selectors
                .keys()
                .any(|&id| id as usize >= material_count)
            || wire
                .flipbook_destinations
                .values()
                .any(|counts| counts.is_empty() || counts.contains(&0))
        {
            return None;
        }
        let mut keys = BTreeMap::new();
        for (key, ids) in wire.keys {
            if ids.iter().any(|&id| id as usize >= material_count) {
                return None;
            }
            keys.insert(key.into_boxed_str(), ids.into_boxed_slice());
        }
        let aliases = wire
            .aliases
            .into_iter()
            .map(|(key, path)| (key.into_boxed_str(), path.into_boxed_str()))
            .collect();
        let fixed_tints = wire
            .fixed_tints
            .into_iter()
            .map(|(key, tint)| (key.into_boxed_str(), tint))
            .collect();
        Some(Self {
            keys,
            aliases,
            fixed_tints,
            flipbooks: wire.flipbooks.into_boxed_slice(),
            flipbook_selectors: wire.flipbook_selectors,
            flipbook_destinations: wire
                .flipbook_destinations
                .into_iter()
                .map(|(key, counts)| (key.into_boxed_str(), counts.into_boxed_slice()))
                .collect(),
        })
    }

    /// Material ids compiled from `key`; empty when the key is unknown.
    #[must_use]
    pub fn materials(&self, key: &str) -> &[u32] {
        self.keys.get(key).map_or(&[], |ids| ids)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.keys.keys().map(AsRef::as_ref)
    }

    /// `(terrain key, image path)` for every vanilla catalog entry.
    pub fn aliases(&self) -> impl Iterator<Item = (&str, &str)> {
        self.aliases
            .iter()
            .map(|(key, path)| (key.as_ref(), path.as_ref()))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::MaterialKeys;

    // The sidecar round-trips and refuses a carrier of a different size.
    #[test]
    fn json_round_trips_and_checks_the_material_count() {
        let keys =
            MaterialKeys::from_entries([(3, "stone"), (1, "stone"), (2, "dirt"), (1, "stone")]);
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
    fn animation_sources_and_destinations_survive_sidecar_round_trip() {
        let source: crate::texture::flipbook::FlipbookSource =
            serde_json::from_value(serde_json::json!({
                "flipbook_texture":"textures/strip", "atlas_tile":"tile", "frames":[2,0,1],
                "atlas_index":3, "atlas_tile_variant":2, "ticks_per_frame":7,
                "replicate":2, "blend_frames":false
            }))
            .unwrap();
        let keys = MaterialKeys::from_entries([(1, "tile"), (2, "tile")])
            .with_aliases([("tile", "textures/static")])
            .with_flipbooks(std::slice::from_ref(&source))
            .with_flipbook_selectors([(1, (3, 2)), (2, (3, 1))]);
        let bytes = keys.to_json(3);
        assert_eq!(MaterialKeys::from_json(&bytes, 3), Some(keys));
        for (field, value) in [
            ("ticks_per_frame", serde_json::json!(0)),
            ("replicate", serde_json::json!(0)),
            ("frames", serde_json::json!([0, 1])),
        ] {
            let mut wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            wire["flipbooks"][0][field] = value;
            if field == "frames" {
                wire["flipbooks"][0]["ticks_per_frame"] = serde_json::json!(u32::MAX);
            }
            assert!(MaterialKeys::from_json(&serde_json::to_vec(&wire).unwrap(), 3).is_none());
        }
    }
}
