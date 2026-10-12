use super::*;

/// Makes a small rounded shape in logical pixels for coverage checks.
fn shape(width: f32, height: f32, radius: f32, border: f32, fringe: f32) -> Shape {
    Shape {
        width,
        height,
        radius,
        border,
        fringe,
    }
}

/// Uses physical-size-independent tint for a checked shape mesh.
fn key(width: f32, height: f32, radius: f32, border: f32, fringe: f32) -> Key {
    Key {
        width: width.to_bits(),
        height: height.to_bits(),
        radius: radius.to_bits(),
        fringe: fringe.to_bits(),
        border: border.to_bits(),
        page: 0,
        color: [0, 0, 255, 64],
        end_color: [0, 0, 255, 64],
        horizontal: false,
        border_color: [255; 4],
    }
}

/// Samples the mesh's interpolated alpha at a physical raster sample without texture filtering.
fn sample_alpha(mesh: &UiMesh, size: [f32; 2], point: [f32; 2]) -> u8 {
    let edge = |from: [f32; 2], to: [f32; 2], point: [f32; 2]| {
        (to[0] - from[0]) * (point[1] - from[1]) - (to[1] - from[1]) * (point[0] - from[0])
    };
    for triangle in mesh.indices().as_chunks::<3>().0 {
        let corners = triangle.map(|index| mesh.vertices()[index as usize]);
        let [a, b, c] =
            corners.map(|vertex| std::array::from_fn(|axis| vertex.position[axis] * size[axis]));
        let area = edge(a, b, c);
        if area.abs() < 0.000001 {
            continue;
        }
        let weights = [
            edge(b, c, point) / area,
            edge(c, a, point) / area,
            edge(a, b, point) / area,
        ];
        if weights.iter().all(|weight| *weight >= -0.00001) {
            return weights
                .into_iter()
                .zip(corners)
                .map(|(weight, vertex)| weight * f32::from(vertex.color[3]))
                .sum::<f32>()
                .round() as u8;
        }
    }
    0
}

/// Raster samples at fractional outlines retain partial coverage on squares, curves and hairlines.
#[test]
fn raster_samples_have_coverage_on_both_sides_of_authored_edges() {
    let white = Key {
        color: [255; 4],
        end_color: [255; 4],
        ..key(10.0, 6.0, 0.0, 0.0, 1.0)
    };
    let square = mesh(white).unwrap();
    assert!((142..=144).contains(&sample_alpha(&square, [10.0, 6.0], [0.25, 0.25])));
    assert_eq!(sample_alpha(&square, [10.0, 6.0], [0.5, 0.5]), 255);
    let curve = mesh(Key {
        width: 20_f32.to_bits(),
        height: 20_f32.to_bits(),
        radius: 10_f32.to_bits(),
        ..white
    })
    .unwrap();
    let alpha = sample_alpha(&curve, [20.0; 2], [2.8, 2.8]);
    assert!(
        alpha > 0 && alpha < 160,
        "outside curved sample should have partial coverage: {alpha}"
    );
    let line = mesh(Key {
        color: [0; 4],
        end_color: [0; 4],
        ..key(20.0, 12.0, 4.0, 0.25, 1.0)
    })
    .unwrap();
    for y in [0.0, 0.5] {
        assert!((63..=65).contains(&sample_alpha(&line, [20.0, 12.0], [10.0, y])));
    }
    assert_eq!(sample_alpha(&line, [20.0, 12.0], [10.0, 1.0]), 0);
}

/// Device density changes the logical band, preserving one physical pixel of edge coverage.
#[test]
fn coverage_band_is_one_physical_pixel_at_each_dpi() {
    for dpi in [1.0, 1.5, 2.0, 3.0] {
        assert!((coverage_width(dpi) * dpi - 1.0).abs() < 0.00001);
    }
}

/// A square has distinct real inset/outset contours instead of a collapsed fringe at each corner.
#[test]
fn square_edges_and_tiny_rectangles_have_visible_coverage_rings() {
    let square = shape(10.0, 6.0, 0.0, 0.0, 1.0);
    assert_eq!(square.point(0.5, 0, [-1.0, 0.0], 0.5), [0.5, 0.5]);
    assert_eq!(square.point(0.5, 2, [1.0, 0.0], 0.5), [9.5, 5.5]);
    assert_eq!(square.point(-0.5, 0, [-1.0, 0.0], 0.5), [-0.5, -0.5]);
    assert_eq!(square.point(0.0, 0, [-1.0, 0.0], 0.0), [0.0, 1.0]);
    assert_eq!(square.point(0.0, 0, [0.0, -1.0], 1.0), [1.0, 0.0]);
    assert_eq!(square.coverage(0.0), [0.5, 0.0]);
    assert_eq!(square.coverage_at([0.0, 0.0], 0.0), [0.25, 0.0]);
    assert_eq!(square.coverage_at([0.0, 1.0], 0.0), [0.5, 0.0]);
    assert_eq!(square.coverage_at([0.25, 0.25], 0.25), [0.5625, 0.0]);
    let tiny = shape(10.0, 0.25, 0.0, 0.0, 1.0);
    assert_eq!(tiny.coverage(0.125), [0.25, 0.0]);
    let mesh = mesh(Key {
        color: [255; 4],
        end_color: [255; 4],
        ..key(10.0, 0.25, 0.0, 0.0, 1.0)
    })
    .unwrap();
    assert_eq!(mesh.vertices()[0].color[3], 64);
    assert!(
        mesh.vertices()
            .iter()
            .any(|vertex| vertex.position[1] < 0.0)
    );
    assert!(mesh.vertices().iter().any(|vertex| vertex.color[3] == 0));
}

/// Subpixel borders distribute their authored width across the full coverage band.
#[test]
fn a_hairline_has_independent_outer_and_inner_antialiasing() {
    let shape = shape(20.0, 12.0, 4.0, 0.25, 1.0);
    assert_eq!(shape.coverage(-0.25), [0.0, 0.25]);
    assert_eq!(shape.coverage(0.0), [0.25, 0.25]);
    assert_eq!(shape.coverage(0.25), [0.5, 0.25]);
    assert_eq!(shape.coverage(0.5), [0.75, 0.25]);
    assert_eq!(shape.coverage(0.75), [1.0, 0.0]);
    for sample in -4..=8 {
        assert!(shape.coverage(sample as f32 * 0.125)[1] <= 0.25);
    }
    let rings = shape.rings();
    assert_eq!(*rings.first().unwrap(), -0.5);
    assert_eq!(*rings.last().unwrap(), 0.75);
}

/// One combined seam uses area-weighted alpha rather than painting a border over a fill.
#[test]
fn translucent_border_and_fill_do_not_double_blend_at_the_seam() {
    assert_eq!(
        covered_color([0, 0, 255, 64], [255; 4], [0.25, 0.25]),
        [204, 204, 255, 80]
    );
    let key = key(40.0, 20.0, 6.0, 2.0, 1.0);
    let shape = shape(40.0, 20.0, 6.0, 2.0, 1.0);
    assert_eq!(shape.coverage(2.0), [0.5, 0.5]);
    assert_eq!(
        covered_color(key.color, key.border_color, shape.coverage(2.0))[3],
        160
    );
    let mesh = mesh(key).unwrap();
    assert_eq!(mesh.vertices()[0].color, key.color);
    assert!(mesh.vertices().iter().any(|vertex| vertex.color[3] == 160));
}

/// Larger display radii receive more arc samples while work remains bounded.
#[test]
fn corner_tessellation_tracks_physical_radius() {
    let small = shape(40.0, 20.0, 6.0, 0.0, 1.0).corner_steps();
    let large = shape(400.0, 200.0, 60.0, 0.0, 1.0).corner_steps();
    let dense = shape(400.0, 200.0, 60.0, 0.0, 0.5).corner_steps();
    assert!(large > small && dense > large);
    for radius in [0.0, 1.0, 6.0, 16.0, 60.0, 120.0] {
        let shape = shape(400.0, 300.0, radius, 0.0, 1.0);
        let angle = std::f32::consts::FRAC_PI_2 / shape.corner_steps() as f32;
        let error = (radius + 0.5) * (1.0 - (angle * 0.5).cos());
        assert!(error <= CHORD_ERROR_PIXELS + 0.00001);
    }
    assert_eq!(
        shape(1_000_000.0, 1_000_000.0, 500_000.0, 0.0, 1.0).corner_steps(),
        MAX_CORNER_STEPS
    );
}

/// Borders that consume the shape never create a phantom fill at a collapsed inner contour.
#[test]
fn full_width_border_and_gradients_preserve_authored_colors() {
    let shape = shape(10.0, 6.0, 3.0, 3.0, 1.0);
    assert_eq!(shape.coverage(3.0), [0.0, 1.0]);
    let mut key = key(40.0, 20.0, 6.0, 0.0, 1.0);
    key.color = [10, 20, 30, 64];
    key.end_color = [110, 120, 130, 128];
    assert_eq!(
        fill_color(key, [20.0, 10.0], [40.0, 20.0]),
        [60, 70, 80, 96]
    );
    assert_eq!(fill_color(key, [0.0, -0.5], [40.0, 20.0]), key.color);
    assert_eq!(mesh(key).unwrap().vertices()[0].color, [60, 70, 80, 96]);
}
