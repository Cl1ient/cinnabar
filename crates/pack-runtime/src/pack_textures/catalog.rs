//! Texture path selection inside catalog entries.

use super::{Value, normalize_texture_path};

/// A texture entry is a path, an object with `path`, or a variation list whose
/// first element is used.
pub(super) fn first_texture_path(value: &Value) -> Option<String> {
    let path = match value {
        Value::String(path) => path.as_str(),
        Value::Object(entry) => entry.get("path")?.as_str()?,
        Value::Array(entries) => return first_texture_path(entries.first()?),
        _ => return None,
    };
    let path = normalize_texture_path(path);
    (!path.is_empty()).then(|| path.to_owned())
}
