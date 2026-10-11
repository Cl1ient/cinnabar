//! Bounded pack reads for the shared geometry compiler.
use pack_compiler::block_geometry::{
    BlockGeometryCatalog, Geometry, MAX_GEOMETRY_FILE_BYTES, MAX_GEOMETRY_FILES,
};
use resource_pack::{LayeredPackView, normalize_jsonc};
use std::collections::{HashMap, HashSet};

/// Supplies every admitted parent definition in layer order, then compiles requested block models.
pub(super) fn geometry_catalog(
    view: &LayeredPackView,
    wanted: &HashSet<&str>,
) -> HashMap<String, Geometry> {
    if wanted.is_empty() {
        return HashMap::new();
    }
    let files = view
        .list_with_suffixes("models/", &[".json"])
        .into_iter()
        .take(MAX_GEOMETRY_FILES)
        .collect::<Vec<_>>();
    let mut catalog = BlockGeometryCatalog::default();
    let mut remaining = pack_compiler::MAX_PACK_ENTITY_BYTES;
    for layer in view.layers() {
        for path in &files {
            let Ok(Some(bytes)) = layer.read_file_with_limit(path, MAX_GEOMETRY_FILE_BYTES as u64)
            else {
                continue;
            };
            let Some(next) = remaining.checked_sub(bytes.len()) else {
                return catalog.compile(wanted);
            };
            remaining = next;
            let Some(bytes) = normalize_jsonc(&bytes) else {
                continue;
            };
            let Ok(root) = serde_json::from_slice(&bytes) else {
                continue;
            };
            catalog.insert(&root);
        }
    }
    catalog.compile(wanted)
}
