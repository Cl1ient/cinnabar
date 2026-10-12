//! Finite authored contours and canonical even-odd cancellation.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{MAX_CONTOURS, MAX_POINTS};

#[derive(Clone, Eq, Hash, PartialEq)]
pub(super) struct Path {
    pub(super) view_box: [u32; 4],
    pub(super) contours: Vec<Vec<[u32; 2]>>,
    pub(super) stroke_width: Option<u32>,
    pub(super) closed: bool,
}

/// Rejects malformed or excessively large path data before allocating raster geometry.
pub(super) fn parse(data: &BTreeMap<String, Value>) -> Option<Path> {
    let source_box = data.get("view_box")?.as_array()?;
    if source_box.len() != 4 {
        return None;
    }
    let number = |value: &Value| {
        let number = value.as_f64()? as f32;
        (number.is_finite() && number.abs() <= 1_000_000.0).then_some(if number == 0.0 {
            0.0
        } else {
            number
        })
    };
    let view_box = [
        number(&source_box[0])?,
        number(&source_box[1])?,
        number(&source_box[2])?,
        number(&source_box[3])?,
    ];
    if view_box[2] <= 0.0 || view_box[3] <= 0.0 {
        return None;
    }
    let stroke_width = match data.get("stroke_width") {
        Some(value) => Some(number(value)?),
        None => None,
    };
    if stroke_width.is_some_and(|value| value <= 0.0) {
        return None;
    }
    let closed = match data.get("closed") {
        Some(value) => value.as_bool()?,
        None => stroke_width.is_none(),
    };
    if stroke_width.is_none() && !closed {
        return None;
    }
    let source_paths = data.get("paths")?.as_array()?;
    if source_paths.is_empty() || source_paths.len() > MAX_CONTOURS {
        return None;
    }
    let mut contours = Vec::with_capacity(source_paths.len());
    let mut total = 0usize;
    for source in source_paths {
        let points = source.as_array()?;
        total = total.checked_add(points.len())?;
        if points.len() < if stroke_width.is_some() { 2 } else { 3 } || total > MAX_POINTS {
            return None;
        }
        let mut contour = Vec::with_capacity(points.len());
        for point in points {
            let point = point.as_array()?;
            if point.len() != 2 {
                return None;
            }
            contour.push([number(&point[0])?.to_bits(), number(&point[1])?.to_bits()]);
        }
        contours.push(contour);
    }
    if stroke_width.is_none() {
        contours = cancel_contours(contours);
    }
    Some(Path {
        view_box: view_box.map(f32::to_bits),
        contours,
        stroke_width: stroke_width.map(f32::to_bits),
        closed,
    })
}

/// Finds a lexicographically smallest cyclic start in linear time, including repeated points.
fn rotation(points: &[[u32; 2]]) -> usize {
    let length = points.len();
    let (mut first, mut second, mut offset) = (0, 1, 0);
    while first < length && second < length && offset < length {
        let a = points[(first + offset) % length];
        let b = points[(second + offset) % length];
        match a.cmp(&b) {
            std::cmp::Ordering::Equal => offset += 1,
            std::cmp::Ordering::Greater => {
                first += offset + 1;
                if first <= second {
                    first = second + 1;
                }
                offset = 0;
            }
            std::cmp::Ordering::Less => {
                second += offset + 1;
                if second <= first {
                    second = first + 1;
                }
                offset = 0;
            }
        }
    }
    first.min(second)
}

/// Normalizes cyclic starts and winding without changing a contour's silhouette.
fn canonical(mut contour: Vec<[u32; 2]>) -> Vec<[u32; 2]> {
    let first = rotation(&contour);
    contour.rotate_left(first);
    let forward = contour.clone();
    contour.reverse();
    let first = rotation(&contour);
    contour.rotate_left(first);
    forward.min(contour)
}

/// Degenerate fills have no area and must not acquire a ghost stroke from distance coverage.
fn has_area(contour: &[[u32; 2]]) -> bool {
    let Some(first) = contour.first() else {
        return false;
    };
    let Some(second) = contour.iter().find(|point| *point != first) else {
        return false;
    };
    let a = first.map(f32::from_bits);
    let b = second.map(f32::from_bits);
    contour.iter().any(|point| {
        let p = point.map(f32::from_bits);
        (b[0] - a[0]) * (p[1] - a[1]) != (b[1] - a[1]) * (p[0] - a[0])
    })
}

/// Identical even-odd boundaries cancel in pairs, independent of start point or winding.
fn cancel_contours(contours: Vec<Vec<[u32; 2]>>) -> Vec<Vec<[u32; 2]>> {
    let mut retained = BTreeMap::<Vec<[u32; 2]>, ()>::new();
    for mut contour in contours {
        contour.dedup();
        if contour.first() == contour.last() {
            contour.pop();
        }
        if contour.len() < 3 || !has_area(&contour) {
            continue;
        }
        let contour = canonical(contour);
        if retained.remove(&contour).is_none() {
            retained.insert(contour, ());
        }
    }
    retained.into_keys().collect()
}
