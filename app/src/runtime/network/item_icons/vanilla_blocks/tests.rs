use super::*;
use crate::runtime::network::block_overlay::{CarriedBlock, OverlayGaps};
use assets::{
    BlockOverlay, BlockVisual, Material, ModelQuad, ModelTemplate, NO_ANIMATION, TextureRef,
};

/// A synthetic half-height slab uses the normal overlay model and texture contracts.
fn slab() -> RuntimeAssets {
    let diagnostic = RuntimeAssets::diagnostic();
    let quads = assets::BlockFace::ALL.map(|face| {
        let (corners, uvs) = assets::gui_item::cube_face(face);
        ModelQuad {
            positions: corners.map(|point| {
                [
                    (point[0] * 256.0) as i16,
                    (point[1] * 128.0) as i16,
                    (point[2] * 256.0) as i16,
                ]
            }),
            uvs: uvs.map(|uv| uv.map(|value| (value * 4096.0) as u16)),
            material: 0,
            flags: face.model_quad_face_id(),
        }
    });
    let overlay = BlockOverlay {
        visuals: vec![BlockVisual {
            faces: [0; 6],
            flags: assets::BlockFlags::empty(),
            kind: VisualKind::Model,
            support: assets::VisualSupport::Exact,
            contributor_role: assets::ContributorRole::Primary,
            model_template: 0,
            animation: NO_ANIMATION,
            variant: 0,
            native_material: assets::native_material::NativeMaterialFacts::Unresolved,
            destroy: assets::compiled::block_destroy::BlockDestroyFacts::Unresolved,
        }],
        light_properties: vec![assets::LightProperties::OPAQUE_DARK],
        materials: vec![Material {
            texture: TextureRef::new(1, 0).unwrap(),
            ..Material::unvaried()
        }],
        model_templates: vec![ModelTemplate {
            quad_start: 0,
            quad_count: quads.len() as u32,
            flags: 0,
        }],
        model_quads: quads.into(),
        texture: Some(diagnostic.texture_array().clone()),
        ..Default::default()
    };
    diagnostic.with_block_overlay(1, &overlay).unwrap()
}

/// Only the optional carried art changes; the slab's geometry comes from the base.
fn textures(flat: bool) -> CompiledBlockOverlay {
    let mut texture = RuntimeAssets::diagnostic().texture_array().clone();
    for mip in &mut texture.mips {
        mip.rgba8 = [80, 150, 220, 255]
            .repeat((mip.size * mip.size) as usize)
            .into();
    }
    CompiledBlockOverlay {
        overlay: BlockOverlay {
            materials: vec![
                Material::unvaried(),
                Material {
                    texture: TextureRef::new(1, 0).unwrap(),
                    ..Material::unvaried()
                },
            ],
            texture: Some(texture),
            ..Default::default()
        },
        gaps: OverlayGaps::default(),
        vanilla_carried: vec![CarriedBlock {
            visual: 1,
            faces: [1; 6],
            flat,
        }],
    }
}

#[test]
fn slab_texture_replacement_retains_its_model_for_inventory_and_hands() {
    let base = slab();
    let mut output = BlockIcons::default();
    let route = Route {
        identifier: "minecraft:fixture_slab".into(),
        metadata: 7,
        visual: 1,
    };
    append_routes(&base, &textures(false), &mut output, &[route]);
    assert_eq!(output.block_sheets.len(), 1);
    assert_eq!(output.icons.len(), 1);
    let model = &output.block_models[&(Arc::from("minecraft:fixture_slab"), 7)];
    assert!(
        model
            .iter()
            .flat_map(|quad| quad.corners)
            .all(|point| (0.0..=0.5).contains(&point[1]))
    );
    assert!(
        model
            .iter()
            .flat_map(|quad| quad.corners)
            .any(|point| point[1] == 0.5)
    );
    let held = render_model::equipment::blocks::carried_model_vertices(model);
    assert!(
        held.iter()
            .all(|vertex| (-0.5..=0.0).contains(&vertex.position[1]))
    );
    assert_eq!(output.block_sheets[0].metadata, 7);
    assert_eq!(
        base.resolve(assets::NetworkIdMode::Sequential, 1)
            .model_template(),
        Some(0)
    );
}

#[test]
fn only_declared_flat_item_routes_use_the_down_face_sprite() {
    let mut output = BlockIcons::default();
    let route = Route {
        identifier: "minecraft:flat_fixture".into(),
        metadata: 0,
        visual: 1,
    };
    append_routes(&slab(), &textures(true), &mut output, &[route]);
    assert_eq!(output.icons.len(), 1);
    assert!(output.block_sheets.is_empty());
    assert!(output.block_models.is_empty());
}
