//! The same equipped actor follows texture-only pack publication without loose base files.

use client_presentation::presentation::equipment::{
    ActorEquipmentInput, EquipmentRuntime, HeldKind, WornItem,
};
use render_model::{ActorArtworkPages, ActorRigSubmission};
use std::sync::Arc;

const ITEM: &str = "fixture:chestplate";
const TEXTURE: &str = "textures/models/fixture_armor.png";

/// Encodes original, solid-colour armour artwork for a synthetic pack.
fn png(color: [u8; 4]) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 2, image::Rgba(color))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

/// Compiles the base equipment and its retained definitions from tiny synthetic documents.
fn base() -> (assets::VanillaEntityRefs, assets::SessionEntityPack) {
    let files: Vec<(Box<str>, Vec<u8>)> = vec![
        (
            "attachables/fixture.json".into(),
            serde_json::json!({
                "format_version": "1.10.0", "minecraft:attachable": {"description": {
                    "identifier": ITEM, "materials": {"default": "armor_leather"},
                    "textures": {"default": "textures/models/fixture_armor"},
                    "geometry": {"default": "geometry.fixture.chestplate"},
                    "render_controllers": ["controller.render.fixture"]
                }}
            })
            .to_string()
            .into_bytes(),
        ),
        (
            "models/entity/fixture.json".into(),
            serde_json::json!({
                "format_version": "1.12.0", "minecraft:geometry": [{
                    "description": {"identifier": "geometry.fixture.chestplate",
                        "texture_width": 4, "texture_height": 2},
                    "bones": [{"name": "body", "pivot": [0, 0, 0],
                        "cubes": [{"origin": [0, 0, 0], "size": [1, 1, 1], "uv": [0, 0]}]}]
                }]
            })
            .to_string()
            .into_bytes(),
        ),
        (
            "render_controllers/fixture.json".into(),
            serde_json::json!({
                "format_version": "1.8.0", "render_controllers": {"controller.render.fixture": {
                    "geometry": "Geometry.default", "materials": [{"*": "Material.default"}],
                    "textures": ["Texture.default"]
                }}
            })
            .to_string()
            .into_bytes(),
        ),
        (TEXTURE.into(), png([10, 20, 30, 255])),
    ];
    let root = tempfile::tempdir().unwrap();
    for (path, bytes) in &files {
        let path = root.path().join(path.as_ref());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let refs = pack_compiler::compile_vanilla_entity_refs(root.path()).unwrap();
    let pack = compiled_pack(files).unwrap();
    (refs, pack)
}

/// Compiles an optional session layer through the same entity and equipment compiler.
fn compiled_pack(files: Vec<(Box<str>, Vec<u8>)>) -> Option<assets::SessionEntityPack> {
    let compiled = pack_compiler::compile_actor_pack(files).unwrap()?;
    Some(assets::SessionEntityPack {
        assets: Arc::new(assets::RuntimeEntityAssets::from_compiled(compiled.entities).unwrap()),
        textures: compiled.textures.into(),
        bindings: compiled.bindings.into(),
        equipment: Some(Arc::new(
            assets::RuntimeEquipmentCatalog::from_parts(
                compiled.identity,
                compiled.equipment_bindings,
                compiled.equipment_textures,
            )
            .unwrap(),
        )),
    })
}

/// Keeps actor identity, pose and equipment unchanged across each pack publication.
fn body(runtime: &mut EquipmentRuntime) -> ActorRigSubmission {
    let rig = render_model::skin_rig_id(0);
    runtime.register_skin_rig(rig, vec!["body".into()]);
    let pose: Arc<[render_model::RenderBoneTransform]> =
        Arc::from([render_model::RenderBoneTransform {
            rotation: [0.0, 0.0, 0.0, 1.0],
            translation_scale: [0.0, 0.0, 0.0, 1.0],
            axis_scale: render_model::UNIT_AXIS_SCALE,
        }]);
    ActorRigSubmission {
        material: Default::default(),
        culling_bounds: Default::default(),
        input: render_model::ActorRigRenderInput {
            identity: render_model::ActorRenderIdentity {
                session_id: 1,
                dimension: 0,
                runtime_id: 2,
                spawn_revision: 1,
                ingress_sequence: 1,
                source_tick: None,
                movement_revision: 0,
                pose_generation: render_api::PoseGeneration::new(1),
                layer: render_model::ACTOR_LAYER_BODY,
            },
            rig,
            previous_bones: Arc::clone(&pose),
            current_bones: pose,
            completed_tick: 0,
            reset_generation: 0,
        },
        world_from_actor: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
        texture_layer: 0,
        route: render_model::ActorRigRoute::Compiled,
        tint: 0,
        overlay_rgba8: 0,
        uv_anim: render_model::IDENTITY_UV_ANIM,
        light: 0,
    }
}

/// Reads the selected armour texel without assuming an atlas page or layer number.
fn worn_pixel(
    runtime: &mut EquipmentRuntime,
    body: &ActorRigSubmission,
    equipped: &ActorEquipmentInput,
    pages: &ActorArtworkPages,
) -> [u8; 4] {
    let layers = runtime.layers_for(body, equipped, None);
    let [layer] = layers else {
        panic!("one worn armour layer");
    };
    assert_eq!(
        layer.submission.input.identity.runtime_id,
        body.input.identity.runtime_id
    );
    let page = &pages.pages()[usize::from(layer.location.page() - 1)];
    let (width, height) = page.dimensions();
    let start = layer.location.layer() as usize * usize::from(width) * usize::from(height) * 4;
    page.pixels()[start..start + 4].try_into().unwrap()
}

#[test]
fn hot_reload_replaces_armour_on_an_already_equipped_actor_and_restores_base() {
    let (refs, base_pack) = base();
    let icons = Arc::new(
        assets::RuntimeIconCatalog::decode(
            &assets::encode_icon_catalog([0; 32], &[], &[]).unwrap(),
        )
        .unwrap(),
    );
    let (mut runtime, base_pages, _) = EquipmentRuntime::build(
        base_pack.assets,
        base_pack.equipment,
        icons,
        None,
        None,
        ActorArtworkPages::default(),
    );
    let body = body(&mut runtime);
    let equipped = ActorEquipmentInput {
        armor: [
            None,
            Some(WornItem {
                grip: assets::item::grip::builtin_item_grip(ITEM),
                use_facts: Default::default(),
                identifier: ITEM.into(),
                metadata: 0,
                damage: None,
                kind: HeldKind::Other,
                dye_rgb: Some(0x234567),
                enchanted: false,
            }),
            None,
            None,
        ],
        ..Default::default()
    };
    assert_eq!(
        worn_pixel(&mut runtime, &body, &equipped, &base_pages),
        [10, 20, 30, 255]
    );
    for color in [[40, 50, 60, 255], [70, 80, 90, 255]] {
        let pixels = png(color);
        let view =
            resource_pack::LayeredPackView::new(super::super::pack_reload_tests::stack(&[(
                TEXTURE, &pixels,
            )]));
        let files = pack_runtime::entity_pack::collect::collect_files(&view, Some(&refs), None);
        let pack = compiled_pack(files);
        let (pages, locations) = match &pack {
            Some(pack) => {
                base_pages
                    .clone()
                    .with_equipment_rasters(&EquipmentRuntime::pack_rasters(
                        pack.equipment.as_ref().unwrap(),
                    ))
            }
            None => (base_pages.clone(), Vec::new()),
        };
        runtime.set_pack_layer(pack.map(|pack| (pack.assets, pack.equipment, locations)));
        assert_ne!(pages.identity(), base_pages.identity());
        assert_eq!(worn_pixel(&mut runtime, &body, &equipped, &pages), color);
    }
    runtime.set_pack_layer(None);
    assert_eq!(
        worn_pixel(&mut runtime, &body, &equipped, &base_pages),
        [10, 20, 30, 255]
    );
}
