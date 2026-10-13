use super::*;

/// Places a spawned player's current and previous feet at one stable point.
fn placed_actor(runtime_id: u64, position: Vec3) -> ActorSnapshot {
    let mut actor = actor();
    actor.runtime_id = runtime_id;
    actor.unique_id = runtime_id as i64;
    actor.position = position.to_array();
    actor.previous_pose.position = actor.position;
    actor.received_pose.position = actor.position;
    actor
}

#[test]
fn third_person_labels_sort_by_camera_depth_instead_of_viewer_distance() {
    let camera = NametagView {
        viewer_position: Vec3::ZERO,
        camera_position: Vec3::new(0.0, 1.62, 4.0),
        camera_forward: Vec3::NEG_Z,
    };
    let boards = ui::ScoreboardStore::default();
    let far = extract_nametag(
        &placed_actor(1, Vec3::Z),
        camera,
        None,
        "AB".into(),
        &boards,
        1.0,
    )
    .unwrap();
    let near = extract_nametag(
        &placed_actor(2, Vec3::Z * 3.0),
        camera,
        None,
        "AB".into(),
        &boards,
        1.0,
    )
    .unwrap();
    assert_eq!(far.view_depth, 3.0);
    assert_eq!(near.view_depth, 1.0);
    let far_anchor = far.position.to_array();
    let scene = scene(&[near, far]);
    let mut pixel = 0.0;
    // Both plates cover the sample, with far glyph ink beneath a blank near glyph.
    for record in &scene.records {
        if record.text != 0 && record.anchor != far_anchor {
            continue;
        }
        let alpha = record.color[3];
        pixel = record.color[0] * alpha + pixel * (1.0 - alpha);
    }
    assert!((pixel - 0.75).abs() < 1e-6);
}

#[test]
fn camera_depth_ignores_sideways_distance_from_the_view_axis() {
    let camera = view(Vec3::ZERO);
    let boards = ui::ScoreboardStore::default();
    let offset = Vec3::new(10.0, 0.0, -2.0);
    let centered = Vec3::new(0.0, 0.0, -4.0);
    let near = extract_nametag(
        &placed_actor(1, offset),
        camera,
        None,
        "AB".into(),
        &boards,
        1.0,
    )
    .unwrap();
    let far = extract_nametag(
        &placed_actor(2, centered),
        camera,
        None,
        "AB".into(),
        &boards,
        1.0,
    )
    .unwrap();
    assert!(offset.length_squared() > centered.length_squared());
    assert_eq!(near.view_depth, 2.0);
    assert_eq!(far.view_depth, 4.0);
    let scene = scene(&[near, far]);
    assert_eq!(scene.records[0].anchor[2], centered.z);
}

#[test]
fn camera_changes_refresh_cached_depth_without_changing_viewer_score_range() {
    let mut actor = placed_actor(1, Vec3::Z);
    actor
        .metadata
        .insert(METADATA_SCORE, ActorMetadataValue::String("7 wins".into()));
    let boards = ui::ScoreboardStore::default();
    let mut cache = NametagTextCache::default();
    let name = Arc::from("AB");
    let first_view = NametagView {
        viewer_position: Vec3::ZERO,
        camera_position: Vec3::Z * 100.0,
        camera_forward: Vec3::NEG_Z,
    };
    let first = cache
        .extract(&actor, first_view, None, Arc::clone(&name), &boards, 1.0)
        .unwrap();
    assert_eq!(first.view_depth, 99.0);
    assert_eq!(first.lines.len(), 2);
    let turned = NametagView {
        camera_position: Vec3::X * 4.0,
        camera_forward: Vec3::NEG_X,
        ..first_view
    };
    let next = cache
        .extract(&actor, turned, None, Arc::clone(&name), &boards, 1.0)
        .unwrap();
    assert_eq!(next.view_depth, 4.0);
    assert!(Arc::ptr_eq(&first.lines, &next.lines));
    let viewer_out_of_range = NametagView {
        viewer_position: Vec3::NEG_Z * 64.0,
        ..turned
    };
    assert!(
        cache
            .extract(&actor, viewer_out_of_range, None, name, &boards, 1.0)
            .is_none()
    );
}

#[test]
fn nearer_plates_attenuate_farther_text_in_every_sneak_combination() {
    for far_sneaking in [false, true] {
        for near_sneaking in [false, true] {
            let mut far = anchor("AB");
            far.runtime_id = 2;
            far.position.z = -8.0;
            far.view_depth = 8.0;
            far.depth_tested = far_sneaking;
            far.text_alpha = if far_sneaking { DIM_TEXT_ALPHA } else { 1.0 };
            let mut near = anchor("AB");
            near.position.z = -4.0;
            near.view_depth = 4.0;
            near.depth_tested = near_sneaking;
            near.text_alpha = if near_sneaking { DIM_TEXT_ALPHA } else { 1.0 };
            let scene = scene(&[near, far]);
            let mut pixel = 0.0;
            let mut depth = f32::INFINITY;
            // Both plates cover this pixel; the far glyph has ink and the near glyph is blank.
            for record in &scene.records {
                let coverage = if record.text != 0 && record.anchor[2] == -4.0 {
                    0.0
                } else {
                    1.0
                };
                let alpha = record.color[3] * coverage;
                let tested = record.depth_tested != 0;
                let at = -record.anchor[2];
                if alpha <= render_model::NAMETAG_ALPHA_CUTOFF || (tested && at > depth) {
                    continue;
                }
                pixel = record.color[0] * alpha + pixel * (1.0 - alpha);
                if tested && record.text != 0 {
                    depth = at;
                }
            }
            let far_ink = if far_sneaking { DIM_TEXT_ALPHA } else { 1.0 };
            assert!(
                (pixel - far_ink * 0.75).abs() < 1e-6,
                "far sneak {far_sneaking}, near sneak {near_sneaking}: {pixel}"
            );
        }
    }
}
