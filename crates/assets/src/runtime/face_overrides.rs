//! Block-specific material rebinding preserves each carrier visual's geometry.

use crate::{AssetError, BlockFace, BlockVisual, Material, ModelQuad, ModelTemplate};

mod leaves;

/// Mutable tables that own a visual's faces, world materials, and model geometry.
pub(super) struct Tables<'a> {
    pub visuals: &'a mut [BlockVisual],
    pub materials: &'a mut Vec<Material>,
    pub templates: &'a mut Vec<ModelTemplate>,
    pub quads: &'a mut Vec<ModelQuad>,
    pub offsets: &'a mut Vec<(u32, block_transform::random_offset::RandomOffsetComponent)>,
}

/// Replaces only named faces and clones shared model parts before changing their materials.
pub(super) fn apply(
    replacements: &[(u32, [Option<u32>; 6])],
    tables: Tables<'_>,
    material_range: std::ops::Range<u32>,
) -> Result<(), AssetError> {
    let Tables {
        visuals,
        materials,
        templates,
        quads,
        offsets,
    } = tables;
    let material_base = material_range.start;
    let local_materials = (material_range.end - material_base) as usize;
    if replacements.is_empty() {
        return Ok(());
    }
    let mut groups = crate::blob::compiled_stair_bases(templates)?
        .into_iter()
        .map(|base| (base, crate::MODEL_TEMPLATE_FLAG_STAIR))
        .chain(crate::blob::compiled_connected_bases(templates)?)
        .collect::<Vec<_>>();
    groups.sort_unstable_by_key(|group| group.0);
    let group_ends = groups
        .iter()
        .enumerate()
        .map(|(index, &(base, flag))| {
            let next = groups
                .get(index + 1)
                .map_or(templates.len(), |group| group.0);
            let different = templates[base..next]
                .iter()
                .position(|template| template.flags != flag)
                .map_or(next, |offset| base + offset);
            (base, different.min(next))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut leaf_groups = leaves::Groups::default();
    let mut seen = std::collections::HashSet::new();
    let mut copied_models = std::collections::HashMap::new();
    for &(id, replacement) in replacements {
        if !seen.insert(id)
            || replacement
                .iter()
                .flatten()
                .any(|&id| id as usize >= local_materials)
        {
            return Err(invalid(
                "base face override is duplicated or references an invalid material",
            ));
        }
        let visual = visuals
            .get_mut(id as usize)
            .ok_or_else(|| invalid("base face override visual is out of bounds"))?;
        let old_faces = visual.faces;
        for (face, material) in replacement.iter().enumerate() {
            if let Some(material) = material {
                visual.faces[face] = material_base + material;
            }
        }
        leaf_groups.rebind(visual, replacement, materials, material_base)?;
        if visual.model_template == crate::NO_MODEL_TEMPLATE {
            continue;
        }
        let cache_key = (visual.model_template, old_faces, replacement);
        if let Some(&template) = copied_models.get(&cache_key) {
            visual.model_template = template;
            continue;
        }
        let first = visual.model_template as usize;
        let parts = if let Some(&end) = group_ends.get(&first) {
            templates[first..end].to_vec()
        } else if visual.variant & crate::BLOCK_VISUAL_VARIANT_PORTAL_UNKNOWN != 0 {
            templates
                .get(first..first + 2)
                .ok_or_else(|| invalid("portal template pair is invalid"))?
                .to_vec()
        } else {
            crate::model_template_parts(templates, visual.model_template)
                .ok_or_else(|| invalid("base face override model is invalid"))?
                .to_vec()
        };
        if templates.len() + parts.len() > crate::MAX_MODEL_TEMPLATES {
            return Err(invalid("rebound models exceed the template limit"));
        }
        let original = visual.model_template;
        visual.model_template = templates.len() as u32;
        copied_models.insert(cache_key, visual.model_template);
        for (index, part) in parts.iter().enumerate() {
            let end = part.quad_start as usize + part.quad_count as usize;
            let mut copied = quads
                .get(part.quad_start as usize..end)
                .ok_or_else(|| invalid("base face override quad range is invalid"))?
                .to_vec();
            if quads.len() + copied.len() > crate::MAX_MODEL_QUADS {
                return Err(invalid("rebound models exceed the quad limit"));
            }
            for quad in &mut copied {
                let face = BlockFace::ALL
                    .into_iter()
                    .find(|face| {
                        face.model_quad_face_id() == quad.flags & crate::MODEL_QUAD_FLAG_FACE_MASK
                    })
                    .filter(|face| old_faces[*face as usize] == quad.material)
                    .or_else(|| {
                        BlockFace::ALL
                            .into_iter()
                            .find(|face| old_faces[*face as usize] == quad.material)
                    });
                if let Some(material) = face.and_then(|face| replacement[face as usize]) {
                    quad.material = material_base + material;
                }
            }
            if let Some((_, component)) = offsets
                .iter()
                .find(|(id, _)| *id == original + index as u32)
                .copied()
            {
                offsets.push((templates.len() as u32, component));
            }
            templates.push(ModelTemplate {
                quad_start: quads.len() as u32,
                ..*part
            });
            quads.extend(copied);
        }
    }
    offsets.sort_unstable_by_key(|entry| entry.0);
    Ok(())
}

/// Names rejected bindings without exposing unchecked runtime references.
fn invalid(detail: &str) -> AssetError {
    AssetError::InvalidCompiledAssets {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_model_rebinding_keeps_other_visuals_and_geometry() {
        let mut visual = crate::runtime::diagnostic_visual();
        visual.kind = crate::VisualKind::Model;
        visual.model_template = 0;
        visual.faces = [1; 6];
        let mut visuals = vec![visual; 2];
        let quad = ModelQuad {
            positions: [[0, 0, 0]; 4],
            uvs: [[0, 0]; 4],
            material: 1,
            flags: BlockFace::Up.model_quad_face_id(),
        };
        let mut quads = vec![quad];
        let mut templates = vec![ModelTemplate {
            quad_start: 0,
            quad_count: 1,
            flags: 0,
        }];
        let mut offsets = Vec::new();
        let mut faces = [None; 6];
        faces[BlockFace::Up as usize] = Some(0);
        apply(
            &[(0, faces)],
            Tables {
                visuals: &mut visuals,
                materials: &mut vec![Material::unvaried(); 3],
                templates: &mut templates,
                quads: &mut quads,
                offsets: &mut offsets,
            },
            2..3,
        )
        .unwrap();
        assert_eq!(visuals[1].model_template, 0);
        assert_eq!(visuals[1].faces, [1; 6]);
        assert_eq!(quads[0], quad);
        let copied = quads[templates[visuals[0].model_template as usize].quad_start as usize];
        assert_eq!(copied.material, 2);
        assert_eq!(copied.positions, quad.positions);
        assert_eq!(copied.uvs, quad.uvs);
        assert_eq!(copied.flags, quad.flags);
    }
}
