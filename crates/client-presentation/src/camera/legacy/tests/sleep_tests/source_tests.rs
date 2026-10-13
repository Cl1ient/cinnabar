use super::*;

/// Starts with an awake input pose and a loaded north-facing bed, without an actor tick.
fn pose_app() -> App {
    let mut app = camera_app();
    let collisions = bed_collisions(&mut app, BED, 2);
    app.insert_resource(collisions);
    app.world_mut()
        .resource_mut::<LocalViewPose>()
        .set_subject_position(Vec3::Y * protocol::STANDING_PLAYER_EYE_HEIGHT, Vec3::ZERO);
    app.world_mut()
        .resource_mut::<LocalViewPose>()
        .set_rotation(Quat::from_euler(EulerRot::YXZ, 0.7, -0.4, 0.0));
    app
}

/// Builds the same retained actor feed that presentation adjusts from the shared view.
fn actor_feed(app: &App) -> client_world::LocalPlayerFeed {
    let view = app.world().resource::<LocalViewPose>();
    let mut feed = crate::actor_feed::build_local_player_feed(
        app.world().resource::<TestPhysics>(),
        view.rotation(),
        false,
        false,
        [1; 16],
        || protocol::PlayerSkin::Unavailable(protocol::PlayerSkinUnavailable::InvalidDimensions),
        client_world::LocalItemUse::Unpredicted,
    )
    .unwrap();
    view.apply_actor_presentation(&mut feed);
    feed
}

#[test]
fn loaded_bed_heading_is_ready_on_the_first_metadata_frame_without_actor_ticks() {
    let mut app = pose_app();
    sleep_state(&mut app, 2, true, Some(BED));
    assert_eq!(
        app.world()
            .resource::<TestWorld>()
            .0
            .authority()
            .actor(2)
            .unwrap()
            .status
            .sleep_rotation,
        None
    );
    app.world_mut()
        .run_system_cached(resolve_presentation)
        .unwrap();
    let view = app.world().resource::<LocalViewPose>();
    assert_eq!(
        view.camera_rotation(),
        crate::local_player::sleeping_view_rotation(270.0)
    );
    assert_eq!(
        app.world()
            .resource::<TestWorld>()
            .0
            .authority()
            .actor(2)
            .unwrap()
            .status
            .sleep_rotation,
        Some(270.0)
    );
}

#[test]
fn actor_camera_heading_reads_the_sleeping_presentation_without_replacing_input_look() {
    let mut app = pose_app();
    let input = app.world().resource::<LocalViewPose>().look_rotation();
    sleep_state(&mut app, 2, true, Some(BED));
    app.world_mut()
        .run_system_cached(resolve_presentation)
        .unwrap();
    let view = app.world().resource::<LocalViewPose>();
    assert_ne!(view.camera_rotation(), input);
    assert_eq!(
        view.camera_rotation(),
        crate::local_player::sleeping_view_rotation(270.0)
    );
    assert_eq!(view.look_rotation(), input);
    assert_eq!(view.rotation(), input);
}

#[test]
fn metadata_only_sleep_and_wake_reanchor_once_while_stable_sleep_keeps_history() {
    let mut app = pose_app();
    let start = app
        .world()
        .resource::<LocalViewPose>()
        .camera_reanchor_epoch();
    sleep_state(&mut app, 2, true, Some(BED));
    app.world_mut()
        .run_system_cached(resolve_presentation)
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<LocalViewPose>()
            .camera_reanchor_epoch(),
        start + 1
    );
    app.world_mut()
        .run_system_cached(resolve_presentation)
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<LocalViewPose>()
            .camera_reanchor_epoch(),
        start + 1
    );
    sleep_state(&mut app, 3, false, Some(BED));
    app.world_mut()
        .run_system_cached(resolve_presentation)
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<LocalViewPose>()
            .camera_reanchor_epoch(),
        start + 2
    );
}

#[test]
fn retained_actor_attachment_origin_snaps_to_the_bed_and_back_without_actor_ticks() {
    let mut app = pose_app();
    let mut initial = actor_feed(&app);
    initial.teleported = true;
    app.world_mut()
        .resource_mut::<TestWorld>()
        .0
        .sync_local_player_pose(&initial);
    let revision = app
        .world()
        .resource::<TestWorld>()
        .0
        .authority()
        .actor(2)
        .unwrap()
        .spawn_revision;
    let (_, feet) = crate::local_player::sleeping_subject_position(BED);
    for (sequence, sleeping) in [(2, true), (3, true), (4, false)] {
        sleep_state(&mut app, sequence, sleeping, Some(BED));
        app.world_mut()
            .run_system_cached(resolve_presentation)
            .unwrap();
        {
            let world = app.world().resource::<TestWorld>();
            let actor = world.0.authority().actor(2).unwrap();
            let expected = if sleeping {
                feet.to_array()
            } else {
                initial.position
            };
            assert_eq!(
                actor.position, expected,
                "camera actor-pose readers see the source immediately"
            );
            for alpha in [0.0, 0.5, 1.0] {
                let attachment = crate::presentation::actors::actor_emitter_transform(
                    actor,
                    None,
                    &[],
                    None,
                    alpha,
                )
                .unwrap();
                assert_eq!(
                    attachment.position, expected,
                    "attachments do not wait for actor preparation"
                );
            }
        }
        let feed = actor_feed(&app);
        let mut world = app.world_mut().resource_mut::<TestWorld>();
        world.0.sync_local_player_pose(&feed);
        let actor = world.0.authority().actor(2).unwrap();
        assert_eq!(
            actor.spawn_revision, revision,
            "bound effects keep the same actor identity"
        );
        for alpha in [0.0, 0.5, 1.0] {
            let attachment =
                crate::presentation::actors::actor_emitter_transform(actor, None, &[], None, alpha)
                    .unwrap();
            assert_eq!(
                attachment.position,
                if sleeping {
                    feet.to_array()
                } else {
                    initial.position
                }
            );
        }
    }
}
