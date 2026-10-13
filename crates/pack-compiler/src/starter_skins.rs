use std::{fs, io::Cursor, path::Path};

use ::image::{ImageFormat, ImageReader};
use assets::{
    parse_skin_geometry,
    starter_skins::{
        MAX_STARTER_SKIN_GEOMETRY_BYTES, STARTER_SKIN_GEOMETRY_IDENTIFIERS, STARTER_SKIN_SIDE,
        STARTER_SKIN_SOURCES, StarterSkin, StarterSkins, encode_starter_skins,
    },
};
use serde_json::{Map, Value};

const MAX_TEXTURE_BYTES: u64 = 64 * 1024;
const MAX_MODELS_BYTES: u64 = 4 * 1024 * 1024;
const MODELS: &str = assets::LEGACY_ENTITY_GEOMETRY_PATH;
const MAX_INHERITANCE_DEPTH: usize = 8;

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let size = fs::metadata(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .len();
    if size > limit {
        return Err(format!("{} exceeds {limit} bytes", path.display()));
    }
    fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn read_skin(path: &Path) -> Result<Box<[u8]>, String> {
    let decoded = ImageReader::with_format(
        Cursor::new(read_bounded(path, MAX_TEXTURE_BYTES)?),
        ImageFormat::Png,
    )
    .decode()
    .map_err(|error| format!("{}: {error}", path.display()))?
    .into_rgba8();
    if decoded.dimensions() != (STARTER_SKIN_SIDE, STARTER_SKIN_SIDE) {
        return Err(format!(
            "{} is {:?}, expected {STARTER_SKIN_SIDE}x{STARTER_SKIN_SIDE}",
            path.display(),
            decoded.dimensions()
        ));
    }
    Ok(decoded.into_raw().into_boxed_slice())
}

/// Keeps a legacy model and its parents in one native-compatible document.
fn skin_geometry(models: &Map<String, Value>, model: &str) -> Result<String, String> {
    let identifier = |key: &str| key.split_once(':').map_or(key, |(own, _)| own).to_owned();
    let mut kept = Map::new();
    let mut next = Some(model.to_owned());
    let mut depth = 0;
    while let Some(wanted) = next.take() {
        depth += 1;
        if depth > MAX_INHERITANCE_DEPTH {
            return Err(format!("{model} inherits too deeply"));
        }
        let (key, value) = models
            .iter()
            .find(|(key, _)| identifier(key) == wanted)
            .ok_or_else(|| format!("{MODELS} has no {wanted}"))?;
        kept.insert(key.clone(), value.clone());
        next = key.split_once(':').map(|(_, parent)| parent.to_owned());
    }
    if let Some(version) = models.get("format_version") {
        kept.insert("format_version".to_owned(), version.clone());
    }
    let geometry = Value::Object(kept).to_string();
    if geometry.len() > MAX_STARTER_SKIN_GEOMETRY_BYTES {
        return Err(format!(
            "skin geometry exceeds {MAX_STARTER_SKIN_GEOMETRY_BYTES} bytes"
        ));
    }
    let patch = serde_json::json!({ "geometry": { "default": model } }).to_string();
    match parse_skin_geometry(&patch, &geometry) {
        Ok(Some(_)) => {}
        _ => return Err(format!("{model} does not resolve as skin geometry")),
    }
    Ok(geometry)
}

/// Uses the entity compiler's identifier selection, with standalone models above legacy models.
fn pack_skin_geometry(
    pack: &Path,
) -> Result<[Box<str>; STARTER_SKIN_GEOMETRY_IDENTIFIERS.len()], String> {
    let mut documents = Vec::new();
    let legacy = pack.join(MODELS);
    if legacy.is_file() {
        documents.push((MODELS.into(), read_bounded(&legacy, MAX_MODELS_BYTES)?));
    }
    let mut paths = Vec::new();
    crate::entity::collect::collect_optional_family(pack, "models/entity", &["json"], &mut paths)
        .map_err(|error| error.to_string())?;
    for (relative, path) in paths {
        documents.push((relative, read_bounded(&path, MAX_MODELS_BYTES)?));
    }
    let mut models = Map::new();
    let mut modern = Vec::new();
    for (_, bytes) in crate::select_entity_geometry(documents) {
        let mut document: Map<String, Value> =
            pack_json::parse_jsonc(&bytes).map_err(|error| error.to_string())?;
        if let Some(Value::Array(entries)) = document.remove("minecraft:geometry") {
            for entry in entries.into_iter().filter(|entry| {
                entry["description"]["identifier"]
                    .as_str()
                    .is_some_and(|id| STARTER_SKIN_GEOMETRY_IDENTIFIERS.contains(&id))
            }) {
                let mut source = document.clone();
                source.insert(
                    "minecraft:geometry".to_owned(),
                    Value::Array(vec![entry.clone()]),
                );
                modern.push((entry, source));
            }
        } else {
            models.extend(document);
        }
    }
    let geometries = STARTER_SKIN_GEOMETRY_IDENTIFIERS
        .iter()
        .map(|model| {
            let geometry = match modern
                .iter()
                .find(|(entry, _)| entry["description"]["identifier"].as_str() == Some(*model))
            {
                Some((_, source)) => Value::Object(source.clone()).to_string(),
                None => skin_geometry(&models, model)?,
            };
            let patch = serde_json::json!({"geometry":{"default":model}}).to_string();
            parse_skin_geometry(&patch, &geometry)
                .map_err(|error| format!("{model}: {error:?}"))?
                .ok_or_else(|| format!("{model} does not resolve as skin geometry"))?;
            Ok(geometry.into_boxed_str())
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(geometries.try_into().expect("one document per model"))
}

/// Reads the starter skins and humanoid skin models from `pack` and encodes the MCBESKN1 carrier.
pub fn compile_starter_skins(pack: &Path) -> Result<Vec<u8>, String> {
    let skins = STARTER_SKIN_SOURCES
        .iter()
        .map(|source| {
            Ok(StarterSkin {
                name: source.name.into(),
                slim: source.slim,
                rgba8: read_skin(&pack.join(source.texture))?,
            })
        })
        .collect::<Result<_, String>>()?;
    let carrier = StarterSkins {
        geometries: pack_skin_geometry(pack)?,
        skins,
    };
    encode_starter_skins(&carrier).map_err(|error| error.to_string())
}

/// Compiles the carrier and writes it to `out` through a temporary sibling file.
pub fn compile_starter_skins_to_file(pack: &Path, out: &Path) -> Result<(), String> {
    let blob = compile_starter_skins(pack)?;
    let temporary = out.with_extension("tmp");
    fs::write(&temporary, &blob).map_err(|error| format!("{}: {error}", temporary.display()))?;
    fs::rename(&temporary, out).map_err(|error| format!("{}: {error}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::starter_skins::{CLASSIC_SKIN_GEOMETRY, SLIM_SKIN_GEOMETRY};

    fn cube_model() -> Value {
        serde_json::json!({
            "texturewidth": 64, "textureheight": 64,
            "bones": [{"name": "body", "pivot": [0, 24, 0],
                "cubes": [{"origin": [-4, 12, -2], "size": [8, 12, 4], "uv": [16, 16]}]}]
        })
    }

    #[test]
    fn keeps_only_skin_models_and_their_parents() {
        let models = serde_json::json!({
            "format_version": "1.8.0",
            "geometry.humanoid": cube_model(),
            "geometry.humanoid.custom:geometry.humanoid": {"bones": []},
            "geometry.humanoid.customSlim": cube_model(),
            "geometry.zombie": cube_model(),
        });
        let geometry = skin_geometry(models.as_object().unwrap(), CLASSIC_SKIN_GEOMETRY).unwrap();
        let kept: Map<String, Value> = serde_json::from_str(&geometry).unwrap();
        let mut keys: Vec<_> = kept.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "format_version",
                "geometry.humanoid",
                "geometry.humanoid.custom:geometry.humanoid",
            ]
        );
    }

    #[test]
    fn a_missing_slim_model_fails_the_compile() {
        let models = serde_json::json!({
            "format_version": "1.8.0",
            "geometry.humanoid.custom": cube_model(),
        });
        assert!(skin_geometry(models.as_object().unwrap(), SLIM_SKIN_GEOMETRY).is_err());
    }

    #[test]
    fn standalone_skin_geometry_overrides_the_legacy_inherited_model() {
        let directory = tempfile::tempdir().unwrap();
        let pack = directory.path();
        fs::create_dir_all(pack.join("models/entity")).unwrap();
        fs::create_dir_all(pack.join("textures/entity")).unwrap();
        let legacy = serde_json::json!({
            "format_version":"1.8.0",
            "geometry.parent":cube_model(),
            "geometry.humanoid.custom:geometry.parent":cube_model(),
            "geometry.humanoid.customSlim":cube_model()
        });
        fs::write(pack.join(MODELS), serde_json::to_vec(&legacy).unwrap()).unwrap();
        let modern = serde_json::json!({
            "format_version":"1.12.0",
            "minecraft:geometry":[{
                "description":{"identifier":CLASSIC_SKIN_GEOMETRY,"texture_width":STARTER_SKIN_SIDE,"texture_height":STARTER_SKIN_SIDE},
                "bones":[{"name":"body","cubes":[{
                    "origin":[-4,12,-2],"size":[8,12,4],"uv":[32,32],"inflate":0.25
                }]}]
            }]
        });
        fs::write(
            pack.join("models/entity/fixture.json"),
            serde_json::to_vec(&modern).unwrap(),
        )
        .unwrap();
        for skin in STARTER_SKIN_SOURCES {
            ::image::RgbaImage::from_pixel(
                STARTER_SKIN_SIDE,
                STARTER_SKIN_SIDE,
                ::image::Rgba([33, 177, 91, 255]),
            )
            .save(pack.join(skin.texture))
            .unwrap();
        }
        let carrier =
            assets::starter_skins::decode_starter_skins(&compile_starter_skins(pack).unwrap())
                .unwrap();
        let patch = serde_json::json!({"geometry":{"default":CLASSIC_SKIN_GEOMETRY}}).to_string();
        let classic: Value = serde_json::from_str(&carrier.geometries[0]).unwrap();
        assert_eq!(classic["format_version"], "1.12.0");
        assert!(classic.get(CLASSIC_SKIN_GEOMETRY).is_none());
        let slim: Value = serde_json::from_str(&carrier.geometries[1]).unwrap();
        assert_eq!(slim["format_version"], "1.8.0");
        assert!(slim.get("minecraft:geometry").is_none());
        let model = parse_skin_geometry(&patch, &carrier.geometries[0])
            .unwrap()
            .unwrap();
        assert_eq!(
            model.bones[0].cubes.len(),
            1,
            "the selected modern model has no inherited coplanar cube"
        );
        assert_eq!(model.bones[0].cubes[0].inflate.get(), 0.25);
        assert_eq!(
            model.bones[0].cubes[0].uv,
            assets::EntityGeometryUv::Box([assets::EntityGeometryScalar::new(32.0).unwrap(); 2])
        );
        let patch = serde_json::json!({"geometry":{"default":SLIM_SKIN_GEOMETRY}}).to_string();
        assert!(
            parse_skin_geometry(&patch, &carrier.geometries[1])
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn selected_skin_models_preserve_authored_coincident_cubes() {
        let mut model = cube_model();
        let mut second = model["bones"][0]["cubes"][0].clone();
        second["uv"] = serde_json::json!([32, 32]);
        model["bones"][0]["cubes"]
            .as_array_mut()
            .unwrap()
            .push(second);
        let models = Map::from_iter([
            (CLASSIC_SKIN_GEOMETRY.to_owned(), model),
            (SLIM_SKIN_GEOMETRY.to_owned(), cube_model()),
        ]);
        let data = skin_geometry(&models, CLASSIC_SKIN_GEOMETRY).unwrap();
        let patch = serde_json::json!({"geometry":{"default":CLASSIC_SKIN_GEOMETRY}}).to_string();
        let geometry = parse_skin_geometry(&patch, &data).unwrap().unwrap();
        let cubes = &geometry.bones[0].cubes;
        assert_eq!(cubes.len(), 2);
        assert_eq!(cubes[0].origin, cubes[1].origin);
        assert_eq!(cubes[0].size, cubes[1].size);
        assert_ne!(cubes[0].uv, cubes[1].uv);
    }

    #[test]
    fn pinned_pack_compiles_when_available() {
        let pack = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(assets::vanilla_source().resource_pack_dir());
        if !pack.join(MODELS).is_file() {
            eprintln!(
                "skipping pinned_pack_compiles_when_available: fixture unavailable; requires the pinned vanilla resource pack under .local"
            );
            return;
        }
        let decoded =
            assets::starter_skins::decode_starter_skins(&compile_starter_skins(&pack).unwrap())
                .unwrap();
        assert_eq!(decoded.skins.len(), STARTER_SKIN_SOURCES.len());
    }
}
