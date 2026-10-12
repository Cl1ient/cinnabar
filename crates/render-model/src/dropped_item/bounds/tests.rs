use super::*;
use crate::dropped_item::{
    OPAQUE_WHITE, cube_mesh, dropped_item_transform, native_dropped_sprite_mesh,
};

/// Builds bounds from the same cube vertices the item renderer uploads.
fn cube() -> DroppedItemBounds {
    DroppedItemBounds::from_mesh(&cube_mesh([0; 6], [OPAQUE_WHITE; 6], 1, 1).unwrap()).unwrap()
}

/// Uses the production reverse-Z near distance with a camera facing the requested point.
fn view(eye: Vec3, target: Vec3) -> DroppedItemCullView {
    let up = if (target - eye).normalize().dot(Vec3::Y).abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    DroppedItemCullView::new(
        glam::camera::rh::proj::directx::perspective_infinite_reverse(
            0.65,
            1.0,
            render_api::CAMERA_NEAR_PLANE_BLOCKS,
        ) * glam::camera::rh::view::look_at_mat4(eye, target, up),
        eye,
    )
    .unwrap()
}

#[test]
fn camera_inside_or_near_plane_intersecting_any_cube_face_retains_rear_surfaces() {
    let bounds = cube()
        .transformed(dropped_item_transform([0.0; 3], 0.0, 1.0))
        .unwrap();
    let near = render_api::CAMERA_NEAR_PLANE_BLOCKS;
    for axis in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
        for eye in [
            axis * 0.25,
            axis * 0.5,
            axis * (0.5 + near * 0.8),
            axis * (0.5 + near),
        ] {
            assert!(
                !bounds.allows_back_face_culling(&view(eye, Vec3::ZERO)),
                "{eye:?}"
            );
        }
        assert!(bounds.allows_back_face_culling(&view(axis * (0.5 + near * 4.0), Vec3::ZERO)));
    }
    assert!(!bounds.allows_back_face_culling(&view(Vec3::ZERO, -Vec3::Z)));
}

#[test]
fn transformed_bounds_enclose_rotated_scaled_and_sheared_mesh_vertices() {
    let mesh = cube_mesh([0; 6], [OPAQUE_WHITE; 6], 1, 1).unwrap();
    for rows in [
        dropped_item_transform([10.0, 2.0, -3.0], 0.7, 2.0),
        [
            [1.0, 0.35, -0.2, 10.0],
            [0.2, 2.0, 0.1, -3.0],
            [-0.3, 0.4, 0.75, 5.0],
        ],
    ] {
        let bounds = cube().transformed(rows).unwrap();
        for vertex in &mesh {
            let point = Vec3::from_array(vertex.position).extend(1.0);
            let position = Vec3::from_array(rows.map(|row| Vec4::from_array(row).dot(point)));
            assert!(position.cmpge(bounds.min).all() && position.cmple(bounds.max).all());
        }
        let center = (bounds.min + bounds.max) * 0.5;
        assert!(!bounds.allows_back_face_culling(&view(center, center - Vec3::Z)));
        assert!(bounds.allows_back_face_culling(&view(center + Vec3::Z * 20.0, center)));
    }
}

#[test]
fn sprite_bounds_retain_the_native_floor_origin_and_depth() {
    let mesh = native_dropped_sprite_mesh(1, 1, &[255; 4], 32, 0).unwrap();
    let bounds = DroppedItemBounds::from_mesh(&mesh).unwrap();
    assert_eq!(bounds.min.y, 0.0);
    assert_eq!(bounds.max.y, 1.0);
    assert_eq!(bounds.min.z, -1.0 / 16.0);
    assert_eq!(bounds.max.z, 0.0);
    assert!(!bounds.allows_back_face_culling(&view(
        Vec3::new(0.0, 0.5, -1.0 / 32.0),
        Vec3::new(0.0, 0.5, -1.0)
    )));
}

#[test]
fn missing_or_nonfinite_geometry_and_views_cannot_justify_culling() {
    assert!(DroppedItemBounds::from_mesh(&[]).is_none());
    let mut mesh = cube_mesh([0; 6], [OPAQUE_WHITE; 6], 1, 1).unwrap();
    mesh[0].position[0] = f32::NAN;
    assert!(DroppedItemBounds::from_mesh(&mesh).is_none());
    assert!(cube().transformed([[f32::NAN; 4]; 3]).is_none());
    assert!(DroppedItemCullView::new(Mat4::ZERO, Vec3::ZERO).is_none());
    assert!(DroppedItemCullView::new(Mat4::from_cols_array(&[f32::NAN; 16]), Vec3::ZERO).is_none());
}

#[test]
fn camera_containment_disables_culling_even_when_its_near_plane_is_behind_the_bound() {
    let inside = DroppedItemCullView::new(Mat4::IDENTITY, Vec3::ZERO).unwrap();
    assert!(!cube().allows_back_face_culling(&inside));
    let outside = DroppedItemCullView::new(Mat4::IDENTITY, Vec3::Z * 2.0).unwrap();
    assert!(cube().allows_back_face_culling(&outside));
    assert!(DroppedItemCullView::new(Mat4::IDENTITY, Vec3::splat(f32::NAN)).is_none());
}
