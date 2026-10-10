use crate::image_processing::{Crop, apply_orientation};
use crate::white_balance::WhiteBalance;
use anyhow::{Result, anyhow};
use image::{DynamicImage, ImageBuffer, Rgba};
use rawler::{
    decoders::{Decoder, Orientation, RawDecodeParams},
    imgop::{
        develop::{DemosaicAlgorithm, Intermediate, ProcessingStep, RawDevelop},
        xyz::Illuminant,
    },
    rawimage::{RawImage, RawPhotometricInterpretation},
    rawsource::RawSource,
};
use rayon::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub fn develop_raw_image(
    file_bytes: &[u8],
    fast_demosaic: bool,
    highlight_compression: f32,
    linear_mode: String,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
    proxy_min_dim: Option<usize>,
) -> Result<DynamicImage> {
    let (developed_image, orientation) = develop_internal(
        file_bytes,
        fast_demosaic,
        highlight_compression,
        linear_mode.clone(),
        cancel_token.clone(),
        proxy_min_dim,
    )
    .or_else(|error| {
        if proxy_min_dim.is_none() {
            return Err(error);
        }
        log::debug!("DNG proxy development failed, trying the full image: {error}");
        develop_internal(
            file_bytes,
            fast_demosaic,
            highlight_compression,
            linear_mode,
            cancel_token,
            None,
        )
    })?;
    let _span = crate::perf_trace::span("decode.orientation");
    Ok(apply_orientation(developed_image, orientation))
}

fn borrowed_raw_source(file_bytes: &[u8]) -> RawSource {
    // SAFETY: every caller in this module keeps the source local and drops it
    // while `file_bytes` is still borrowed.
    unsafe { RawSource::new_from_slice_unchecked(file_bytes) }
}

pub fn with_raw_source<T>(file_bytes: &[u8], f: impl FnOnce(&RawSource) -> T) -> T {
    let source = borrowed_raw_source(file_bytes);
    f(&source)
}

fn metadata_orientation(decoder: &dyn Decoder, source: &RawSource) -> Result<Orientation> {
    let metadata = decoder.raw_metadata(source, &RawDecodeParams::default())?;
    Ok(metadata
        .exif
        .orientation
        .map(Orientation::from_u16)
        .unwrap_or(Orientation::Normal))
}

// Equivalent to upstream rawler a32bc1ff; retain RapidRoom's pinned decoder fixes.
fn reconcile_camera_cfa(raw_image: &mut RawImage) {
    if raw_image.make == "OLYMPUS CORPORATION"
        && raw_image.model == "E-M1X"
        && raw_image.cpp == 1
        && let RawPhotometricInterpretation::Cfa(config) = &mut raw_image.photometric
        && config.cfa.name == "BGGR"
    {
        let cfa = rawler::cfa::CFA::new("RGGB");
        config.cfa = cfa.clone();
        raw_image.camera.cfa = cfa;
    }
}

fn is_linear_raw_format(raw_image: &RawImage) -> bool {
    matches!(
        raw_image.photometric,
        RawPhotometricInterpretation::LinearRaw
    )
}

#[inline]
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[inline]
fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn recover_clipped_pixel(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max_c = r.max(g).max(b);

    if max_c <= 0.50 {
        return (r, g, b);
    }

    let mut cur_r = r;
    let mut cur_g = g;
    let mut cur_b = b;

    let outer_blend = smootherstep(0.50, 1.5, max_c);

    let magenta = (cur_r.min(cur_b) - cur_g).max(0.0);
    if magenta > 0.0 {
        let target_g = cur_r.min(cur_b) * 0.80 + ((cur_r + cur_b) * 0.5) * 0.20;
        let correction = (target_g - cur_g).max(0.0);
        let magenta_weight = smoothstep(0.0, 0.25, magenta / max_c);
        cur_g += correction * outer_blend * magenta_weight;
    }

    let residual = (cur_r.min(cur_b) - cur_g).max(0.0);
    if residual > 0.0 {
        cur_g += residual * outer_blend;
    }

    let new_max = cur_r.max(cur_g).max(cur_b);
    let min_c = cur_r.min(cur_g).min(cur_b);

    let knee = smoothstep(0.50, 1.5, new_max);

    if knee > 0.0 {
        let neutrality = (min_c / new_max.max(1e-5)).clamp(0.0, 1.0);

        let core_burn = smoothstep(0.60, 3.0, new_max);

        let desat = (knee * (neutrality * 0.85 + core_burn * 0.15)).clamp(0.0, 1.0);
        let smooth_desat = desat * desat * (3.0 - 2.0 * desat);

        let neutral_value = min_c + (new_max - min_c) * 1.0;

        cur_r = cur_r * (1.0 - smooth_desat) + neutral_value * smooth_desat;
        cur_g = cur_g * (1.0 - smooth_desat) + neutral_value * smooth_desat;
        cur_b = cur_b * (1.0 - smooth_desat) + neutral_value * smooth_desat;
    }

    (cur_r, cur_g, cur_b)
}

fn develop_internal(
    file_bytes: &[u8],
    fast_demosaic: bool,
    _highlight_compression: f32,
    linear_mode: String,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
    proxy_min_dim: Option<usize>,
) -> Result<(DynamicImage, Orientation)> {
    let check_cancel = || -> Result<()> {
        if let Some((tracker, generation)) = &cancel_token
            && tracker.load(Ordering::SeqCst) != *generation
        {
            return Err(anyhow!("Load cancelled"));
        }
        Ok(())
    };

    check_cancel()?;

    let decode_span = crate::perf_trace::span("decode.rawler_decode");
    let source = borrowed_raw_source(file_bytes);
    let decoder = rawler::get_decoder(&source)?;

    check_cancel()?;
    let decode_params = RawDecodeParams {
        proxy_min_dim,
        ..Default::default()
    };
    let mut raw_image: RawImage = decoder.raw_image(&source, &decode_params, false)?;
    reconcile_camera_cfa(&mut raw_image);

    // Retain the full recommended sensor image for editable camera aspect crops.
    if let Some(default_area) = raw_image.default_crop_area {
        raw_image.crop_area = Some(default_area);
    }

    let orientation = metadata_orientation(decoder.as_ref(), &source)?;
    drop(decode_span);

    let is_linear_format = is_linear_raw_format(&raw_image);

    let (apply_ungamma, apply_calibration) = match linear_mode.as_str() {
        "gamma" => (true, true),
        "skip_calib" => (false, false),
        "gamma_skip_calib" => (true, false),
        _ => (false, true),
    };

    let original_white_level = raw_image
        .whitelevel
        .0
        .first()
        .cloned()
        .unwrap_or(u16::MAX as u32) as f32;
    let original_black_level = raw_image
        .blacklevel
        .levels
        .first()
        .map(|r| r.as_f32())
        .unwrap_or(0.0);

    for level in raw_image.whitelevel.0.iter_mut() {
        *level = u32::MAX;
    }

    let mut developer = RawDevelop::default();

    if is_linear_format {
        developer.steps.retain(|&step| {
            step != ProcessingStep::SRgb
                && step != ProcessingStep::Demosaic
                && (apply_calibration || step != ProcessingStep::Calibrate)
        });
    } else if fast_demosaic {
        developer.demosaic_algorithm = DemosaicAlgorithm::Speed;
        developer.steps.retain(|&step| step != ProcessingStep::SRgb);
    } else {
        developer.steps.retain(|&step| step != ProcessingStep::SRgb);
    }

    raw_image.wb_coeffs =
        crate::multi_exposure::neutralize_wb_if_multiexposure(raw_image.wb_coeffs, file_bytes);

    check_cancel()?;
    let develop_span = crate::perf_trace::span("decode.develop_intermediate");
    let mut developed_intermediate = developer.develop_intermediate(&raw_image)?;
    drop(develop_span);

    drop(raw_image);

    let denominator = (original_white_level - original_black_level).max(1.0);
    let rescale_factor = (u32::MAX as f32 - original_black_level) / denominator;

    let safe_highlight_compression = 1000.0;

    let clamp_limit = if fast_demosaic {
        1.0
    } else {
        safe_highlight_compression
    };

    let (width, height) = {
        let dim = developed_intermediate.dim();
        (dim.w as u32, dim.h as u32)
    };

    check_cancel()?;

    let post_span = crate::perf_trace::span("decode.rescale_recover");
    match &mut developed_intermediate {
        Intermediate::Monochrome(pixels) => {
            pixels.data.par_iter_mut().for_each(|p| {
                let mut linear_val = *p * rescale_factor;
                if is_linear_format && apply_ungamma {
                    linear_val = srgb_to_linear(linear_val.max(0.0));
                }
                *p = linear_val.clamp(0.0, clamp_limit);
            });
        }
        Intermediate::ThreeColor(pixels) => {
            pixels.data.par_iter_mut().for_each(|p| {
                let mut r = (p[0] * rescale_factor).max(0.0);
                let mut g = (p[1] * rescale_factor).max(0.0);
                let mut b = (p[2] * rescale_factor).max(0.0);

                if is_linear_format && apply_ungamma {
                    r = srgb_to_linear(r.max(0.0));
                    g = srgb_to_linear(g.max(0.0));
                    b = srgb_to_linear(b.max(0.0));
                }

                let (rec_r, rec_g, rec_b) = recover_clipped_pixel(r, g, b);

                p[0] = rec_r.clamp(0.0, clamp_limit);
                p[1] = rec_g.clamp(0.0, clamp_limit);
                p[2] = rec_b.clamp(0.0, clamp_limit);
            });
        }
        Intermediate::FourColor(pixels) => {
            pixels.data.par_iter_mut().for_each(|p| {
                p.iter_mut().for_each(|c| {
                    let mut linear_val = *c * rescale_factor;
                    if is_linear_format && apply_ungamma {
                        linear_val = srgb_to_linear(linear_val.max(0.0));
                    }
                    *c = linear_val.clamp(0.0, clamp_limit);
                });
            });
        }
    }

    drop(post_span);
    check_cancel()?;

    let _convert_span = crate::perf_trace::span("decode.to_rgba32f");
    let dynamic_image = match developed_intermediate {
        Intermediate::ThreeColor(pixels) => {
            DynamicImage::ImageRgba32F(rgb_pixels_to_rgba(&pixels.data, width, height)?)
        }
        Intermediate::Monochrome(pixels) => {
            DynamicImage::ImageRgba32F(mono_pixels_to_rgba(&pixels.data, width, height)?)
        }
        _ => {
            return Err(anyhow!("Unsupported intermediate format for conversion"));
        }
    };

    Ok((dynamic_image, orientation))
}

pub fn get_raw_dimensions(file_bytes: &[u8]) -> Option<(u32, u32, bool)> {
    std::panic::catch_unwind(|| {
        let source = borrowed_raw_source(file_bytes);
        let decoder = rawler::get_decoder(&source).ok()?;
        let raw_img = decoder
            .raw_image(&source, &RawDecodeParams::default(), true)
            .ok()?;
        let (w, h) = raw_img
            .crop_area
            .map_or((raw_img.width, raw_img.height), |r| (r.d.w, r.d.h));
        Some((w as u32, h as u32, is_linear_raw_format(&raw_img)))
    })
    .ok()
    .flatten()
}

fn rgb_pixels_to_rgba(
    data: &[[f32; 3]],
    width: u32,
    height: u32,
) -> Result<ImageBuffer<Rgba<f32>, Vec<f32>>> {
    let mut rgba = vec![0.0f32; data.len() * 4];
    rgba.par_chunks_exact_mut(4)
        .zip(data.par_iter())
        .for_each(|(dst, p)| dst.copy_from_slice(&[p[0], p[1], p[2], 1.0]));
    ImageBuffer::from_raw(width, height, rgba)
        .ok_or_else(|| anyhow!("Developed image size does not match its dimensions"))
}

fn mono_pixels_to_rgba(
    data: &[f32],
    width: u32,
    height: u32,
) -> Result<ImageBuffer<Rgba<f32>, Vec<f32>>> {
    let mut rgba = vec![0.0f32; data.len() * 4];
    rgba.par_chunks_exact_mut(4)
        .zip(data.par_iter())
        .for_each(|(dst, &p)| dst.copy_from_slice(&[p, p, p, 1.0]));
    ImageBuffer::from_raw(width, height, rgba)
        .ok_or_else(|| anyhow!("Developed image size does not match its dimensions"))
}

pub fn read_as_shot_white_balance(file_bytes: &[u8]) -> Option<WhiteBalance> {
    let source = borrowed_raw_source(file_bytes);
    let decoder = rawler::get_decoder(&source).ok()?;
    let raw_image = decoder
        .raw_image(&source, &RawDecodeParams::default(), true)
        .ok()?;
    as_shot_white_balance_from_raw(&raw_image, file_bytes)
}

fn as_shot_white_balance_from_raw(raw_image: &RawImage, file_bytes: &[u8]) -> Option<WhiteBalance> {
    if raw_image.cpp == 1 && !matches!(raw_image.photometric, RawPhotometricInterpretation::Cfa(_))
    {
        return None;
    }

    // Sony lossless M/S ARWs contain already white-balanced RGB. Rawler replaces
    // their sensor WB coefficients with unity; these are development gains, not
    // a camera neutral. Inverting the sensor matrices invents an extreme tint,
    // which the first relative edit clamps and turns the whole image magenta.
    // Use the existing reference fallback until original sensor WB is available.
    if raw_image.make.eq_ignore_ascii_case("SONY")
        && raw_image.cpp == 3
        && is_linear_raw_format(raw_image)
        && raw_image.wb_coeffs[..3] == [1.0; 3]
    {
        return None;
    }

    let wb_coeffs =
        crate::multi_exposure::neutralize_wb_if_multiexposure(raw_image.wb_coeffs, file_bytes);
    let neutral = if wb_coeffs[0].is_nan() {
        [1.0; 4]
    } else {
        wb_coeffs.map(|c| 1.0 / c)
    };

    let matrices = &raw_image.color_matrix;
    if let (Some(matrix_a), Some(matrix_d65)) =
        (matrices.get(&Illuminant::A), matrices.get(&Illuminant::D65))
    {
        return WhiteBalance::from_dual_illuminant_camera_neutral(matrix_a, matrix_d65, &neutral);
    }

    let color_matrix = matrices
        .get(&Illuminant::D65)
        .or_else(|| matrices.values().next())?;
    WhiteBalance::from_camera_neutral(color_matrix, &neutral)
}

#[cfg(test)]
mod as_shot_white_balance_tests {
    use super::*;
    use rawler::{
        cfa::CFA,
        decoders::Camera,
        pixarray::PixU16,
        rawimage::{BlackLevel, CFAConfig, WhiteLevel},
    };

    fn sony_metadata(cpp: usize, wb: [f32; 4]) -> RawImage {
        let mut camera = Camera::new();
        camera.make = "SONY".into();
        camera.model = "ILCE-7CR".into();
        camera.cfa = CFA::new("RGGB");
        // Pinned rawler's ILCE-7CR matrices and CC0 sample's sensor coefficients.
        camera.color_matrix.insert(
            Illuminant::A,
            vec![
                0.9185, -0.4857, 0.0505, -0.3651, 1.1061, 0.2982, -0.0161, 0.0698, 0.6769,
            ],
        );
        camera.color_matrix.insert(
            Illuminant::D65,
            vec![
                0.82, -0.2976, -0.0719, -0.4296, 1.2053, 0.2532, -0.0429, 0.1282, 0.5774,
            ],
        );
        let photometric = if cpp == 3 {
            RawPhotometricInterpretation::LinearRaw
        } else {
            RawPhotometricInterpretation::Cfa(CFAConfig::new_from_camera(&camera))
        };
        RawImage::new(
            camera,
            PixU16::new_with(vec![0; 4 * cpp], 2 * cpp, 2),
            cpp,
            wb,
            photometric,
            Some(BlackLevel::zero(1, 1, cpp)),
            Some(WhiteLevel::new(vec![16383; cpp])),
            false,
        )
    }

    #[test]
    fn upstream_em1x_cfa_correction_is_scoped_to_its_native_bayer_metadata() {
        let mut image = sony_metadata(1, [1.0; 4]);
        image.make = "OLYMPUS CORPORATION".into();
        image.model = "E-M1X".into();
        image.camera.cfa = CFA::new("BGGR");
        image.photometric =
            RawPhotometricInterpretation::Cfa(CFAConfig::new_from_camera(&image.camera));
        let unchanged = image.clone();
        reconcile_camera_cfa(&mut image);
        assert_eq!(image.camera.cfa.name, "RGGB");
        let RawPhotometricInterpretation::Cfa(config) = image.photometric else {
            panic!("expected Bayer metadata");
        };
        assert_eq!(config.cfa.name, "RGGB");
        let mut other = unchanged.clone();
        other.model = "E-M1".into();
        reconcile_camera_cfa(&mut other);
        assert_eq!(other.photometric, unchanged.photometric);
        let mut linear = unchanged;
        linear.cpp = 3;
        linear.photometric = RawPhotometricInterpretation::LinearRaw;
        reconcile_camera_cfa(&mut linear);
        assert_eq!(linear.photometric, RawPhotometricInterpretation::LinearRaw);
        assert_eq!(linear.camera.cfa.name, "BGGR");
    }

    #[test]
    fn sony_reduced_rgb_does_not_invent_a_sensor_white_balance() {
        let image = sony_metadata(3, [1.0, 1.0, 1.0, f32::NAN]);
        let invented = WhiteBalance::from_dual_illuminant_camera_neutral(
            &image.color_matrix[&Illuminant::A],
            &image.color_matrix[&Illuminant::D65],
            &[1.0; 3],
        )
        .unwrap();
        assert!(invented.tint < -150.0);
        let wb =
            as_shot_white_balance_from_raw(&image, &[0; 8]).unwrap_or_else(WhiteBalance::reference);
        assert_eq!(wb, WhiteBalance::reference());
        for (temperature, tint) in [(8.0, -4.0), (-5.0, 0.0)] {
            let gains =
                crate::white_balance::adaptation_log_gains(wb, wb.shifted(temperature, tint));
            assert!(
                gains
                    .iter()
                    .all(|gain| gain.is_finite() && gain.abs() < 0.25)
            );
        }
    }

    #[test]
    fn sony_cfa_and_original_linear_sensor_coefficients_remain_available() {
        let sensor_wb = [2688.0 / 1024.0, 1.0, 1636.0 / 1024.0, f32::NAN];
        let cfa = as_shot_white_balance_from_raw(&sony_metadata(1, sensor_wb), &[0; 8]).unwrap();
        assert!((cfa.temperature - 6021.31).abs() < 0.1);
        assert!((cfa.tint - 19.012).abs() < 0.01);
        let linear = as_shot_white_balance_from_raw(&sony_metadata(3, sensor_wb), &[0; 8]).unwrap();
        assert_eq!(linear, cfa);
        let mut other_linear = sony_metadata(3, [1.0, 1.0, 1.0, f32::NAN]);
        other_linear.make = "Apple".into();
        assert!(as_shot_white_balance_from_raw(&other_linear, &[0; 8]).is_some());
    }
}

pub fn get_fast_demosaic_scale_factor(
    file_bytes: &[u8],
    decoded_width: u32,
    decoded_height: u32,
) -> f32 {
    let source = borrowed_raw_source(file_bytes);
    if let Ok(decoder) = rawler::get_decoder(&source)
        && let Ok(raw_img) = decoder.raw_image(&source, &RawDecodeParams::default(), true)
    {
        let max_comp = (decoded_width as f32).max(decoded_height as f32);
        if is_linear_raw_format(&raw_img) {
            let max_crop = raw_img
                .crop_area
                .map_or(raw_img.width.max(raw_img.height), |r| r.d.w.max(r.d.h))
                as f32;
            let ratio = if max_crop > 0.0 {
                max_comp / max_crop
            } else {
                1.0
            };
            return if ratio > 0.97 { 1.0 } else { ratio };
        }
        let max_orig = (raw_img.width as f32).max(raw_img.height as f32);
        if max_orig > 0.0 {
            let ratio = max_comp / max_orig;
            if ratio > 0.1 && ratio < 0.35 {
                return 0.25;
            } else if (0.35..0.75).contains(&ratio) {
                return 0.5;
            }
        }
    }
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{assert_bits_eq, sample_values};

    const W: u32 = 37;
    const H: u32 = 23;

    #[test]
    fn rgb_pixels_to_rgba_matches_serial_conversion() {
        let values = sample_values((W * H * 3) as usize);
        let data: Vec<[f32; 3]> = values.as_chunks::<3>().0.to_vec();
        let expected = ImageBuffer::<Rgba<f32>, _>::from_fn(W, H, |x, y| {
            let p = data[(y * W + x) as usize];
            Rgba([p[0], p[1], p[2], 1.0])
        });

        let actual = rgb_pixels_to_rgba(&data, W, H).unwrap();

        assert_bits_eq(expected.as_raw(), actual.as_raw());
    }

    #[test]
    fn mono_pixels_to_rgba_matches_serial_conversion() {
        let data = sample_values((W * H) as usize);
        let expected = ImageBuffer::<Rgba<f32>, _>::from_fn(W, H, |x, y| {
            let p = data[(y * W + x) as usize];
            Rgba([p, p, p, 1.0])
        });

        let actual = mono_pixels_to_rgba(&data, W, H).unwrap();

        assert_bits_eq(expected.as_raw(), actual.as_raw());
    }

    #[test]
    fn pixel_conversion_rejects_mismatched_dimensions() {
        let data = vec![[0.0f32; 3]; (W * H) as usize];
        assert!(rgb_pixels_to_rgba(&data, W + 1, H).is_err());
        assert!(mono_pixels_to_rgba(&vec![0.0; (W * H) as usize], W, H + 1).is_err());
    }

    #[test]
    fn srgb_linearization_matches_reference_values() {
        for (input, expected) in [
            (0.0, 0.0),
            (0.04045, 0.003130805),
            (0.5, 0.21404114),
            (1.0, 1.0),
        ] {
            assert!((srgb_to_linear(input) - expected).abs() < 1e-7);
        }
    }

    #[test]
    fn srgb_linearization_meets_at_the_segment_join() {
        assert!((srgb_to_linear(0.040451) - srgb_to_linear(0.04045)).abs() < 1e-6);
    }

    #[test]
    fn magenta_correction_does_not_jump_across_the_green_blue_boundary() {
        for (below, above) in [(0.009, 0.011), (0.0099999, 0.0100001)] {
            let (_, green_below, _) = recover_clipped_pixel(2.8, 0.010, below);
            let (_, green_above, _) = recover_clipped_pixel(2.8, 0.010, above);
            assert!((green_above - green_below).abs() < 0.005);
        }
    }

    #[test]
    fn recovery_preserves_dark_pixels_and_strong_magenta_highlights() {
        assert_eq!(recover_clipped_pixel(0.4, 0.1, 0.3), (0.4, 0.1, 0.3));
        let (red, green, blue) = recover_clipped_pixel(2.22, 0.82, 1.57);
        assert!((red - 2.22).abs() < 1e-6);
        assert!((green - 2.104).abs() < 0.001);
        assert!((blue - 2.091).abs() < 0.001);
    }

    #[test]
    fn raw_dimensions_of_corrupt_input_is_none() {
        let mut tiff_header = b"II*\0".to_vec();
        tiff_header.extend_from_slice(&8u32.to_le_bytes());
        tiff_header.extend_from_slice(&u16::MAX.to_le_bytes());
        let cases: [&[u8]; 4] = [b"", b"II*\0", b"not a raw file", &tiff_header];
        for case in cases {
            assert_eq!(get_raw_dimensions(case), None);
        }
    }
}

fn oriented_camera_crop(
    crop: rawler::imgop::Rect,
    base: rawler::imgop::Rect,
    orientation: Orientation,
) -> Crop {
    let mut x = crop.p.x - base.p.x;
    let mut y = crop.p.y - base.p.y;
    let (mut width, mut height) = (crop.d.w, crop.d.h);
    let (transpose, flip_x, flip_y) = orientation.to_flips();
    if flip_x {
        x = base.d.w - x - width;
    }
    if flip_y {
        y = base.d.h - y - height;
    }
    if transpose {
        std::mem::swap(&mut x, &mut y);
        std::mem::swap(&mut width, &mut height);
    }
    Crop {
        x: x as f64,
        y: y as f64,
        width: width as f64,
        height: height as f64,
    }
}

/// Camera aspect crop in the full developed image's oriented pixel coordinates.
/// Dummy decoding reads metadata without developing the sensor pixels.
pub fn camera_crop_from_bytes(file_bytes: &[u8]) -> Option<Crop> {
    let source = borrowed_raw_source(file_bytes);
    let decoder = rawler::get_decoder(&source).ok()?;
    let raw_image = decoder
        .raw_image(&source, &RawDecodeParams::default(), true)
        .ok()?;
    let mut base = raw_image.default_crop_area?;
    if let Some(active) = raw_image.active_area {
        base = base.intersection(&active);
    }
    let crop = raw_image.crop_area?;
    let orientation = metadata_orientation(decoder.as_ref(), &source).ok()?;
    Some(oriented_camera_crop(crop, base, orientation))
}

pub fn apply_camera_crop_default(adjustments: &mut serde_json::Value, crop: Option<Crop>) {
    // An explicit crop (including null for the full frame) always wins.
    if adjustments.get("crop").is_some() {
        return;
    }
    let Some(crop) = crop else {
        return;
    };
    if adjustments.is_null() {
        *adjustments = serde_json::json!({});
    }
    if let Some(object) = adjustments.as_object_mut() {
        object.insert("crop".into(), serde_json::json!(crop));
        object
            .entry("aspectRatio")
            .or_insert_with(|| serde_json::json!(crop.width / crop.height));
    }
}

pub fn apply_camera_crop_default_from_path(
    adjustments: &mut serde_json::Value,
    source_path: &std::path::Path,
) {
    if adjustments.get("crop").is_some()
        || !source_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("arw"))
    {
        return;
    }
    if let Ok(bytes) = crate::file_management::read_file_mapped(source_path) {
        let crop = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            camera_crop_from_bytes(&bytes)
        }))
        .ok()
        .flatten();
        apply_camera_crop_default(adjustments, crop);
    }
}

#[cfg(test)]
mod camera_crop_tests {
    use super::*;
    use rawler::imgop::{Dim2, Point, Rect};

    #[test]
    fn camera_crop_uses_default_frame_and_exif_orientation() {
        let base = Rect::new(Point::new(12, 8), Dim2::new(7008, 4672));
        let crop = Rect::new(Point::new(404, 8), Dim2::new(6224, 4672));
        let normal = oriented_camera_crop(crop, base, Orientation::Normal);
        assert_eq!(
            (normal.x, normal.y, normal.width, normal.height),
            (392.0, 0.0, 6224.0, 4672.0)
        );
        let rotated = oriented_camera_crop(crop, base, Orientation::Rotate90);
        assert_eq!(
            (rotated.x, rotated.y, rotated.width, rotated.height),
            (0.0, 392.0, 4672.0, 6224.0)
        );
        // Asymmetric rectangle makes all flips/transposes observable.
        let crop = Rect::new(Point::new(112, 208), Dim2::new(4000, 3000));
        for (orientation, expected) in [
            (Orientation::Normal, (100.0, 200.0, 4000.0, 3000.0)),
            (Orientation::HorizontalFlip, (2908.0, 200.0, 4000.0, 3000.0)),
            (Orientation::Rotate180, (2908.0, 1472.0, 4000.0, 3000.0)),
            (Orientation::VerticalFlip, (100.0, 1472.0, 4000.0, 3000.0)),
            (Orientation::Transpose, (200.0, 100.0, 3000.0, 4000.0)),
            (Orientation::Rotate90, (1472.0, 100.0, 3000.0, 4000.0)),
            (Orientation::Transverse, (1472.0, 2908.0, 3000.0, 4000.0)),
            (Orientation::Rotate270, (200.0, 2908.0, 3000.0, 4000.0)),
        ] {
            let result = oriented_camera_crop(crop, base, orientation);
            assert_eq!((result.x, result.y, result.width, result.height), expected);
        }
    }

    #[test]
    fn explicit_saved_crop_and_full_frame_override_camera_default() {
        let crop = Some(Crop {
            x: 392.0,
            y: 0.0,
            width: 6224.0,
            height: 4672.0,
        });
        for mut saved in [
            serde_json::json!({"crop": null}),
            serde_json::json!({"crop": {"x": 50, "y": 60, "width": 100, "height": 200}}),
        ] {
            let original = saved.clone();
            apply_camera_crop_default(&mut saved, crop);
            assert_eq!(saved, original);
        }
        let mut fresh = serde_json::Value::Null;
        apply_camera_crop_default(&mut fresh, crop);
        assert_eq!(fresh["crop"]["x"], 392.0);
        assert_eq!(fresh["aspectRatio"], 6224.0 / 4672.0);
        let mut full_aspect = serde_json::json!({"exposure": 1});
        apply_camera_crop_default(&mut full_aspect, None);
        assert_eq!(full_aspect, serde_json::json!({"exposure": 1}));
    }
}
