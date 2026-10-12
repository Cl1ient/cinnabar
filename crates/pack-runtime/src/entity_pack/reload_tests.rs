//! Texture-only reloads resolve retained equipment identifiers through the accepted stack.

use super::*;
use serde_json::json;

const ITEM: &str = "fixture:chestplate";
const TEXTURE: &str = "textures/models/fixture_armor";
const ATTACHABLE: &str = "attachables/fixture.player.json";

/// Base definitions that can be used without an installed loose resource pack.
fn base_refs() -> Arc<assets::VanillaEntityRefs> {
    let mut refs = assets::VanillaEntityRefs::new();
    refs.attachables.insert(
        ATTACHABLE.into(),
        json!({"format_version": "1.10.0", "minecraft:attachable": {"description": {
            "identifier": "fixture:chestplate.player",
            "item": {(ITEM): "1"},
            "materials": {"default": "armor_leather"},
            "textures": {"default": TEXTURE},
            "geometry": {"default": "geometry.fixture.chestplate"},
            "render_controllers": ["controller.render.fixture"]
        }}}),
    );
    refs.render_controllers.insert(
        "controller.render.fixture".into(),
        json!({"geometry": "Geometry.default", "materials": [{"*": "Material.default"}],
            "textures": ["Texture.default"]}),
    );
    refs.geometry_index
        .insert("geometry.fixture.chestplate".into(), 0);
    refs.geometry_files.push(assets::VanillaGeometryFile {
        path: "models/entity/fixture.json".into(),
        text: json!({"format_version": "1.12.0", "minecraft:geometry": [{
            "description": {"identifier": "geometry.fixture.chestplate",
                "texture_width": 4, "texture_height": 2},
            "bones": [{"name": "body", "pivot": [0, 0, 0],
                "cubes": [{"origin": [0, 0, 0], "size": [1, 1, 1], "uv": [0, 0]}]}]
        }]})
        .to_string(),
    });
    Arc::new(refs)
}

/// A stack with the same pack and texture names but independently authored pixels.
fn texture_stack(color: [u8; 4]) -> Arc<resource_pack::ValidatedPackStack> {
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 2, image::Rgba(color))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    crate::test_support::stack(&[(&format!("{TEXTURE}.png"), png.get_ref())])
}

/// Compiles on the same cache used by session preparation, retaining its dependency reads.
fn resolve(
    cache: &EntityCache,
    stack: &Arc<resource_pack::ValidatedPackStack>,
    refs: &Arc<assets::VanillaEntityRefs>,
) -> (
    Option<Arc<SessionEntityPack>>,
    crate::reload_diff::Dependencies,
) {
    let mut dependencies = Default::default();
    let pack = crate::reload_diff::compile(
        crate::reload_diff::Subscriber::Entities,
        stack,
        &mut dependencies,
        |view| {
            cache
                .get_or_compile(
                    EntityInputs {
                        stack: crate::stack_fingerprint(stack),
                        vanilla: Some(refs.clone()),
                        vanilla_pack_dir: None,
                    },
                    view,
                    None,
                    &|| false,
                    super::super::compile_encoded,
                )
                .expect("the compile was not cancelled")
        },
    );
    (pack, dependencies)
}

#[test]
fn a_texture_only_reload_changes_artwork_for_the_same_equipped_item() {
    let (refs, cache) = (base_refs(), EntityCache::default());
    let equipped = ITEM;
    let first_stack = texture_stack([21, 43, 65, 255]);
    let (first, reads) = resolve(&cache, &first_stack, &refs);
    let first = first.expect("texture-only armour inherits the retained attachable");
    let first_catalog = first.equipment.as_ref().unwrap();
    let first_binding = first_catalog.binding(equipped).unwrap();
    let first_pixels = first_catalog
        .texture(&first_binding.texture.identifier)
        .unwrap();
    assert_eq!(&first_pixels.rgba8[..4], &[21, 43, 65, 255]);
    let (unchanged, _) = resolve(&cache, &first_stack, &refs);
    assert!(Arc::ptr_eq(&first, &unchanged.unwrap()));

    let next_stack = texture_stack([87, 109, 131, 255]);
    let changes = crate::reload_diff::Changes::between(
        &next_stack,
        Some((
            &resource_pack::PackAdmission::Validated(first_stack),
            &reads,
        )),
    );
    assert!(changes.entities, "changed pixels must refresh equipment");
    let (next, _) = resolve(&cache, &next_stack, &refs);
    let next = next.unwrap();
    assert!(!Arc::ptr_eq(&first, &next));
    let next_catalog = next.equipment.as_ref().unwrap();
    let next_binding = next_catalog.binding(equipped).unwrap();
    assert_eq!(next_binding.geometry, first_binding.geometry);
    assert_eq!(next_binding.material, first_binding.material);
    assert_ne!(
        next_catalog.source_manifest_sha256(),
        first_catalog.source_manifest_sha256()
    );
    assert_eq!(
        &next_catalog
            .texture(&next_binding.texture.identifier)
            .unwrap()
            .rgba8[..4],
        &[87, 109, 131, 255]
    );
    assert_eq!(&first_pixels.rgba8[..4], &[21, 43, 65, 255]);
    let empty = crate::test_support::stack(&[]);
    let removed = crate::reload_diff::Changes::between(
        &empty,
        Some((&resource_pack::PackAdmission::Validated(next_stack), &reads)),
    );
    assert!(removed.entities);
    assert!(resolve(&cache, &empty, &refs).0.is_none());
}

#[test]
fn authored_attachables_keep_precedence_over_retained_base_documents() {
    let refs = base_refs();
    let authored = json!({"minecraft:attachable": {"description": {
        "identifier": ITEM,
        "textures": {"default": "textures/fixture/authored"}
    }}})
    .to_string();
    let stack = crate::test_support::stack(&[
        ("attachables/authored.json", authored.as_bytes()),
        (&format!("{TEXTURE}.png"), b"fixture pixels"),
    ]);
    let files = super::super::collect::collect_files(
        &resource_pack::LayeredPackView::new(stack),
        Some(&refs),
        None,
    );
    assert!(
        files
            .iter()
            .any(|(path, _)| path.as_ref() == "attachables/authored.json")
    );
    assert!(!files.iter().any(|(path, _)| path.as_ref() == ATTACHABLE));
}
