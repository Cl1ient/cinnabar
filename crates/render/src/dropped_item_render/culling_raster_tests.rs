use super::*;
use crate::gpu_snapshot::{self, Draw, Gpu, RasterState};
use bevy::math::Vec3;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Invokes the production vertex shader with the builders' unchanged vertex data.
const VERTEX: &str = r#"
@group(0) @binding(22) var<storage, read> mesh: array<u32>;
@vertex fn fixture_vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
    let base = index * 10u;
    var input: VertexInput;
    input.position = vec3(bitcast<f32>(mesh[base]), bitcast<f32>(mesh[base+1u]), bitcast<f32>(mesh[base+2u]));
    input.uv = vec2(bitcast<f32>(mesh[base+3u]), bitcast<f32>(mesh[base+4u]));
    input.normal = vec3(bitcast<f32>(mesh[base+5u]), bitcast<f32>(mesh[base+6u]), bitcast<f32>(mesh[base+7u]));
    input.layer = mesh[base+8u];
    input.color = unpack4x8unorm(mesh[base+9u]);
    input.row_0 = vec4(1.0,0.0,0.0,0.0);
    input.row_1 = vec4(0.0,1.0,0.0,0.0);
    input.row_2 = vec4(0.0,0.0,1.0,0.0);
    input.packed = vec4(0u,15u,15u,0u);
    return item_vertex(input);
}
"#;

/// Draws one model through the real item shader and requested cull/coverage mode.
fn raster(gpu: &Gpu, model: &DroppedItemModel, eye: Vec3, samples: u32, cull: bool) -> Vec<u8> {
    let side = 32;
    let mut atlas = vec![255; side * side * 4];
    let vertices = build_model(&mut atlas, model, side as u32).unwrap();
    let mesh = gpu.words(bytemuck::cast_slice(&vertices), wgpu::BufferUsages::STORAGE);
    let texture = gpu.device.create_texture_with_data(
        &gpu.queue,
        &wgpu::TextureDescriptor {
            label: Some("owned dropped item culling fixture"),
            size: wgpu::Extent3d {
                width: side as u32,
                height: side as u32,
                depth_or_array_layers: (atlas.len() / (side * side * 4)) as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &atlas,
    );
    let atlas = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..default()
    });
    let sampler = gpu
        .device
        .create_sampler(&wgpu::SamplerDescriptor::default());
    let center = if matches!(model, DroppedItemModel::NativeSprite(_)) {
        Vec3::new(0.0, 0.5, 0.0)
    } else {
        Vec3::ZERO
    };
    let eye = center + eye;
    let up = if (eye - center).normalize().dot(Vec3::Y).abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let matrix = glam::camera::rh::proj::directx::perspective_infinite_reverse(0.65, 1.0, 0.05)
        * glam::camera::rh::view::look_at_mat4(eye, center, up);
    let view = gpu.buffer(
        &gpu_snapshot::view(matrix, eye),
        wgpu::BufferUsages::UNIFORM,
    );
    let light = gpu.buffer(&[1.0; 256 * 4], wgpu::BufferUsages::UNIFORM);
    let atmosphere = gpu.buffer(&[0.0; 32], wgpu::BufferUsages::UNIFORM);
    let bindings = [
        wgpu::BindGroupEntry {
            binding: 0,
            resource: view.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(&atlas),
        },
        wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::Sampler(&sampler),
        },
        wgpu::BindGroupEntry {
            binding: 20,
            resource: light.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 21,
            resource: atmosphere.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 22,
            resource: mesh.as_entire_binding(),
        },
    ];
    let mut source = crate::shader_source::standalone(
        include_str!("../dropped_item.wesl"),
        if samples > 1 {
            &[crate::alpha_coverage::SHADER_DEF]
        } else {
            &[]
        },
    )
    .replace("@vertex\nfn item_vertex", "fn item_vertex")
    .replace("@group(1) @binding(0)", "@group(0) @binding(20)")
    .replace("@group(1) @binding(1)", "@group(0) @binding(21)");
    source.push_str(VERTEX);
    gpu.render_srgb_with_state(
        &source,
        "fixture_vertex",
        &[Draw {
            fragment: "item_fragment",
            vertices: 0..vertices.len() as u32,
            bindings: &bindings,
            blend: None,
            write_depth: true,
        }],
        RasterState {
            primitive: wgpu::PrimitiveState {
                cull_mode: cull.then_some(wgpu::Face::Back),
                ..default()
            },
            multisample: wgpu::MultisampleState {
                count: samples,
                alpha_to_coverage_enabled: samples > 1,
                ..default()
            },
            ..default()
        },
    )
}

/// Produces a colored binary-alpha sprite with a central hole and distinct edge texels.
fn sprite() -> render_model::DroppedItemSprite {
    let mut pixels: Vec<u8> = (0..16)
        .flat_map(|i| [40 + i * 10, 200 - i * 5, 80, 255])
        .collect();
    for i in [5, 6, 9, 10] {
        pixels[i * 4 + 3] = 0;
    }
    render_model::DroppedItemSprite {
        width: 4,
        height: 4,
        rgba8: pixels.into(),
    }
}

#[test]
fn closed_item_culling_preserves_pixels_from_faces_edges_and_corners() {
    let Some(gpu) = Gpu::for_fixture("dropped item culling pixels") else {
        return;
    };
    let sprite = sprite();
    let cube = render_model::DroppedItemCube {
        tile: 1,
        faces: std::array::from_fn(|i| Arc::from([60 + i as u8 * 25, 150, 70, 255])),
        tints: [render_model::OPAQUE_WHITE; 6],
    };
    let quads: Vec<_> = cube_mesh([0; 6], [render_model::OPAQUE_WHITE; 6], 1, 1)
        .unwrap()
        .as_chunks::<6>()
        .0
        .iter()
        .map(|face| {
            let order = if face[1].position == face[5].position {
                [0, 4, 1, 2]
            } else {
                [0, 1, 2, 5]
            };
            assets::ModelQuad {
                positions: order.map(|i| face[i].position.map(|p| ((p + 0.5) * 256.0) as i16)),
                uvs: [[0, 0], [4096, 0], [4096, 4096], [0, 4096]],
                material: 0,
                flags: 0,
            }
        })
        .collect();
    let block = render_model::DroppedItemBlock {
        materials: Arc::from([(
            render_model::DroppedItemSprite {
                width: 1,
                height: 1,
                rgba8: Arc::from([70, 180, 90, 255]),
            },
            render_model::OPAQUE_WHITE,
        )]),
        quads: quads.into(),
        rotation: 1,
    };
    let models = [
        DroppedItemModel::Sprite(sprite.clone()),
        DroppedItemModel::NativeSprite(sprite),
        DroppedItemModel::Cube(cube),
        DroppedItemModel::Block(block),
    ];
    for (index, model) in models.iter().enumerate() {
        assert!(model.can_cull_back_faces());
        for samples in [1, 4] {
            for (angle, eye) in [
                Vec3::X,
                -Vec3::X,
                Vec3::Y,
                -Vec3::Y,
                Vec3::Z,
                -Vec3::Z,
                Vec3::new(1.0, 0.3, 1.0),
                Vec3::new(-1.0, 0.3, -1.0),
                Vec3::new(0.3, -1.0, 0.4),
            ]
            .into_iter()
            .enumerate()
            {
                let before = raster(&gpu, model, eye.normalize() * 2.5, samples, false);
                // Multisampled passes retain rear faces for their partial edge coverage.
                let after = raster(&gpu, model, eye.normalize() * 2.5, samples, samples == 1);
                assert!(
                    before.as_chunks::<4>().0.iter().any(|p| p != &before[..4]),
                    "empty fixture {index}/{angle}/{samples}"
                );
                let differences = before.iter().zip(&after).filter(|(a, b)| a != b).count();
                assert_eq!(
                    differences, 0,
                    "model {index}, angle {angle}, samples {samples}"
                );
                if angle == 6 && samples == 1 && index == 2 {
                    gpu_snapshot::save("dropped-item-culling-before", &before);
                    gpu_snapshot::save("dropped-item-culling-after", &after);
                }
            }
        }
    }
}
