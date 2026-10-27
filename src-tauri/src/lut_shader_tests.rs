//! Runs the production LUT coordinate and tetrahedral sampling functions on a GPU.

use std::{sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;

fn definition(source: &str, kind: &str, name: &str) -> String {
    let start = source.find(&format!("{kind} {name}")).unwrap();
    let brace = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, byte) in source[brace..].bytes().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return format!("{}\n", &source[start..=brace + offset]);
        }
    }
    panic!("unterminated {name}");
}

fn run(cases: &[[f32; 12]]) -> Vec<[f32; 4]> {
    let source = include_str!("shaders/shader.wgsl");
    let mut shader = definition(source, "fn", "lut_domain_coordinates");
    shader.push_str(&definition(source, "fn", "sample_lut_tetrahedral"));
    shader.push_str(
        r#"
struct Case { color: vec4<f32>, domain_min: vec4<f32>, domain_max: vec4<f32>, }
@group(0) @binding(0) var<storage, read> cases: array<Case>;
@group(0) @binding(1) var<storage, read_write> outputs: array<vec4<f32>>;
@group(0) @binding(2) var lut_texture: texture_3d<f32>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let c = cases[id.x];
    let lut_color = sample_lut_tetrahedral(c.color.rgb, c.domain_min.xyz, c.domain_max.xyz);
    outputs[id.x] = vec4<f32>(mix(c.color.rgb, lut_color, c.color.a), 1.0);
}
"#,
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("LUT regression adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let data: &[u8] = bytemuck::cast_slice(cases);
    let input = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: data.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&input, 0, data);
    let size = (cases.len() * 16) as u64;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let texels: Vec<half::f16> = (0..8)
        .flat_map(|i| {
            [
                half::f16::from_f32((i & 1) as f32),
                half::f16::from_f32(((i >> 1) & 1) as f32),
                half::f16::from_f32(((i >> 2) & 1) as f32),
                half::f16::ONE,
            ]
        })
        .collect();
    let texture = device.create_texture_with_data(
        &queue,
        &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 2,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::MipMajor,
        bytemuck::cast_slice(&texels),
    );
    let view = texture.create_view(&Default::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&view),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(cases.len() as u32, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &read, 0, size);
    queue.submit([encoder.finish()]);
    let slice = read.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
    let bytes = slice.get_mapped_range();
    bytes
        .as_chunks::<16>()
        .0
        .iter()
        .map(|c| {
            std::array::from_fn(|i| f32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().unwrap()))
        })
        .collect()
}

#[test]
#[ignore = "requires a GPU adapter; run pinned binary under GPU lock"]
fn production_lut_sampler_normalizes_once_and_preserves_intensity() {
    let cases = [
        [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0, 0.0],
        [0.0, 1.0, 3.0, 1.0, -1.0, 0.0, 2.0, 0.0, 1.0, 4.0, 6.0, 0.0],
        [
            -3.0, 10.0, 6.0, 1.0, -1.0, 0.0, 2.0, 0.0, 1.0, 4.0, 6.0, 0.0,
        ],
        [1.0, 1.0, 1.0, 0.5, 0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0, 0.0],
        [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0, 0.0],
        [
            0.125, 0.75, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0,
        ],
    ];
    let expected = [
        [0.5; 3],
        [0.5, 0.25, 0.25],
        [0.0, 1.0, 1.0],
        [0.75; 3],
        [1.0; 3],
        [0.125, 0.75, 0.5],
    ];
    for (actual, expected) in run(&cases).iter().zip(expected) {
        for channel in 0..3 {
            assert_eq!(actual[channel], expected[channel]);
        }
    }
}
