//! Carried vanilla block bindings published through the existing session icon consumers.

use super::{BlockIcons, MAX_SESSION_ICONS};
use crate::runtime::network::block_overlay::CompiledBlockOverlay;
use assets::{ItemVisualDefinitionRoute, RuntimeAssets, RuntimeEntityAssets, VisualKind};
use client_ui::ui_runtime::presentation::SessionIcon;
use std::sync::{Arc, OnceLock};

struct Route {
    identifier: Arc<str>,
    metadata: u32,
    visual: u32,
}
static ROUTES: OnceLock<Vec<Route>> = OnceLock::new();

/// Retains item-to-block identities from the same immutable item carrier as sprite routes.
pub(super) fn install(entities: &RuntimeEntityAssets) {
    let _ = ROUTES.set(
        entities
            .item_visuals()
            .iter()
            .filter_map(|item| {
                let ItemVisualDefinitionRoute::BlockItem { block_visual } = item.route else {
                    return None;
                };
                Some(Route {
                    identifier: Arc::from(item.key.identifier.as_ref()),
                    metadata: item.key.metadata,
                    visual: block_visual.0,
                })
            })
            .collect(),
    );
}

/// Adds changed carried thumbnails and cube sheets while retaining their metadata indices.
pub(in crate::runtime::network) fn append(
    base: &RuntimeAssets,
    compiled: &CompiledBlockOverlay,
    icons: &mut BlockIcons,
) {
    let Some(routes) = ROUTES.get() else {
        return;
    };
    append_routes(base, compiled, icons, routes);
}

/// Prepares the supplied carrier routes without changing the installed process-wide inventory.
fn append_routes(
    base: &RuntimeAssets,
    compiled: &CompiledBlockOverlay,
    icons: &mut BlockIcons,
    routes: &[Route],
) {
    for route in routes {
        if icons.icons.len() >= MAX_SESSION_ICONS {
            break;
        }
        let Ok(index) = compiled
            .vanilla_carried
            .binary_search_by_key(&route.visual, |entry| entry.visual)
        else {
            continue;
        };
        let carried = &compiled.vanilla_carried[index];
        let faces = carried.faces;
        let block = base.resolve(assets::NetworkIdMode::Sequential, route.visual);
        let cube = block.kind() == VisualKind::Cube
            || assets::gui_item::full_cube_model_quads(base, assets::BlockVisualId(route.visual))
                .is_some();
        let mut model = None;
        let sprite = if cube {
            pack_compiler::overlay_cube_icon(&compiled.overlay, faces)
        } else if carried.flat {
            let id = faces[assets::BlockFace::Down as usize];
            let (pixels, side, _) =
                assets::gui_item::material_tile(&compiled.overlay.materials, id, |_| {
                    compiled.overlay.texture.as_ref()
                })
                .ok()
                .unwrap_or((&[], 0, false));
            (side != 0).then(|| assets::IconSprite {
                width: side as u16,
                height: side as u16,
                rgba8: Arc::from(pixels),
            })
        } else {
            model = assets::gui_item::carried_block_quads(
                base,
                assets::BlockVisualId(route.visual),
                faces,
            )
            .ok();
            model
                .as_deref()
                .and_then(|quads| pack_compiler::overlay_model_icon(&compiled.overlay, quads))
        };
        if let Some(sprite) = sprite {
            icons.icons.push(session_icon(route, sprite));
        }
        if (cube || model.is_some())
            && let Some(sheet) =
                render_model::equipment::blocks::overlay_face_sheet(&compiled.overlay, faces)
        {
            if let Some(mut quads) = model {
                let rects = render_model::equipment::blocks::face_rects([0.0, 0.0, 1.0, 1.0]);
                for quad in &mut quads {
                    let face = assets::gui_item::quad_face(quad.corners);
                    let rect = rects[face as usize];
                    quad.uvs = quad.uvs.map(|uv| {
                        std::array::from_fn(|axis| {
                            rect[axis] + uv[axis] * (rect[axis + 2] - rect[axis])
                        })
                    });
                }
                icons.block_models.insert(
                    (Arc::clone(&route.identifier), route.metadata),
                    quads.into(),
                );
            }
            icons.block_sheets.push(session_icon(route, sheet));
            let flags = faces.iter().fold(0, |flags, &id| {
                flags | compiled.overlay.materials[id as usize].flags
            });
            icons
                .block_material_flags
                .insert(Arc::clone(&route.identifier), flags);
        }
    }
}

/// Shares the route's identifier and metadata across the thumbnail and carried sheet.
fn session_icon(route: &Route, sprite: assets::IconSprite) -> SessionIcon {
    SessionIcon {
        identifier: Arc::clone(&route.identifier),
        metadata: route.metadata,
        width: u32::from(sprite.width),
        height: u32::from(sprite.height),
        rgba8: sprite.rgba8.to_vec().into_boxed_slice(),
    }
}

#[cfg(test)]
mod tests;
