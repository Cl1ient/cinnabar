//! Source routes retained alongside the compiled world carrier.

use super::*;

/// Records terrain aliases, flipbooks, and the destination each compiled material uses.
pub(super) fn build(
    pack: &PackSources,
    descriptors: &BTreeMap<Descriptor, u32>,
    materials: &[Material],
    animations: &[Animation],
) -> MaterialKeys {
    MaterialKeys::from_entries(
        descriptors
            .iter()
            .map(|(descriptor, &material)| (material, descriptor.texture_key.as_ref())),
    )
    .with_aliases(pack.terrain.first_paths())
    .with_flipbooks(&pack.flipbooks)
    .with_flipbook_destinations(pack.terrain.flipbook_destinations())
    .with_flipbook_selectors(descriptors.iter().map(|(descriptor, &material)| {
        let animated = animations
            .get(materials[material as usize].animation as usize)
            .map(|animation| (animation.atlas_index, animation.atlas_tile_variant));
        let (_, index, variant) = destination(descriptor, animated);
        (material, (index, variant))
    }))
}

/// Identifies a material's terrain destination independently of its deduplicated pixels.
pub(super) fn destination(
    descriptor: &Descriptor,
    animated: Option<(u32, u32)>,
) -> (&str, u32, u32) {
    let (index, variant) = animated.unwrap_or_else(|| {
        descriptor
            .flipbook_selector
            .unwrap_or((descriptor.state_variant, 0))
    });
    (&descriptor.texture_key, index, variant)
}

/// Adds generated materials without losing the original routes or animation descriptions.
pub(super) fn extend(
    keys: MaterialKeys,
    lily_pad_keys: &[(u32, Box<str>)],
    lily_pad_tints: impl IntoIterator<Item = (Box<str>, [u8; 3])>,
    seasonal_copies: &[(u32, u32)],
) -> MaterialKeys {
    let original = keys
        .keys()
        .flat_map(|key| keys.materials(key).iter().map(move |&id| (id, key)))
        .collect::<Vec<_>>();
    MaterialKeys::from_entries(
        original
            .iter()
            .copied()
            .chain(lily_pad_keys.iter().map(|(id, key)| (*id, key.as_ref())))
            .chain(seasonal_copies.iter().flat_map(|&(copy, original_id)| {
                original
                    .iter()
                    .filter_map(move |&(id, key)| (id == original_id).then_some((copy, key)))
            })),
    )
    .with_aliases(keys.aliases())
    .with_fixed_tints(lily_pad_tints)
    .with_flipbooks(keys.flipbooks())
    .with_flipbook_destinations(keys.flipbook_destinations())
    .with_flipbook_selectors(
        keys.flipbook_selectors().chain(
            seasonal_copies
                .iter()
                .map(|&(copy, original)| (copy, keys.flipbook_selector(original))),
        ),
    )
}

#[cfg(test)]
mod tests;
