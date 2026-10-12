use super::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::render::render_phase::{DrawFunctionId, SortedRenderPhase};
use particles::DrawLists;

/// Uploads and queues all material lists through the production systems.
fn queue_lists(app: &mut App, frame: ParticleGpuFrame) {
    app.init_resource::<ParticlePipeline>()
        .add_render_command::<Transparent3d, DrawParticles<{ ParticleMode::Opaque as u8 }>>()
        .add_render_command::<Transparent3d, DrawParticles<{ ParticleMode::Blend as u8 }>>()
        .add_render_command::<Transparent3d, DrawParticles<{ ParticleMode::Add as u8 }>>();
    app.world_mut().run_system_once(init_particle_gpu).unwrap();
    app.insert_resource(frame);
    app.world_mut()
        .run_system_once(prepare_particle_resources)
        .unwrap();
    app.world_mut().run_system_once(queue_particles).unwrap();
}

/// The extracted frame for camera-sorted draw lists.
fn frame(lists: DrawLists) -> ParticleGpuFrame {
    ParticleGpuFrame {
        opaque: lists.opaque.into(),
        blend: lists.blend.into(),
        add: lists.add.into(),
        ..Default::default()
    }
}

/// Queues a depth-writing particle through the production upload and queue systems.
pub(crate) fn queue_opaque(app: &mut App) -> (DrawFunctionId, RenderPipelineDescriptor) {
    queue_lists(
        app,
        ParticleGpuFrame {
            opaque: vec![ParticleInstance::default()].into(),
            ..Default::default()
        },
    );
    let draw = app
        .world()
        .resource::<DrawFunctions<Transparent3d>>()
        .read()
        .id::<DrawParticles<{ ParticleMode::Opaque as u8 }>>();
    let id = app
        .world()
        .resource::<ViewSortedRenderPhases<Transparent3d>>()
        .values()
        .flat_map(|phase| phase.items.values())
        .find(|item| item.draw_function == draw)
        .unwrap()
        .pipeline;
    let mut cache = app.world_mut().resource_mut::<PipelineCache>();
    (
        draw,
        crate::queue_review_support::queued_descriptor(&mut cache, id).clone(),
    )
}

#[test]
fn additive_particles_compose_at_their_depth_with_only_cutout_particles_present() {
    for distance in [-10.0, -2.0] {
        let (mut app, view) = crate::queue_review_support::app();
        queue_lists(
            &mut app,
            ParticleGpuFrame {
                opaque: vec![ParticleInstance {
                    center_light: [100.0, 0.0, -1.0, 0.0],
                    ..Default::default()
                }]
                .into(),
                add: vec![ParticleInstance {
                    center_light: [0.0, 0.0, distance, 0.0],
                    ..Default::default()
                }]
                .into(),
                ..Default::default()
            },
        );
        let functions = app
            .world()
            .resource::<DrawFunctions<Transparent3d>>()
            .read();
        let add = functions.id::<DrawParticles<{ ParticleMode::Add as u8 }>>();
        let terrain = functions.id::<DrawParticles<{ ParticleMode::Blend as u8 }>>();
        drop(functions);
        let mut phase = std::mem::take(
            app.world_mut()
                .resource_mut::<ViewSortedRenderPhases<Transparent3d>>()
                .get_mut(&view)
                .unwrap(),
        );
        let item = phase
            .items
            .values()
            .find(|item| item.draw_function == add)
            .unwrap();
        let extra = Transparent3d {
            sorting_info: bevy::core_pipeline::core_3d::TransparentSortingInfo3d::AlwaysOnTop,
            entity: item.entity,
            pipeline: item.pipeline,
            draw_function: terrain,
            distance: -5.0,
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::None,
            indexed: false,
        };
        crate::transparent_phase::add(&mut phase, extra);
        phase.sort();
        let items = phase.items.into_values();
        let mut pixel = 0.2;
        let mut depth = 0.0;
        for item in items {
            if item.draw_function == terrain {
                pixel = 0.1 * 0.5 + pixel * 0.5;
                depth = 0.5;
            } else if item.draw_function == add {
                let particle_depth = if distance < -5.0 { 0.25 } else { 0.75 };
                if particle_depth >= depth {
                    pixel += 0.6;
                }
            }
        }
        let expected: f32 = if distance < -5.0 { 0.45 } else { 0.75 };
        assert!(
            (pixel - expected).abs() < 0.0001,
            "additive color must compose through translucent terrain at its own depth"
        );
    }
}

#[test]
fn blended_and_additive_particles_keep_their_translucent_pipeline() {
    let (mut app, _) = crate::queue_review_support::app();
    queue_opaque(&mut app);
    for material in [ParticleMode::Blend, ParticleMode::Add] {
        let world = app.world_mut();
        let id = world.resource_scope(|world, mut pipeline: Mut<ParticlePipeline>| {
            pipeline
                .variants
                .specialize(
                    world.resource::<PipelineCache>(),
                    ParticlePipelineKey {
                        msaa: Msaa::Off,
                        hdr: false,
                        material,
                    },
                )
                .unwrap()
        });
        let mut cache = world.resource_mut::<PipelineCache>();
        let descriptor = crate::queue_review_support::queued_descriptor(&mut cache, id);
        assert_eq!(
            descriptor.fragment.as_ref().unwrap().targets[0]
                .as_ref()
                .unwrap()
                .write_mask,
            ColorWrites::COLOR
        );
        assert_eq!(
            descriptor
                .depth_stencil
                .as_ref()
                .unwrap()
                .depth_write_enabled,
            Some(false)
        );
        assert!(
            descriptor.fragment.as_ref().unwrap().targets[0]
                .as_ref()
                .unwrap()
                .blend
                .is_some()
        );
    }
}

#[test]
fn particles_on_both_sides_of_glass_compose_in_depth_order() {
    for additive in [false, true] {
        let (mut app, view) = crate::queue_review_support::app();
        let particles = [-10.0, -2.0]
            .into_iter()
            .zip([0.4, 0.6])
            .map(|(z, red)| ParticleInstance {
                center_light: [0.0, 0.0, z, 0.0],
                color: [red, 0.0, 0.0, 0.5],
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let lists = if additive {
            DrawLists {
                add: particles.clone(),
                ..Default::default()
            }
        } else {
            DrawLists {
                blend: particles.clone(),
                ..Default::default()
            }
        };
        queue_lists(&mut app, frame(lists));
        let functions = app
            .world()
            .resource::<DrawFunctions<Transparent3d>>()
            .read();
        let blend = functions.id::<DrawParticles<{ ParticleMode::Blend as u8 }>>();
        let add = functions.id::<DrawParticles<{ ParticleMode::Add as u8 }>>();
        drop(functions);
        let phases = &mut *app
            .world_mut()
            .resource_mut::<ViewSortedRenderPhases<Transparent3d>>();
        let phase = phases.get_mut(&view).unwrap();
        add_glass(phase, blend);
        let len = phase.items.len();
        coalesce_ranges(phase.items.as_mut_slice(), len, [blend, add]);
        let ordered: Vec<_> = phase
            .items
            .values()
            .filter(|item| !item.batch_range.is_empty())
            .map(|item| {
                if item.distance == -5.0 {
                    ("glass", 0..1)
                } else {
                    let range = match &item.extra_index {
                        PhaseItemExtraIndex::IndirectParametersIndex { range, .. } => range.clone(),
                        _ => 0..particles.len() as u32,
                    };
                    ("particle", range)
                }
            })
            .collect();
        eprintln!(
            "PARTICLE_PHASE {} {}",
            if additive { "add" } else { "blend" },
            serde_json::to_string(&ordered).unwrap()
        );
        let mut pixel = 0.0;
        for (kind, range) in &ordered {
            if *kind == "glass" {
                pixel = 0.1 * 0.5 + pixel * 0.5;
            } else {
                for particle in &particles[range.start as usize..range.end as usize] {
                    pixel = particle.color[0] * 0.5 + if additive { pixel } else { pixel * 0.5 };
                }
            }
        }
        let expected = if additive { 0.45 } else { 0.375 };
        assert!(
            (pixel - expected).abs() < 0.0001,
            "both particle depths must compose across the glass"
        );
    }
}

#[test]
fn adjacent_particle_instances_share_a_draw_without_crossing_glass() {
    let (mut app, view) = crate::queue_review_support::app();
    queue_lists(
        &mut app,
        frame(DrawLists {
            blend: [-12.0, -10.0, -3.0, -2.0]
                .map(|z| ParticleInstance {
                    center_light: [0.0, 0.0, z, 0.0],
                    ..Default::default()
                })
                .to_vec(),
            ..Default::default()
        }),
    );
    let functions = app
        .world()
        .resource::<DrawFunctions<Transparent3d>>()
        .read();
    let blend = functions.id::<DrawParticles<{ ParticleMode::Blend as u8 }>>();
    let add = functions.id::<DrawParticles<{ ParticleMode::Add as u8 }>>();
    drop(functions);
    let phases = &mut *app
        .world_mut()
        .resource_mut::<ViewSortedRenderPhases<Transparent3d>>();
    let phase = phases.get_mut(&view).unwrap();
    add_glass(phase, blend);
    let len = phase.items.len();
    coalesce_ranges(phase.items.as_mut_slice(), len, [blend, add]);
    let ranges = phase
        .items
        .values()
        .filter(|item| !item.batch_range.is_empty())
        .filter_map(|item| match &item.extra_index {
            PhaseItemExtraIndex::IndirectParametersIndex { range, .. } => Some(range.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ranges, [0..2, 2..4]);
}

#[test]
fn queued_particle_ranges_use_the_extracted_frame_before_gpu_preparation() {
    let (mut app, view) = crate::queue_review_support::app();
    queue_lists(
        &mut app,
        frame(DrawLists {
            blend: vec![ParticleInstance::default()],
            ..Default::default()
        }),
    );
    crate::queue_review_support::clear(&mut app, view);
    app.insert_resource(frame(DrawLists {
        opaque: vec![ParticleInstance::default()],
        blend: [-10.0, -8.0, -4.0, -2.0]
            .map(|z| ParticleInstance {
                center_light: [0.0, 0.0, z, 0.0],
                ..Default::default()
            })
            .to_vec(),
        ..Default::default()
    }));
    app.world_mut().run_system_once(queue_particles).unwrap();
    let mut ranges: Vec<_> = crate::queue_review_support::items(&app, view)
        .iter()
        .filter_map(|item| match &item.extra_index {
            PhaseItemExtraIndex::IndirectParametersIndex { range, .. } => Some(range.clone()),
            _ => None,
        })
        .collect();
    ranges.sort_by_key(|range| range.start);
    assert_eq!(ranges, [1..2, 2..3, 3..4, 4..5]);
}

/// Adds a translucent terrain draw between the fixture's particles, then sorts the phase.
fn add_glass(phase: &mut SortedRenderPhase<Transparent3d>, glass: DrawFunctionId) {
    let first = phase.items.values().next().unwrap();
    let (entity, pipeline) = (first.entity, first.pipeline);
    crate::transparent_phase::add(
        phase,
        Transparent3d {
            sorting_info: bevy::core_pipeline::core_3d::TransparentSortingInfo3d::AlwaysOnTop,
            entity,
            pipeline,
            draw_function: glass,
            distance: -5.0,
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::None,
            indexed: false,
        },
    );
    phase.sort();
}
