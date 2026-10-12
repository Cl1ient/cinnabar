//! Texture-only packs inherit the base attachables that reference their images.

use super::*;

/// Inherits matching base documents from the sidecar, with loose files for older sidecars.
pub(super) fn collect(
    view: &LayeredPackView,
    vanilla: Option<&VanillaEntityRefs>,
    vanilla_pack_dir: Option<&Path>,
) -> Vec<(Box<str>, Vec<u8>)> {
    let mut files = view
        .list("attachables/")
        .into_iter()
        .filter(|path| path.ends_with(".json"))
        .filter_map(|path| Some((Box::<str>::from(path), canonical_json(&view.read(path)?)?)))
        .collect::<Vec<_>>();
    // Track the directory even when empty: adding a texture creates inherited bindings.
    let texture_stems = view
        .list("textures/")
        .into_iter()
        .filter_map(|path| {
            path.strip_suffix(".png")
                .or_else(|| path.strip_suffix(".tga"))
        })
        .collect::<BTreeSet<_>>();
    if texture_stems.is_empty() {
        return files;
    }
    let authored_paths = files
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    let authored_items = files
        .iter()
        .filter_map(|(_, bytes)| parse_pack_json(bytes))
        .flat_map(|value| bound_items(&value))
        .collect::<BTreeSet<_>>();
    let installed;
    let inherited = match vanilla.filter(|refs| !refs.attachables.is_empty()) {
        Some(refs) => &refs.attachables,
        None => {
            installed = installed_attachables(vanilla_pack_dir);
            &installed
        }
    };
    for (relative, value) in inherited {
        if authored_paths.contains(relative.as_str())
            || bound_items(value)
                .iter()
                .any(|item| authored_items.contains(item))
        {
            continue;
        }
        let mut referenced = BTreeSet::new();
        collect_texture_strings(value, &mut referenced);
        if referenced
            .iter()
            .any(|stem| texture_stems.contains(stem.as_str()))
            && let Ok(bytes) = serde_json::to_vec(value)
        {
            files.push((relative.as_str().into(), bytes));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// Reads bounded base documents when an older sidecar has no retained attachables.
fn installed_attachables(root: Option<&Path>) -> BTreeMap<String, Value> {
    let mut files = BTreeMap::new();
    let Some(root) = root else {
        return files;
    };
    let mut directories = vec![root.join("attachables")];
    while let Some(directory) = directories.pop() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                directories.push(entry.path());
                continue;
            }
            let path = entry.path();
            if !kind.is_file() || path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            let Ok(file) = std::fs::File::open(&path) else {
                continue;
            };
            let mut bytes = Vec::new();
            if file
                .take(resource_pack::MAX_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
                .is_err()
                || bytes.len() as u64 > resource_pack::MAX_FILE_BYTES
            {
                continue;
            }
            let Some(value) = parse_pack_json(&bytes) else {
                continue;
            };
            files.insert(relative, value);
        }
    }
    files
}

/// Item identifiers supplied by one attachable document, including owner-specific variants.
fn bound_items(value: &Value) -> Vec<String> {
    let description = &value["minecraft:attachable"]["description"];
    if let Some(items) = description["item"].as_object() {
        return items.keys().cloned().collect();
    }
    description["identifier"]
        .as_str()
        .map(str::to_owned)
        .into_iter()
        .collect()
}
