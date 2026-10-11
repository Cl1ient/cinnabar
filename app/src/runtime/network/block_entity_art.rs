//! Worker-built block-entity artwork over the immutable startup carrier.

use assets::RuntimeBlockEntityAssets;
use resource_pack::LayeredPackView;
use std::sync::{Arc, OnceLock};

static BASE: OnceLock<Arc<RuntimeBlockEntityAssets>> = OnceLock::new();

/// Retains the startup carrier so every stack is resolved against the same base.
pub(crate) fn install_base(base: Option<Arc<RuntimeBlockEntityAssets>>) {
    if let Some(base) = base {
        let _ = BASE.set(base);
    }
}

/// Returns the immutable carrier used by join and reload workers.
pub(super) fn base() -> Option<&'static Arc<RuntimeBlockEntityAssets>> {
    BASE.get()
}

/// Reads each logical carrier binding from the winning layer before repacking on the worker.
pub(super) fn prepare(view: &LayeredPackView) -> Option<Arc<RuntimeBlockEntityAssets>> {
    let base = base()?;
    Some(prepare_over(view, base.as_ref()).map_or_else(|| base.clone(), Arc::new))
}

/// Rebuilds one complete atlas; invalid overrides retain their original tile.
pub(super) fn prepare_over(
    view: &LayeredPackView,
    base: &RuntimeBlockEntityAssets,
) -> Option<RuntimeBlockEntityAssets> {
    pack_compiler::overlay_block_entity_assets(base, |name| {
        pack_runtime::pack_textures::decode_pack_texture(view, name).map(|texture| {
            pack_compiler::BlockEntityTexture {
                width: texture.width, height: texture.height, rgba8: texture.rgba8,
            }
        })
    }).map_err(|error| bevy::log::warn!(%error, "block-entity artwork retained after invalid atlas overlay")).ok()
}
