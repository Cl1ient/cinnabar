use super::*;
use super::{coverage::Coverage, raster::Domain};
use serde_json::json;

/// Constructs an authored fixture without depending on any product's artwork.
fn key(paths: Value, stroke: Option<f32>, size: f32, dpi: f32) -> Key {
    let mut data: BTreeMap<String, Value> = serde_json::from_value(json!({
        "view_box": [0, 0, 16, 16], "paths": paths,
    }))
    .unwrap();
    if let Some(width) = stroke {
        data.insert("stroke_width".into(), json!(width));
    }
    Key {
        path: parse(&data).unwrap(),
        size: [size.to_bits(); 2],
        dpi: dpi.to_bits(),
        page: 0,
        color: [255, 255, 255, 128],
    }
}

#[test]
fn diagonal_strokes_have_continuous_round_coverage_without_overdraw() {
    let key = key(
        json!([[[2, 2], [8, 8], [14, 2]], [[8, 8], [8, 14]]]),
        Some(2.0),
        32.0,
        1.0,
    );
    let coverage = Coverage::new(&key).unwrap();
    for step in 0..=96 {
        let coordinate = 4.0 + step as f32 / 96.0 * 12.0;
        assert_eq!(coverage.at([coordinate; 2]), 1.0);
    }
    assert_eq!(
        coverage.at([16.0; 2]),
        1.0,
        "rounded joined strokes are covered once"
    );
    let mesh = mesh(&key).unwrap();
    assert!(mesh.vertices().iter().all(|vertex| vertex.color[3] <= 128));
    assert!(
        mesh.vertices()
            .iter()
            .any(|vertex| vertex.color[3] > 0 && vertex.color[3] < 128)
    );
}

#[test]
fn filled_contours_leave_holes_transparent_and_center_each_edge_band() {
    let key = key(
        json!([
            [[0, 0], [16, 0], [16, 16], [0, 16]],
            [[4, 4], [12, 4], [12, 12], [4, 12]],
        ]),
        None,
        32.0,
        1.0,
    );
    let coverage = Coverage::new(&key).unwrap();
    assert_eq!(coverage.at([16.0; 2]), 0.0);
    assert_eq!(coverage.at([4.0; 2]), 1.0);
    for (x, expected) in [
        (-0.5, 0.0),
        (-0.25, 0.25),
        (0.0, 0.5),
        (0.25, 0.75),
        (0.5, 1.0),
    ] {
        assert!((coverage.at([x, 16.0]) - expected).abs() < 1e-6);
    }
    assert_eq!(
        coverage.at([8.0, 16.0]),
        0.5,
        "inner and outer silhouettes both soften"
    );
}

#[test]
fn coverage_width_stays_one_physical_pixel_at_fractional_dpi() {
    for dpi in [1.0, 1.25, 1.5, 2.0] {
        let key = key(
            json!([[[0, 0], [16, 0], [16, 16], [0, 16]]]),
            None,
            32.0,
            dpi,
        );
        let coverage = Coverage::new(&key).unwrap();
        assert!((coverage.at([-0.25 / dpi, 16.0]) - 0.25).abs() < 1e-6);
        assert!((coverage.at([0.25 / dpi, 16.0]) - 0.75).abs() < 1e-6);
    }
}

#[test]
fn malformed_or_unbounded_authored_paths_are_rejected() {
    let base = json!({"view_box": [0, 0, 16, 16], "paths": [[[0, 0], [16, 0], [16, 16]]]});
    for (field, value) in [
        ("view_box", json!([0, 0, 0, 16])),
        ("paths", json!([])),
        ("paths", json!([[[0, 0], [1, 1]]])),
        ("paths", json!([[[0, 0], [1, 1], [1e100, 0]]])),
        ("paths", json!([vec![[0, 0]; MAX_POINTS + 1]])),
        ("paths", json!(vec![vec![[0, 0]; 3]; MAX_CONTOURS + 1])),
        ("stroke_width", json!(0)),
        ("closed", json!("false")),
        ("closed", json!(false)),
    ] {
        let mut value_base = base.clone();
        value_base[field] = value;
        let data = serde_json::from_value(value_base).unwrap();
        assert!(parse(&data).is_none(), "rejected {field}");
    }
}

#[test]
fn hot_path_meshes_remain_retained_when_cold_entries_expire() {
    let mut cache = Cache::default();
    let path = json!([[[2, 2], [14, 14]]]);
    let first = cache
        .get_or_create(key(path.clone(), Some(2.0), 16.0, 1.0))
        .unwrap();
    for revision in 0..MAX_CACHED_PATHS {
        let mut cold = key(path.clone(), Some(2.0), 16.0, 1.0);
        cold.color[0] = revision as u8;
        cache.get_or_create(cold).unwrap();
        assert!(Arc::ptr_eq(
            &cache
                .get_or_create(key(path.clone(), Some(2.0), 16.0, 1.0))
                .unwrap(),
            &first
        ));
    }
    assert!(cache.bytes <= MAX_CACHED_BYTES);
    assert!(cache.entries.len() <= MAX_CACHED_PATHS);
}

/// Paths too large for half-pixel samples are refused rather than silently losing thin features.
#[test]
fn oversized_thin_strokes_are_rejected_without_coarsening() {
    let paths = json!([[[8, 0], [8, 16]]]);
    let oversized = key(paths.clone(), Some(0.125), 128.0, 1.0);
    assert!(Domain::new(&Coverage::new(&oversized).unwrap()).is_none());
    assert!(mesh(&oversized).is_none());
    let admitted = key(paths, Some(0.125), 96.0, 1.0);
    assert!(Domain::new(&Coverage::new(&admitted).unwrap()).is_some());
    assert!(mesh(&admitted).is_some());
}

/// Source bounds and stroke support determine geometry, so an outside endpoint keeps its round cap.
#[test]
fn outside_view_box_endpoints_retain_round_cap_coverage() {
    let key = key(json!([[[-2, 8], [0, 8]]]), Some(2.0), 32.0, 1.0);
    let coverage = Coverage::new(&key).unwrap();
    let (min, max) = coverage.raster_bounds().unwrap();
    assert_eq!(min, [-6.5, 13.5]);
    assert_eq!(max, [2.5, 18.5]);
    assert_eq!(coverage.at([2.25, 16.0]), 0.25);
    let mesh = mesh(&key).unwrap();
    assert!(
        mesh.vertices()
            .iter()
            .any(|vertex| vertex.position[0] < -6.0 / 32.0)
    );
    assert!(
        mesh.vertices()
            .iter()
            .any(|vertex| vertex.position[0] > 2.0 / 32.0)
    );
}

/// Rotation, reversed winding and a repeated closing point preserve even-odd cancellation.
#[test]
fn duplicate_filled_contours_leave_no_ghost_outline() {
    let square = json!([[0, 0], [16, 0], [16, 16], [0, 16]]);
    let reversed = json!([[16, 16], [16, 0], [0, 0], [0, 16], [16, 16]]);
    let cancelled = key(json!([square, reversed]), None, 32.0, 1.0);
    assert!(cancelled.path.contours.is_empty());
    let coverage = Coverage::new(&cancelled).unwrap();
    assert_eq!(coverage.at([0.0, 16.0]), 0.0);
    assert_eq!(coverage.at([16.0; 2]), 0.0);
    assert!(mesh(&cancelled).is_none());
    let odd = key(json!([square, reversed, square]), None, 32.0, 1.0);
    assert_eq!(odd.path.contours.len(), 1);
    assert_eq!(Coverage::new(&odd).unwrap().at([16.0; 2]), 1.0);
    assert!(mesh(&odd).is_some());
    let line = key(json!([[[0, 0], [8, 8], [16, 16]]]), None, 32.0, 1.0);
    assert!(mesh(&line).is_none(), "zero-area fill is not a stroke");
}

/// Subpixel stroke width controls peak coverage as the exact one-dimensional stripe overlap.
#[test]
fn subpixel_strokes_preserve_their_physical_width() {
    let key = key(json!([[[2, 8], [14, 8]]]), Some(0.25), 16.0, 1.0);
    let coverage = Coverage::new(&key).unwrap();
    for distance in [0.0, 0.1, 0.25] {
        assert_eq!(coverage.at([8.0, 8.0 + distance]), 0.25);
    }
    assert_eq!(coverage.at([8.0, 8.5]), 0.125);
    assert_eq!(coverage.at([8.0, 8.625]), 0.0);
    assert!(
        mesh(&key)
            .unwrap()
            .vertices()
            .iter()
            .all(|vertex| vertex.color[3] <= 32)
    );
}

/// Work and expanded-index admission fail before publishing oversized meshes.
#[test]
fn raster_work_and_expanded_geometry_have_independent_limits() {
    let points: Vec<_> = (0..MAX_POINTS)
        .map(|index| [if index % 2 == 0 { 0 } else { 16 }; 2])
        .collect();
    let expensive = key(json!([points]), Some(1.0), 32.0, 1.0);
    let coverage = Coverage::new(&expensive).unwrap();
    assert_eq!(coverage.segment_count(), MAX_POINTS - 1);
    assert!(Domain::new(&coverage).is_none());
    assert!(mesh(&expensive).is_none());
    let square = json!([[[0, 0], [16, 0], [16, 16], [0, 16]]]);
    let dense = key(square.clone(), None, 80.0, 1.0);
    assert!(Domain::new(&Coverage::new(&dense).unwrap()).is_some());
    assert!(
        mesh(&dense).is_none(),
        "expanded indices exceed the per-mesh budget"
    );
    let admitted = key(square, None, 30.0, 1.0);
    assert!(mesh(&admitted).unwrap().indices().len() <= MAX_MESH_INDICES);
}

/// Sparse strokes retain only grid vertices referenced by visible cells.
#[test]
fn sparse_mesh_packing_avoids_retaining_the_full_grid() {
    let key = key(json!([[[2, 2], [14, 14]]]), Some(2.0), 32.0, 1.0);
    let mesh = mesh(&key).unwrap();
    assert!(mesh.vertices().len() < 2_000);
    assert!(mesh.indices().len() <= MAX_MESH_INDICES);
}
