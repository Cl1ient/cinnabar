use super::{DroppedItemBlock, DroppedItemModel, DroppedItemSprite, block};
use glam::Vec3;
use std::collections::BTreeMap;

impl DroppedItemModel {
    /// Whether this model has a closed, opaque surface for alpha-tested drawing.
    /// Open templates, two-sided planes, partial alpha and padded UVs retain two-sided rendering.
    #[must_use]
    pub fn can_cull_back_faces(&self) -> bool {
        match self {
            Self::Vacant => false,
            Self::NativeSprite(sprite) => binary_alpha(sprite),
            Self::Sprite(sprite) => {
                // The legacy rear artwork is mirrored. Its mask must enclose the same solid.
                binary_alpha(sprite)
                    && (0..sprite.height as usize).all(|row| {
                        (0..sprite.width as usize).all(|column| {
                            let offset = row * sprite.width as usize;
                            sprite.rgba8[(offset + column) * 4 + 3]
                                == sprite.rgba8
                                    [(offset + sprite.width as usize - 1 - column) * 4 + 3]
                        })
                    })
            }
            Self::Cube(cube) => {
                cube.tints.iter().all(|color| color >> 24 == 255)
                    && cube.faces.iter().all(|pixels| opaque(pixels))
            }
            Self::Block(model) => {
                model
                    .materials
                    .iter()
                    .all(|(sprite, tint)| tint >> 24 == 255 && opaque(&sprite.rgba8))
                    && closed_block(model)
            }
        }
    }
}

/// Checks a sprite's dimensions and rejects alpha that can expose a rear surface through coverage.
fn binary_alpha(sprite: &DroppedItemSprite) -> bool {
    sprite.width != 0
        && sprite.height != 0
        && Some(sprite.rgba8.len())
            == (sprite.width as usize)
                .checked_mul(sprite.height as usize)
                .and_then(|n| n.checked_mul(4))
        && sprite
            .rgba8
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| matches!(pixel[3], 0 | 255))
}

/// Requires opaque pixels so cutout or partial coverage cannot reveal removed faces.
fn opaque(pixels: &[u8]) -> bool {
    !pixels.is_empty()
        && pixels.len().is_multiple_of(4)
        && pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255)
}

/// Keeps each triangle's UV interior within the opaque tile, away from atlas padding.
/// Two corners below the upper edge on each axis prevent any emitted triangle from
/// sampling a constant coordinate at that edge, regardless of its winding.
fn block_uvs_stay_in_tile(quad: &assets::ModelQuad) -> bool {
    quad.uvs
        .iter()
        .flatten()
        .all(|&uv| uv <= block::UV_TILE_UNITS)
        && (0..2).all(|axis| {
            quad.uvs
                .iter()
                .filter(|uv| uv[axis] < block::UV_TILE_UNITS)
                .count()
                >= 2
        })
}

/// Proves closure from paired directed edges and outward planes enclosing all corners.
/// Templates with unmatched edges, degenerate faces or non-convex surfaces conservatively fall back.
fn closed_block(model: &DroppedItemBlock) -> bool {
    if model.quads.is_empty() {
        return false;
    }
    let mut edges = BTreeMap::new();
    for quad in model.quads.iter() {
        if quad.flags & assets::MODEL_QUAD_FLAG_TWO_SIDED != 0 || !block_uvs_stay_in_tile(quad) {
            return false;
        }
        let Some(normal) = block::quad_normal(quad) else {
            return false;
        };
        let normal = Vec3::from_array(normal);
        let order = block::quad_order(quad, normal.to_array());
        let positions = order.map(|i| quad.positions[i]);
        let [a, b, c, d] = positions.map(|p| Vec3::from_array(p.map(f32::from)));
        if (b - a).cross(c - a).dot(normal) <= 0.0
            || (c - a).cross(d - a).dot(normal) <= 0.0
            || [b, c, d].iter().any(|p| (*p - a).dot(normal) != 0.0)
            || model
                .quads
                .iter()
                .flat_map(|q| q.positions)
                .any(|p| (Vec3::from_array(p.map(f32::from)) - a).dot(normal) > 0.0)
        {
            return false;
        }
        for i in 0..4 {
            let (a, b) = (positions[i], positions[(i + 1) % 4]);
            if a == b {
                return false;
            }
            *edges.entry((a, b)).or_insert(0usize) += 1;
        }
    }
    edges
        .iter()
        .all(|(&(a, b), &count)| count == 1 && edges.get(&(b, a)) == Some(&1))
}

#[cfg(test)]
mod tests;
