use crate::image_processing::{Crop, apply_orientation};
use anyhow::{Result, anyhow};
use image::{DynamicImage, ImageBuffer, Rgba};
use rawler::{
    decoders::{Orientation, RawDecodeParams},
    imgop::develop::{DemosaicAlgorithm, Intermediate, ProcessingStep, RawDevelop},
    rawimage::{RawImage, RawPhotometricInterpretation},
    rawsource::RawSource,
};
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
) -> Result<DynamicImage> {
    let (developed_image, orientation) = develop_internal(
        file_bytes,
        fast_demosaic,
        highlight_compression,
        linear_mode,
        cancel_token,
    )?;
    Ok(apply_orientation(developed_image, orientation))
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
        ((value + 0.055) / 1.055).powf(3.0)
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
        cur_g += correction * outer_blend;
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

    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source)?;

    check_cancel()?;
    let mut raw_image: RawImage = decoder.raw_image(&source, &RawDecodeParams::default(), false)?;

    // Retain the full recommended sensor image for editable camera aspect crops.
    if let Some(default_area) = raw_image.default_crop_area {
        raw_image.crop_area = Some(default_area);
    }

    let metadata = decoder.raw_metadata(&source, &RawDecodeParams::default())?;
    let orientation = metadata
        .exif
        .orientation
        .map(Orientation::from_u16)
        .unwrap_or(Orientation::Normal);

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
    let mut developed_intermediate = developer.develop_intermediate(&raw_image)?;

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

    match &mut developed_intermediate {
        Intermediate::Monochrome(pixels) => {
            pixels.data.iter_mut().for_each(|p| {
                let mut linear_val = *p * rescale_factor;
                if is_linear_format && apply_ungamma {
                    linear_val = srgb_to_linear(linear_val.max(0.0));
                }
                *p = linear_val.clamp(0.0, clamp_limit);
            });
        }
        Intermediate::ThreeColor(pixels) => {
            pixels.data.iter_mut().for_each(|p| {
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
            pixels.data.iter_mut().for_each(|p| {
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

    check_cancel()?;

    let dynamic_image = match developed_intermediate {
        Intermediate::ThreeColor(pixels) => {
            let buffer = ImageBuffer::<Rgba<f32>, _>::from_fn(width, height, |x, y| {
                let p = pixels.data[(y * width + x) as usize];
                Rgba([p[0], p[1], p[2], 1.0])
            });
            DynamicImage::ImageRgba32F(buffer)
        }
        Intermediate::Monochrome(pixels) => {
            let buffer = ImageBuffer::<Rgba<f32>, _>::from_fn(width, height, |x, y| {
                let p = pixels.data[(y * width + x) as usize];
                Rgba([p, p, p, 1.0])
            });
            DynamicImage::ImageRgba32F(buffer)
        }
        _ => {
            return Err(anyhow!("Unsupported intermediate format for conversion"));
        }
    };

    Ok((dynamic_image, orientation))
}

pub fn get_fast_demosaic_scale_factor(
    file_bytes: &[u8],
    decoded_width: u32,
    decoded_height: u32,
) -> f32 {
    let source = RawSource::new_from_slice(file_bytes);
    if let Ok(decoder) = rawler::get_decoder(&source)
        && let Ok(raw_img) = decoder.raw_image(&source, &RawDecodeParams::default(), true)
    {
        let max_orig = (raw_img.width as f32).max(raw_img.height as f32);
        let max_comp = (decoded_width as f32).max(decoded_height as f32);
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
    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source).ok()?;
    let raw_image = decoder
        .raw_image(&source, &RawDecodeParams::default(), true)
        .ok()?;
    let mut base = raw_image.default_crop_area?;
    if let Some(active) = raw_image.active_area {
        base = base.intersection(active);
    }
    let crop = raw_image.crop_area?;
    let metadata = decoder
        .raw_metadata(&source, &RawDecodeParams::default())
        .ok()?;
    let orientation = metadata
        .exif
        .orientation
        .map(Orientation::from_u16)
        .unwrap_or(Orientation::Normal);
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
