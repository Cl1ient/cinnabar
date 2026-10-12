//! Source facts retained alongside the immutable world carrier.

use serde::{Deserialize, Serialize};

/// The terrain state and raster from which one material was compiled.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialTextureSource {
    pub key: Box<str>,
    pub variant: u32,
    pub path: Box<str>,
}

/// A block's binding identity and face orientation without a geometry override.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlockTextureSource {
    pub visual: u32,
    pub name: Box<str>,
    pub binding_name: Box<str>,
    pub source_faces: [u8; 6],
    /// Both world faces and carried art may be rebound only for admitted full cubes.
    pub cube_binding_eligible: bool,
    pub carried_keys: [Option<Box<str>>; 6],
    pub carried_fallback_keys: [Option<Box<str>>; 6],
}

/// Largest number of state variants or positional choices in a terrain entry.
pub const MAX_TERRAIN_VARIANTS: usize = 256;

/// The largest raw terrain choice weight before normalization.
const MAX_TERRAIN_WEIGHT: f32 = 1_000_000.0;

/// Clamps raw terrain weights before normalizing the nonempty choice set.
pub fn normalize_terrain_weights(weights: &mut [f32]) -> bool {
    if weights.is_empty() || weights.iter().any(|weight| weight.is_nan()) {
        return false;
    }
    for weight in weights.iter_mut() {
        *weight = weight.clamp(0.05, MAX_TERRAIN_WEIGHT);
    }
    let total: f32 = weights.iter().sum();
    for weight in weights {
        *weight /= total;
    }
    true
}
