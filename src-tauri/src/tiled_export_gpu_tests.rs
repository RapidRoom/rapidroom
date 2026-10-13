//! Native-GPU evidence; run serially under the shared build lock.
//! RAPIDROOM_EXPORT_MEMORY_MODE=legacy|bounded selects separate 60MP memory runs.
use super::*;
use crate::image_processing::get_all_adjustments_from_json;
use base64::Engine;
use image::{GrayImage, Rgb};
use serde_json::json;

fn context() -> GpuContext {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let limits = adapter.limits();
    let features = adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        required_limits: limits.clone(),
        ..Default::default()
    }))
    .unwrap();
    eprintln!("export evidence adapter: {:?}", adapter.get_info());
    GpuContext {
        device: Arc::new(device),
        queue: Arc::new(queue),
        limits,
        adapter_info: adapter.get_info(),
        display: Default::default(),
        pending_surface_size: Default::default(),
    }
}

fn image(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
        let bright = (x.abs_diff(width / 2) < 17 && y.abs_diff(height / 2) < 53) as u8;
        Rgb([
            ((x * 13 + y * 7) % 211) as u8 + bright * 40,
            ((x * 5 + y * 17) % 197) as u8 + bright * 50,
            ((x * 31 + y * 3) % 181) as u8 + bright * 60,
        ])
    }))
}

fn upload(context: &GpuContext, image: &DynamicImage) -> wgpu::TextureView {
    let pixels = to_rgba_f16(image);
    context
        .device
        .create_texture_with_data(
            &context.queue,
            &wgpu::TextureDescriptor {
                label: Some("Full-source test reference"),
                size: wgpu::Extent3d {
                    width: image.width(),
                    height: image.height(),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            TextureDataOrder::MipMajor,
            bytemuck::cast_slice(&pixels),
        )
        .create_view(&Default::default())
}

fn compare(name: &str, expected: &DynamicImage, actual: &DynamicImage) {
    assert_eq!(expected.dimensions(), actual.dimensions(), "{name}");
    let e = expected.to_rgba16();
    let a = actual.to_rgba16();
    let differences: Vec<_> = e
        .as_raw()
        .iter()
        .zip(a.as_raw())
        .filter_map(|(e, a)| (e != a).then_some(e.abs_diff(*a)))
        .collect();
    let max = differences.iter().copied().max().unwrap_or(0);
    eprintln!(
        "{name}: {} unequal samples, max u16 delta={max}",
        differences.len()
    );
    assert_eq!(max, 0, "{name} must remain exact with identical arithmetic");
}

#[test]
#[ignore = "requires native GPU and shared build lock"]
fn tiled_export_full_tiled_region_matrix() {
    let context = context();
    let raw = image(1301, 1103);
    let mut depth_png = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageLuma8(GrayImage::from_fn(64, 64, |_, y| Luma([(y * 4) as u8])))
        .write_to(&mut depth_png, image::ImageFormat::Png)
        .unwrap();
    let depth = base64::engine::general_purpose::STANDARD.encode(depth_png.into_inner());
    let cases = [
        ("identity", json!({})),
        (
            "clarity-halation-glow",
            json!({"clarity":70,"structure":40,"glowAmount":70,"halationAmount":60,"shadows":25,"sharpness":50}),
        ),
        (
            "nr-ca-vignette",
            json!({"lumaNoiseReduction":80,"colorNoiseReduction":100,"chromaticAberrationRedCyan":75,"chromaticAberrationBlueYellow":-90,"vignetteAmount":-50,"grainAmount":30}),
        ),
        ("flare", json!({"flareAmount":70,"exposure":0.8})),
        (
            "lens-rotated-crop",
            json!({"lensDistortionParams":{"k1":0.03,"k2":-0.01},"lensDistortionEnabled":true,"rotation":11,"crop":{"x":73,"y":61,"width":1101,"height":941}}),
        ),
        (
            "lens-blur",
            json!({"lensBlurEnabled":true,"lensBlurAmount":70,"lensBlurDepthMap":depth,"lensBlurMinDepth":35,"lensBlurMaxDepth":65}),
        ),
    ];
    for (name, adjustments) in cases {
        // Production geometry order: warp/lens blur, rotate, crop, then GPU.
        if name == "lens-rotated-crop" {
            assert!(!crate::image_processing::is_geometry_identity(
                &crate::image_processing::get_geometry_params_from_json(&adjustments)
            ));
        }
        let (base, _) = crate::apply_all_transformations(&raw, &adjustments);
        if name == "lens-blur" {
            assert!(matches!(base, std::borrow::Cow::Owned(_)));
        }

        let (w, h) = base.dimensions();
        let mut a = get_all_adjustments_from_json(&adjustments, false, None);
        let masks = [GrayImage::from_fn(w, h, |x, y| {
            Luma([((x * 17 + y * 11) % 256) as u8])
        })];
        a.mask_count = u32::from(name != "identity");
        let bitmaps = if name == "identity" {
            &[][..]
        } else {
            &masks[..]
        };
        a.mask_adjustments[0].clarity = 0.55;
        a.mask_adjustments[0].glow_amount = 0.35;
        a.mask_adjustments[0].exposure = 0.4;
        let request = || RenderRequest {
            adjustments: a,
            mask_bitmaps: bitmaps,
            lut: None,
            roi: None,
        };
        let full_plan = ExportPlan::with_tile_size(
            w,
            h,
            &request(),
            context.limits.max_texture_dimension_2d,
            w.max(h),
        )
        .unwrap();
        let full_view = upload(&context, &base);
        let full_processor =
            GpuProcessor::new_with_layout(context.clone(), w, h, Some(full_plan)).unwrap();
        let (pixels, ow, oh, _, _) = full_processor
            .run(
                &full_view,
                None,
                w,
                h,
                request(),
                false,
                RenderOutputPrecision::SixteenBit,
            )
            .unwrap();
        let full = rendered_image(pixels, ow, oh).unwrap();
        drop(full_processor);
        let tile_plan = ExportPlan::with_tile_size(
            w,
            h,
            &request(),
            context.limits.max_texture_dimension_2d,
            512,
        )
        .unwrap();
        let tiled = GpuProcessor::new_with_layout(context.clone(), w, h, Some(tile_plan)).unwrap();
        let (pixels, ow, oh, _, _) = tiled
            .run(
                &full_view,
                Some(&base),
                w,
                h,
                request(),
                false,
                RenderOutputPrecision::SixteenBit,
            )
            .unwrap();
        compare(name, &full, &rendered_image(pixels, ow, oh).unwrap());
        // Off-grid, crossing both a row and column boundary.
        let roi = Roi {
            x: 493,
            y: 501,
            width: 79,
            height: 63,
        };
        let mut r = request();
        r.roi = Some(roi);
        let (pixels, ow, oh, _, _) = tiled
            .run(
                &full_view,
                Some(&base),
                w,
                h,
                r,
                false,
                RenderOutputPrecision::SixteenBit,
            )
            .unwrap();
        compare(
            &format!("{name} region"),
            &full.crop_imm(roi.x, roi.y, roi.width, roi.height),
            &rendered_image(pixels, ow, oh).unwrap(),
        );
    }
}

#[test]
#[ignore = "requires native GPU and shared build lock; explicit 60MP memory evidence"]
fn tiled_export_sixty_mp_memory() {
    let mode =
        std::env::var("RAPIDROOM_EXPORT_MEMORY_MODE").expect("choose legacy or bounded memory run");
    let context = context();
    let base = image(10000, 6000);
    let (w, h) = base.dimensions();
    // Worst-case mask allocation: all 32 full CPU bitmaps, with a visible first mask.
    let masks: Vec<_> = (0..MAX_MASKS)
        .map(|i| GrayImage::from_fn(w, h, |x, y| Luma([((x + y + i as u32) % 256) as u8])))
        .collect();
    let mut a = get_all_adjustments_from_json(&json!({}), false, None);
    a.mask_count = 1;
    a.mask_adjustments[0].exposure = 0.5;
    let r = RenderRequest {
        adjustments: a,
        mask_bitmaps: &masks,
        lut: None,
        roi: None,
    };
    let (pixels, ow, oh, _, _) = match mode.as_str() {
        "legacy" => {
            let view = upload(&context, &base);
            let processor = GpuProcessor::new(context.clone(), w, h).unwrap();
            processor
                .run(&view, None, w, h, r, false, RenderOutputPrecision::EightBit)
                .unwrap()
        }
        "bounded" => {
            let plan = ExportPlan::new(w, h, &r, context.limits.max_texture_dimension_2d).unwrap();
            eprintln!(
                "bounded allocation estimate={} budget={}",
                plan.estimated_bytes,
                tiled_export::GPU_BUDGET
            );
            let processor =
                GpuProcessor::new_with_layout(context.clone(), w, h, Some(plan)).unwrap();
            processor
                .run(
                    &processor.dummy_blur_view,
                    Some(&base),
                    w,
                    h,
                    r,
                    false,
                    RenderOutputPrecision::EightBit,
                )
                .unwrap()
        }
        _ => panic!("unknown memory mode"),
    };
    let out = rendered_image(pixels, ow, oh).unwrap();
    assert_eq!(out.dimensions(), (10000, 6000));
    let checksum = out
        .as_bytes()
        .iter()
        .fold(0u64, |a, &b| a.wrapping_mul(31).wrapping_add(u64::from(b)));
    eprintln!("60MP memory {mode}: checksum={checksum}");
}

#[test]
#[ignore = "requires native GPU and shared build lock; wide radius above legacy halo"]
fn tiled_export_wide_halo_matches_untiled() {
    let context = context();
    let base = image(4097, 3601);
    let (w, h) = base.dimensions();
    let a = get_all_adjustments_from_json(
        &json!({"glowAmount":75,"structure":60,"clarity":65,"halationAmount":50}),
        false,
        None,
    );
    let roi = Roi {
        x: 1937,
        y: 1951,
        width: 257,
        height: 239,
    };
    let request = || RenderRequest {
        adjustments: a,
        mask_bitmaps: &[],
        lut: None,
        roi: Some(roi),
    };
    let full = ExportPlan::with_tile_size(
        w,
        h,
        &request(),
        context.limits.max_texture_dimension_2d,
        w.max(h),
    )
    .unwrap();
    assert!(full.work_halo > 128);
    let view = upload(&context, &base);
    let processor = GpuProcessor::new_with_layout(context.clone(), w, h, Some(full)).unwrap();
    let (p, ow, oh, _, _) = processor
        .run(
            &view,
            None,
            w,
            h,
            request(),
            false,
            RenderOutputPrecision::SixteenBit,
        )
        .unwrap();
    let reference = rendered_image(p, ow, oh).unwrap();
    drop(processor);
    let plan = ExportPlan::new(w, h, &request(), context.limits.max_texture_dimension_2d).unwrap();
    let processor = GpuProcessor::new_with_layout(context.clone(), w, h, Some(plan)).unwrap();
    let (p, ow, oh, _, _) = processor
        .run(
            &view,
            Some(&base),
            w,
            h,
            request(),
            false,
            RenderOutputPrecision::SixteenBit,
        )
        .unwrap();
    compare(
        "wide glow/structure/clarity/halation seam region",
        &reference,
        &rendered_image(p, ow, oh).unwrap(),
    );
}
