use rawler::{decoders::RawDecodeParams, rawsource::RawSource};
use regex::Regex;
use regex::regex;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use uuid::Uuid;

use crate::file_management::Preset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum XmpImageKind {
    Raw,
    RenderedJpeg,
    Rendered,
}

#[derive(Copy, Clone, Debug)]
enum Num {
    I(i64),
    F(f64),
}

fn parse_num(s: &str) -> Option<Num> {
    if let Ok(i) = s.parse::<i64>() {
        Some(Num::I(i))
    } else if let Ok(f) = s.parse::<f64>() {
        Some(Num::F(f))
    } else {
        None
    }
}

fn num_to_json(num: Num) -> Option<Value> {
    match num {
        Num::I(i) => Some(Value::Number(i.into())),
        Num::F(f) => serde_json::Number::from_f64(f).map(Value::Number),
    }
}

fn get_attr_as_f64(attrs: &HashMap<String, String>, key: &str) -> Option<f64> {
    attrs
        .get(key)
        .and_then(|s| s.trim_start_matches('+').parse::<f64>().ok())
}

pub(crate) fn extract_namespaced_scalar(content: &str, prefix: &str, key: &str) -> Option<String> {
    let prefix = regex::escape(prefix);
    let key = regex::escape(key);
    let attr_pattern = format!(r#"{}:{}="([^"]*)""#, prefix, key);
    let attr_re = Regex::new(&attr_pattern).ok()?;
    if let Some(captures) = attr_re.captures(content) {
        return captures
            .get(1)
            .map(|value| value.as_str().trim().to_string());
    }

    let element_pattern = format!(
        r#"(?s)<{}:{}>\s*([^<]+?)\s*</{}:{}>"#,
        prefix, key, prefix, key
    );
    Regex::new(&element_pattern)
        .ok()?
        .captures(content)
        .and_then(|captures| {
            captures
                .get(1)
                .map(|value| value.as_str().trim().to_string())
        })
}

fn get_namespaced_f64(content: &str, prefix: &str, key: &str) -> Option<f64> {
    extract_namespaced_scalar(content, prefix, key)?
        .trim_start_matches('+')
        .parse::<f64>()
        .ok()
}

fn is_xmp_true(value: Option<&String>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1")
}

fn orient_crop_bounds(
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    orientation: u16,
) -> (f64, f64, f64, f64) {
    let (x1, y1) = orient_normalized_point(left, top, orientation);
    let (x2, y2) = orient_normalized_point(right, bottom, orientation);
    (x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2))
}

pub(crate) fn orient_normalized_point(x: f64, y: f64, orientation: u16) -> (f64, f64) {
    let (transpose, flip_x, flip_y) =
        rawler::decoders::Orientation::from_u16(orientation).to_flips();
    let x = if flip_x { 1.0 - x } else { x };
    let y = if flip_y { 1.0 - y } else { y };
    if transpose { (y, x) } else { (x, y) }
}

pub(crate) fn rotate_point_clockwise(
    x: f64,
    y: f64,
    center_x: f64,
    center_y: f64,
    angle_degrees: f64,
) -> (f64, f64) {
    let angle = angle_degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let translated_x = x - center_x;
    let translated_y = y - center_y;
    (
        center_x + cos * translated_x - sin * translated_y,
        center_y + sin * translated_x + cos * translated_y,
    )
}

/// Unoriented image size in pixels, from the sidecar or the image itself.
fn xmp_image_dimensions(
    xmp_content: &str,
    fallback_image_dimensions: Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    let image_width = get_namespaced_f64(xmp_content, "tiff", "ImageWidth")
        .or_else(|| get_namespaced_f64(xmp_content, "exif", "PixelXDimension"))
        .or_else(|| fallback_image_dimensions.map(|dimensions| dimensions.0))?;
    let image_height = get_namespaced_f64(xmp_content, "tiff", "ImageLength")
        .or_else(|| get_namespaced_f64(xmp_content, "exif", "PixelYDimension"))
        .or_else(|| fallback_image_dimensions.map(|dimensions| dimensions.1))?;
    (image_width >= 1.0 && image_height >= 1.0).then_some((image_width, image_height))
}

fn xmp_orientation(xmp_content: &str) -> u16 {
    extract_namespaced_scalar(xmp_content, "tiff", "Orientation")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(1)
}

fn import_xmp_crop(
    xmp_content: &str,
    attrs: &HashMap<String, String>,
    adjustments: &mut Map<String, Value>,
    fallback_image_dimensions: Option<(f64, f64)>,
) {
    if !is_xmp_true(attrs.get("HasCrop")) || is_xmp_true(attrs.get("AlreadyApplied")) {
        return;
    }

    let Some(left) = get_attr_as_f64(attrs, "CropLeft") else {
        return;
    };
    let Some(top) = get_attr_as_f64(attrs, "CropTop") else {
        return;
    };
    let Some(right) = get_attr_as_f64(attrs, "CropRight") else {
        return;
    };
    let Some(bottom) = get_attr_as_f64(attrs, "CropBottom") else {
        return;
    };

    if ![left, top, right, bottom]
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        || right <= left
        || bottom <= top
    {
        return;
    }

    let Some((image_width, image_height)) =
        xmp_image_dimensions(xmp_content, fallback_image_dimensions)
    else {
        return;
    };

    let orientation = xmp_orientation(xmp_content);
    let (left, top, right, bottom) = orient_crop_bounds(left, top, right, bottom, orientation);
    let (oriented_width, oriented_height) = if (5..=8).contains(&orientation) {
        (image_height, image_width)
    } else {
        (image_width, image_height)
    };

    let crop_angle = get_attr_as_f64(attrs, "CropAngle")
        .filter(|angle| angle.is_finite())
        .unwrap_or(0.0);
    let (x, y, width, height) = if crop_angle.abs() > f64::EPSILON {
        // With a non-zero CropAngle, Lightroom stores CropLeft/CropTop and
        // CropRight/CropBottom as the opposing crop corners in the unrotated
        // image. RapidRAW applies its fine rotation before its axis-aligned
        // crop, so rotate both corners into that post-rotation coordinate
        // system. Treating the values as an ordinary bounding box changes the
        // crop aspect ratio (for _DSC1822, 5655x3585 instead of 5836x3284).
        let center_x = oriented_width / 2.0;
        let center_y = oriented_height / 2.0;
        let first = rotate_point_clockwise(
            left * oriented_width,
            top * oriented_height,
            center_x,
            center_y,
            -crop_angle,
        );
        let second = rotate_point_clockwise(
            right * oriented_width,
            bottom * oriented_height,
            center_x,
            center_y,
            -crop_angle,
        );
        let x = first.0.min(second.0).round().clamp(0.0, oriented_width);
        let y = first.1.min(second.1).round().clamp(0.0, oriented_height);
        let width = (first.0 - second.0).abs().round().min(oriented_width - x);
        let height = (first.1 - second.1).abs().round().min(oriented_height - y);
        (x, y, width, height)
    } else {
        let x = (left * oriented_width).ceil();
        let y = (top * oriented_height).ceil();
        let width = ((right - left) * oriented_width)
            .floor()
            .min(oriented_width - x);
        let height = ((bottom - top) * oriented_height)
            .floor()
            .min(oriented_height - y);
        (x, y, width, height)
    };
    if width < 1.0 || height < 1.0 {
        return;
    }

    adjustments.insert(
        "crop".to_string(),
        json!({
            "x": x,
            "y": y,
            "width": width,
            "height": height,
        }),
    );
    adjustments.insert("aspectRatio".to_string(), json!(width / height));

    if crop_angle.abs() > f64::EPSILON {
        // Lightroom stores CropAngle with the opposite sign from RapidRAW's
        // clockwise-positive rotation control.
        adjustments.insert("rotation".to_string(), json!(-crop_angle));
    }
}

fn import_lens_profile(attrs: &HashMap<String, String>, adjustments: &mut Map<String, Value>) {
    if !is_xmp_true(attrs.get("LensProfileEnable")) {
        return;
    }

    // The lens itself is resolved from EXIF after conversion, the same way as
    // RapidRAW's own automatic lens correction.
    adjustments.insert("lensCorrectionMode".to_string(), json!("auto"));
    adjustments.insert("lensDistortionEnabled".to_string(), json!(true));
    adjustments.insert("lensVignetteEnabled".to_string(), json!(true));
    adjustments.insert(
        "lensTcaEnabled".to_string(),
        json!(is_xmp_true(attrs.get("AutoLateralCA"))),
    );
    for (xmp_key, rapidraw_key) in [
        ("LensProfileDistortionScale", "lensDistortionAmount"),
        ("LensProfileVignettingScale", "lensVignetteAmount"),
    ] {
        if let Some(value) = get_attr_as_f64(attrs, xmp_key).filter(|value| value.is_finite()) {
            adjustments.insert(rapidraw_key.to_string(), json!(value.clamp(0.0, 200.0)));
        }
    }
}

fn probe_xmp_crop_image_dimensions(image_path: &Path) -> Option<(f64, f64)> {
    if crate::formats::is_raw_file(image_path) {
        let bytes = fs::read(image_path).ok()?;
        let source = RawSource::new_from_slice(&bytes);
        let decoder = rawler::get_decoder(&source).ok()?;
        // Dummy decoding retains the RAW active/default crop metadata without
        // allocating and developing the full pixel buffer.
        let raw_image = decoder
            .raw_image(&source, &RawDecodeParams::default(), true)
            .ok()?;
        let dimensions = raw_image
            .default_crop_area
            .or(raw_image.crop_area)
            .or(raw_image.active_area)
            .map(|crop| (crop.d.w, crop.d.h))
            .unwrap_or((raw_image.width, raw_image.height));
        let (width, height) = if raw_image.orientation.to_flips().0 {
            (dimensions.1, dimensions.0)
        } else {
            dimensions
        };
        return Some((width as f64, height as f64));
    }

    image::image_dimensions(image_path)
        .ok()
        .map(|(width, height)| (width as f64, height as f64))
}

/// RapidRAW's exposure slider moves 1.25 EV per unit, so a 1:1 copy of
/// Lightroom's EV value overshoots (+1.23 EV becomes +1.54 EV). Scaling fixes
/// that, but on its own it darkens an already too dark midrange. Keep it off
/// until it can be enabled together with the tone calibration.
const SCALE_LIGHTROOM_EXPOSURE_TO_RAPIDRAW_UNITS: bool = false;
const RAPIDRAW_EV_PER_EXPOSURE_UNIT: f64 = 1.25;

fn scale_lightroom_exposure(adjustments: &mut Map<String, Value>, enabled: bool) {
    if !enabled {
        return;
    }
    if let Some(ev) = adjustments.get("exposure").and_then(Value::as_f64) {
        adjustments.insert(
            "exposure".to_string(),
            json!(ev / RAPIDRAW_EV_PER_EXPOSURE_UNIT),
        );
    }
}

fn import_legacy_basic_adjustments(
    attrs: &HashMap<String, String>,
    adjustments: &mut Map<String, Value>,
    image_kind: Option<XmpImageKind>,
) {
    // Process Version 2003/2010 uses a different Basic panel from PV2012.
    // Only use these fields as fallbacks so a sidecar containing both process
    // representations always prefers the newer PV2012 values.
    if !attrs.contains_key("Exposure2012")
        && let Some(value) = get_attr_as_f64(attrs, "Exposure")
    {
        adjustments.insert("exposure".to_string(), json!(value.clamp(-5.0, 5.0)));
    }

    if !attrs.contains_key("Highlights2012")
        && let Some(value) = get_attr_as_f64(attrs, "HighlightRecovery")
    {
        adjustments.insert("highlights".to_string(), json!((-value).clamp(-100.0, 0.0)));
    }

    if !attrs.contains_key("Shadows2012")
        && let Some(value) = get_attr_as_f64(attrs, "FillLight")
    {
        adjustments.insert("shadows".to_string(), json!(value.clamp(0.0, 100.0)));
    }

    if !attrs.contains_key("Blacks2012")
        && let Some(value) = get_attr_as_f64(attrs, "Shadows")
    {
        const LEGACY_NEUTRAL_BLACKS: f64 = 5.0;
        adjustments.insert(
            "blacks".to_string(),
            json!((LEGACY_NEUTRAL_BLACKS - value).clamp(-100.0, 100.0)),
        );
    }

    if !attrs.contains_key("Exposure2012")
        && let Some(value) = get_attr_as_f64(attrs, "Brightness")
    {
        const LEGACY_NEUTRAL_BRIGHTNESS: f64 = 50.0;
        // Adobe's legacy Brightness control is a broad midtone adjustment, not
        // an exposure value. The former /10 conversion turned the common
        // Brightness=0 value into RapidRAW -5 and made rendered JPEG imports
        // nearly black. A four-image Lightroom holdout found that a ten-times
        // gentler response removes that failure and best matches the three
        // Brightness=0 samples on average. Keep reusable presets and RAW imports
        // unchanged because this experiment only covers already-rendered JPEGs.
        let legacy_brightness_to_rr = if image_kind == Some(XmpImageKind::RenderedJpeg) {
            100.0
        } else {
            10.0
        };
        adjustments.insert(
            "brightness".to_string(),
            json!(((value - LEGACY_NEUTRAL_BRIGHTNESS) / legacy_brightness_to_rr).clamp(-5.0, 5.0)),
        );
    }

    if !attrs.contains_key("Contrast2012")
        && let Some(value) = get_attr_as_f64(attrs, "Contrast")
    {
        const LEGACY_NEUTRAL_CONTRAST: f64 = 25.0;
        adjustments.insert(
            "contrast".to_string(),
            json!((value - LEGACY_NEUTRAL_CONTRAST).clamp(-100.0, 100.0)),
        );
    }

    if !attrs.contains_key("Clarity2012")
        && let Some(value) = get_attr_as_f64(attrs, "Clarity")
    {
        adjustments.insert("clarity".to_string(), json!(value.clamp(-100.0, 100.0)));
    }
}

fn interpolate_piecewise(value: f64, knots: &[(f64, f64)]) -> f64 {
    debug_assert!(!knots.is_empty());
    if value <= knots[0].0 {
        return knots[0].1;
    }
    for pair in knots.windows(2) {
        let (left_x, left_y) = pair[0];
        let (right_x, right_y) = pair[1];
        if value <= right_x {
            let fraction = (value - left_x) / (right_x - left_x);
            return left_y + fraction * (right_y - left_y);
        }
    }
    knots[knots.len() - 1].1
}

fn map_rendered_pv5_value(value: f64, knots: &[(f64, f64)]) -> Option<f64> {
    let (minimum, maximum) = (knots.first()?.0, knots.last()?.0);
    (value.is_finite() && (minimum..=maximum).contains(&value))
        .then(|| interpolate_piecewise(value, knots))
}

fn map_rendered_pv5_color_response(
    saturation: Option<f64>,
    vibrance: Option<f64>,
) -> (Option<f64>, Option<f64>) {
    const SATURATION_KNOTS: [(f64, f64); 12] = [
        (-100.0, -100.0),
        (-75.0, -74.0),
        (-50.0, -52.0),
        (-25.0, -28.0),
        (0.0, 0.0),
        (2.0, 2.0),
        (18.0, 18.0),
        (25.0, 22.0),
        (50.0, 36.0),
        (68.0, 50.0),
        (75.0, 64.0),
        (100.0, 80.0),
    ];
    const VIBRANCE_KNOTS: [(f64, f64); 11] = [
        (-100.0, -98.0),
        (-75.0, -98.0),
        (-50.0, -98.0),
        (-25.0, -72.0),
        (0.0, 0.0),
        (25.0, 20.0),
        (33.0, 20.0),
        (50.0, 46.0),
        (58.0, 60.0),
        (75.0, 84.0),
        (100.0, 98.0),
    ];
    // Each interaction knot stores the additional RapidRAW Saturation and
    // Vibrance response per point of positive Adobe Vibrance. The coefficients
    // come from joint target matching after the independent axes were fitted.
    const SATURATION_TRANSFER_KNOTS: [(f64, f64); 7] = [
        (0.0, 0.0),
        (2.0, 11.0 / 33.0),
        (25.0, 5.0 / 25.0),
        (50.0, 32.0 / 50.0),
        (68.0, 30.0 / 58.0),
        (75.0, 22.0 / 75.0),
        (100.0, 14.0 / 100.0),
    ];
    const VIBRANCE_TRANSFER_KNOTS: [(f64, f64); 7] = [
        (0.0, 0.0),
        (2.0, -11.0 / 33.0),
        (25.0, -10.0 / 25.0),
        (50.0, -32.0 / 50.0),
        (68.0, -30.0 / 58.0),
        (75.0, -29.0 / 75.0),
        (100.0, 2.0 / 100.0),
    ];

    let mut mapped_saturation =
        saturation.and_then(|value| map_rendered_pv5_value(value, &SATURATION_KNOTS));
    let mut mapped_vibrance =
        vibrance.and_then(|value| map_rendered_pv5_value(value, &VIBRANCE_KNOTS));
    if let (Some(adobe_saturation), Some(adobe_vibrance)) = (saturation, vibrance)
        && adobe_saturation > 0.0
        && adobe_vibrance > 0.0
        && adobe_saturation <= 100.0
        && adobe_vibrance <= 100.0
    {
        let saturation_transfer =
            interpolate_piecewise(adobe_saturation, &SATURATION_TRANSFER_KNOTS);
        let vibrance_transfer = interpolate_piecewise(adobe_saturation, &VIBRANCE_TRANSFER_KNOTS);
        mapped_saturation = mapped_saturation
            .map(|value| (value + saturation_transfer * adobe_vibrance).clamp(-100.0, 100.0));
        mapped_vibrance = mapped_vibrance
            .map(|value| (value + vibrance_transfer * adobe_vibrance).clamp(-100.0, 100.0));
    }
    (mapped_saturation, mapped_vibrance)
}

fn apply_rendered_pv5_policy(
    attrs: &HashMap<String, String>,
    adjustments: &mut Map<String, Value>,
    image_kind: Option<XmpImageKind>,
) {
    const EXPOSURE_KNOTS: [(f64, f64); 5] = [
        (-1.0, -0.4),
        (-0.5, -0.2),
        (0.0, 0.0),
        (0.5, 0.25),
        (1.0, 0.65),
    ];
    const BRIGHTNESS_KNOTS: [(f64, f64); 5] = [
        (0.0, -0.55),
        (25.0, -0.2),
        (50.0, 0.0),
        (75.0, 0.25),
        (100.0, 0.75),
    ];
    const CONTRAST_KNOTS: [(f64, f64); 5] = [
        (0.0, -10.0),
        (25.0, 0.0),
        (50.0, 10.0),
        (68.0, 14.0),
        (100.0, 24.0),
    ];
    const BLACKS_KNOTS: [(f64, f64); 8] = [
        (0.0, 48.0),
        (2.0, 20.0),
        (5.0, 0.0),
        (8.0, -16.0),
        (25.0, -94.0),
        (50.0, -100.0),
        (75.0, -100.0),
        (100.0, -100.0),
    ];
    const CLARITY_KNOTS: [(f64, f64); 10] = [
        (-100.0, -96.0),
        (-50.0, -96.0),
        (-25.0, -96.0),
        (0.0, 0.0),
        (25.0, 42.0),
        (31.0, 42.0),
        (50.0, 42.0),
        (52.0, 42.0),
        (75.0, 42.0),
        (100.0, 42.0),
    ];

    let process_version = get_attr_as_f64(attrs, "ProcessVersion");
    if image_kind != Some(XmpImageKind::RenderedJpeg)
        || !process_version.is_some_and(|version| (5.0..6.0).contains(&version))
        || attrs.contains_key("Exposure2012")
    {
        return;
    }

    // Lightroom's Process Version 5 Basic controls have materially different
    // response curves from RapidRAW's controls. These piecewise mappings were
    // measured on three color/luminance targets and then validated on natural
    // PV5 JPEG holdouts. Values outside a calibrated axis retain the established
    // legacy fallback imported above.
    adjustments.insert("toneMapper".to_string(), json!("basic"));
    for (xmp_key, rapidraw_key, knots) in [
        ("Exposure", "exposure", EXPOSURE_KNOTS.as_slice()),
        ("Brightness", "brightness", BRIGHTNESS_KNOTS.as_slice()),
        ("Contrast", "contrast", CONTRAST_KNOTS.as_slice()),
        ("Shadows", "blacks", BLACKS_KNOTS.as_slice()),
        ("Clarity", "clarity", CLARITY_KNOTS.as_slice()),
    ] {
        if let Some(mapped) =
            get_attr_as_f64(attrs, xmp_key).and_then(|value| map_rendered_pv5_value(value, knots))
        {
            adjustments.insert(rapidraw_key.to_string(), json!(mapped));
        }
    }

    // Two independent natural PV5 holdouts with the same legacy combination
    // (Brightness=0, Shadows=0, positive Clarity) both need less midtone
    // reduction than the isolated chart axis: IMG_8172 peaks at -0.15 and the
    // independent IMG_0151 legacy holdout peaks at -0.10. Use the slightly
    // better two-image average while keeping the residual narrowly guarded.
    let use_positive_clarity_brightness_residual = get_attr_as_f64(attrs, "Brightness")
        .is_some_and(|value| value.abs() < f64::EPSILON)
        && get_attr_as_f64(attrs, "Shadows").is_some_and(|value| value.abs() < f64::EPSILON)
        && get_attr_as_f64(attrs, "Clarity").is_some_and(|value| value >= 25.0);
    if use_positive_clarity_brightness_residual {
        adjustments.insert("brightness".to_string(), json!(-0.15));
    }

    let saturation = get_attr_as_f64(attrs, "Saturation").filter(|value| value.is_finite());
    let vibrance = get_attr_as_f64(attrs, "Vibrance").filter(|value| value.is_finite());
    let (mapped_saturation, mapped_vibrance) =
        map_rendered_pv5_color_response(saturation, vibrance);
    if let Some(value) = mapped_saturation {
        adjustments.insert("saturation".to_string(), json!(value));
    }
    if let Some(value) = mapped_vibrance {
        adjustments.insert("vibrance".to_string(), json!(value));
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_bounded_affine_adjustment(
    attrs: &HashMap<String, String>,
    adjustments: &mut Map<String, Value>,
    xmp_key: &str,
    rapidraw_key: &str,
    slope: f64,
    intercept: f64,
    minimum: f64,
    maximum: f64,
) {
    if let Some(value) = get_attr_as_f64(attrs, xmp_key).filter(|value| value.is_finite()) {
        adjustments.insert(
            rapidraw_key.to_string(),
            json!((intercept + slope * value).clamp(minimum, maximum)),
        );
    }
}

fn interpolate_rendered_pv2012_blacks_tail_residual(value: f64) -> (f64, f64) {
    // (Adobe Blacks2012, RapidRAW Shadows residual, RapidRAW Contrast residual)
    const KNOTS: [(f64, f64, f64); 3] = [
        (-100.0, -100.0, 87.0),
        (-75.0, -100.0, 5.0),
        (-50.0, -20.0, -4.0),
    ];
    let value = value.clamp(-100.0, -50.0);
    for index in 0..KNOTS.len() - 1 {
        let (left_x, left_shadows, left_contrast) = KNOTS[index];
        let (right_x, right_shadows, right_contrast) = KNOTS[index + 1];
        if value <= right_x {
            let fraction = (value - left_x) / (right_x - left_x);
            return (
                left_shadows + fraction * (right_shadows - left_shadows),
                left_contrast + fraction * (right_contrast - left_contrast),
            );
        }
    }
    (KNOTS[KNOTS.len() - 1].1, KNOTS[KNOTS.len() - 1].2)
}

fn map_rendered_pv2012_blacks_tail(value: f64) -> (f64, f64, f64) {
    const CURRENT_SLOPE: f64 = 0.482_411_120_021_384_6;
    const CURRENT_INTERCEPT: f64 = -21.201_603_849_238_17;
    const TAIL_START: f64 = -50.0;
    const TAIL_FULL_STRENGTH: f64 = -75.0;
    const SHADOWS_RESIDUAL_SCALE: f64 = 0.75;
    const CONTRAST_RESIDUAL_SCALE: f64 = 0.60;

    let value = value.clamp(-100.0, 100.0);
    let current = (CURRENT_INTERCEPT + CURRENT_SLOPE * value).clamp(-100.0, 100.0);
    if value >= TAIL_START {
        return (current, 0.0, 0.0);
    }

    // Controlled chart response calibration proved that Adobe's negative tail
    // is stronger than RapidRAW Blacks=-100. Preserve the existing renderer
    // baseline compensation through -50, blend continuously to the calibrated
    // combination by -75, then use Shadows/Contrast for the residual response.
    let tail_strength = ((TAIL_START - value) / (TAIL_START - TAIL_FULL_STRENGTH)).clamp(0.0, 1.0);
    let (shadows_residual, contrast_residual) =
        interpolate_rendered_pv2012_blacks_tail_residual(value);
    (
        (current + tail_strength * (-100.0 - current)).clamp(-100.0, 100.0),
        tail_strength * SHADOWS_RESIDUAL_SCALE * shadows_residual,
        tail_strength * CONTRAST_RESIDUAL_SCALE * contrast_residual,
    )
}

fn map_rendered_pv2012_blacks_clarity_contrast_residual(blacks: f64, clarity: f64) -> f64 {
    if clarity <= 0.0 || blacks >= 0.0 || blacks <= -55.0 {
        return 0.0;
    }
    let gate = if blacks >= -50.0 {
        0.25 + 0.95 * ((-blacks - 25.0) / 25.0).clamp(0.0, 1.0)
    } else {
        (blacks + 55.0) / 5.0 * 1.2
    };
    (clarity * gate).clamp(0.0, 60.0)
}

fn map_rendered_pv2012_saturation(value: f64) -> f64 {
    const POSITIVE_SATURATION_SCALE: f64 = 0.866_666_666_666_666_7;
    if value > 0.0 {
        value * POSITIVE_SATURATION_SCALE
    } else {
        value
    }
    .clamp(-100.0, 100.0)
}

fn map_rendered_pv2012_vibrance(value: f64) -> f64 {
    // Renderer-relative OKLab response matching on the color-response-v1
    // calibration batch. RapidRAW's positive Vibrance response is stronger and
    // differently curved than Lightroom's. Negative Vibrance is completed by
    // map_rendered_pv2012_color_response, which also contributes Saturation.
    const VIBRANCE_KNOTS: [(f64, f64); 9] = [
        (-100.0, 5.0),
        (-75.0, 16.0),
        (-50.0, 10.0),
        (-25.0, -4.0),
        (0.0, 0.0),
        (25.0, 20.0),
        (50.0, 42.0),
        (75.0, 70.0),
        (100.0, 100.0),
    ];

    let value = value.clamp(-100.0, 100.0);
    for index in 0..VIBRANCE_KNOTS.len() - 1 {
        let (left_x, left_y) = VIBRANCE_KNOTS[index];
        let (right_x, right_y) = VIBRANCE_KNOTS[index + 1];
        if value <= right_x {
            let fraction = (value - left_x) / (right_x - left_x);
            return left_y + fraction * (right_y - left_y);
        }
    }
    VIBRANCE_KNOTS[VIBRANCE_KNOTS.len() - 1].1
}

fn map_rendered_pv2012_negative_vibrance_saturation(value: f64) -> f64 {
    const KNOTS: [(f64, f64); 5] = [
        (-100.0, -71.0),
        (-75.0, -68.0),
        (-50.0, -51.0),
        (-25.0, -25.0),
        (0.0, 0.0),
    ];
    let value = value.clamp(-100.0, 0.0);
    for index in 0..KNOTS.len() - 1 {
        let (left_x, left_y) = KNOTS[index];
        let (right_x, right_y) = KNOTS[index + 1];
        if value <= right_x {
            let fraction = (value - left_x) / (right_x - left_x);
            return left_y + fraction * (right_y - left_y);
        }
    }
    0.0
}

fn map_rendered_pv2012_color_response(
    saturation: Option<f64>,
    vibrance: Option<f64>,
) -> (Option<f64>, Option<f64>) {
    const POSITIVE_INTERACTION_TRANSFER: f64 = 0.407_555_555_555_555_5;
    // The chart interaction samples begin at Adobe Saturation +25, where the
    // transfer is already fully active. A current-process natural-image holdout
    // (IMG_0151, Saturation +14 / Vibrance +38) showed that extending the
    // linear gate all the way to +25 leaves too much selective Vibrance. Reach
    // full strength at the lowest validated natural-image saturation instead.
    const FULL_INTERACTION_AT_SATURATION: f64 = 14.0;
    const NEGATIVE_SATURATION_OVERLAP: f64 = 1.053_223_320_337_292_5;
    const NEGATIVE_SATURATION_VIBRANCE_SCALE: f64 = 0.94;
    const POSITIVE_SATURATION_VIBRANCE_SCALE: f64 = 1.292_307_692_307_692_4;

    let mut mapped_saturation = saturation.map(map_rendered_pv2012_saturation);
    let mut mapped_vibrance = vibrance.map(map_rendered_pv2012_vibrance);
    if let Some(adobe_vibrance) = vibrance
        && adobe_vibrance < 0.0
    {
        let adobe_saturation = saturation.unwrap_or(0.0);
        let standard_saturation = map_rendered_pv2012_saturation(adobe_saturation);
        let negative_saturation = map_rendered_pv2012_negative_vibrance_saturation(adobe_vibrance);
        let mut combined_saturation = standard_saturation + negative_saturation;
        let mut combined_vibrance = map_rendered_pv2012_vibrance(adobe_vibrance);
        if adobe_saturation < 0.0 {
            combined_saturation +=
                NEGATIVE_SATURATION_OVERLAP * standard_saturation * negative_saturation / 100.0;
            combined_vibrance += NEGATIVE_SATURATION_VIBRANCE_SCALE
                * standard_saturation
                * (25.0 / adobe_vibrance.abs()).min(1.0);
        } else if adobe_saturation > 0.0 {
            combined_vibrance -= POSITIVE_SATURATION_VIBRANCE_SCALE
                * standard_saturation
                * (50.0 / adobe_vibrance.abs()).min(1.0);
        }
        return (
            Some(combined_saturation.clamp(-100.0, 100.0)),
            Some(combined_vibrance.clamp(-100.0, 100.0)),
        );
    }
    if saturation.unwrap_or(0.0) == 0.0
        && let Some(adobe_vibrance) = vibrance
        && adobe_vibrance > 0.0
        && adobe_vibrance < 18.0
    {
        // Six natural-photo holdouts with Adobe Vibrance +6…+17 and no
        // Saturation all benefited from a small global-Saturation assist while
        // retaining the calibrated selective-Vibrance response. Ramp in by +6
        // and taper to zero at the first independently validated higher-value
        // group so the established mid/high Vibrance mapping stays unchanged.
        let low_gate = (adobe_vibrance / 6.0).clamp(0.0, 1.0);
        let high_gate = (18.0 - adobe_vibrance).clamp(0.0, 1.0);
        mapped_saturation = Some(4.0 * low_gate * high_gate);
    }
    if let (Some(adobe_saturation), Some(adobe_vibrance)) = (saturation, vibrance)
        && adobe_saturation > 0.0
        && adobe_vibrance > 0.0
    {
        let gate = (adobe_saturation / FULL_INTERACTION_AT_SATURATION).clamp(0.0, 1.0);
        let transfer = POSITIVE_INTERACTION_TRANSFER * adobe_vibrance * gate;
        mapped_saturation = mapped_saturation.map(|value| (value + transfer).clamp(-100.0, 100.0));
        mapped_vibrance = mapped_vibrance.map(|value| (value - transfer).clamp(-100.0, 100.0));
    }
    (mapped_saturation, mapped_vibrance)
}

fn apply_rendered_pv2012_policy(
    attrs: &HashMap<String, String>,
    adjustments: &mut Map<String, Value>,
    image_kind: Option<XmpImageKind>,
) {
    // (Adobe key, RapidRAW key, slope, intercept, minimum, maximum)
    const RENDERED_PV2012_BASIC_POLICY_V1: [(&str, &str, f64, f64, f64, f64); 5] = [
        ("Exposure2012", "brightness", 1.0, 0.0, -5.0, 5.0),
        ("Contrast2012", "contrast", 1.0, 0.0, -100.0, 100.0),
        (
            "Highlights2012",
            "highlights",
            0.473_377_978_459_916_9,
            -3.844_543_841_733_09,
            -100.0,
            100.0,
        ),
        ("Shadows2012", "shadows", 1.0, 0.0, -100.0, 100.0),
        ("Whites2012", "whites", 1.0, 0.0, -100.0, 100.0),
    ];

    if image_kind != Some(XmpImageKind::RenderedJpeg)
        || (!attrs.contains_key("Blacks2012")
            && !RENDERED_PV2012_BASIC_POLICY_V1
                .iter()
                .any(|(xmp_key, ..)| attrs.contains_key(*xmp_key)))
    {
        return;
    }

    // Adobe PV2012 rendered-JPEG policy with the validated continuous Blacks
    // tail. Highlights retains its grouped-CV affine mapping; the other Basic
    // controls are bounded identities. Exposure is routed through RapidRAW
    // brightness because that produced the closest calibrated response.
    adjustments.insert("toneMapper".to_string(), json!("basic"));
    if attrs.contains_key("Exposure2012") {
        adjustments.insert("exposure".to_string(), json!(0.0));
    }
    for (xmp_key, rapidraw_key, slope, intercept, minimum, maximum) in
        RENDERED_PV2012_BASIC_POLICY_V1
    {
        insert_bounded_affine_adjustment(
            attrs,
            adjustments,
            xmp_key,
            rapidraw_key,
            slope,
            intercept,
            minimum,
            maximum,
        );
    }
    if let Some(adobe_blacks) =
        get_attr_as_f64(attrs, "Blacks2012").filter(|value| value.is_finite())
    {
        let (mapped_blacks, shadows_delta, contrast_delta) =
            map_rendered_pv2012_blacks_tail(adobe_blacks);
        adjustments.insert("blacks".to_string(), json!(mapped_blacks));
        for (key, delta) in [("shadows", shadows_delta), ("contrast", contrast_delta)] {
            if delta.abs() > f64::EPSILON {
                let base = adjustments.get(key).and_then(Value::as_f64).unwrap_or(0.0);
                adjustments.insert(key.to_string(), json!((base + delta).clamp(-100.0, 100.0)));
            }
        }
    }
    if let (Some(adobe_blacks), Some(adobe_clarity)) = (
        get_attr_as_f64(attrs, "Blacks2012").filter(|value| value.is_finite()),
        get_attr_as_f64(attrs, "Clarity2012").filter(|value| value.is_finite()),
    ) {
        let residual =
            map_rendered_pv2012_blacks_clarity_contrast_residual(adobe_blacks, adobe_clarity);
        if residual > 0.0 {
            let base = adjustments
                .get("contrast")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            adjustments.insert(
                "contrast".to_string(),
                json!((base + residual).clamp(-100.0, 100.0)),
            );
        }
    }
    let adobe_saturation = get_attr_as_f64(attrs, "Saturation").filter(|value| value.is_finite());
    let adobe_vibrance = get_attr_as_f64(attrs, "Vibrance").filter(|value| value.is_finite());
    let (mapped_saturation, mapped_vibrance) =
        map_rendered_pv2012_color_response(adobe_saturation, adobe_vibrance);
    if let Some(value) = mapped_saturation {
        adjustments.insert("saturation".to_string(), json!(value));
    }
    if let Some(value) = mapped_vibrance {
        adjustments.insert("vibrance".to_string(), json!(value));
    }

    // Rendered JPEGs store relative white-balance edits separately from RAW
    // absolute Temperature/Tint. Joint target matching on IMG_0151 selected
    // 0.8 Temperature / 0.6 Tint when both positive controls interact. For
    // Temperature-only edits, a broader natural-photo holdout selected 1.3x for
    // modest values, tapering continuously to 1.0x by Adobe +21; that avoids the
    // cool bias of the joint-control scale without over-warming larger edits.
    // Preserve the half-scale Temperature mapping when Tint is negative, and
    // keep negative controls neutral until those quadrants have an independent
    // calibration.
    let incremental_tint =
        get_attr_as_f64(attrs, "IncrementalTint").filter(|value| value.is_finite());
    if let Some(value) = get_attr_as_f64(attrs, "IncrementalTemperature")
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        let scale = match incremental_tint {
            Some(tint) if tint < 0.0 => 0.5,
            Some(tint) if tint > 0.0 => 0.8,
            _ => 1.3 - 0.3 * ((value - 12.0) / 9.0).clamp(0.0, 1.0),
        };
        adjustments.insert(
            "temperature".to_string(),
            json!((scale * value).clamp(0.0, 100.0)),
        );
        let adobe_vibrance = get_attr_as_f64(attrs, "Vibrance").unwrap_or(0.0);
        let adobe_saturation = get_attr_as_f64(attrs, "Saturation").unwrap_or(0.0);
        if incremental_tint.unwrap_or(0.0) == 0.0
            && adobe_saturation == 0.0
            && adobe_vibrance > 0.0
            && adobe_vibrance < 18.0
        {
            // Both eligible natural-photo holdouts retained a green residual
            // after their Temperature-only and low-Vibrance mappings. A small
            // magenta assist improved both, while applying it to the broader
            // Temperature-only set regressed several Vibrance >=18 images.
            adjustments.insert("tint".to_string(), json!(4.5));
        }
    }
    if let Some(value) = incremental_tint.filter(|value| *value > 0.0) {
        adjustments.insert("tint".to_string(), json!((0.6 * value).clamp(0.0, 100.0)));
    }
}

fn unescape_xml(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let decoded = rest.find(';').and_then(|end| {
            let entity = &rest[1..end];
            let c = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .map(|hex| u32::from_str_radix(hex, 16))
                    .or_else(|| entity.strip_prefix('#').map(|dec| dec.parse::<u32>()))
                    .and_then(Result::ok)
                    .and_then(char::from_u32),
            };
            c.map(|c| (c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn extract_xmp_name(xmp_content: &str) -> Option<String> {
    regex!(r#"(?s)<crs:Name>.*?<rdf:Alt>.*?<rdf:li[^>]*>([^<]+)</rdf:li>.*?</crs:Name>"#)
        .captures(xmp_content)
        .and_then(|c| {
            c.get(1)
                .map(|m| unescape_xml(m.as_str()).trim().to_string())
        })
}

fn extract_tone_curve_points(xmp_str: &str, curve_name: &str) -> Option<Vec<Value>> {
    let pattern = format!(
        r"(?s)<crs:{}>\s*<rdf:Seq>(.*?)</rdf:Seq>\s*</crs:{}>",
        curve_name, curve_name
    );
    let re = Regex::new(&pattern).ok()?;
    let captures = re.captures(xmp_str)?;
    let seq_content = captures.get(1)?.as_str();

    let point_re = regex!(r"<rdf:li>(\d+),\s*(\d+)</rdf:li>");
    let mut points = Vec::new();

    for point_cap in point_re.captures_iter(seq_content) {
        let x: u32 = point_cap.get(1)?.as_str().parse().ok()?;
        let y: u32 = point_cap.get(2)?.as_str().parse().ok()?;

        let mut point = Map::new();
        point.insert("x".to_string(), Value::Number(x.into()));
        point.insert("y".to_string(), Value::Number(y.into()));
        points.push(Value::Object(point));
    }

    if points.is_empty() {
        None
    } else {
        Some(points)
    }
}

const MAX_RAPIDRAW_CURVE_POINTS: usize = 16;

fn strip_nested_looks(xmp_content: &str) -> std::borrow::Cow<'_, str> {
    regex!(r"(?s)<crs:Look>.*?</crs:Look>").replace_all(xmp_content, "")
}

fn curve_points_as_f64(points: &[Value]) -> Vec<(f64, f64)> {
    points
        .iter()
        .filter_map(|point| Some((point["x"].as_f64()?, point["y"].as_f64()?)))
        .collect()
}

// Port of apply_curve in shader.wgsl (monotone cubic Hermite on 0-255), so a
// composed curve matches what RapidRAW renders for the source curves.
fn evaluate_rapidraw_curve(points: &[(f64, f64)], x: f64) -> f64 {
    let count = points.len();
    if count < 2 {
        return x;
    }
    if x <= points[0].0 {
        return points[0].1;
    }
    if x >= points[count - 1].0 {
        return points[count - 1].1;
    }
    for i in 0..count - 1 {
        let (p1, p2) = (points[i], points[i + 1]);
        if x > p2.0 {
            continue;
        }
        let p0 = points[i.max(1) - 1];
        let p3 = points[(i + 2).min(count - 1)];
        let delta_before = (p1.1 - p0.1) / (p1.0 - p0.0).max(0.001);
        let delta_current = (p2.1 - p1.1) / (p2.0 - p1.0).max(0.001);
        let delta_after = (p3.1 - p2.1) / (p3.0 - p2.0).max(0.001);
        let mut tangent_at_p1 = if i == 0 {
            delta_current
        } else if delta_before * delta_current <= 0.0 {
            0.0
        } else {
            (delta_before + delta_current) / 2.0
        };
        let mut tangent_at_p2 = if i + 1 == count - 1 {
            delta_current
        } else if delta_current * delta_after <= 0.0 {
            0.0
        } else {
            (delta_current + delta_after) / 2.0
        };
        if delta_current != 0.0 {
            let alpha = tangent_at_p1 / delta_current;
            let beta = tangent_at_p2 / delta_current;
            if alpha * alpha + beta * beta > 9.0 {
                let tau = 3.0 / (alpha * alpha + beta * beta).sqrt();
                tangent_at_p1 *= tau;
                tangent_at_p2 *= tau;
            }
        }
        let dx = p2.0 - p1.0;
        if dx <= 0.0 {
            return p1.1;
        }
        let t = (x - p1.0) / dx;
        let (t2, t3) = (t * t, t * t * t);
        let y = (2.0 * t3 - 3.0 * t2 + 1.0) * p1.1
            + (t3 - 2.0 * t2 + t) * tangent_at_p1 * dx
            + (-2.0 * t3 + 3.0 * t2) * p2.1
            + (t3 - t2) * tangent_at_p2 * dx;
        return y.clamp(0.0, 255.0);
    }
    points[count - 1].1
}

/// Tone curves of nested looks (the Adobe Color profile look, creative
/// profiles), each scaled by its crs:Amount, in the order Lightroom applies them.
fn extract_nested_look_luma_curves(xmp_content: &str) -> Vec<(f64, Vec<(f64, f64)>)> {
    regex!(r"(?s)<crs:Look>(.*?)</crs:Look>")
        .captures_iter(xmp_content)
        .filter_map(|captures| {
            let look = captures.get(1)?.as_str();
            let amount = extract_namespaced_scalar(look, "crs", "Amount")
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .unwrap_or(1.0)
                .clamp(0.0, 2.0);
            let points = curve_points_as_f64(&extract_tone_curve_points(look, "ToneCurvePV2012")?);
            let is_identity = points.iter().all(|(x, y)| x == y);
            (amount > 0.0 && !is_identity).then_some((amount, points))
        })
        .collect()
}

/// Lightroom applies the profile look before the user's tone curve. RapidRAW
/// has one luma curve, so sample the composition at evenly spaced inputs.
fn compose_luma_curve_with_looks(
    looks: &[(f64, Vec<(f64, f64)>)],
    outer: Option<&[(f64, f64)]>,
) -> Vec<Value> {
    let step = 255.0 / (MAX_RAPIDRAW_CURVE_POINTS - 1) as f64;
    (0..MAX_RAPIDRAW_CURVE_POINTS)
        .map(|index| {
            let x = (index as f64 * step).round();
            let mut y = x;
            for (amount, points) in looks {
                y += amount * (evaluate_rapidraw_curve(points, y) - y);
            }
            if let Some(outer) = outer {
                y = evaluate_rapidraw_curve(outer, y);
            }
            json!({ "x": x as u32, "y": y.round().clamp(0.0, 255.0) as u32 })
        })
        .collect()
}

pub fn convert_xmp_to_preset(xmp_content: &str) -> Result<Preset, String> {
    convert_xmp_to_preset_with_crop(xmp_content, false, Some(5500.0), None, None, None, None)
}

#[cfg(test)]
pub fn convert_xmp_sidecar_to_preset(xmp_content: &str) -> Result<Preset, String> {
    convert_xmp_sidecar_to_preset_with_as_shot_temperature(xmp_content, None)
}

pub fn convert_xmp_sidecar_to_preset_for_image(
    xmp_content: &str,
    image_path: &Path,
) -> Result<Preset, String> {
    convert_xmp_sidecar_to_preset_for_image_with_mask_report(xmp_content, image_path)
        .map(|(preset, _)| preset)
}

pub fn convert_xmp_sidecar_to_preset_for_image_with_mask_report(
    xmp_content: &str,
    image_path: &Path,
) -> Result<(Preset, crate::lightroom_masks::MaskImportSummary), String> {
    let extension = image_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    let image_kind =
        if extension.as_deref() == Some("dng") || crate::formats::is_raw_file(image_path) {
            XmpImageKind::Raw
        } else if matches!(extension.as_deref(), Some("jpg" | "jpeg")) {
            XmpImageKind::RenderedJpeg
        } else {
            XmpImageKind::Rendered
        };
    let geometry_needs_image_dimensions = (parse_xmp_attributes(xmp_content)
        .ok()
        .is_some_and(|attrs| is_xmp_true(attrs.get("HasCrop")))
        || crate::lightroom_masks::has_mask_group_corrections(xmp_content))
        && (get_namespaced_f64(xmp_content, "tiff", "ImageWidth")
            .or_else(|| get_namespaced_f64(xmp_content, "exif", "PixelXDimension"))
            .is_none()
            || get_namespaced_f64(xmp_content, "tiff", "ImageLength")
                .or_else(|| get_namespaced_f64(xmp_content, "exif", "PixelYDimension"))
                .is_none());
    let fallback_image_dimensions = geometry_needs_image_dimensions
        .then(|| probe_xmp_crop_image_dimensions(image_path))
        .flatten();
    let mut summary = crate::lightroom_masks::MaskImportSummary::default();
    let preset = convert_xmp_to_preset_with_crop(
        xmp_content,
        true,
        None,
        Some(image_kind),
        fallback_image_dimensions,
        Some(image_path),
        Some(&mut summary),
    )?;
    Ok((preset, summary))
}

#[cfg(test)]
fn convert_adobe_camera_raw_xmp_to_preset(
    xmp_content: &str,
    image_kind: XmpImageKind,
) -> Result<Preset, String> {
    convert_xmp_to_preset_with_crop(xmp_content, true, None, Some(image_kind), None, None, None)
}

#[cfg(test)]
fn adobe_camera_raw_already_applied(xmp_content: &str) -> Result<bool, String> {
    Ok(is_xmp_true(
        parse_xmp_attributes(xmp_content)?.get("AlreadyApplied"),
    ))
}

#[cfg(test)]
fn convert_xmp_sidecar_to_preset_with_as_shot_temperature(
    xmp_content: &str,
    as_shot_temperature: Option<f64>,
) -> Result<Preset, String> {
    convert_xmp_to_preset_with_crop(
        xmp_content,
        true,
        as_shot_temperature,
        None,
        None,
        None,
        None,
    )
}

/// Lightroom settings in a sidecar that the import could not carry over, as
/// stable keys for the import report.
pub fn lightroom_settings_not_transferred(xmp_content: &str, preset: &Preset) -> Vec<&'static str> {
    let (_, summary) = crate::lightroom_masks::import_lightroom_masks(xmp_content, None, |_| {});
    lightroom_settings_not_transferred_with_mask_summary(xmp_content, preset, &summary)
}

pub fn lightroom_settings_not_transferred_with_mask_summary(
    xmp_content: &str,
    preset: &Preset,
    masks: &crate::lightroom_masks::MaskImportSummary,
) -> Vec<&'static str> {
    let Ok(attrs) = parse_xmp_attributes(xmp_content) else {
        return Vec::new();
    };
    let mut items = Vec::new();

    // Lightroom Classic sidecars carry no as-shot values, so a custom white
    // balance often can't be turned into RapidRAW's relative controls.
    let custom_white_balance = attrs
        .get("WhiteBalance")
        .is_some_and(|value| !value.eq_ignore_ascii_case("As Shot"));
    if custom_white_balance
        && preset.adjustments.get("temperature").is_none()
        && preset.adjustments.get("tint").is_none()
    {
        items.push("whiteBalance");
    }

    // Only a look's tone curve is imported, never its colour table.
    let has_profile_look = regex!(r"(?s)<crs:Look>(.*?)</crs:Look>")
        .captures_iter(xmp_content)
        .any(|captures| {
            extract_namespaced_scalar(&captures[1], "crs", "Amount")
                .and_then(|value| value.parse::<f64>().ok())
                .is_none_or(|amount| amount > 0.0)
        });
    if has_profile_look {
        items.push("profileLook");
    }

    // An already applied AI Denoise lives in the enhanced DNG's pixels.
    if attrs.keys().any(|key| key.starts_with("EnhanceDenoise"))
        && !is_xmp_true(attrs.get("EnhanceDenoiseAlreadyApplied"))
    {
        items.push("aiDenoise");
    }

    // Only crs:MaskGroupBasedCorrections are imported; the older per-tool
    // lists are reported as they are.
    let imported_masks = preset
        .adjustments
        .get("masks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if masks.skipped_unsupported > 0
        || imported_masks < masks.convertible
        || regex!(
            r"(?s)<crs:(?:PaintBasedCorrections|GradientBasedCorrections|CircularGradientBasedCorrections)>\s*<rdf:Seq>\s*<rdf:li"
        )
        .is_match(xmp_content)
    {
        items.push("masks");
    }
    if masks.skipped_ai > 0 {
        items.push("aiMasks");
    }
    if masks.with_brush_tables > 0 {
        items.push("maskBrushTable");
    }
    if masks.with_unmapped_adjustments > 0 && imported_masks > 0 {
        items.push("localAdjustments");
    }

    if regex!(r"(?s)<crs:PointColors>\s*<rdf:Seq>\s*<rdf:li").is_match(xmp_content) {
        items.push("pointColor");
    }

    items
}

fn parse_xmp_attributes(xmp_content: &str) -> Result<HashMap<String, String>, String> {
    // Lightroom sidecars can contain nested rdf:Description elements for
    // profiles and looks. Only the outer description represents the image's
    // active settings; nested attributes must not overwrite those values.
    let description_re = Regex::new(r#"(?s)<rdf:Description\b([^>]*)>"#)
        .map_err(|e| format!("Regex compilation failed: {}", e))?;
    let attribute_source = description_re
        .captures(xmp_content)
        .and_then(|captures| captures.get(1))
        .map_or(xmp_content, |attributes| attributes.as_str());

    let attr_re = regex!(r#"crs:([A-Za-z0-9]+)="([^"]*)""#);
    let mut attrs: HashMap<String, String> = HashMap::new();
    for cap in attr_re.captures_iter(attribute_source) {
        attrs.insert(cap[1].to_string(), cap[2].to_string());
    }

    let elem_re = Regex::new(r#"(?s)<crs:([A-Za-z0-9]+)>\s*([^<]+?)\s*</crs:([A-Za-z0-9]+)>"#)
        .map_err(|e| format!("Regex compilation failed: {}", e))?;
    for cap in elem_re.captures_iter(xmp_content) {
        if cap[1] == cap[3] {
            attrs
                .entry(cap[1].to_string())
                .or_insert_with(|| cap[2].trim().to_string());
        }
    }

    Ok(attrs)
}

const BASIC_MAPPINGS: &[(&str, &str)] = &[
    ("Exposure2012", "exposure"),
    ("Contrast2012", "contrast"),
    ("Highlights2012", "highlights"),
    ("Shadows2012", "shadows"),
    ("Whites2012", "whites"),
    ("Blacks2012", "blacks"),
    ("Clarity2012", "clarity"),
    ("Dehaze", "dehaze"),
    ("Vibrance", "vibrance"),
    ("Saturation", "saturation"),
    ("Texture", "structure"),
    ("LuminanceSmoothing", "lumaNoiseReduction"),
    ("ColorNoiseReduction", "colorNoiseReduction"),
    ("ChromaticAberrationRedCyan", "chromaticAberrationRedCyan"),
    (
        "ChromaticAberrationBlueYellow",
        "chromaticAberrationBlueYellow",
    ),
    ("PostCropVignetteAmount", "vignetteAmount"),
    ("PostCropVignetteMidpoint", "vignetteMidpoint"),
    ("PostCropVignetteFeather", "vignetteFeather"),
    ("PostCropVignetteRoundness", "vignetteRoundness"),
    ("GrainAmount", "grainAmount"),
    ("GrainSize", "grainSize"),
    ("GrainFrequency", "grainRoughness"),
    ("ColorGradeBlending", "blending"),
];

pub(crate) fn is_mapped_xmp_scalar(key: &str) -> bool {
    if BASIC_MAPPINGS.iter().any(|(name, _)| *name == key) {
        return true;
    }
    for prefix in [
        "HueAdjustment",
        "SaturationAdjustment",
        "LuminanceAdjustment",
        "GrayMixer",
    ] {
        if let Some(color) = key.strip_prefix(prefix)
            && [
                "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
            ]
            .contains(&color)
        {
            return true;
        }
    }
    matches!(
        key,
        "Sharpness"
            | "SharpenEdgeMasking"
            | "Exposure"
            | "Brightness"
            | "Contrast"
            | "Recovery"
            | "FillLight"
            | "Shadows"
            | "Clarity"
            | "Temperature"
            | "Tint"
            | "AsShotTemperature"
            | "AsShotTint"
            | "WhiteBalance"
            | "IncrementalTemperature"
            | "IncrementalTint"
            | "ConvertToGrayscale"
            | "HasCrop"
            | "CropLeft"
            | "CropRight"
            | "CropTop"
            | "CropBottom"
            | "CropAngle"
            | "CropConstrainAspectRatio"
            | "LensProfileEnable"
            | "LensProfileDistortionScale"
            | "LensProfileVignettingScale"
            | "AutoLateralCA"
            | "SplitToningShadowHue"
            | "SplitToningShadowSaturation"
            | "SplitToningHighlightHue"
            | "SplitToningHighlightSaturation"
            | "SplitToningBalance"
            | "ColorGradeMidtoneHue"
            | "ColorGradeMidtoneSat"
            | "ColorGradeShadowLum"
            | "ColorGradeMidtoneLum"
            | "ColorGradeHighlightLum"
            | "ColorGradeGlobalHue"
            | "ColorGradeGlobalSat"
            | "ColorGradeGlobalLum"
            | "Version"
            | "ProcessVersion"
            | "HasSettings"
            | "AlreadyApplied"
            | "CameraProfile"
            | "ToneCurveName"
            | "ToneCurveName2012"
    )
}

fn convert_xmp_to_preset_with_crop(
    xmp_content: &str,
    include_crop_transform: bool,
    as_shot_temperature: Option<f64>,
    image_kind: Option<XmpImageKind>,
    fallback_image_dimensions: Option<(f64, f64)>,
    image_path: Option<&Path>,
    mask_report: Option<&mut crate::lightroom_masks::MaskImportSummary>,
) -> Result<Preset, String> {
    let attrs = parse_xmp_attributes(xmp_content)?;
    let convert_to_grayscale = is_xmp_true(attrs.get("ConvertToGrayscale"));

    let mut adjustments = Map::new();
    let mut hsl_map = Map::new();
    let mut color_grading_map = Map::new();
    let mut curves_map = Map::new();

    for &(xmp_key, rr_key) in BASIC_MAPPINGS {
        if let Some(raw_val) = attrs.get(xmp_key)
            && let Some(num) = parse_num(raw_val.trim_start_matches('+'))
            && let Some(json_val) = num_to_json(num)
        {
            if rr_key == "blending" {
                color_grading_map.insert(rr_key.to_string(), json_val);
            } else {
                adjustments.insert(rr_key.to_string(), json_val);
            }
        }
    }

    import_legacy_basic_adjustments(&attrs, &mut adjustments, image_kind);
    scale_lightroom_exposure(&mut adjustments, SCALE_LIGHTROOM_EXPOSURE_TO_RAPIDRAW_UNITS);
    apply_rendered_pv5_policy(&attrs, &mut adjustments, image_kind);
    apply_rendered_pv2012_policy(&attrs, &mut adjustments, image_kind);
    if convert_to_grayscale {
        // RapidRAW has no separate monochrome mode. Full desaturation produces
        // a luminance image, while the HSL luminance controls below approximate
        // Lightroom's per-color B&W mixer before that conversion.
        adjustments.insert("saturation".to_string(), json!(-100.0));
    }

    if let Some(sharpness_val) = get_attr_as_f64(&attrs, "Sharpness") {
        let scaled_sharpness = (sharpness_val / 150.0) * 100.0;
        adjustments.insert(
            "sharpness".to_string(),
            json!(scaled_sharpness.clamp(0.0, 100.0)),
        );
    }

    if let Some(masking) = get_attr_as_f64(&attrs, "SharpenEdgeMasking").filter(|v| v.is_finite()) {
        // Lightroom's Masking runs 0-100 and RapidRAW's sharpening threshold
        // slider 0-80; both leave flat areas unsharpened as they rise.
        adjustments.insert(
            "sharpnessThreshold".to_string(),
            json!((masking * 0.8).clamp(0.0, 80.0)),
        );
    }

    let white_balance_is_as_shot = attrs
        .get("WhiteBalance")
        .is_some_and(|value| value.eq_ignore_ascii_case("As Shot"));

    // Camera Raw temperature and tint for rendered sources do not have the RAW
    // as-shot baseline needed by RapidRAW's relative controls. Omit them rather
    // than guessing. Preset imports keep their established neutral baseline.
    let can_convert_white_balance = !matches!(
        image_kind,
        Some(XmpImageKind::RenderedJpeg | XmpImageKind::Rendered)
    );

    if can_convert_white_balance
        && !white_balance_is_as_shot
        && let Some(adjusted_k) = get_attr_as_f64(&attrs, "Temperature")
    {
        const MAX_MIRED_SHIFT: f64 = 150.0;
        if let Some(as_shot_k) = get_attr_as_f64(&attrs, "AsShotTemperature")
            .or(as_shot_temperature)
            .filter(|temperature| *temperature > 0.0)
            && adjusted_k > 0.0
        {
            let mired_adjusted = 1_000_000.0 / adjusted_k;
            let mired_as_shot = 1_000_000.0 / as_shot_k;
            let mired_delta = mired_adjusted - mired_as_shot;
            let temp_value = (-mired_delta / MAX_MIRED_SHIFT) * 100.0;
            adjustments.insert(
                "temperature".to_string(),
                json!(temp_value.clamp(-100.0, 100.0)),
            );
        }
    }

    if can_convert_white_balance
        && !white_balance_is_as_shot
        && let Some(tint_val) = get_attr_as_f64(&attrs, "Tint")
    {
        // Lightroom stores tint as an absolute, camera-profile-dependent value.
        // Sidecars therefore need an as-shot baseline before that value can be
        // translated into RapidRAW's relative tint control. Preset XMP files do
        // not describe a particular source image, so retain their established
        // direct conversion behavior.
        let tint_delta = get_attr_as_f64(&attrs, "AsShotTint")
            .map(|as_shot_tint| tint_val - as_shot_tint)
            .or((!include_crop_transform).then_some(tint_val));
        if let Some(tint_delta) = tint_delta {
            let scaled_tint = (tint_delta / 150.0) * 100.0;
            adjustments.insert("tint".to_string(), json!(scaled_tint.clamp(-100.0, 100.0)));
        }
    }

    let colors = [
        ("Red", "reds"),
        ("Orange", "oranges"),
        ("Yellow", "yellows"),
        ("Green", "greens"),
        ("Aqua", "aquas"),
        ("Blue", "blues"),
        ("Purple", "purples"),
        ("Magenta", "magentas"),
    ];
    for (src, dst) in colors {
        let mut color_map = Map::new();
        if convert_to_grayscale {
            if let Some(value) = get_attr_as_f64(&attrs, &format!("GrayMixer{}", src))
                .filter(|value| value.is_finite())
            {
                color_map.insert("luminance".to_string(), json!(value.clamp(-100.0, 100.0)));
            }
        } else {
            if let Some(raw) = attrs.get(&format!("HueAdjustment{}", src))
                && let Some(num) = parse_num(raw.trim_start_matches('+'))
                && let Some(Value::Number(number)) = num_to_json(num)
                && let Some(value) = number.as_f64()
            {
                color_map.insert("hue".to_string(), json!(value * 0.75));
            }
            if let Some(raw) = attrs.get(&format!("SaturationAdjustment{}", src))
                && let Some(num) = parse_num(raw.trim_start_matches('+'))
                && let Some(json_val) = num_to_json(num)
            {
                color_map.insert("saturation".to_string(), json_val);
            }
            if let Some(raw) = attrs.get(&format!("LuminanceAdjustment{}", src))
                && let Some(num) = parse_num(raw.trim_start_matches('+'))
                && let Some(json_val) = num_to_json(num)
            {
                color_map.insert("luminance".to_string(), json_val);
            }
        }
        if !color_map.is_empty() {
            hsl_map.insert(dst.to_string(), Value::Object(color_map));
        }
    }
    if !hsl_map.is_empty() {
        adjustments.insert("hsl".to_string(), Value::Object(hsl_map));
    }
    let mut shadows_map = Map::new();
    let mut midtones_map = Map::new();
    let mut highlights_map = Map::new();
    let mut global_map = Map::new();
    if let Some(raw) = attrs.get("SplitToningShadowHue")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        shadows_map.insert("hue".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeMidtoneHue")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        midtones_map.insert("hue".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("SplitToningHighlightHue")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        highlights_map.insert("hue".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("SplitToningShadowSaturation")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        shadows_map.insert("saturation".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeMidtoneSat")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        midtones_map.insert("saturation".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("SplitToningHighlightSaturation")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        highlights_map.insert("saturation".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeShadowLum")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        shadows_map.insert("luminance".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeMidtoneLum")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        midtones_map.insert("luminance".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeHighlightLum")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        highlights_map.insert("luminance".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeGlobalHue")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        global_map.insert("hue".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeGlobalSat")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        global_map.insert("saturation".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("ColorGradeGlobalLum")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        global_map.insert("luminance".to_string(), json_val);
    }
    if let Some(raw) = attrs.get("SplitToningBalance")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        color_grading_map.insert("balance".to_string(), json_val);
    }
    if !shadows_map.is_empty() {
        color_grading_map.insert("shadows".to_string(), Value::Object(shadows_map));
    }
    if !midtones_map.is_empty() {
        color_grading_map.insert("midtones".to_string(), Value::Object(midtones_map));
    }
    if !highlights_map.is_empty() {
        color_grading_map.insert("highlights".to_string(), Value::Object(highlights_map));
    }
    if !global_map.is_empty() {
        color_grading_map.insert("global".to_string(), Value::Object(global_map));
    }
    if !color_grading_map.is_empty() {
        adjustments.insert("colorGrading".to_string(), Value::Object(color_grading_map));
    }

    let curve_mappings = [
        (["ToneCurvePV2012", "ToneCurve"], "luma"),
        (["ToneCurvePV2012Red", "ToneCurveRed"], "red"),
        (["ToneCurvePV2012Green", "ToneCurveGreen"], "green"),
        (["ToneCurvePV2012Blue", "ToneCurveBlue"], "blue"),
    ];
    let outer_content = strip_nested_looks(xmp_content);
    for (xmp_curves, rr_curve) in curve_mappings {
        for xmp_curve in xmp_curves {
            if let Some(points) = extract_tone_curve_points(&outer_content, xmp_curve) {
                curves_map.insert(rr_curve.to_string(), Value::Array(points));
                break;
            }
        }
    }
    let look_curves = extract_nested_look_luma_curves(xmp_content);
    if !look_curves.is_empty() {
        let outer_luma = curves_map
            .get("luma")
            .and_then(Value::as_array)
            .map(|points| curve_points_as_f64(points));
        curves_map.insert(
            "luma".to_string(),
            Value::Array(compose_luma_curve_with_looks(
                &look_curves,
                outer_luma.as_deref(),
            )),
        );
    }
    if !curves_map.is_empty() {
        adjustments.insert("curves".to_string(), Value::Object(curves_map));
    }

    if include_crop_transform {
        import_xmp_crop(
            xmp_content,
            &attrs,
            &mut adjustments,
            fallback_image_dimensions,
        );
        import_lens_profile(&attrs, &mut adjustments);

        // Masks live in the same space as the crop: oriented and rotated by
        // the imported rotation, before cropping.
        let frame =
            xmp_image_dimensions(xmp_content, fallback_image_dimensions).map(|(width, height)| {
                crate::lightroom_masks::ImageFrame {
                    width,
                    height,
                    orientation: xmp_orientation(xmp_content),
                    rotation: adjustments
                        .get("rotation")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                }
            });
        let (masks, summary) = crate::lightroom_masks::import_lightroom_masks_for_image(
            xmp_content,
            frame.as_ref(),
            image_path,
            |mask| scale_lightroom_exposure(mask, SCALE_LIGHTROOM_EXPOSURE_TO_RAPIDRAW_UNITS),
        );
        if let Some(report) = mask_report {
            *report = summary;
        }
        if !masks.is_empty() {
            adjustments.insert("masks".to_string(), Value::Array(masks));
        }
    }

    let preset_name =
        extract_xmp_name(xmp_content).unwrap_or_else(|| "Imported Preset".to_string());

    Ok(Preset {
        id: Uuid::new_v4().to_string(),
        name: preset_name,
        adjustments: Value::Object(adjustments),
        include_masks: Some(false),
        include_crop_transform: Some(include_crop_transform),
        preset_type: Some("style".to_string()),
        favorite: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn imported_crops_and_masks_follow_image_pixels_in_all_exif_orientations() {
        use crate::image_processing::apply_orientation;
        use crate::lightroom_masks::tests::sidecar_with_corrections;
        use image::{DynamicImage, Rgba, RgbaImage};
        use rawler::decoders::Orientation;

        let marker = Rgba([255, 0, 0, 255]);
        let mut image = RgbaImage::new(8, 4);
        image.put_pixel(1, 0, marker);
        let corrections = r#"<rdf:li crs:What="Correction" crs:CorrectionAmount="1"
            crs:CorrectionActive="true" crs:LocalExposure2012="0.1">
            <crs:CorrectionMasks><rdf:Seq><rdf:li crs:What="Mask/CircularGradient"
            crs:MaskActive="true" crs:MaskBlendMode="0" crs:MaskInverted="false"
            crs:MaskValue="1" crs:Left="0.125" crs:Right="0.25"
            crs:Top="0" crs:Bottom="0.25" crs:Angle="0"
            crs:Feather="0" crs:Flipped="true"/></rdf:Seq></crs:CorrectionMasks></rdf:li>"#;

        for orientation in 1..=8 {
            let oriented = apply_orientation(
                DynamicImage::ImageRgba8(image.clone()),
                Orientation::from_u16(orientation),
            )
            .to_rgba8();
            let (x, y, _) = oriented
                .enumerate_pixels()
                .find(|(_, _, pixel)| **pixel == marker)
                .unwrap();
            let attributes = format!(
                r#"tiff:Orientation="{orientation}" crs:HasCrop="True"
                crs:CropLeft="0.125" crs:CropRight="0.25"
                crs:CropTop="0" crs:CropBottom="0.25""#
            );
            let xmp = sidecar_with_corrections(&attributes, corrections)
                .replace(r#"tiff:ImageWidth="6000""#, r#"tiff:ImageWidth="8""#)
                .replace(r#"tiff:ImageLength="4000""#, r#"tiff:ImageLength="4""#);
            let preset = convert_xmp_sidecar_to_preset(&xmp).unwrap();
            let crop = &preset.adjustments["crop"];
            for (key, expected) in [
                ("x", f64::from(x)),
                ("y", f64::from(y)),
                ("width", 1.0),
                ("height", 1.0),
            ] {
                assert!(
                    (crop[key].as_f64().unwrap() - expected).abs() < 1e-6,
                    "orientation {orientation}, crop {key}: {crop}"
                );
            }
            let mask = &preset.adjustments["masks"][0]["subMasks"][0]["parameters"];
            for (key, expected) in [
                ("centerX", f64::from(x) + 0.5),
                ("centerY", f64::from(y) + 0.5),
            ] {
                assert!(
                    (mask[key].as_f64().unwrap() - expected).abs() < 1e-6,
                    "orientation {orientation}, mask {key}: {mask}"
                );
            }
        }
    }

    #[test]
    fn converts_attribute_based_xmp_adjustments() {
        let preset = convert_xmp_to_preset(
            r#"<rdf:Description crs:Exposure2012="+0.50" crs:Contrast2012="25" />"#,
        )
        .unwrap();

        assert_eq!(preset.adjustments["exposure"], json!(0.5));
        assert_eq!(preset.adjustments["contrast"], json!(25));
    }

    #[test]
    fn preserves_shadows_2012_value_one_to_one() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description crs:Shadows2012="+46" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert_eq!(preset.adjustments["shadows"], json!(46.0));
    }

    #[test]
    fn applies_calibrated_pv2012_policy_to_rendered_jpeg_sources() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:ProcessVersion="15.4"
                crs:Exposure2012="+0.80"
                crs:Contrast2012="-5"
                crs:Highlights2012="-46"
                crs:Shadows2012="+46"
                crs:Whites2012="+12"
                crs:Blacks2012="+1"
                crs:Vibrance="+38"
                crs:Saturation="+14"
                crs:IncrementalTemperature="+7"
                crs:IncrementalTint="+13" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert_eq!(preset.adjustments["toneMapper"], json!("basic"));
        assert_eq!(preset.adjustments["exposure"], json!(0.0));
        assert_eq!(preset.adjustments["brightness"], json!(0.8));
        assert_eq!(preset.adjustments["contrast"], json!(-5.0));
        assert_eq!(preset.adjustments["shadows"], json!(46.0));
        assert_eq!(preset.adjustments["whites"], json!(12.0));
        assert!(
            (preset.adjustments["highlights"].as_f64().unwrap() + 25.619_930_850_889_27).abs()
                < 1e-12
        );
        assert!(
            (preset.adjustments["blacks"].as_f64().unwrap() + 20.719_192_729_216_786).abs() < 1e-12
        );
        assert!(
            (preset.adjustments["vibrance"].as_f64().unwrap() - 15.952_888_888_888_89).abs()
                < 1e-12
        );
        assert!(
            (preset.adjustments["saturation"].as_f64().unwrap() - 27.620_444_444_444_446).abs()
                < 1e-12
        );
        assert!((preset.adjustments["temperature"].as_f64().unwrap() - 5.6).abs() < 1e-12);
        assert!((preset.adjustments["tint"].as_f64().unwrap() - 7.8).abs() < 1e-12);
    }

    #[test]
    fn image_aware_sidecar_import_selects_rendered_jpeg_calibration() {
        let preset = convert_xmp_sidecar_to_preset_for_image(
            r#"<rdf:Description crs:Exposure2012="+0.80" crs:Contrast2012="-5" />"#,
            Path::new("/photos/IMG_0598.JPG"),
        )
        .unwrap();

        assert_eq!(preset.adjustments["toneMapper"], json!("basic"));
        assert_eq!(preset.adjustments["exposure"], json!(0.0));
        assert_eq!(preset.adjustments["brightness"], json!(0.8));
    }

    #[test]
    fn applies_calibrated_blacks_tail_only_to_rendered_jpegs() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:ProcessVersion="15.4"
                crs:Contrast2012="0"
                crs:Shadows2012="-8"
                crs:Blacks2012="-89" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert_eq!(preset.adjustments["toneMapper"], json!("basic"));
        assert_eq!(preset.adjustments["blacks"], json!(-100.0));
        assert_eq!(preset.adjustments["shadows"], json!(-83.0));
        assert!((preset.adjustments["contrast"].as_f64().unwrap() - 30.552).abs() < 1e-12);

        let boundary = map_rendered_pv2012_blacks_tail(-50.0);
        assert!((boundary.0 + 45.322_159_850_307_4).abs() < 1e-12);
        assert_eq!((boundary.1, boundary.2), (0.0, 0.0));

        let transition = map_rendered_pv2012_blacks_tail(-54.0);
        assert!((transition.0 + 55.691_515_637_530_074).abs() < 1e-12);
        assert!((transition.1 + 3.936).abs() < 1e-12);
        assert!((transition.2 + 0.245_76).abs() < 1e-12);

        let raw = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Contrast2012="0"
                crs:Shadows2012="-8"
                crs:Blacks2012="-89" />"#,
            XmpImageKind::Raw,
        )
        .unwrap();
        assert_eq!(raw.adjustments["blacks"], json!(-89));
        assert_eq!(raw.adjustments["shadows"], json!(-8));
        assert_eq!(raw.adjustments["contrast"], json!(0));
    }

    #[test]
    fn applies_continuous_pv2012_blacks_clarity_contrast_interaction() {
        assert_eq!(
            map_rendered_pv2012_blacks_clarity_contrast_residual(-50.0, 11.0),
            13.2
        );
        assert_eq!(
            map_rendered_pv2012_blacks_clarity_contrast_residual(-25.0, 11.0),
            2.75
        );
        assert_eq!(
            map_rendered_pv2012_blacks_clarity_contrast_residual(-52.5, 11.0),
            6.6
        );
        for (blacks, clarity) in [(0.0, 11.0), (-55.0, 11.0), (-50.0, 0.0), (-50.0, -11.0)] {
            assert_eq!(
                map_rendered_pv2012_blacks_clarity_contrast_residual(blacks, clarity),
                0.0
            );
        }

        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:ProcessVersion="6.7"
                crs:Contrast2012="0"
                crs:Blacks2012="-50"
                crs:Clarity2012="11" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();
        assert_eq!(preset.adjustments["contrast"], json!(13.2));

        let raw = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:ProcessVersion="6.7"
                crs:Contrast2012="0"
                crs:Blacks2012="-50"
                crs:Clarity2012="11" />"#,
            XmpImageKind::Raw,
        )
        .unwrap();
        assert_eq!(raw.adjustments["contrast"], json!(0));
    }

    #[test]
    fn maps_rendered_jpeg_color_response_axes_and_interaction() {
        let vibrance_cases = [
            (-100.0, 5.0),
            (-75.0, 16.0),
            (-50.0, 10.0),
            (-25.0, -4.0),
            (-12.5, -2.0),
            (0.0, 0.0),
            (25.0, 20.0),
            (50.0, 42.0),
            (75.0, 70.0),
            (100.0, 100.0),
        ];
        for (adobe_value, expected) in vibrance_cases {
            assert!((map_rendered_pv2012_vibrance(adobe_value) - expected).abs() < 1e-12);
        }

        assert!((map_rendered_pv2012_saturation(25.0) - 21.666_666_666_666_668).abs() < 1e-12);
        assert!((map_rendered_pv2012_saturation(50.0) - 43.333_333_333_333_336).abs() < 1e-12);
        assert!((map_rendered_pv2012_saturation(75.0) - 65.0).abs() < 1e-12);
        assert!((map_rendered_pv2012_saturation(100.0) - 86.666_666_666_666_67).abs() < 1e-12);
        assert_eq!(map_rendered_pv2012_saturation(-50.0), -50.0);

        let (saturation, vibrance) = map_rendered_pv2012_color_response(Some(25.0), Some(75.0));
        assert!((saturation.unwrap() - 52.233_333_333_333_334).abs() < 1e-12);
        assert!((vibrance.unwrap() - 39.433_333_333_333_34).abs() < 1e-12);

        let (saturation, vibrance) = map_rendered_pv2012_color_response(Some(0.0), Some(15.0));
        assert_eq!(saturation, Some(4.0));
        assert_eq!(vibrance, Some(12.0));

        let (saturation, vibrance) = map_rendered_pv2012_color_response(None, Some(3.0));
        assert_eq!(saturation, Some(2.0));
        assert!((vibrance.unwrap() - 2.4).abs() < 1e-12);

        let (saturation, vibrance) = map_rendered_pv2012_color_response(Some(0.0), Some(17.5));
        assert_eq!(saturation, Some(2.0));
        assert_eq!(vibrance, Some(14.0));

        let (saturation, vibrance) = map_rendered_pv2012_color_response(Some(0.0), Some(18.0));
        assert_eq!(saturation, Some(0.0));
        assert!((vibrance.unwrap() - 14.4).abs() < 1e-12);

        let (saturation, vibrance) = map_rendered_pv2012_color_response(None, Some(-50.0));
        assert_eq!(saturation.unwrap(), -51.0);
        assert_eq!(vibrance.unwrap(), 10.0);
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description crs:Exposure2012="0" crs:Vibrance="-50" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();
        assert_eq!(preset.adjustments["saturation"], json!(-51.0));
        assert_eq!(preset.adjustments["vibrance"], json!(10.0));

        let (saturation, vibrance) = map_rendered_pv2012_color_response(Some(-50.0), Some(-50.0));
        assert!((saturation.unwrap() + 74.142_805_331_399_05).abs() < 1e-12);
        assert!((vibrance.unwrap() + 13.5).abs() < 1e-12);

        let (saturation, vibrance) = map_rendered_pv2012_color_response(Some(50.0), Some(-50.0));
        assert!((saturation.unwrap() + 7.666_666_666_666_664).abs() < 1e-12);
        assert!((vibrance.unwrap() + 46.0).abs() < 1e-12);
    }

    #[test]
    fn keeps_raw_pv2012_mapping_unchanged() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Exposure2012="+0.80"
                crs:Highlights2012="-46"
                crs:Blacks2012="+1"
                crs:Vibrance="+38"
                crs:Saturation="+14"
                crs:IncrementalTemperature="+7"
                crs:IncrementalTint="+13" />"#,
            XmpImageKind::Raw,
        )
        .unwrap();

        assert_eq!(preset.adjustments["exposure"], json!(0.8));
        assert_eq!(preset.adjustments["highlights"], json!(-46));
        assert_eq!(preset.adjustments["blacks"], json!(1));
        assert_eq!(preset.adjustments["vibrance"], json!(38));
        assert_eq!(preset.adjustments["saturation"], json!(14));
        assert!(preset.adjustments.get("brightness").is_none());
        assert!(preset.adjustments.get("toneMapper").is_none());
        assert!(preset.adjustments.get("temperature").is_none());
        assert!(preset.adjustments.get("tint").is_none());
    }

    #[test]
    fn keeps_non_jpeg_rendered_pv2012_mapping_unchanged() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Exposure2012="+0.80"
                crs:Highlights2012="-46"
                crs:Blacks2012="+1"
                crs:Vibrance="+38"
                crs:Saturation="+14"
                crs:IncrementalTemperature="+7"
                crs:IncrementalTint="+13" />"#,
            XmpImageKind::Rendered,
        )
        .unwrap();

        assert_eq!(preset.adjustments["exposure"], json!(0.8));
        assert_eq!(preset.adjustments["highlights"], json!(-46));
        assert_eq!(preset.adjustments["blacks"], json!(1));
        assert_eq!(preset.adjustments["vibrance"], json!(38));
        assert_eq!(preset.adjustments["saturation"], json!(14));
        assert!(preset.adjustments.get("brightness").is_none());
        assert!(preset.adjustments.get("toneMapper").is_none());
        assert!(preset.adjustments.get("temperature").is_none());
        assert!(preset.adjustments.get("tint").is_none());
    }

    #[test]
    fn imports_grayscale_mixer_and_applies_rendered_jpeg_policy() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:ConvertToGrayscale="True"
                crs:Saturation="25"
                crs:Exposure2012="+0.80"
                crs:Highlights2012="-46"
                crs:Blacks2012="+1"
                crs:HueAdjustmentRed="+40"
                crs:SaturationAdjustmentRed="+20"
                crs:LuminanceAdjustmentRed="+30"
                crs:GrayMixerRed="-10"
                crs:GrayMixerOrange="-19"
                crs:GrayMixerYellow="-23"
                crs:GrayMixerGreen="-27"
                crs:GrayMixerAqua="-17"
                crs:GrayMixerBlue="+12"
                crs:GrayMixerPurple="+17"
                crs:GrayMixerMagenta="+4" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert_eq!(preset.adjustments["saturation"], json!(-100.0));
        assert_eq!(preset.adjustments["toneMapper"], json!("basic"));
        assert_eq!(preset.adjustments["exposure"], json!(0.0));
        assert_eq!(preset.adjustments["brightness"], json!(0.8));
        assert!(
            (preset.adjustments["highlights"].as_f64().unwrap() + 25.619_930_850_889_27).abs()
                < 1e-12
        );
        assert!(
            (preset.adjustments["blacks"].as_f64().unwrap() + 20.719_192_729_216_786).abs() < 1e-12
        );

        let expected_mix = [
            ("reds", -10.0),
            ("oranges", -19.0),
            ("yellows", -23.0),
            ("greens", -27.0),
            ("aquas", -17.0),
            ("blues", 12.0),
            ("purples", 17.0),
            ("magentas", 4.0),
        ];
        for (color, luminance) in expected_mix {
            assert_eq!(
                preset.adjustments["hsl"][color]["luminance"],
                json!(luminance)
            );
            assert!(preset.adjustments["hsl"][color].get("hue").is_none());
            assert!(preset.adjustments["hsl"][color].get("saturation").is_none());
        }
    }

    #[test]
    fn scales_hsl_hue_and_preserves_tone_curve_values() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description crs:HueAdjustmentRed="+40">
              <crs:ToneCurvePV2012>
                <rdf:Seq>
                  <rdf:li>0, 0</rdf:li>
                  <rdf:li>16, 32</rdf:li>
                  <rdf:li>255, 255</rdf:li>
                </rdf:Seq>
              </crs:ToneCurvePV2012>
            </rdf:Description>"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert_eq!(preset.adjustments["hsl"]["reds"]["hue"], json!(30.0));
        assert_eq!(
            preset.adjustments["curves"]["luma"],
            json!([
                { "x": 0, "y": 0 },
                { "x": 16, "y": 32 },
                { "x": 255, "y": 255 }
            ])
        );
    }

    #[test]
    fn converts_element_based_xmp_adjustments() {
        let preset = convert_xmp_to_preset(
            r#"
            <rdf:Description>
              <crs:Exposure2012>+0.50</crs:Exposure2012>
              <crs:Contrast2012>25</crs:Contrast2012>
            </rdf:Description>
            "#,
        )
        .unwrap();

        assert_eq!(preset.adjustments["exposure"], json!(0.5));
        assert_eq!(preset.adjustments["contrast"], json!(25));
    }

    #[test]
    fn converts_legacy_process_adjustments_and_tone_curve() {
        let preset = convert_xmp_sidecar_to_preset(
            r#"<rdf:Description
                crs:ProcessVersion="5.7"
                crs:Exposure="0.00"
                crs:HighlightRecovery="20"
                crs:FillLight="0"
                crs:Shadows="5"
                crs:Brightness="+50"
                crs:Contrast="+25"
                crs:Clarity="0">
              <crs:ToneCurve>
                <rdf:Seq>
                  <rdf:li>0, 0</rdf:li>
                  <rdf:li>32, 16</rdf:li>
                  <rdf:li>64, 50</rdf:li>
                  <rdf:li>128, 128</rdf:li>
                  <rdf:li>192, 202</rdf:li>
                  <rdf:li>255, 255</rdf:li>
                </rdf:Seq>
              </crs:ToneCurve>
            </rdf:Description>"#,
        )
        .unwrap();

        assert_eq!(preset.adjustments["exposure"], json!(0.0));
        assert_eq!(preset.adjustments["highlights"], json!(-20.0));
        assert_eq!(preset.adjustments["shadows"], json!(0.0));
        assert_eq!(preset.adjustments["blacks"], json!(0.0));
        assert_eq!(preset.adjustments["brightness"], json!(0.0));
        assert_eq!(preset.adjustments["contrast"], json!(0.0));
        assert_eq!(preset.adjustments["clarity"], json!(0.0));
        assert_eq!(
            preset.adjustments["curves"]["luma"],
            json!([
                { "x": 0, "y": 0 },
                { "x": 32, "y": 16 },
                { "x": 64, "y": 50 },
                { "x": 128, "y": 128 },
                { "x": 192, "y": 202 },
                { "x": 255, "y": 255 }
            ])
        );
    }

    #[test]
    fn scales_legacy_brightness_conservatively_for_rendered_jpegs() {
        let zero = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:ProcessVersion="5.7"
                crs:Exposure="0.00"
                crs:Brightness="0" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();
        let raised = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:ProcessVersion="5.7"
                crs:Exposure="+0.50"
                crs:Brightness="+57" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert_eq!(zero.adjustments["exposure"], json!(0.0));
        assert_eq!(zero.adjustments["brightness"], json!(-0.55));
        assert_eq!(raised.adjustments["exposure"], json!(0.25));
        assert!((raised.adjustments["brightness"].as_f64().unwrap() - 0.07).abs() < 1e-12);
        assert_eq!(zero.adjustments["toneMapper"], json!("basic"));
    }

    #[test]
    fn applies_calibrated_pv5_axes_and_color_interactions_to_rendered_jpegs() {
        let snow = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:ProcessVersion="5.0"
                crs:Exposure="0"
                crs:Brightness="0"
                crs:Contrast="0"
                crs:Shadows="0"
                crs:Clarity="31"
                crs:Saturation="2"
                crs:Vibrance="33" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();
        assert_eq!(snow.adjustments["exposure"], json!(0.0));
        assert_eq!(snow.adjustments["brightness"], json!(-0.15));
        assert_eq!(snow.adjustments["contrast"], json!(-10.0));
        assert_eq!(snow.adjustments["blacks"], json!(48.0));
        assert_eq!(snow.adjustments["clarity"], json!(42.0));
        assert_eq!(snow.adjustments["saturation"], json!(13.0));
        assert_eq!(snow.adjustments["vibrance"], json!(9.0));

        let positive_tone = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:ProcessVersion="5.7"
                crs:Exposure="+0.50"
                crs:Brightness="+57"
                crs:Contrast="+68"
                crs:Shadows="2"
                crs:Clarity="+52"
                crs:Saturation="+18"
                crs:Vibrance="0" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();
        assert_eq!(positive_tone.adjustments["exposure"], json!(0.25));
        assert!((positive_tone.adjustments["brightness"].as_f64().unwrap() - 0.07).abs() < 1e-12);
        assert_eq!(positive_tone.adjustments["contrast"], json!(14.0));
        assert_eq!(positive_tone.adjustments["blacks"], json!(20.0));
        assert_eq!(positive_tone.adjustments["clarity"], json!(42.0));
        assert_eq!(positive_tone.adjustments["saturation"], json!(18.0));
        assert_eq!(positive_tone.adjustments["vibrance"], json!(0.0));

        let strong_color = map_rendered_pv5_color_response(Some(68.0), Some(58.0));
        assert_eq!(strong_color.0, Some(80.0));
        assert!((strong_color.1.unwrap() - 30.0).abs() < 1e-12);
    }

    #[test]
    fn limits_calibrated_pv5_policy_to_pv5_rendered_jpegs() {
        let xmp = r#"<rdf:Description
            crs:ProcessVersion="5.7"
            crs:Exposure="+0.50"
            crs:Brightness="0"
            crs:Contrast="0"
            crs:Shadows="0"
            crs:Clarity="31"
            crs:Saturation="2"
            crs:Vibrance="33" />"#;
        let raw = convert_adobe_camera_raw_xmp_to_preset(xmp, XmpImageKind::Raw).unwrap();
        assert_eq!(raw.adjustments["exposure"], json!(0.5));
        assert_eq!(raw.adjustments["brightness"], json!(-5.0));
        assert_eq!(raw.adjustments["contrast"], json!(-25.0));
        assert_eq!(raw.adjustments["blacks"], json!(5.0));
        assert_eq!(raw.adjustments["clarity"], json!(31.0));
        assert_eq!(raw.adjustments["saturation"], json!(2));
        assert_eq!(raw.adjustments["vibrance"], json!(33));
        assert!(raw.adjustments.get("toneMapper").is_none());

        let pv67 = convert_adobe_camera_raw_xmp_to_preset(
            &xmp.replace("5.7", "6.7"),
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();
        assert_eq!(pv67.adjustments["exposure"], json!(0.5));
        assert_eq!(pv67.adjustments["brightness"], json!(-0.5));
        assert_eq!(pv67.adjustments["contrast"], json!(-25.0));
        assert_eq!(pv67.adjustments["blacks"], json!(5.0));
        assert_eq!(pv67.adjustments["clarity"], json!(31.0));
        assert_eq!(pv67.adjustments["saturation"], json!(2));
        assert_eq!(pv67.adjustments["vibrance"], json!(33));
        assert!(pv67.adjustments.get("toneMapper").is_none());
    }

    #[test]
    fn prefers_pv2012_values_and_curves_over_legacy_fallbacks() {
        let preset = convert_xmp_to_preset(
            r#"<rdf:Description
                crs:Exposure2012="+0.50"
                crs:Exposure="+2.00">
              <crs:ToneCurvePV2012>
                <rdf:Seq>
                  <rdf:li>0, 0</rdf:li>
                  <rdf:li>255, 255</rdf:li>
                </rdf:Seq>
              </crs:ToneCurvePV2012>
              <crs:ToneCurve>
                <rdf:Seq>
                  <rdf:li>0, 10</rdf:li>
                  <rdf:li>255, 245</rdf:li>
                </rdf:Seq>
              </crs:ToneCurve>
            </rdf:Description>"#,
        )
        .unwrap();

        assert_eq!(preset.adjustments["exposure"], json!(0.5));
        assert_eq!(
            preset.adjustments["curves"]["luma"],
            json!([{ "x": 0, "y": 0 }, { "x": 255, "y": 255 }])
        );
    }

    #[test]
    fn ignores_adjustment_attributes_in_nested_lightroom_look() {
        let preset = convert_xmp_sidecar_to_preset(
            r#"<rdf:Description crs:Exposure2012="+0.50" crs:ProcessVersion="11.0">
              <crs:Look>
                <rdf:Description crs:Exposure2012="+2.00" crs:ProcessVersion="15.4" />
              </crs:Look>
            </rdf:Description>"#,
        )
        .unwrap();

        assert_eq!(preset.adjustments["exposure"], json!(0.5));
    }

    #[test]
    fn converts_sidecar_crop_to_rapidraw_pixels() {
        let preset = convert_xmp_sidecar_to_preset(
            r#"<rdf:Description
                tiff:ImageWidth="6000"
                tiff:ImageLength="4000"
                tiff:Orientation="1"
                crs:CropTop="0.078"
                crs:CropLeft="0"
                crs:CropBottom="0.922"
                crs:CropRight="1"
                crs:CropAngle="0"
                crs:HasCrop="True"
                crs:AlreadyApplied="False" />"#,
        )
        .unwrap();

        assert_eq!(
            preset.adjustments["crop"],
            json!({ "x": 0.0, "y": 312.0, "width": 6000.0, "height": 3376.0 })
        );
        assert_eq!(preset.adjustments["aspectRatio"], json!(6000.0 / 3376.0));
    }

    #[test]
    fn converts_crop_with_image_dimension_fallback() {
        let preset = convert_xmp_to_preset_with_crop(
            r#"<rdf:Description
                crs:CropTop="0.078"
                crs:CropLeft="0"
                crs:CropBottom="0.922"
                crs:CropRight="1"
                crs:CropAngle="0"
                crs:HasCrop="True"
                crs:AlreadyApplied="False" />"#,
            true,
            None,
            Some(XmpImageKind::Raw),
            Some((6000.0, 4000.0)),
            None,
            None,
        )
        .unwrap();

        assert_eq!(
            preset.adjustments["crop"],
            json!({ "x": 0.0, "y": 312.0, "width": 6000.0, "height": 3376.0 })
        );
        assert_eq!(preset.adjustments["aspectRatio"], json!(6000.0 / 3376.0));
    }

    #[test]
    fn transforms_sidecar_crop_for_exif_orientation() {
        let preset = convert_xmp_sidecar_to_preset(
            r#"<rdf:Description
                tiff:ImageWidth="6000"
                tiff:ImageLength="4000"
                tiff:Orientation="8"
                crs:CropTop="0.078"
                crs:CropLeft="0"
                crs:CropBottom="0.922"
                crs:CropRight="1"
                crs:HasCrop="True" />"#,
        )
        .unwrap();

        assert_eq!(
            preset.adjustments["crop"],
            json!({ "x": 312.0, "y": 0.0, "width": 3376.0, "height": 6000.0 })
        );
    }

    #[test]
    fn imports_element_based_crop_and_rotation() {
        let preset = convert_xmp_sidecar_to_preset(
            r#"<rdf:Description>
                <tiff:ImageWidth>6000</tiff:ImageWidth>
                <tiff:ImageLength>4000</tiff:ImageLength>
                <crs:CropTop>0.1</crs:CropTop>
                <crs:CropLeft>0.2</crs:CropLeft>
                <crs:CropBottom>0.9</crs:CropBottom>
                <crs:CropRight>0.8</crs:CropRight>
                <crs:CropAngle>2.05755</crs:CropAngle>
                <crs:HasCrop>True</crs:HasCrop>
            </rdf:Description>"#,
        )
        .unwrap();

        assert_eq!(
            preset.adjustments["crop"],
            json!({ "x": 1144.0, "y": 466.0, "width": 3713.0, "height": 3069.0 })
        );
        assert_eq!(preset.adjustments["rotation"], json!(-2.05755));
    }

    #[test]
    fn converts_rotated_lightroom_crop_to_post_rotation_coordinates() {
        let preset = convert_xmp_sidecar_to_preset(
            r#"<rdf:Description
                tiff:ImageWidth="6000"
                tiff:ImageLength="4000"
                tiff:Orientation="1"
                crs:CropTop="0.020203"
                crs:CropLeft="0.028736"
                crs:CropBottom="0.916557"
                crs:CropRight="0.971264"
                crs:CropAngle="3.01"
                crs:HasCrop="True"
                crs:AlreadyApplied="False" />"#,
        )
        .unwrap();

        assert_eq!(
            preset.adjustments["crop"],
            json!({ "x": 76.0, "y": 232.0, "width": 5836.0, "height": 3284.0 })
        );
        assert_eq!(preset.adjustments["aspectRatio"], json!(5836.0 / 3284.0));
        assert_eq!(preset.adjustments["rotation"], json!(-3.01));
    }

    #[test]
    fn keeps_crop_out_of_reusable_xmp_presets() {
        let preset = convert_xmp_to_preset(
            r#"<rdf:Description
                tiff:ImageWidth="6000"
                tiff:ImageLength="4000"
                crs:CropTop="0.1"
                crs:CropLeft="0.2"
                crs:CropBottom="0.9"
                crs:CropRight="0.8"
                crs:HasCrop="True" />"#,
        )
        .unwrap();

        assert!(preset.adjustments.as_object().unwrap().is_empty());
        assert_eq!(preset.include_crop_transform, Some(false));
    }

    #[test]
    fn converts_custom_white_balance_relative_to_raw_as_shot_values() {
        let xmp = r#"<rdf:Description
                crs:WhiteBalance="Custom"
                crs:Temperature="5068"
                crs:AsShotTemperature="4440"
                crs:Tint="-11"
                crs:AsShotTint="-5" />"#;
        let preset = convert_xmp_sidecar_to_preset(xmp).unwrap();

        assert!((preset.adjustments["temperature"].as_f64().unwrap() - 18.6).abs() < 0.1);
        assert!((preset.adjustments["tint"].as_f64().unwrap() + 4.0).abs() < 0.01);
    }

    #[test]
    fn keeps_as_shot_white_balance_neutral() {
        let xmp = r#"<rdf:Description
                crs:WhiteBalance="As Shot"
                crs:Temperature="6800"
                crs:Tint="+38" />"#;
        let preset = convert_xmp_sidecar_to_preset(xmp).unwrap();

        assert!(preset.adjustments.get("temperature").is_none());
        assert!(preset.adjustments.get("tint").is_none());
    }

    #[test]
    fn omits_custom_temperature_when_as_shot_baseline_is_unknown() {
        let preset = convert_xmp_sidecar_to_preset(
            r#"<rdf:Description crs:WhiteBalance="Custom" crs:Temperature="5068" />"#,
        )
        .unwrap();

        assert!(preset.adjustments.get("temperature").is_none());
    }

    #[test]
    fn omits_absolute_sidecar_white_balance_without_as_shot_baseline() {
        let preset = convert_xmp_sidecar_to_preset(
            r#"<rdf:Description
                crs:WhiteBalance="Custom"
                crs:Temperature="6190"
                crs:Tint="+12" />"#,
        )
        .unwrap();

        assert!(preset.adjustments.get("temperature").is_none());
        assert!(preset.adjustments.get("tint").is_none());
    }

    #[test]
    fn omits_camera_raw_white_balance_for_rendered_jpeg_sources() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:WhiteBalance="Custom"
                crs:Temperature="5068"
                crs:AsShotTemperature="4440"
                crs:Tint="-11"
                crs:AsShotTint="-5"
                crs:Exposure2012="+0.25" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert!(preset.adjustments.get("temperature").is_none());
        assert!(preset.adjustments.get("tint").is_none());
        assert_eq!(preset.adjustments["exposure"], json!(0.0));
        assert_eq!(preset.adjustments["brightness"], json!(0.25));
        assert_eq!(preset.adjustments["toneMapper"], json!("basic"));
    }

    #[test]
    fn ignores_negative_incremental_temperature_and_incremental_tint() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Exposure2012="0"
                crs:IncrementalTemperature="-9"
                crs:IncrementalTint="-15" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert!(preset.adjustments.get("temperature").is_none());
        assert!(preset.adjustments.get("tint").is_none());
    }

    #[test]
    fn maps_positive_incremental_tint_without_mapping_negative_temperature() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Exposure2012="0"
                crs:IncrementalTemperature="-3"
                crs:IncrementalTint="+17" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert!(preset.adjustments.get("temperature").is_none());
        assert!((preset.adjustments["tint"].as_f64().unwrap() - 10.2).abs() < 1e-12);
    }

    #[test]
    fn maps_modest_positive_temperature_only_edits_with_the_holdout_scale() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Exposure2012="0"
                crs:IncrementalTemperature="+11"
                crs:IncrementalTint="0" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert!((preset.adjustments["temperature"].as_f64().unwrap() - 14.3).abs() < 1e-12);
        assert!(preset.adjustments.get("tint").is_none());
    }

    #[test]
    fn adds_tint_assist_to_low_vibrance_temperature_only_edits() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Exposure2012="0"
                crs:Saturation="0"
                crs:Vibrance="+15"
                crs:IncrementalTemperature="+11"
                crs:IncrementalTint="0" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert!((preset.adjustments["temperature"].as_f64().unwrap() - 14.3).abs() < 1e-12);
        assert_eq!(preset.adjustments["tint"], json!(4.5));
    }

    #[test]
    fn omits_temperature_only_tint_assist_at_mid_vibrance() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Exposure2012="0"
                crs:Saturation="0"
                crs:Vibrance="+18"
                crs:IncrementalTemperature="+10"
                crs:IncrementalTint="0" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert!(preset.adjustments.get("tint").is_none());
    }

    #[test]
    fn tapers_large_positive_temperature_only_edits_to_identity() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Exposure2012="0"
                crs:IncrementalTemperature="+21" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert_eq!(preset.adjustments["temperature"], json!(21.0));
    }

    #[test]
    fn keeps_positive_temperature_half_scaled_when_incremental_tint_is_negative() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Exposure2012="0"
                crs:IncrementalTemperature="+20"
                crs:IncrementalTint="-39" />"#,
            XmpImageKind::RenderedJpeg,
        )
        .unwrap();

        assert_eq!(preset.adjustments["temperature"], json!(10.0));
        assert!(preset.adjustments.get("tint").is_none());
    }

    #[test]
    fn detects_already_applied_camera_raw_metadata() {
        assert!(
            adobe_camera_raw_already_applied(r#"<rdf:Description crs:AlreadyApplied="True" />"#)
                .unwrap()
        );
        assert!(
            !adobe_camera_raw_already_applied(r#"<rdf:Description crs:AlreadyApplied="False" />"#)
                .unwrap()
        );
    }

    const ADOBE_COLOR_LOOK: &str = r#"<crs:Look>
                <rdf:Description crs:Name="Adobe Color" crs:Amount="1">
                  <crs:Parameters>
                    <rdf:Description crs:ProcessVersion="11.0" crs:LookTable="0000">
                      <crs:ToneCurvePV2012>
                        <rdf:Seq>
                          <rdf:li>0, 0</rdf:li>
                          <rdf:li>64, 48</rdf:li>
                          <rdf:li>192, 208</rdf:li>
                          <rdf:li>255, 255</rdf:li>
                        </rdf:Seq>
                      </crs:ToneCurvePV2012>
                    </rdf:Description>
                  </crs:Parameters>
                </rdf:Description>
              </crs:Look>"#;

    fn luma_at(preset: &Preset, x: u64) -> u64 {
        preset.adjustments["curves"]["luma"]
            .as_array()
            .unwrap()
            .iter()
            .find(|point| point["x"].as_u64() == Some(x))
            .and_then(|point| point["y"].as_u64())
            .unwrap()
    }

    #[test]
    fn evaluates_curves_like_the_shader() {
        let curve = [(0.0, 0.0), (64.0, 48.0), (192.0, 208.0), (255.0, 255.0)];
        assert_eq!(evaluate_rapidraw_curve(&curve, 0.0), 0.0);
        assert_eq!(evaluate_rapidraw_curve(&curve, 64.0), 48.0);
        assert_eq!(evaluate_rapidraw_curve(&curve, 255.0), 255.0);
        let identity = [(0.0, 0.0), (255.0, 255.0)];
        assert!((evaluate_rapidraw_curve(&identity, 100.0) - 100.0).abs() < 1e-9);
        let mid = evaluate_rapidraw_curve(&curve, 128.0);
        assert!(mid > 120.0 && mid < 136.0, "{mid}");
    }

    #[test]
    fn composes_nested_look_tone_curve_into_luma() {
        let look_only = convert_xmp_sidecar_to_preset(&format!(
            r#"<rdf:Description crs:Exposure2012="0">{ADOBE_COLOR_LOOK}</rdf:Description>"#
        ))
        .unwrap();
        let luma = look_only.adjustments["curves"]["luma"].as_array().unwrap();
        assert_eq!(luma.len(), MAX_RAPIDRAW_CURVE_POINTS);
        assert_eq!(luma_at(&look_only, 0), 0);
        assert_eq!(luma_at(&look_only, 255), 255);
        assert!(luma_at(&look_only, 51) < 51);
        assert!(luma_at(&look_only, 204) > 204);

        // The user's curve applies on top of the look: here it inverts it.
        let with_user_curve = convert_xmp_sidecar_to_preset(&format!(
            r#"<rdf:Description crs:Exposure2012="0">
              <crs:ToneCurvePV2012>
                <rdf:Seq>
                  <rdf:li>0, 255</rdf:li>
                  <rdf:li>255, 0</rdf:li>
                </rdf:Seq>
              </crs:ToneCurvePV2012>
              {ADOBE_COLOR_LOOK}
            </rdf:Description>"#
        ))
        .unwrap();
        for x in [0, 51, 102, 153, 204, 255] {
            assert_eq!(
                luma_at(&with_user_curve, x),
                255 - luma_at(&look_only, x),
                "{x}"
            );
        }
    }

    #[test]
    fn reads_outer_curve_even_when_the_look_comes_first() {
        let preset = convert_xmp_sidecar_to_preset(&format!(
            r#"<rdf:Description crs:Exposure2012="0">
              {}
              <crs:ToneCurvePV2012>
                <rdf:Seq>
                  <rdf:li>0, 0</rdf:li>
                  <rdf:li>255, 255</rdf:li>
                </rdf:Seq>
              </crs:ToneCurvePV2012>
            </rdf:Description>"#,
            ADOBE_COLOR_LOOK.replace("crs:Amount=\"1\"", "crs:Amount=\"0\"")
        ))
        .unwrap();

        assert_eq!(
            preset.adjustments["curves"]["luma"],
            json!([{ "x": 0, "y": 0 }, { "x": 255, "y": 255 }])
        );
    }

    #[test]
    fn scales_nested_look_curve_by_its_amount() {
        let full = convert_xmp_sidecar_to_preset(&format!(
            r#"<rdf:Description crs:Exposure2012="0">{ADOBE_COLOR_LOOK}</rdf:Description>"#
        ))
        .unwrap();
        let half = convert_xmp_sidecar_to_preset(&format!(
            r#"<rdf:Description crs:Exposure2012="0">{}</rdf:Description>"#,
            ADOBE_COLOR_LOOK.replace("crs:Amount=\"1\"", "crs:Amount=\"0.5\"")
        ))
        .unwrap();

        let full_shift = 204 - luma_at(&full, 204) as i64;
        let half_shift = 204 - luma_at(&half, 204) as i64;
        assert!(
            (full_shift - 2 * half_shift).abs() <= 1,
            "{full_shift} {half_shift}"
        );
    }

    fn not_transferred(xmp: &str) -> Vec<&'static str> {
        let preset = convert_xmp_sidecar_to_preset(xmp).unwrap();
        lightroom_settings_not_transferred(xmp, &preset)
    }

    #[test]
    fn reports_lightroom_settings_that_were_not_transferred() {
        let xmp = format!(
            r#"<rdf:Description
                crs:WhiteBalance="Custom"
                crs:Temperature="5600"
                crs:Tint="+10"
                crs:EnhanceDenoiseVersion="1"
                crs:EnhanceDenoiseLumaAmount="50">
              {ADOBE_COLOR_LOOK}
              <crs:MaskGroupBasedCorrections>
                <rdf:Seq>
                  <rdf:li><rdf:Description crs:What="Correction" crs:LocalExposure2012="0.5" /></rdf:li>
                </rdf:Seq>
              </crs:MaskGroupBasedCorrections>
              <crs:PointColors>
                <rdf:Seq>
                  <rdf:li>3.5, 0.6, 0.5, 0, 0, 0, 0</rdf:li>
                </rdf:Seq>
              </crs:PointColors>
            </rdf:Description>"#
        );

        assert_eq!(
            not_transferred(&xmp),
            vec![
                "whiteBalance",
                "profileLook",
                "aiDenoise",
                "masks",
                "pointColor"
            ]
        );
    }

    #[test]
    fn reports_nothing_for_a_fully_transferred_sidecar() {
        for xmp in [
            r#"<rdf:Description crs:WhiteBalance="As Shot" crs:Exposure2012="+0.5" />"#,
            r#"<rdf:Description
                crs:WhiteBalance="Custom"
                crs:Temperature="5068"
                crs:AsShotTemperature="4440" />"#,
            r#"<rdf:Description
                crs:Exposure2012="+0.5"
                crs:EnhanceDenoiseAlreadyApplied="True"
                crs:EnhanceDenoiseLumaAmount="50">
              <crs:MaskGroupBasedCorrections>
                <rdf:Seq />
              </crs:MaskGroupBasedCorrections>
            </rdf:Description>"#,
        ] {
            assert!(not_transferred(xmp).is_empty(), "{xmp}");
        }
    }

    #[test]
    fn reports_legacy_local_corrections_as_masks() {
        let xmp = r#"<rdf:Description crs:Exposure2012="0">
              <crs:GradientBasedCorrections>
                <rdf:Seq>
                  <rdf:li><rdf:Description crs:What="Correction" /></rdf:li>
                </rdf:Seq>
              </crs:GradientBasedCorrections>
            </rdf:Description>"#;
        assert_eq!(not_transferred(xmp), vec!["masks"]);
    }

    #[test]
    fn maps_sharpen_masking_and_drops_keys_nothing_reads() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description
                crs:Sharpness="40"
                crs:SharpenRadius="+1.0"
                crs:SharpenDetail="25"
                crs:SharpenEdgeMasking="50"
                crs:LuminanceSmoothing="10"
                crs:ColorNoiseReduction="25"
                crs:ColorNoiseReductionDetail="50"
                crs:ColorNoiseReductionSmoothness="50" />"#,
            XmpImageKind::Raw,
        )
        .unwrap();

        assert_eq!(preset.adjustments["sharpnessThreshold"], json!(40.0));
        assert_eq!(preset.adjustments["lumaNoiseReduction"], json!(10));
        assert_eq!(preset.adjustments["colorNoiseReduction"], json!(25));
        for dead_key in [
            "sharpenRadius",
            "sharpenDetail",
            "sharpenMasking",
            "colorNoiseDetail",
            "colorNoiseSmoothness",
        ] {
            assert!(preset.adjustments.get(dead_key).is_none(), "{dead_key}");
        }
    }

    #[test]
    fn keeps_exposure_one_to_one_while_unit_scaling_is_off() {
        let preset = convert_adobe_camera_raw_xmp_to_preset(
            r#"<rdf:Description crs:Exposure2012="+1.25" />"#,
            XmpImageKind::Raw,
        )
        .unwrap();

        assert_eq!(preset.adjustments["exposure"], json!(1.25));
    }

    #[test]
    fn scales_lightroom_ev_to_rapidraw_exposure_units_when_enabled() {
        let mut adjustments = Map::new();
        adjustments.insert("exposure".to_string(), json!(1.25));
        scale_lightroom_exposure(&mut adjustments, true);
        assert_eq!(adjustments["exposure"], json!(1.0));

        let mut legacy = Map::new();
        legacy.insert("exposure".to_string(), json!(-0.5));
        scale_lightroom_exposure(&mut legacy, true);
        assert_eq!(legacy["exposure"], json!(-0.4));

        let mut unchanged = Map::new();
        unchanged.insert("exposure".to_string(), json!(1.25));
        scale_lightroom_exposure(&mut unchanged, false);
        assert_eq!(unchanged["exposure"], json!(1.25));
    }

    #[test]
    fn enables_automatic_lens_correction_for_lightroom_lens_profile() {
        let preset = convert_xmp_sidecar_to_preset(
            r#"<rdf:Description
                crs:LensProfileEnable="1"
                crs:LensProfileDistortionScale="100"
                crs:LensProfileVignettingScale="80"
                crs:AutoLateralCA="0" />"#,
        )
        .unwrap();

        assert_eq!(preset.adjustments["lensCorrectionMode"], json!("auto"));
        assert_eq!(preset.adjustments["lensDistortionEnabled"], json!(true));
        assert_eq!(preset.adjustments["lensVignetteEnabled"], json!(true));
        assert_eq!(preset.adjustments["lensTcaEnabled"], json!(false));
        assert_eq!(preset.adjustments["lensDistortionAmount"], json!(100.0));
        assert_eq!(preset.adjustments["lensVignetteAmount"], json!(80.0));
    }

    #[test]
    fn leaves_lens_correction_alone_without_lightroom_lens_profile() {
        for xmp in [
            r#"<rdf:Description crs:LensProfileEnable="0" crs:Exposure2012="+0.50" />"#,
            r#"<rdf:Description crs:Exposure2012="+0.50" />"#,
        ] {
            let preset = convert_xmp_sidecar_to_preset(xmp).unwrap();
            assert!(preset.adjustments.get("lensCorrectionMode").is_none());
        }

        let reusable =
            convert_xmp_to_preset(r#"<rdf:Description crs:LensProfileEnable="1" />"#).unwrap();
        assert!(reusable.adjustments.get("lensCorrectionMode").is_none());
    }
}
