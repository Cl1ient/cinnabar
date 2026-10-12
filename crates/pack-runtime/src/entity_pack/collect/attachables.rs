//! Only authored attachables create pack bindings; texture overrides keep startup bindings.

use super::*;

/// Collects the accepted stack's authored attachable documents in path order.
pub(super) fn collect(view: &LayeredPackView) -> Vec<(Box<str>, Vec<u8>)> {
    view.list("attachables/")
        .into_iter()
        .filter(|path| path.ends_with(".json"))
        .filter_map(|path| Some((Box::<str>::from(path), canonical_json(&view.read(path)?)?)))
        .collect()
}
