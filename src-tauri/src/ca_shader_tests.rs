//! Runs the production manual-CA sampling functions against an affine colour
//! ramp: bilinear samples have an analytic value, unlike rounded coordinates.
use std::{sync::mpsc, time::Duration};

fn function(source: &str, name: &str) -> String {
    let start = source.find(&format!("fn {name}(")).unwrap();
    let brace = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, byte) in source[brace..].bytes().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return source[start..=brace + offset].to_owned();
        }
    }
    panic!("unterminated {name}");
}

#[test]
#[ignore = "requires a GPU adapter; run the pinned test executable under the GPU lock"]
fn manual_ca_samples_fractional_positions_and_preserves_unshifted_channels() {
    let (width, height) = (16u32, 12u32);
    let colour = |x: f32, y: f32| [1.0 + x + 0.25 * y, 2.0 + y, 2.0 * x + 0.5 * y, 1.0];
    let pixels: Vec<[f32; 4]> = (0..height)
        .flat_map(|y| (0..width).map(move |x| colour(x as f32, y as f32)))
        .collect();
    let mut cases = Vec::<[f32; 4]>::new();
    for (red, blue) in [
        (0.0, 0.0),
        (0.0001, 0.0),
        (0.0002, -0.0003),
        (0.025, -0.0125),
        (-0.025, 0.0125),
        (1.0, -1.0),
    ] {
        for (x, y) in [(0, 0), (1, 1), (15, 11), (13, 10), (8, 6)] {
            cases.push([x as f32, y as f32, red, blue]);
        }
    }
    let production = include_str!("shaders/shader.wgsl");
    let mut shader = String::from(
        r#"
@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var<storage, read> cases: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> outputs: array<vec4<f32>>;
"#,
    );
    for name in ["sample_ca_bilinear", "apply_ca_correction"] {
        shader.push_str(&function(production, name));
    }
    shader.push_str(
        r#"
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let c = cases[id.x];
    outputs[id.x] = vec4<f32>(apply_ca_correction(vec2<u32>(c.xy), c.z, c.w), 1.0);
}
"#,
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("CA reference adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&pixels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 16),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let size = (cases.len() * 16) as u64;
    let input = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&input, 0, bytemuck::cast_slice(&cases));
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
    let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            storage(1, true),
            storage(2, false),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let view = texture.create_view(&Default::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output.as_entire_binding(),
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
    let data = slice.get_mapped_range();
    let actual: &[[f32; 4]] = bytemuck::cast_slice(&data);
    for (case, result) in cases.iter().zip(actual) {
        let [x, y, red, blue] = *case;
        let coordinate = |position: f32, center: f32, amount: f32, max: u32| {
            (position - (position - center) * amount).clamp(0.0, max as f32)
        };
        let rx = coordinate(x, 8.0, red, width - 1);
        let ry = coordinate(y, 6.0, red, height - 1);
        let bx = coordinate(x, 8.0, blue, width - 1);
        let by = coordinate(y, 6.0, blue, height - 1);
        let expected = [colour(rx, ry)[0], colour(x, y)[1], colour(bx, by)[2], 1.0];
        for channel in 0..4 {
            assert!(
                (result[channel] - expected[channel]).abs() < 8e-6,
                "{case:?}: {result:?} != {expected:?}"
            );
        }
        assert_eq!(result[1].to_bits(), expected[1].to_bits());
        if red == 0.0 {
            assert_eq!(result[0].to_bits(), colour(x, y)[0].to_bits());
        }
        if blue == 0.0 {
            assert_eq!(result[2].to_bits(), colour(x, y)[2].to_bits());
        }
    }
}
