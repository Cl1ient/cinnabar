use super::*;

/// Supplies a client-owned player without any server-authored actor metadata.
fn local_feed() -> client_world::LocalPlayerFeed {
    client_world::LocalPlayerFeed {
        game_mode: None,
        prefer_client_skin: false,
        uuid: [0; 16],
        username: "Player".into(),
        skin: protocol::PlayerSkin::Standard(render_api::StandardSkin {
            geometry: None,
            cape: None,
            width: 64,
            height: 64,
            rgba8: vec![0; 64 * 64 * 4].into(),
        }),
        position: [0.0; 3],
        velocity: [0.0; 3],
        on_ground: true,
        flying: false,
        gliding: false,
        fall_fly_ticks: 0,
        yaw: 0.0,
        head_yaw: 0.0,
        pitch: 0.0,
        main_hand: None,
        off_hand: None,
        held_network_ids: [0; 2],
        main_hand_metadata: 0,
        main_hand_stack_id: None,
        main_hand_slot: 0,
        bedrock_swing_ticks: actor_animation::ACTOR_SWING_TICKS,
        java_swing_ticks: actor_animation::ACTOR_SWING_TICKS,
        teleported: false,
        first_person: false,
        view_bobbing: true,
        sneaking: false,
        sprinting: false,
        item_use: Default::default(),
    }
}

#[test]
fn client_generated_flags_allow_own_tag_extraction_and_preserve_server_visibility() {
    let mut stream = actor_stream(0, None, 1);
    let mut feed = local_feed();
    let boards = ui::ScoreboardStore::default();
    stream.sync_local_player_pose(&feed);
    let own_tag = |stream: &chunk_pipeline::WorldStream| {
        let authority = stream.authority();
        let actor = authority
            .actor(authority.local_player_runtime_id())
            .unwrap();
        extract_nametag(
            actor,
            view(Vec3::ZERO),
            None,
            authority.actor_display_name(actor.unique_id).unwrap(),
            &boards,
            1.0,
        )
    };
    assert!(own_tag(&stream).is_some());
    assert!(
        project_nametags(
            &mut NametagTextCache::default(),
            &boards,
            &stream,
            view(Vec3::ZERO),
            None,
            1.0,
            true,
        )
        .is_empty()
    );
    feed.sneaking = true;
    stream.sync_local_player_pose(&feed);
    assert!(own_tag(&stream).unwrap().depth_tested);
    for (index, (flags, expected)) in [
        (0, false),
        (
            (1 << ACTOR_FLAG_SHOW_NAME) | (1 << ACTOR_FLAG_INVISIBLE),
            false,
        ),
        (1 << ACTOR_FLAG_SHOW_NAME, true),
    ]
    .into_iter()
    .enumerate()
    {
        stream
            .submit(
                index as u64 + 1,
                protocol::WorldEvent::Actor(protocol::ActorEvent::Metadata(
                    protocol::ActorMetadataUpdateEvent {
                        dimension: 0,
                        runtime_id: 1,
                        metadata: Arc::from([protocol::ActorMetadata {
                            key: 0,
                            value: ActorMetadataValue::Flags(flags),
                        }]),
                        properties: Arc::from([]),
                        tick: 0,
                    },
                )),
            )
            .unwrap();
        stream.poll([0.0; 3], 0);
        stream.sync_local_player_pose(&feed);
        assert_eq!(own_tag(&stream).is_some(), expected);
    }
}
