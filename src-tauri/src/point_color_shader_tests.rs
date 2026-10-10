//! Executes production Point Color WGSL on a GPU: exact no-ops, signed shifts,
//! eight overlapping ranges, near-greys and hue wrapping. Run the ignored
//! test under the shared build lock on a GPU host.

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

fn run(cases: &[[f32; 100]]) -> Vec<[f32; 4]> {
    let source = include_str!("shaders/shader.wgsl");
    let mut shader = definition(source, "struct", "PointColor");
    for name in [
        "point_rgb_to_lab",
        "point_lab_to_rgb",
        "point_axis_weight",
        "point_hue_distance",
        "apply_point_color",
    ] {
        shader.push_str(&definition(source, "fn", name));
    }
    shader.push_str(
        r#"
struct Case { color: vec4<f32>, points: array<PointColor, 8>, }
@group(0) @binding(0) var<storage, read> cases: array<Case>;
@group(0) @binding(1) var<storage, read_write> outputs: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let c = cases[id.x];
    outputs[id.x] = vec4<f32>(apply_point_color(c.color.rgb, c.points), 1.0);
}
"#,
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Point Color regression adapter: {:?}", adapter.get_info());
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

fn case(rgb: [f32; 3], points: [crate::point_color::GpuPointColor; 8]) -> [f32; 100] {
    let mut case = [0.0; 100];
    case[..3].copy_from_slice(&rgb);
    case[4..].copy_from_slice(bytemuck::cast_slice(&points));
    case
}
fn lab_to_rgb(lightness: f32, chroma: f32, hue: f32) -> [f32; 3] {
    let lightness = f64::from(lightness);
    let chroma = f64::from(chroma);
    let hue = f64::from(hue);
    let a = chroma * hue.to_radians().cos();
    let b = chroma * hue.to_radians().sin();
    let l = (lightness + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (lightness - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (lightness - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    let result = [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ];
    result.map(|value| value as f32)
}
fn rgb_to_lch(rgb: &[f32]) -> [f32; 3] {
    let rgb = [f64::from(rgb[0]), f64::from(rgb[1]), f64::from(rgb[2])];
    let l = (0.4122214708 * rgb[0] + 0.5363325363 * rgb[1] + 0.0514459929 * rgb[2]).cbrt();
    let m = (0.2119034982 * rgb[0] + 0.6806995451 * rgb[1] + 0.1073969566 * rgb[2]).cbrt();
    let s = (0.0883024619 * rgb[0] + 0.2817188376 * rgb[1] + 0.6299787005 * rgb[2]).cbrt();
    let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
    let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
    let result = [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        a.hypot(b),
        b.atan2(a).to_degrees(),
    ];
    result.map(|value| value as f32)
}
#[test]
#[ignore = "requires a working GPU; run under the shared build lock"]
fn production_shader_noop_wrap_and_monotonic_shifts() {
    use crate::point_color::{GpuPointColor, parse};
    use serde_json::json;
    let zero = [GpuPointColor::default(); 8];
    let mut cases: Vec<_> = [[-0.1, 0.5, 2.0], [0.18, 0.18, 0.18], [2.0, 3.0, 4.0]]
        .into_iter()
        .map(|rgb| case(rgb, zero))
        .collect();
    let inactive = parse(&json!([{"color":{"lightness":0.7,"chroma":0.1,"hue":60}}]));
    cases.push(case([0.5, 0.2, 0.1], inactive));
    for axis in [0, 1, 2] {
        for shift in [-100, 100] {
            let key = ["luminanceShift", "saturationShift", "hueShift"][axis];
            let mut point = json!({"color":{"lightness":0.7,"chroma":0.15,"hue":60},"hueRange":45,"chromaRange":0.1,"lightnessRange":0.25});
            point[key] = json!(shift);
            let points = parse(&json!([
                point.clone(),
                point.clone(),
                point.clone(),
                point.clone(),
                point.clone(),
                point.clone(),
                point.clone(),
                point
            ]));
            for index in 0..=400 {
                let mut color = [0.7, 0.15, 60.0];
                color[axis] = match axis {
                    0 => 0.4 + index as f32 * 0.0015,
                    1 => 0.001 + index as f32 * 0.000875,
                    _ => index as f32 * 0.3,
                };
                cases.push(case(lab_to_rgb(color[0], color[1], color[2]), points));
            }
        }
    }
    let results = run(&cases);
    for index in 0..4 {
        assert_eq!(&results[index][..3], &cases[index][..3]);
    }
    let mut offset = 4;
    for axis in [0, 1, 2] {
        for _shift in [-100, 100] {
            let mut previous = f32::NEG_INFINITY;
            for result in &results[offset..offset + 401] {
                let color = rgb_to_lch(result);
                assert!(color.iter().all(|value| value.is_finite()));
                assert!(
                    color[axis] > previous,
                    "axis {axis} inverted: {} <= {previous}",
                    color[axis]
                );
                previous = color[axis];
            }
            offset += 401;
        }
    }
    let point =
        json!({"color":{"lightness":0.7,"chroma":0.15,"hue":359},"hueShift":50,"hueRange":20});
    let a = parse(&json!([point]));
    let b = parse(
        &json!([{"color":{"lightness":0.7,"chroma":0.15,"hue":0},"hueShift":50,"hueRange":20}]),
    );
    let near_wrap = run(&[
        case(lab_to_rgb(0.7, 0.15, 0.0), a),
        case(lab_to_rgb(0.7, 0.15, 0.0), b),
    ]);
    for channel in 0..3 {
        assert!((near_wrap[0][channel] - near_wrap[1][channel]).abs() < 0.0001);
    }
}
