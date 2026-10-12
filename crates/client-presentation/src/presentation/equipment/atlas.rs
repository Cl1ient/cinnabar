//! Item sprites packed into shared atlas layers so equipment costs few texture pages.

use assets::IconSprite;
use render_model::EquipmentRaster;

pub(super) const ATLAS_SIDE: u16 = 512;

/// Where one sprite sits in the atlas: layer index and pixel rectangle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Placement {
    pub(super) layer: usize,
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) width: u16,
    pub(super) height: u16,
    side: u16,
}

impl Placement {
    /// The sprite's `[u0, v0, u1, v1]` region of its atlas layer.
    pub(super) fn uv_rect(self) -> [f32; 4] {
        let side = f32::from(self.side);
        [
            f32::from(self.x) / side,
            f32::from(self.y) / side,
            f32::from(self.x + self.width) / side,
            f32::from(self.y + self.height) / side,
        ]
    }
}

pub(super) struct SpriteAtlas {
    pub(super) layers: Vec<EquipmentRaster>,
    /// Indexed like the icon catalog's sprites; `None` for one that could not be placed.
    pub(super) placements: Vec<Option<Placement>>,
}

struct Layer {
    side: u16,
    rgba8: Vec<u8>,
    shelf_y: u16,
    shelf_height: u16,
    cursor_x: u16,
}

impl Layer {
    /// Allocates one admitted physical atlas layer.
    fn new(side: u16) -> Self {
        Self {
            side,
            rgba8: vec![0; usize::from(side) * usize::from(side) * 4],
            shelf_y: 0,
            shelf_height: 0,
            cursor_x: 0,
        }
    }

    /// Reserves a `width` x `height` slot on the current or next shelf, if it fits.
    fn reserve(&mut self, width: u16, height: u16) -> Option<(u16, u16)> {
        if self.cursor_x + width > self.side {
            self.shelf_y += self.shelf_height;
            self.shelf_height = 0;
            self.cursor_x = 0;
        }
        if self.shelf_y + height > self.side {
            return None;
        }
        let slot = (self.cursor_x, self.shelf_y);
        self.cursor_x += width;
        self.shelf_height = self.shelf_height.max(height);
        Some(slot)
    }
}

impl SpriteAtlas {
    /// Shelf-packs every valid sprite, tallest first.
    pub(super) fn pack(sprites: &[IconSprite]) -> Self {
        let policy = assets::texture::policy::device_policy();
        let valid = |sprite: &IconSprite| {
            sprite.width != 0
                && sprite.height != 0
                && policy.source_bytes(u32::from(sprite.width), u32::from(sprite.height), false)
                    == Some(sprite.rgba8.len())
        };
        let mut order = (0..sprites.len())
            .filter(|index| valid(&sprites[*index]))
            .collect::<Vec<_>>();
        order.sort_by_key(|index| {
            let sprite = &sprites[*index];
            (
                std::cmp::Reverse(sprite.height),
                std::cmp::Reverse(sprite.width),
                *index,
            )
        });
        let side = order
            .iter()
            .map(|&index| sprites[index].width.max(sprites[index].height))
            .max()
            .unwrap_or(ATLAS_SIDE)
            .max(ATLAS_SIDE)
            .next_power_of_two();
        let layer_bytes = policy.page_bytes(u32::from(side), u32::from(side));
        let max_layers = layer_bytes.map_or(0, |bytes| {
            assets::texture::policy::MAX_SUBSCRIBER_BYTES / bytes
        });
        let mut layers = if max_layers == 0 {
            Vec::new()
        } else {
            vec![Layer::new(side)]
        };
        let mut placements = vec![None; sprites.len()];
        for index in order {
            let sprite = &sprites[index];
            let mut slot = layers
                .last_mut()
                .and_then(|layer| layer.reserve(sprite.width, sprite.height));
            if slot.is_none() && layers.len() < max_layers {
                layers.push(Layer::new(side));
                slot = layers
                    .last_mut()
                    .and_then(|layer| layer.reserve(sprite.width, sprite.height));
            }
            let Some((x, y)) = slot else {
                continue;
            };
            let layer_index = layers.len() - 1;
            let layer = &mut layers[layer_index];
            let row_bytes = usize::from(sprite.width) * 4;
            for row in 0..usize::from(sprite.height) {
                let target = ((usize::from(y) + row) * usize::from(side) + usize::from(x)) * 4;
                layer.rgba8[target..target + row_bytes]
                    .copy_from_slice(&sprite.rgba8[row * row_bytes..(row + 1) * row_bytes]);
            }
            placements[index] = Some(Placement {
                layer: layer_index,
                x,
                y,
                width: sprite.width,
                height: sprite.height,
                side,
            });
        }
        if layers
            .last()
            .is_some_and(|layer| layer.shelf_height == 0 && layer.cursor_x == 0)
        {
            layers.pop();
        }
        Self {
            layers: layers
                .into_iter()
                .map(|layer| EquipmentRaster {
                    width: side,
                    height: side,
                    rgba8: layer.rgba8.into(),
                })
                .collect(),
            placements,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hd_held_sprites_keep_pixels_and_uv_extent() {
        let width = 1024;
        let height = 128;
        let pixels = (0..usize::from(width) * usize::from(height))
            .flat_map(|i| [if i % 2 == 0 { 255 } else { 0 }, 73, 91, 255])
            .collect::<Vec<_>>();
        let sprite = IconSprite {
            width,
            height,
            rgba8: pixels.clone().into(),
        };
        let atlas = SpriteAtlas::pack(&[sprite]);
        let placement = atlas.placements[0].unwrap();
        assert_eq!([placement.width, placement.height], [width, height]);
        assert_eq!(placement.uv_rect(), [0.0, 0.0, 1.0, 0.125]);
        let page = &atlas.layers[placement.layer];
        assert_eq!([page.width, page.height], [width; 2]);
        assert_eq!(&page.rgba8[..pixels.len()], &pixels);
    }
}
