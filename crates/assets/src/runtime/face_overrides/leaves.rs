//! World leaf groups retain their exposure and depth flags when textures are rebound.

use crate::{
    AssetError, BLOCK_VISUAL_VARIANT_MATERIAL_MASK, BLOCK_VISUAL_VARIANT_NONSEASONAL_LEAF,
    BLOCK_VISUAL_VARIANT_SEASONAL_LEAF, BlockFace, BlockVisual, MAX_MATERIALS, Material,
    SEASONAL_LEAF_MATERIAL_COUNT,
};
use std::collections::HashMap;

/// Shares identical visual-local copies without changing another visual's world group.
#[derive(Default)]
pub(super) struct Groups {
    copied: HashMap<(u32, [Option<u32>; 6]), u32>,
}

impl Groups {
    /// Repoints the encoded leaf group as well as its ordinary carried face table.
    pub(super) fn rebind(
        &mut self,
        visual: &mut BlockVisual,
        faces: [Option<u32>; 6],
        materials: &mut Vec<Material>,
        local_base: u32,
    ) -> Result<(), AssetError> {
        let leaf = visual.variant
            & (BLOCK_VISUAL_VARIANT_SEASONAL_LEAF | BLOCK_VISUAL_VARIANT_NONSEASONAL_LEAF);
        if leaf == 0 {
            return Ok(());
        }
        let original = visual.variant & BLOCK_VISUAL_VARIANT_MATERIAL_MASK;
        if let Some(&base) = self.copied.get(&(original, faces)) {
            visual.variant = leaf | base;
            return Ok(());
        }
        let mut group = materials
            .get(original as usize..original as usize + SEASONAL_LEAF_MATERIAL_COUNT as usize)
            .ok_or_else(|| invalid("rebound leaf group is out of bounds"))?
            .to_vec();
        let mut weighted = Vec::new();
        for (index, world) in group.iter_mut().enumerate() {
            let Some(local) = faces[index % BlockFace::ALL.len()] else {
                continue;
            };
            let replacement = materials[(local_base + local) as usize];
            world.texture = replacement.texture;
            world.animation = replacement.animation;
            world.variation_start = 0;
            world.variation_count = replacement.variation_count;
            world.variation_weight = replacement.variation_weight;
            if replacement.variation_count != 0 {
                let start = replacement.variation_start as usize;
                let choices = materials
                    .get(start..start + replacement.variation_count as usize)
                    .ok_or_else(|| invalid("rebound leaf choices are out of bounds"))?;
                world.variation_start = (materials.len() + weighted.len()) as u32;
                weighted.extend(choices.iter().map(|choice| Material {
                    flags: world.flags,
                    ..*choice
                }));
            }
        }
        if materials.len() + weighted.len() + group.len() > MAX_MATERIALS {
            return Err(invalid("rebound leaf materials exceed the material limit"));
        }
        materials.extend(weighted);
        let base = materials.len() as u32;
        materials.extend(group);
        self.copied.insert((original, faces), base);
        visual.variant = leaf | base;
        Ok(())
    }
}

/// Reports invalid material groups before a visual can reference them.
fn invalid(detail: &str) -> AssetError {
    AssetError::InvalidCompiledAssets {
        detail: detail.into(),
    }
}
