//! Exercises the production colour functions on a GPU, including HDR/negative
//! inputs that cannot survive an ordinary clamped export. Run explicitly with
//! `cargo test --lib hsl_shader_tests -- --ignored --nocapture` on a GPU host.

use std::{sync::mpsc, time::Duration};

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

fn run(cases: &[[f32; 36]]) -> Vec<[f32; 4]> {
    let source = include_str!("shaders/shader.wgsl");
    let mut shader = definition(source, "struct", "HslColor");
    shader.push_str(
        &source[source.find("struct HslRange").unwrap()
            ..source.find("@group(0) @binding(0)").unwrap()],
    );
    let luma = source.find("const LUMA_COEFF").unwrap();
    shader.push_str(&source[luma..=luma + source[luma..].find(';').unwrap()]);
    for name in [
        "get_luma",
        "srgb_to_linear",
        "linear_to_srgb_extended",
        "rgb_to_hsv",
        "hsv_to_rgb",
        "get_raw_hsl_influence",
        "apply_hsl_panel",
        "apply_creative_color",
    ] {
        shader.push_str(&definition(source, "fn", name));
    }
    shader.push_str(
        r#"
struct Case { color: vec4<f32>, bands: array<HslColor, 8>, }
@group(0) @binding(0) var<storage, read> cases: array<Case>;
@group(0) @binding(1) var<storage, read_write> outputs: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let c = cases[id.x];
    var rgb = apply_hsl_panel(c.color.rgb, c.bands, vec2<i32>(0));
    if (c.color.a == 1.0) { rgb = apply_creative_color(c.color.rgb, 0.0, 0.1); }
    outputs[id.x] = vec4<f32>(rgb, 1.0);
}
"#,
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Colour regression adapter: {:?}", adapter.get_info());
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
#[ignore = "requires a GPU adapter; run explicitly during rendering validation"]
fn inactive_mixer_preserves_bits_and_active_mixer_preserves_hdr() {
    let colors = [
        [0.0, 0.0, 0.0],
        [-0.0, 0.7, 0.2],
        [-0.1, 0.7, 0.2],
        [-2.0, -1.0, -0.5],
        [0.5, 0.1, 0.9],
        [4.0, 1.0, 0.25],
        [1e-6, 0.001, 0.0001],
        [0.1, 0.1, 0.1],
    ];
    let mut cases: Vec<[f32; 36]> = colors
        .iter()
        .map(|c| {
            let mut data = [0.0; 36];
            data[..3].copy_from_slice(c);
            data
        })
        .collect();
    // An adjustment in the last effective band must not be skipped. This is
    // also the path taken when the global mixer is zero but a mask is active.
    let mut last_band = cases[4];
    last_band[4 + 7 * 4] = 0.1;
    cases.push(last_band);
    let mut hdr = cases[5];
    for band in 0..8 {
        hdr[4 + band * 4] = 0.1;
        hdr[5 + band * 4] = 0.2;
    }
    cases.push(hdr);
    let output = run(&cases);
    for (i, color) in colors.iter().enumerate() {
        for channel in 0..3 {
            assert_eq!(
                output[i][channel].to_bits(),
                color[channel].to_bits(),
                "inactive mixer, case {i}, channel {channel}"
            );
        }
    }
    assert_ne!(&output[8][..3], &last_band[..3]);
    assert!(
        output[9][0] > 1.0,
        "active HDR must not be clamped to display range"
    );
    let luma = |c: &[f32]| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
    assert!((luma(&output[9]) - luma(&hdr)).abs() < 1e-5);
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly during rendering validation"]
fn positive_vibrance_keeps_fully_saturated_colors_finite_and_colored() {
    // Division rounding varies by adapter. Sweep a deterministic saturated
    // gamut instead of relying on one adapter's problematic RGB value.
    let mut state = 81u32;
    let cases: Vec<[f32; 36]> = (0..60000)
        .map(|i| {
            let mut c = [0.0; 36];
            for value in &mut c[..3] {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                *value = 0.01 + (state >> 8) as f32 / 16777216.0 * 0.99;
            }
            c[i % 3] = 0.0;
            c[3] = 1.0;
            c
        })
        .collect();
    for (i, (input, output)) in cases.iter().zip(run(&cases)).enumerate() {
        for channel in 0..3 {
            assert!(output[channel].is_finite(), "case {i}");
            assert!(
                (input[channel] - output[channel]).abs() < 1e-5,
                "positive vibrance must leave saturation=1 unchanged, case {i}: {input:?} -> {output:?}"
            );
        }
    }
}
