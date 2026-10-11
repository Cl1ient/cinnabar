use assets::RegistryRecord;
use assets::block_shapes::ShapeBox;

/// Compiles six half-cell pick pieces; movement corner state does not affect clipping.
pub(super) fn selection(record: &RegistryRecord) -> Option<Box<[ShapeBox]>> {
    if record.model_family != assets::ModelFamily::Stair {
        return None;
    }
    let state = super::state::State::read(record)?;
    let direction = usize::try_from(state.number("weirdo_direction")?).ok()?;
    if direction >= 4 {
        return None;
    }
    let upper = usize::from(state.bit("upside_down_bit")?);
    let empty = [
        [2, 6],
        [3, 7],
        [2, 3],
        [6, 7],
        [0, 4],
        [1, 5],
        [0, 1],
        [4, 5],
    ][direction + upper * 4];
    Some(
        (0..8)
            .filter(|index| !empty.contains(index))
            .map(|index| {
                let min = [index & 1, (index >> 1) & 1, (index >> 2) & 1]
                    .map(|value| f64::from(value) * 0.5);
                ShapeBox::new(min, min.map(|value| value + 0.5))
            })
            .collect(),
    )
}

/// Builds native stair pieces from the registry's direction, half and corner state.
pub(super) fn shapes(record: &RegistryRecord) -> Option<Vec<ShapeBox>> {
    if !record.name.ends_with("_stairs") {
        return None;
    }
    let state: serde_json::Value = serde_json::from_str(&record.canonical_state).ok()?;
    let value = |name: &str| state.get(name)?.get("value");
    let turns = assets::StairDirection::from_raw(
        u32::try_from(value("weirdo_direction")?.as_u64()?).ok()?,
    )?
    .turns_from_east();
    let upside_down = value("upside_down_bit")?.as_i64()? != 0;
    let corner = value("minecraft:corner")?.as_str()?;
    let side = match corner {
        "inner_left" | "outer_left" => (0.0, 0.5),
        "inner_right" | "outer_right" => (0.5, 1.0),
        "none" => (0.0, 1.0),
        _ => return None,
    };
    // Vanilla emits slab, step, then the optional inner piece.
    let mut boxes = vec![ShapeBox::new([0.0; 3], [1.0, 0.5, 1.0])];
    let (z_min, z_max) = if corner.starts_with("outer_") {
        side
    } else {
        (0.0, 1.0)
    };
    boxes.push(ShapeBox::new([0.5, 0.5, z_min], [1.0, 1.0, z_max]));
    if corner.starts_with("inner_") {
        boxes.push(ShapeBox::new([0.0, 0.5, side.0], [0.5, 1.0, side.1]));
    }
    for shape in &mut boxes {
        for _ in 0..turns {
            *shape = ShapeBox::new(
                [1.0 - shape.max[2], shape.min[1], shape.min[0]],
                [1.0 - shape.min[2], shape.max[1], shape.max[0]],
            );
        }
        if upside_down {
            (shape.min[1], shape.max[1]) = (1.0 - shape.max[1], 1.0 - shape.min[1]);
        }
    }
    Some(boxes)
}
