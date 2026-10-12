//! Carrier source facts used by optional terrain catalogs and block bindings.

use super::*;
use assets::material_keys::{BlockTextureSource, MaterialTextureSource};

/// Keeps state selectors, weighted leaves, and per-block identities in the sidecar.
pub(super) fn retain(
    keys: MaterialKeys,
    pack: &PackSources,
    descriptors: &BTreeMap<Descriptor, u32>,
    materials: &[Material],
    blocks: (&[RegistryRecord], &[BlockVisual]),
    lily_pad_keys: &[(u32, Box<str>)],
    seasonal_copies: &[(u32, u32)],
) -> MaterialKeys {
    let (records, visuals) = blocks;
    let mut sources = BTreeMap::new();
    for (descriptor, &id) in descriptors {
        sources.insert(
            id,
            MaterialTextureSource {
                key: descriptor.texture_key.clone(),
                variant: descriptor.state_variant,
                path: descriptor.path.clone(),
            },
        );
        let material = materials[id as usize];
        if material.variation_count != 0
            && let Some(choices) = pack
                .terrain
                .position_variations
                .get(&(descriptor.texture_key.clone(), descriptor.state_variant))
        {
            for (index, choice) in choices.iter().enumerate() {
                sources.insert(
                    material.variation_start + index as u32,
                    MaterialTextureSource {
                        key: descriptor.texture_key.clone(),
                        variant: descriptor.state_variant,
                        path: choice.path.clone(),
                    },
                );
            }
        }
    }
    for (id, key) in lily_pad_keys {
        if let Some(path) = pack.terrain.get(key) {
            sources.insert(
                *id,
                MaterialTextureSource {
                    key: key.clone(),
                    variant: 0,
                    path: path.into(),
                },
            );
        }
    }
    for &(copy, original) in seasonal_copies {
        if let Some(source) = sources.get(&original).cloned() {
            sources.insert(copy, source);
        }
    }
    let blocks = records
        .iter()
        .map(|record| {
            let (binding_name, source_faces) =
                crate::pack::runtime_binding_identity(&pack.blocks, record);
            BlockTextureSource {
                visual: record.sequential_id,
                name: record.name.clone(),
                binding_name,
                source_faces,
                flat_item: assets::gui_item::block_item_is_flat(
                    record,
                    visuals[record.sequential_id as usize].kind,
                ),
                carried_fallback_keys: BlockFace::ALL.map(|face| {
                    crate::pack::resolve_carried_fallback_key(&pack.blocks, record, face)
                }),
                carried_keys: BlockFace::ALL.map(|face| {
                    crate::pack::resolve_carried_face_key(&pack.blocks, record, face)
                        .map(Into::into)
                }),
            }
        })
        .collect::<Vec<_>>();
    keys.with_sources(pack.terrain.runtime_entries(), sources, blocks)
}

/// Extends the material index with copies derived from existing compiler source routes.
pub(super) fn with_copies(
    keys: MaterialKeys,
    extra: &[(u32, Box<str>)],
    copies: &[(u32, u32)],
) -> MaterialKeys {
    let original = keys
        .keys()
        .flat_map(|key| keys.materials(key).iter().map(move |&id| (id, key)))
        .collect::<Vec<_>>();
    MaterialKeys::from_entries(
        original
            .iter()
            .copied()
            .chain(extra.iter().map(|(id, key)| (*id, key.as_ref())))
            .chain(copies.iter().flat_map(|&(copy, original_id)| {
                original
                    .iter()
                    .filter_map(move |&(id, key)| (id == original_id).then_some((copy, key)))
            })),
    )
    .with_aliases(keys.aliases())
}
