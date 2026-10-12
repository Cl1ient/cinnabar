//! Terrain texture keys and flipbooks resolved across the stack.

use std::collections::{HashMap, HashSet};

use assets::flipbook::FlipbookSource;
use resource_pack::LayeredPackView;

use pack_runtime::pack_textures::DecodedTexture;
mod diagnostics;
mod terrain;
mod tint;
use diagnostics::{FailureReason, TextureDiagnostics};
use pack_runtime::pack_textures::{decode_pack_texture, texture_key_paths};
pub(super) use terrain::{admit_static_rectangle, source_mip_chain};

/// Texture keys resolved across the stack; a higher pack replaces a key.
pub(super) struct TextureCatalog<'a> {
    view: &'a LayeredPackView,
    terrain: HashMap<String, String>,
    grids: HashMap<String, u8>,
    tints: HashMap<String, [u8; 3]>,
    server_keys: HashSet<String>,
    flipbooks: HashMap<String, std::collections::BTreeMap<(u32, u32), FlipbookSource>>,
    diagnostics: TextureDiagnostics,
}

impl<'a> TextureCatalog<'a> {
    pub(super) fn new(view: &'a LayeredPackView, base: Option<&assets::MaterialKeys>) -> Self {
        let mut terrain = pack_runtime::pack_textures::base_terrain_catalog();
        if let Some(base) = base {
            terrain.extend(
                base.aliases()
                    .map(|(key, path)| (key.to_owned(), path.to_owned())),
            );
        }
        let server_terrain = texture_key_paths(view, "textures/terrain_texture.json");
        let server_keys = server_terrain.keys().cloned().collect();
        terrain.extend(server_terrain);
        let diagnostics = TextureDiagnostics::default();
        diagnostics.catalogs(view);
        let mut flipbooks = HashMap::<String, std::collections::BTreeMap<_, _>>::new();
        for source in pack_runtime::pack_flipbooks::flipbooks(
            view,
            base.map_or(&[], assets::MaterialKeys::flipbooks),
        ) {
            flipbooks
                .entry(source.atlas_tile.to_string())
                .or_default()
                .insert((source.atlas_index, source.atlas_tile_variant), source);
        }
        Self {
            view,
            terrain,
            grids: pack_runtime::pack_textures::terrain_texture_grids(view),
            tints: tint::catalog_tints(view, base),
            server_keys,
            flipbooks,
            diagnostics,
        }
    }

    pub(super) fn terrain_keys(&self) -> impl Iterator<Item = &str> {
        self.terrain.keys().map(String::as_str)
    }

    /// Resolves an exact destination, then an animation covering all its UV variants.
    pub(super) fn flipbook(&self, key: &str, selector: (u32, u32)) -> Option<&FlipbookSource> {
        let entries = self.flipbooks.get(key)?;
        entries.get(&selector).or_else(|| {
            entries
                .get(&(selector.0, 0))
                .filter(|source| source.all_variants)
        })
    }

    /// Returns the winning terrain entry's normalized UV-grid exponent.
    pub(super) fn grid(&self, key: &str) -> u8 {
        self.grids.get(key).copied().unwrap_or(0)
    }

    /// Decodes the image a terrain key names.
    pub(super) fn decode(&self, key: &str) -> Option<DecodedTexture> {
        let Some(path) = self.terrain.get(key) else {
            self.diagnostics
                .failure(FailureReason::TerrainKeyMissing, key, None);
            return None;
        };
        self.decode_path(key, path)
    }

    /// Decodes an independent animation source with the destination tile's atlas tint.
    pub(super) fn decode_path(&self, key: &str, path: &str) -> Option<DecodedTexture> {
        let mut texture = decode_pack_texture(self.view, path);
        if let Some(tint) = self.tints.get(key)
            && let Some(texture) = texture.as_mut()
        {
            pack_compiler::apply_atlas_tint(&mut texture.rgba8, *tint);
        }
        if texture.is_none() {
            let reason = diagnostics::failure_reason(self.view, path);
            // Base aliases have no server raster until a pack overrides them.
            if self.server_keys.contains(key) || reason != FailureReason::TextureFileMissing {
                self.diagnostics.failure(reason, key, Some(path));
            }
        }
        texture
    }

    /// Records a route the overlay could not admit because its texture array is full.
    pub(super) fn report_full(&self, key: &str) {
        self.diagnostics
            .failure(FailureReason::TextureArrayFull, key, None);
    }

    /// Emits the counted terrain failures for this stack.
    pub(super) fn emit_diagnostics(&self) {
        self.diagnostics.emit();
    }
}

/// Cuts bounded square frames in timeline order, crops replication from the upper-left,
/// and shrinks each retained frame before it enters the overlay source budget.
pub(super) fn flipbook_frames(
    texture: &DecodedTexture,
    flipbook: &FlipbookSource,
    max_frames: usize,
    max_side: u32,
) -> Vec<DecodedTexture> {
    let side = texture.width;
    if max_frames == 0 || side == 0 || texture.height == 0 {
        return Vec::new();
    }
    if texture.height < side || !texture.height.is_multiple_of(side) {
        return Vec::new();
    }
    let count = texture.height / side;
    let crop = (side / flipbook.replicate.max(1)).max(1);
    let output = crop.min(max_side).max(1);
    let len = if flipbook.frames.is_empty() {
        count as usize
    } else {
        flipbook.frames.len()
    };
    (0..len.min(max_frames))
        .map(|index| {
            let frame = if flipbook.frames.is_empty() {
                index as u32
            } else {
                flipbook.frames[index]
            };
            let y_start = frame.min(count - 1) * side;
            let mut rgba8 = Vec::with_capacity((output * output * 4) as usize);
            for y in 0..output {
                let source_y = y_start + y * crop / output;
                for x in 0..output {
                    let source_x = x * crop / output;
                    let start = ((source_y * texture.width + source_x) * 4) as usize;
                    rgba8.extend_from_slice(&texture.rgba8[start..start + 4]);
                }
            }
            DecodedTexture {
                width: output,
                height: output,
                rgba8: rgba8.into_boxed_slice(),
            }
        })
        .collect()
}

/// Downscales aspect-preserving so the longest side is at most `max_side`;
/// smaller images are returned unchanged. Nearest-neighbour keeps pixel art crisp.
pub(super) fn shrink_to_max(texture: &DecodedTexture, max_side: u32) -> DecodedTexture {
    let longest = texture.width.max(texture.height);
    if longest <= max_side {
        return texture.clone();
    }
    let scale = |side: u32| (side * max_side / longest).max(1);
    let (width, height) = (scale(texture.width), scale(texture.height));
    let mut rgba8 = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let source_y = (u64::from(y) * u64::from(texture.height) / u64::from(height)) as usize;
        for x in 0..width {
            let source_x = (u64::from(x) * u64::from(texture.width) / u64::from(width)) as usize;
            let offset = (source_y * texture.width as usize + source_x) * 4;
            rgba8.extend_from_slice(&texture.rgba8[offset..offset + 4]);
        }
    }
    DecodedTexture {
        width,
        height,
        rgba8: rgba8.into_boxed_slice(),
    }
}

/// Resamples to a square power-of-two tile: exact halvings in linear light when
/// possible, nearest-neighbour otherwise so pixel art stays crisp.
pub(super) fn resample_square(texture: &DecodedTexture, tile: u32) -> Box<[u8]> {
    if texture.width == texture.height && texture.width.is_power_of_two() && texture.width >= tile {
        let mut pixels = texture.rgba8.clone();
        let mut size = texture.width;
        while size > tile {
            pixels = assets::downsample_linear_premultiplied(&pixels, size);
            size /= 2;
        }
        return pixels;
    }
    let mut pixels = Vec::with_capacity((tile * tile * 4) as usize);
    for y in 0..tile {
        let source_y = (u64::from(y) * u64::from(texture.height) / u64::from(tile)) as usize;
        for x in 0..tile {
            let source_x = (u64::from(x) * u64::from(texture.width) / u64::from(tile)) as usize;
            let offset = (source_y * texture.width as usize + source_x) * 4;
            pixels.extend_from_slice(&texture.rgba8[offset..offset + 4]);
        }
    }
    pixels.into_boxed_slice()
}
