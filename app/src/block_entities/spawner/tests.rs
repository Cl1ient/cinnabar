use super::*;

/// Builds an authored display with two controller layers and an explicit pack texture.
fn fixture() -> (Pool, chunk_pipeline::WorldStream, ActorArtworkPages) {
    let entity = br#"{"format_version":"1.10.0","minecraft:client_entity":{"description":{"identifier":"custom:display","materials":{"default":"entity"},"textures":{"default":"textures/entity/display"},"geometry":{"default":"geometry.display"},"render_controllers":["controller.render.lit","controller.render.unlit"]}}}"#;
    let geometry = br#"{"format_version":"1.12.0","minecraft:geometry":[{"description":{"identifier":"geometry.display","texture_width":16,"texture_height":16},"bones":[{"name":"body","cubes":[{"origin":[-4,0,-4],"size":[8,16,8],"uv":[0,0]}]}]}]}"#;
    let controllers = br#"{"format_version":"1.8.0","render_controllers":{"controller.render.lit":{"geometry":"Geometry.default","materials":[{"*":"Material.default"}],"textures":["Texture.default"]},"controller.render.unlit":{"geometry":"Geometry.default","materials":[{"*":"Material.default"}],"textures":["Texture.default"],"ignore_lighting":true}}}"#;
    let compiled = pack_compiler::compile_entity_pack(vec![
        ("entity/display.json".into(), entity.to_vec()),
        ("models/entity/display.geo.json".into(), geometry.to_vec()),
        (
            "render_controllers/display.json".into(),
            controllers.to_vec(),
        ),
        (
            "textures/entity/display.png".into(),
            b"synthetic-raster".to_vec(),
        ),
    ])
    .unwrap()
    .unwrap();
    let texture_source = compiled.assets.render.candidates[0].source;
    let source = Arc::new(assets::RuntimeEntityAssets::from_compiled(compiled.assets).unwrap());
    let mut actors = DisplayActorStore::new(1, Arc::clone(&source));
    actors.set_pack(Some((Arc::clone(&source), vec![0])));
    let stream = chunk_pipeline::WorldStream::new(protocol::WorldBootstrap {
        dimension: 1,
        local_player_runtime_id: 1,
        local_player_unique_id: 1,
        player_position: [0.0; 3],
        world_spawn_position: [0; 3],
        air_network_id: protocol::SEQUENTIAL_AIR_NETWORK_ID,
        block_network_ids_are_hashes: false,
    });
    let artwork = ActorArtworkPages::default().with_pack_artwork(
        &[assets::ActorTexture {
            source: texture_source,
            width: 16,
            height: 16,
            pixel_sha256: [0; 32],
            rgba8: vec![255; 16 * 16 * 4].into(),
        }],
        &[assets::ActorArtworkBinding {
            rig: 0,
            geometry_candidate: 0,
            entity_symbol: 0,
            geometry: 0,
            render_controller: 0,
            texture: 0,
            material: "entity".into(),
            pose_mode: assets::ActorPoseMode::CompiledLiteral,
        }],
    );
    let pool = Pool {
        source,
        session: (stream.authority().actor_session_id(), 1),
        pack: None,
        actors,
        clock: Default::default(),
        columns: Default::default(),
        spins: Default::default(),
        poses: Default::default(),
        layers: Default::default(),
    };
    (pool, stream, artwork)
}

/// Publishes one resolved display using the production conversion and controller path.
fn draw(
    pool: &mut Pool,
    stream: &chunk_pipeline::WorldStream,
    artwork: &ActorArtworkPages,
) -> ActorPresentationBatch {
    let position = [0, 512, 0];
    let runtime = pool.actors.resolve(position, "custom:display").unwrap();
    pool.actors.advance(1);
    display_batch(
        pool,
        stream,
        artwork,
        0.5,
        vec![(runtime, position, Default::default())],
    )
}

#[test]
fn regression_retired_display_poses_release_their_bone_allocations() {
    let (mut pool, stream, artwork) = fixture();
    let position = [0, 512, 0];
    let runtime = pool.actors.resolve(position, "custom:display").unwrap();
    pool.actors.advance(1);
    let presentation = entity_rig_presentation_cached(
        &pool.actors.rig(runtime).unwrap(),
        pool.actors.actor(runtime).unwrap(),
        &artwork,
        0.5,
        Some(&mut pool.poses),
    )
    .unwrap();
    let bones = Arc::downgrade(&presentation.submission.input.current_bones);
    drop(presentation);
    pool.actors.retain(&BTreeSet::new());
    assert!(
        bones.upgrade().is_some(),
        "recently drawn poses remain cached"
    );
    for _ in 0..256 {
        display_batch(&mut pool, &stream, &artwork, 0.5, Vec::new());
    }
    assert!(
        bones.upgrade().is_none(),
        "retired display poses must be released"
    );
    assert!(!draw(&mut pool, &stream, &artwork).submissions.is_empty());
}

#[test]
fn regression_display_controller_lighting_overrides_survive_solved_world_light() {
    let (mut pool, stream, artwork) = fixture();
    // An out-of-height sample supplies resolved darkness without a terrain fixture.
    assert_eq!(stream.solved_light_at([0.5, 512.5, 0.5]), Some((0, 0)));
    let batch = draw(&mut pool, &stream, &artwork);
    assert_eq!(batch.submissions.len(), 2);
    assert_eq!(batch.submissions[0].light, render::pack_actor_light(0, 0));
    assert_eq!(
        batch.submissions[1].light,
        render::pack_actor_light_without_lightmap()
    );
}
