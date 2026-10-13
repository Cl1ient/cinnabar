//! Terrain texture keys and flipbooks resolved across the stack.

use std::collections::HashMap;

use resource_pack::LayeredPackView;
use serde_json::Value;

use pack_runtime::pack_textures::DecodedTexture;
mod carrier;
mod diagnostics;
mod terrain;
pub(super) use carrier::carrier_image;
use diagnostics::{FailureReason, TextureDiagnostics};
use pack_runtime::pack_textures::{
    MAX_CATALOG_ENTRIES, parse_pack_json, resolve_pack_texture_with_policy,
};
pub(super) use terrain::{admit_static_rectangle, source_mip_chain};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(super) struct Flipbook {
    pub(super) frames: Option<Vec<u32>>,
    pub(super) ticks_per_frame: u32,
    pub(super) blend: bool,
}

/// Pixels supplied by either a pack image or the immutable carrier.
pub(super) struct TextureImage {
    pub(super) texture: DecodedTexture,
    pub(super) image_path: String,
    pub(super) baked_tint: Option<[u8; 3]>,
    pub(super) pack: bool,
}

/// Pixel and animation policy shared by equivalent catalog and recorded-path routes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct TextureIdentity {
    path: TexturePath,
    tint: Option<[u8; 3]>,
    grid: u8,
    pub(super) flipbook: Option<Flipbook>,
}

/// Keeps logical aliases separate from resolved filenames that happen to share their spelling.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum TexturePath {
    Catalog(String),
    Image(String),
}

impl TextureIdentity {
    /// Replaces the catalog spelling with the source that actually supplied the pixels.
    pub(super) fn with_image(mut self, path: String) -> Self {
        self.path = TexturePath::Image(path);
        self
    }
}

/// Texture keys resolved across the stack; a higher pack replaces a key.
pub(super) struct TextureCatalog<'a> {
    view: &'a LayeredPackView,
    terrain: HashMap<String, pack_runtime::pack_textures::terrain::Entry>,
    base_terrain: HashMap<String, pack_runtime::pack_textures::terrain::Entry>,
    flipbooks: HashMap<String, Flipbook>,
    diagnostics: TextureDiagnostics,
}

impl<'a> TextureCatalog<'a> {
    pub(super) fn new(
        view: &'a LayeredPackView,
        base: Option<&assets::material_keys::MaterialKeys>,
    ) -> Self {
        let mut terrain = pack_runtime::pack_textures::base_terrain_catalog()
            .into_iter()
            .filter_map(|(key, path)| {
                let entry = pack_runtime::pack_textures::terrain::parse_entry(
                    &serde_json::json!({"textures":path}),
                )?;
                Some((key, entry))
            })
            .collect::<HashMap<_, _>>();
        if let Some(base) = base {
            let fixed_tints = base.fixed_tints().collect::<HashMap<_, _>>();
            for (key, path) in base.aliases() {
                if let Some(entry) = pack_runtime::pack_textures::terrain::parse_entry(
                    &serde_json::json!({"textures":path}),
                ) {
                    terrain.insert(
                        key.to_owned(),
                        entry.with_base_tint(fixed_tints.get(key).copied()),
                    );
                }
            }
            for (key, value) in base.terrain_entries() {
                if let Some(entry) = pack_runtime::pack_textures::terrain::parse_entry(value) {
                    terrain.insert(key.to_owned(), entry);
                }
            }
        }
        let server_terrain = pack_runtime::pack_textures::terrain::entries(view);
        let diagnostics = TextureDiagnostics::default();
        diagnostics.catalogs(view);
        let mut flipbooks = HashMap::new();
        for layer in view.read_layers("textures/flipbook_textures.json") {
            let Some(Value::Array(entries)) = parse_pack_json(&layer) else {
                continue;
            };
            for entry in entries.iter().take(MAX_CATALOG_ENTRIES) {
                let Some(tile) = entry["atlas_tile"].as_str() else {
                    continue;
                };
                let frames = entry["frames"].as_array().map(|frames| {
                    frames
                        .iter()
                        .filter_map(|frame| {
                            frame.as_u64().and_then(|frame| u32::try_from(frame).ok())
                        })
                        .collect()
                });
                let ticks = entry["ticks_per_frame"]
                    .as_u64()
                    .unwrap_or(1)
                    .clamp(1, 1 << 16);
                flipbooks.insert(
                    tile.to_owned(),
                    Flipbook {
                        frames,
                        ticks_per_frame: ticks as u32,
                        blend: entry["blend_frames"]
                            .as_bool()
                            .unwrap_or(pack_compiler::DEFAULT_BLEND_FRAMES),
                    },
                );
            }
        }
        Self {
            view,
            terrain: server_terrain,
            base_terrain: terrain,
            flipbooks,
            diagnostics,
        }
    }

    pub(super) fn flipbook(&self, key: &str) -> Option<&Flipbook> {
        self.flipbooks.get(key)
    }

    /// Returns the winning terrain entry's normalized UV-grid exponent.
    pub(super) fn grid(&self, key: &str) -> u8 {
        self.entry(key).map_or(0, |entry| entry.grid)
    }

    /// Whether the winning state choices or UV grid differ from the immutable base catalog.
    pub(super) fn redefined(&self, key: &str, variant: u32) -> bool {
        self.terrain.get(key).is_some_and(|entry| {
            self.base_terrain.get(key).is_none_or(|base| {
                entry.grid != base.grid || entry.variant(variant) != base.variant(variant)
            })
        })
    }

    /// Resolves a complete entry from the optional stack before falling back to the base.
    fn entry(&self, key: &str) -> Option<&pack_runtime::pack_textures::terrain::Entry> {
        self.terrain.get(key).or_else(|| self.base_terrain.get(key))
    }

    /// Choices for one state variant, independent of the other variants.
    pub(super) fn choices(
        &self,
        key: &str,
        variant: u32,
    ) -> Option<&[pack_runtime::pack_textures::terrain::Choice]> {
        Some(self.entry(key)?.variant(variant))
    }

    /// Resolves the state index used for selector caching and animation eligibility.
    pub(super) fn resolved_variant(&self, key: &str, variant: u32) -> Option<u32> {
        Some(self.entry(key)?.resolved_variant(variant))
    }

    /// Decodes a state choice through the shared raster, tint, and image-source resolution.
    #[cfg(test)]
    pub(super) fn decode_choice(
        &self,
        key: &str,
        variant: u32,
        choice: u32,
    ) -> Option<DecodedTexture> {
        self.decode_route(
            &super::TextureRoute::Catalog {
                key: key.to_owned(),
                variant,
                choice,
            },
            |_, _| None,
        )
        .map(|image| image.texture)
    }

    /// Returns the resolved state and choice indices together with the winning raster policy.
    fn resolve_choice(
        &self,
        route: &super::TextureRoute,
    ) -> Option<(u32, usize, &pack_runtime::pack_textures::terrain::Choice)> {
        use pack_runtime::pack_textures::texture_image_paths;
        let entry = self.entry(route.key())?;
        let (variant, choice) = match route {
            super::TextureRoute::Catalog {
                variant, choice, ..
            } => (entry.resolved_variant(*variant), *choice as usize),
            super::TextureRoute::Path { variant, path, .. } => {
                let paths = texture_image_paths(path);
                let variant = entry.resolved_variant(*variant);
                let choice = entry.variant(variant).iter().position(|choice| {
                    texture_image_paths(&choice.path)
                        .iter()
                        .any(|path| paths.contains(path))
                })?;
                (variant, choice)
            }
        };
        Some((variant, choice, entry.variant(variant).get(choice)?))
    }

    /// Retains every pixel or animation property that can change an admitted raster slot.
    pub(super) fn identity(&self, route: &super::TextureRoute) -> Option<TextureIdentity> {
        let Some((variant, index, choice)) = self.resolve_choice(route) else {
            self.diagnostics
                .failure(FailureReason::TerrainKeyMissing, route.key(), None);
            return None;
        };
        let animate = variant == 0 && index == 0;
        Some(TextureIdentity {
            path: TexturePath::Catalog(choice.path.clone()),
            tint: choice.tint,
            grid: self.grid(route.key()),
            flipbook: animate
                .then(|| self.flipbook(route.key()).cloned())
                .flatten(),
        })
    }

    /// Resolves one route, using normalized image filenames for both pack and carrier sources.
    pub(super) fn decode_route(
        &self,
        route: &super::TextureRoute,
        mut carrier: impl FnMut(&str, Option<[u8; 3]>) -> Option<TextureImage>,
    ) -> Option<TextureImage> {
        use pack_runtime::pack_textures::texture_image_paths;
        let key = route.key();
        let Some((_, _, choice)) = self.resolve_choice(route) else {
            self.diagnostics
                .failure(FailureReason::TerrainKeyMissing, key, None);
            return None;
        };
        let mut image = resolve_pack_texture_with_policy(
            self.view,
            &choice.path,
            assets::texture::policy::device_policy(),
            true,
        )
        .map(|resolved| TextureImage {
            texture: resolved.texture,
            image_path: resolved.image_path,
            baked_tint: None,
            pack: true,
        })
        .or_else(|| {
            texture_image_paths(&choice.path)
                .into_iter()
                .find_map(|path| carrier(&path, choice.tint))
        });
        if let Some(image) = image.as_mut()
            && image.baked_tint.is_none()
            && let Some(tint) = choice.tint
        {
            pack_compiler::apply_atlas_tint(&mut image.texture.rgba8, tint);
        }
        if image.is_none() {
            let reason = diagnostics::failure_reason(self.view, &choice.path);
            if self.terrain.contains_key(key) || reason != FailureReason::TextureFileMissing {
                self.diagnostics.failure(reason, key, Some(&choice.path));
            }
        }
        image
    }

    /// Records a route the overlay could not admit because its texture array is full.
    pub(super) fn report_full(&self, key: &str) {
        self.diagnostics
            .failure(FailureReason::TextureArrayFull, key, None);
    }

    /// Names a route whose full-resolution storage would exceed the page budget.
    pub(super) fn report_budget(&self, key: &str) {
        self.diagnostics
            .failure(FailureReason::TextureBudgetExceeded, key, None);
    }

    /// Emits the counted terrain failures for this stack.
    pub(super) fn emit_diagnostics(&self) {
        self.diagnostics.emit();
    }
}

/// Splits a vertical strip into up to `max_frames` square frames, each shrunk to
/// `max_side`; a non-strip image is its single shrunk frame. Frames are shrunk
/// as they are cut so the caller never holds full-size copies.
pub(super) fn flipbook_frames(
    texture: &DecodedTexture,
    flipbook: &Flipbook,
    max_frames: usize,
    max_side: u32,
) -> Vec<DecodedTexture> {
    let side = texture.width;
    if max_frames == 0 {
        return Vec::new();
    }
    if texture.height <= side || !texture.height.is_multiple_of(side) {
        return vec![shrink_to_max(texture, max_side)];
    }
    let count = texture.height / side;
    let order = flipbook
        .frames
        .clone()
        .filter(|frames| !frames.is_empty())
        .unwrap_or_else(|| (0..count).collect());
    let frame_bytes = (side * side * 4) as usize;
    order
        .into_iter()
        .take(max_frames)
        .map(|frame| {
            let start = (frame.min(count - 1) as usize) * frame_bytes;
            let frame = DecodedTexture {
                width: side,
                height: side,
                rgba8: texture.rgba8[start..start + frame_bytes].into(),
            };
            shrink_to_max(&frame, max_side)
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
