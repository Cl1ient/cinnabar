//! Door carrier pose groups and their selector offsets.

use super::*;

/// Makes a complete door group whose selector offsets can be tampered independently.
fn door_group_assets() -> CompiledAssets {
    let mut compiled = transparent_cube_assets();
    compiled.materials[1].flags = assets::MATERIAL_FLAG_ALPHA_CUTOUT;
    let quads = compiled.model_quads.to_vec();
    compiled.model_templates = (0..assets::door::TEMPLATE_COUNT)
        .map(|offset| assets::ModelTemplate {
            quad_start: offset * quads.len() as u32,
            quad_count: quads.len() as u32,
            flags: assets::MODEL_TEMPLATE_FLAG_DOOR,
        })
        .collect();
    compiled.model_quads = (0..assets::door::TEMPLATE_COUNT)
        .flat_map(|_| quads.iter().copied())
        .collect();
    compiled
}

#[test]
fn door_carrier_checks_selector_offsets_and_complete_pose_groups() {
    for offset in 0..assets::door::TEMPLATE_COUNT {
        let mut compiled = door_group_assets();
        compiled.visuals[0].model_template = offset;
        compiled.visuals[0].variant = offset << 2;
        let bytes = encode_blob(&compiled).expect("encode every door pose");
        assets::RuntimeAssets::decode(&bytes).expect("decode every door pose");
    }
    for (template, variant) in [
        (0, 1),
        (0, assets::door::TEMPLATE_COUNT << 2),
        (0, 4),
        (1, 0),
    ] {
        let mut compiled = door_group_assets();
        compiled.visuals[0].model_template = template;
        compiled.visuals[0].variant = variant;
        assert!(
            encode_blob(&compiled).is_err(),
            "accepted mismatched selector offset"
        );
        let mut forged = encode_blob(&door_group_assets()).unwrap();
        let visual_start = u64::from_le_bytes(forged[192..200].try_into().unwrap()) as usize;
        forged[visual_start + 32..visual_start + 36].copy_from_slice(&template.to_le_bytes());
        forged[visual_start + 40..visual_start + 44].copy_from_slice(&variant.to_le_bytes());
        let end = forged.len() - 32;
        let digest = Sha256::digest(&forged[..end]);
        forged[end..].copy_from_slice(&digest);
        assert!(
            assets::RuntimeAssets::decode(&forged).is_err(),
            "admitted malformed serialized selectors"
        );
    }
    let mut truncated = door_group_assets();
    truncated.model_templates =
        truncated.model_templates[..assets::door::TEMPLATE_COUNT as usize - 1].into();
    truncated.model_quads = truncated.model_quads[..truncated.model_quads.len() - 6].into();
    assert!(
        encode_blob(&truncated).is_err(),
        "admitted a truncated door pose group"
    );
}
