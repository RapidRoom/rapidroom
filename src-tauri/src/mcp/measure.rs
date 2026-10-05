use ab_glyph::{Font, FontRef};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::{DynamicImage, GenericImageView, Rgb, RgbImage, imageops};
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::AppHandle;

use super::{adjustments, preview, tools};

const FONT: &[u8] = include_bytes!("../../../rapidroom/assets/poppins/Poppins-Regular.ttf");
const MAX_PIXELS: u64 = 4096 * 4096;

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct PixelRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn pixel_region(value: &Value) -> Result<PixelRegion, String> {
    let region: PixelRegion = serde_json::from_value(value.clone())
        .map_err(|_| "region requires nonnegative integer x/y and positive integer width/height in rendered native pixels")?;
    if region.width == 0
        || region.height == 0
        || region.width > 2048
        || region.height > 2048
        || region.x.checked_add(region.width).is_none()
        || region.y.checked_add(region.height).is_none()
    {
        return Err("Native region is limited to 2048 pixels per edge".into());
    }
    Ok(region)
}

fn dimension(arguments: &Value) -> Result<u32, String> {
    let value = arguments.get("maxDimension").map_or(Ok(1280), |v| {
        v.as_u64()
            .ok_or_else(|| "maxDimension must be an integer".to_string())
    })?;
    if !(128..=4096).contains(&value) {
        return Err("maxDimension must be between 128 and 4096".into());
    }
    Ok(value as u32)
}

fn linear(value: u8) -> f64 {
    let value = f64::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn channel_median(histogram: &[u64; 256], count: u64) -> f64 {
    let ranks = [(count - 1) / 2, count / 2];
    let mut total = 0;
    let mut values = [0.0; 2];
    let mut rank = 0;
    for (value, frequency) in histogram.iter().enumerate() {
        total += frequency;
        while rank < 2 && total > ranks[rank] {
            values[rank] = value as f64;
            rank += 1;
        }
    }
    (values[0] + values[1]) / 2.0
}

fn statistics(image: &DynamicImage, include_histogram: bool) -> Result<Value, String> {
    let count = u64::from(image.width()) * u64::from(image.height());
    if count == 0 || count > MAX_PIXELS {
        return Err("Measurement requires 1–16777216 rendered pixels".into());
    }
    let rgb = image.to_rgb8();
    let mut channels = [[0_u64; 256]; 3];
    let mut histogram = [0_u64; 256];
    let mut sums = [0_u64; 3];
    let mut luminances = Vec::with_capacity(count as usize);
    let mut luminance_sum = 0.0;
    for pixel in rgb.pixels() {
        for channel in 0..3 {
            channels[channel][pixel[channel] as usize] += 1;
            sums[channel] += u64::from(pixel[channel]);
        }
        let luminance =
            0.2126 * linear(pixel[0]) + 0.7152 * linear(pixel[1]) + 0.0722 * linear(pixel[2]);
        histogram[(luminance * 255.0).floor().min(255.0) as usize] += 1;
        luminance_sum += luminance;
        luminances.push(luminance);
    }
    luminances.sort_unstable_by(f64::total_cmp);
    let count_float = count as f64;
    let mean_rgb = sums.map(|sum| sum as f64 / count_float);
    let median_rgb = channels.map(|histogram| channel_median(&histogram, count));
    let clipping = channels.map(|histogram| {
        json!({
            "shadowPixels": histogram[0], "highlightPixels": histogram[255],
            "shadowPercent": histogram[0] as f64 * 100.0 / count_float,
            "highlightPercent": histogram[255] as f64 * 100.0 / count_float,
        })
    });
    let percentile = |fraction: f64| {
        let rank = fraction * (count - 1) as f64;
        let lower = rank.floor() as usize;
        let upper = rank.ceil() as usize;
        luminances[lower] + (luminances[upper] - luminances[lower]) * rank.fract()
    };
    let mut value = json!({
        "width":image.width(), "height":image.height(), "pixelCount":count,
        "stage":"rendered_srgb_u8_before_jpeg", "rgbUnits":"0–255 encoded sRGB",
        "luminanceUnits":"0–1 linear relative luminance, sRGB transfer and Rec.709 weights",
        "clippingDefinition":"encoded channel equals 0 (shadow) or 255 (highlight); not sensor RAW clipping",
        "meanRGB":mean_rgb, "medianRGB":median_rgb,
        "meanLuminance":luminance_sum / count_float,
        "medianLuminance":(luminances[(count as usize-1)/2] + luminances[count as usize/2])/2.0,
        "clipping": {"red":clipping[0],"green":clipping[1],"blue":clipping[2]},
        "luminancePercentiles":{"p05":percentile(0.05),"p25":percentile(0.25),"p50":percentile(0.5),"p75":percentile(0.75),"p95":percentile(0.95)},
        "percentileMethod":"Linear interpolation at p*(pixelCount-1) in sorted linear luminance",
    });
    if include_histogram {
        value["histogram"] = json!({"red":channels[0].as_slice(),"green":channels[1].as_slice(),"blue":channels[2].as_slice(),
            "linearLuminance":histogram.as_slice(),"bins":256,"smoothed":false,"normalized":false});
    }
    Ok(value)
}

fn white_balance(stats: &Value) -> Value {
    let rgb = stats["meanRGB"].as_array().unwrap();
    let means: Vec<_> = rgb.iter().map(|v| v.as_f64().unwrap()).collect();
    if means.iter().any(|v| *v < 1.0) || stats["meanLuminance"].as_f64().unwrap() < 0.01 {
        return json!({"available":false,"reason":"Patch is too dark or has a zero channel"});
    }
    json!({"available":true,"assumption":"User selected a neutral patch",
        "neutralRGBGainSuggestion":[means[1]/means[0],1.0,means[1]/means[2]],
        "applied":false,"warning":"Rendered sRGB gain ratios are not temperature/tint slider values or sensor calibration; review visually"})
}

fn png(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(bytes.into_inner())
}

fn content(images: &[DynamicImage]) -> Result<Value, String> {
    let mut total = 0_usize;
    let mut result = Vec::with_capacity(images.len());
    for image in images {
        let bytes = png(image)?;
        total = total
            .checked_add(bytes.len())
            .ok_or("Image payload overflow")?;
        if total > preview::MAX_PREVIEW_BYTES {
            return Err(
                "Combined image payload exceeds 8 MiB; request a smaller maxDimension".into(),
            );
        }
        result.push(json!({"type":"image","data":BASE64.encode(bytes),"mimeType":"image/png"}));
    }
    Ok(Value::Array(result))
}

fn contact_sheet(
    images: &[DynamicImage],
    labels: &[String],
    bound: u32,
) -> Result<DynamicImage, String> {
    let first = images.first().ok_or("Contact sheet needs images")?;
    let font = FontRef::try_from_slice(FONT).map_err(|e| e.to_string())?;
    let columns = if matches!(images.len(), 2 | 4) { 2 } else { 3 };
    let rows = (images.len() as u32).div_ceil(columns);
    let max_width = bound / columns;
    let max_height = bound / rows;
    let label_height = (max_width / 5).clamp(16, 48).min(max_height / 2);
    let thumbnail = first.thumbnail(max_width, max_height - label_height);
    let cell_width = thumbnail.width();
    let image_height = thumbnail.height();
    let cell_height = image_height + label_height;
    let mut sheet =
        RgbImage::from_pixel(columns * cell_width, rows * cell_height, Rgb([24, 24, 24]));
    for (index, (image, label)) in images.iter().zip(labels).enumerate() {
        if label.chars().any(|c| {
            c.is_control()
                || font.glyph_id(c).0 == 0
                || (c > '\u{024f}' && !(('\u{2000}'..='\u{206f}').contains(&c)))
        }) {
            return Err("Contact-sheet font cannot display this complete label; use separate images with exact text labels".into());
        }
        let title = format!("{}: {}", (b'A' + index as u8) as char, label);
        let mut scale = 18.0_f32.min(label_height as f32 - 4.0);
        while scale >= 8.0
            && imageproc::drawing::text_size(scale, &font, &title).0 > cell_width.saturating_sub(8)
        {
            scale -= 1.0;
        }
        if scale < 8.0 {
            return Err("Complete contact-sheet label does not fit; increase maxDimension or use separate images".into());
        }
        let x = index as u32 % columns * cell_width;
        let y = index as u32 / columns * cell_height;
        imageproc::drawing::draw_text_mut(
            &mut sheet,
            Rgb([240, 240, 240]),
            (x + 4) as i32,
            (y + 2) as i32,
            scale,
            &font,
            &title,
        );
        let thumb = image.thumbnail(cell_width, image_height).to_rgb8();
        imageops::overlay(
            &mut sheet,
            &thumb,
            (x + (cell_width - thumb.width()) / 2) as i64,
            (y + label_height + (image_height - thumb.height()) / 2) as i64,
        );
    }
    Ok(DynamicImage::ImageRgb8(sheet))
}

fn geometry(value: &Value) -> Value {
    let fields = [
        "crop",
        "orientationSteps",
        "rotation",
        "flipHorizontal",
        "flipVertical",
        "transformDistortion",
        "transformVertical",
        "transformHorizontal",
        "transformRotate",
        "transformAspect",
        "transformScale",
        "transformXOffset",
        "transformYOffset",
        "guidedPerspective",
        "lensDistortionParams",
        "lensDistortionAmount",
        "lensDistortionEnabled",
        "lensTcaAmount",
        "lensTcaEnabled",
    ];
    Value::Object(
        fields
            .into_iter()
            .map(|key| (key.to_string(), value[key].clone()))
            .collect(),
    )
}

fn same_geometry_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_geometry_value(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, value)| {
                    b.get(key)
                        .is_some_and(|other| same_geometry_value(value, other))
                })
        }
        _ => left == right,
    }
}

pub(super) fn tool_definitions() -> Vec<Value> {
    let fractions = json!({"type":"object","additionalProperties":false,"properties":{
        "x":{"type":"number","minimum":0,"maximum":1},"y":{"type":"number","minimum":0,"maximum":1},
        "width":{"type":"number","exclusiveMinimum":0,"maximum":1},"height":{"type":"number","exclusiveMinimum":0,"maximum":1}},
        "required":["x","y","width","height"]});
    let mut definitions = Vec::new();
    for (name, description) in [
        (
            "analyze",
            "Read exact rendered sRGB clipping, RGB/linear-luminance means and medians, and luminance percentiles. Optional histogram:true adds four unsmoothed 256-bin histograms. Does not change edits.",
        ),
        (
            "sample_region",
            "Read mean/median rendered RGB and linear luminance in a fractional rectangle. Optional neutral-patch RGB gain suggestion is read-only and is not temperature/tint slider calibration.",
        ),
        (
            "render_region",
            "Return a native 1:1 rendered pixel crop after edit geometry, at most 2048 pixels per edge, without upsampling or changing edits/history/revision.",
        ),
        (
            "render_compare",
            "Render 2–6 temporary patches or existing virtual copies of the active photo at the same geometry and size. Return a labelled contact sheet or separate images. Never applies edits.",
        ),
    ] {
        let mut properties = json!({"imagePath":{"type":"string"},"expectedRevision":{"type":"string","description":"Advisory; renders the latest edit and returns its editRevision even when this revision is stale."},
            "maxDimension":{"type":"integer","minimum":128,"maximum":4096,"default":1280},
            "stage":{"type":"string","enum":["edited","original"],"default":"edited"},"region":fractions});
        let mut required = vec!["imagePath"];
        if matches!(name, "analyze" | "sample_region") {
            properties["histogram"] = json!({"type":"boolean","default":false,"description":"Include four unsmoothed 256-bin pixel-count histograms."});
        }
        if name == "sample_region" {
            required.push("region");
            properties["suggestWhiteBalance"] = json!({"type":"boolean","default":false});
        }
        if name == "render_region" {
            properties.as_object_mut().unwrap().remove("maxDimension");
            properties["region"] = json!({"type":"object","additionalProperties":false,"properties":{
                "x":{"type":"integer","minimum":0},"y":{"type":"integer","minimum":0},
                "width":{"type":"integer","minimum":1,"maximum":2048},"height":{"type":"integer","minimum":1,"maximum":2048}},
                "required":["x","y","width","height"]});
            required.push("region");
        }
        if name == "render_compare" {
            properties.as_object_mut().unwrap().remove("stage");
            properties["separateImages"] = json!({"type":"boolean","default":false});
            properties["variants"] = json!({"type":"array","minItems":2,"maxItems":6,"items":{
                "type":"object","additionalProperties":false,"properties":{
                    "name":{"type":"string","minLength":1,"maxLength":80},"imagePath":{"type":"string"},
                    "changes":{"type":"object"}}}});
            required.push("variants");
        }
        definitions.push(json!({"name":name,"description":description,"annotations":{"readOnlyHint":true,"destructiveHint":false},
            "inputSchema":{"type":"object","additionalProperties":false,"properties":properties,"required":required}}));
    }
    definitions
}

fn snapshot(app: &AppHandle, arguments: &Value) -> Result<Value, String> {
    tools::get_image_state(app, arguments)
}

fn stage_adjustments(arguments: &Value, current: &Value) -> Result<Value, String> {
    let stage = match arguments.get("stage") {
        None => "edited",
        Some(value) => value.as_str().ok_or("stage must be a string")?,
    };
    match stage {
        "edited" => Ok(current.clone()),
        "original" => {
            let mut neutral = tools::original_adjustments()?;
            for (key, value) in geometry(current).as_object().unwrap() {
                neutral[key] = value.clone();
            }
            neutral["toneMapper"] = current["toneMapper"].clone();
            Ok(neutral)
        }
        _ => Err("stage must be edited or original".into()),
    }
}

pub(super) async fn call(app: &AppHandle, name: &str, arguments: &Value) -> Result<Value, String> {
    let state = snapshot(app, arguments)?;
    let path = tools::required_image_path(arguments)?;
    if name == "render_compare" {
        return compare(app, arguments, &state).await;
    }
    let parameters = stage_adjustments(arguments, &state["adjustments"])?;
    let suggest = name == "sample_region" && preview::option(arguments, "suggestWhiteBalance")?;
    let histogram =
        matches!(name, "analyze" | "sample_region") && preview::option(arguments, "histogram")?;
    let (target, native) = if name == "render_region" {
        let region = pixel_region(arguments.get("region").ok_or("region is required")?)?;
        (0, Some((region.x, region.y, region.width, region.height)))
    } else {
        (dimension(arguments)?, None)
    };
    let fraction = if native.is_none() {
        preview::region(arguments)?
    } else {
        None
    };
    if name == "sample_region" && fraction.is_none() {
        return Err("region is required".into());
    }
    let image = crate::generate_measurement_pixels_for_path(
        path.clone(),
        parameters,
        target,
        native,
        app.clone(),
    )
    .await?;
    let image = preview::crop(image, fraction);
    let mut value = if name == "render_region" {
        json!({"width":image.width(),"height":image.height(),"nativePixels":true,
            "coordinates":"pixels of the full rendered image after edit geometry/crop","content":content(&[image])?})
    } else {
        let mut value = statistics(&image, histogram)?;
        if suggest {
            value["whiteBalanceSuggestion"] = white_balance(&value);
        }
        value
    };
    tools::ensure_read_state(app, arguments, &state).await?;
    value["imagePath"] = json!(path);
    value["editRevision"] = state["editRevision"].clone();
    value["region"] = arguments.get("region").cloned().unwrap_or(Value::Null);
    value["stage"] = json!(
        arguments
            .get("stage")
            .and_then(Value::as_str)
            .unwrap_or("edited")
    );
    Ok(value)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    name: Option<String>,
    #[serde(rename = "imagePath")]
    path: Option<String>,
    changes: Option<Value>,
}

fn copy_parameters(
    active_path: &str,
    path: &str,
    current: &Value,
) -> Result<(Value, String), String> {
    use std::io::Read;
    let id = path
        .rsplit_once("?vc=")
        .map(|(_, id)| id)
        .ok_or("Variant imagePath must be an existing virtual copy")?;
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err("Invalid virtual copy identifier".into());
    }
    let (source, sidecar) = crate::file_management::parse_virtual_path(path);
    let (active, _) = crate::file_management::parse_virtual_path(active_path);
    if std::fs::canonicalize(&source).map_err(|e| e.to_string())?
        != std::fs::canonicalize(active).map_err(|e| e.to_string())?
    {
        return Err("Comparison virtual copies must share the active photo source".into());
    }
    let parameters = if path == active_path {
        current.clone()
    } else {
        // load_metadata and load_sidecar can write XMP or heal files; this path only reads a bounded snapshot.
        let file = std::fs::File::open(sidecar).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(12 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 12 * 1024 * 1024 {
            return Err("Virtual copy sidecar exceeds 12 MiB".into());
        }
        let metadata: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if !metadata["adjustments"].is_object() {
            return Err("Virtual copy has no adjustment snapshot".into());
        }
        adjustments::merge_adjustments(
            tools::original_adjustments()?,
            metadata["adjustments"].clone(),
        )?
    };
    // Match the current GUI reference label. Named-copy support belongs to #120.
    let label = format!(
        "{} (VC)",
        source.file_name().unwrap_or_default().to_string_lossy()
    );
    Ok((parameters, label))
}

async fn compare(app: &AppHandle, arguments: &Value, state: &Value) -> Result<Value, String> {
    let path = tools::required_image_path(arguments)?;
    let variants: Vec<Variant> = serde_json::from_value(
        arguments
            .get("variants")
            .cloned()
            .ok_or("variants is required")?,
    )
    .map_err(|e| format!("Invalid variants: {e}"))?;
    if !(2..=6).contains(&variants.len()) {
        return Err("Comparison requires 2–6 variants".into());
    }
    let bound = dimension(arguments)?;
    let fraction = preview::region(arguments)?;
    let separate = preview::option(arguments, "separateImages")?;
    let mut prepared = Vec::new();
    for (index, variant) in variants.into_iter().enumerate() {
        let (mut parameters, label) = if let Some(copy) = variant.path {
            let (parameters, label) = copy_parameters(&path, &copy, &state["adjustments"])?;
            if variant.name.as_ref().is_some_and(|name| name != &label) {
                return Err("Virtual-copy name must match its GUI label".into());
            }
            (parameters, label)
        } else {
            (
                state["adjustments"].clone(),
                variant
                    .name
                    .unwrap_or_else(|| format!("Variant {}", (b'A' + index as u8) as char)),
            )
        };
        if label.trim().is_empty()
            || label.chars().count() > 80
            || label.chars().any(char::is_control)
        {
            return Err("Variant names must be 1–80 characters without control characters".into());
        }
        if let Some(changes) = variant.changes {
            parameters = adjustments::merge_adjustments(parameters, changes)?;
        }
        adjustments::validate_adjustments(&parameters)?;
        if !same_geometry_value(&geometry(&parameters), &geometry(&state["adjustments"])) {
            return Err("Variants must keep the active crop/orientation/lens geometry; color and tone patches are allowed".into());
        }
        prepared.push((parameters, label));
    }
    let mut images = Vec::new();
    let mut labels = Vec::new();
    for (parameters, label) in prepared {
        let image = crate::generate_measurement_pixels_for_path(
            path.clone(),
            parameters,
            bound,
            None,
            app.clone(),
        )
        .await?;
        let image = preview::crop(image, fraction);
        if images
            .first()
            .is_some_and(|first: &DynamicImage| first.dimensions() != image.dimensions())
        {
            return Err("Comparison render dimensions differ".into());
        }
        images.push(image);
        labels.push(label);
    }
    let dimensions = images
        .iter()
        .map(|image| json!([image.width(), image.height()]))
        .collect::<Vec<_>>();
    let output = if separate {
        images
    } else {
        vec![contact_sheet(&images, &labels, bound)?]
    };
    let mut encoded = content(&output)?;
    if separate {
        let mapping = labels
            .iter()
            .enumerate()
            .map(|(i, label)| format!("{}: {}", (b'A' + i as u8) as char, label))
            .collect::<Vec<_>>()
            .join("\n");
        encoded.as_array_mut().unwrap().insert(
            0,
            json!({"type":"text","text":format!("Images follow in this order:\n{mapping}")}),
        );
    }
    tools::ensure_read_state(app, arguments, state).await?;
    Ok(
        json!({"imagePath":path,"editRevision":state["editRevision"],"labels":labels,"variantDimensions":dimensions,
        "identifiers":(0..labels.len()).map(|i|((b'A'+i as u8) as char).to_string()).collect::<Vec<_>>(),
        "separateImages":separate,"width":output[0].width(),"height":output[0].height(),"content":encoded,
        "readOnly":true,"geometry":"active rendered crop/orientation/lens geometry","mimeType":"image/png"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_keeps_exact_statistics_and_percentiles_without_histogram_echoes() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_fn(2, 1, |x, _| {
            Rgb([if x == 0 { 0 } else { 255 }; 3])
        }));
        let summary = statistics(&image, false).unwrap();
        let mut full = statistics(&image, true).unwrap();
        assert!(summary.get("histogram").is_none());
        assert_eq!(summary["luminancePercentiles"]["p05"], json!(0.05));
        assert_eq!(summary["luminancePercentiles"]["p95"], json!(0.95));
        assert!(
            serde_json::to_vec(&summary).unwrap().len() * 2
                < serde_json::to_vec(&full).unwrap().len()
        );
        full.as_object_mut().unwrap().remove("histogram");
        assert_eq!(summary, full);
    }

    #[test]
    fn comparison_grid_tracks_landscape_portrait_and_three_tile_geometry() {
        for (count, width, height, expected) in [
            (4, 1000, 666, (1000, 762)),
            (4, 666, 1000, (602, 1000)),
            (3, 1000, 666, (999, 270)),
        ] {
            let images =
                vec![
                    DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([90, 30, 40])));
                    count
                ];
            let labels = vec![String::from("Current"); count];
            let sheet = contact_sheet(&images, &labels, 1000).unwrap();
            assert_eq!(sheet.dimensions(), expected);
            assert!(sheet.width() <= 1000 && sheet.height() <= 1000);
        }
    }

    #[test]
    fn red_pixels_keep_channel_identity_and_rec709_luminance() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(1, 1, Rgb([255, 0, 0])));
        let value = statistics(&image, true).unwrap();
        assert_eq!(value["meanRGB"], json!([255.0, 0.0, 0.0]));
        assert_eq!(value["meanLuminance"], json!(0.2126));
        assert_eq!(value["clipping"]["red"]["highlightPercent"], json!(100.0));
        assert_eq!(value["clipping"]["green"]["shadowPercent"], json!(100.0));
        assert_eq!(value["histogram"]["blue"][0], json!(1));
    }

    #[test]
    fn color_patches_preserve_geometry_and_invalid_copy_ids_are_refused_before_io() {
        let current =
            json!({"exposure":0,"rotation":0,"crop":{"x":1,"y":2,"width":20,"height":10}});
        let mut alternative = current.clone();
        alternative["exposure"] = json!(0.5);
        assert_eq!(geometry(&current), geometry(&alternative));
        alternative["rotation"] = json!(90);
        assert_ne!(geometry(&current), geometry(&alternative));
        alternative["rotation"] = json!(0.0);
        assert!(same_geometry_value(
            &geometry(&current),
            &geometry(&alternative)
        ));
        for path in [
            "/not-a-photo?vc=../../secret",
            "/not-a-photo?vc=",
            "/not-a-photo",
        ] {
            assert!(copy_parameters("/not-a-photo", path, &current).is_err());
        }
    }

    #[test]
    fn incompressible_image_payload_is_refused_at_the_byte_limit() {
        let noise = RgbImage::from_fn(2048, 2048, |x, y| {
            let mut n = (y * 2048 + x).wrapping_add(0x9e3779b9);
            n = (n ^ (n >> 16)).wrapping_mul(0x85ebca6b);
            n = (n ^ (n >> 13)).wrapping_mul(0xc2b2ae35);
            n ^= n >> 16;
            Rgb([n as u8, (n >> 8) as u8, (n >> 16) as u8])
        });
        assert!(
            content(&[DynamicImage::ImageRgb8(noise)])
                .unwrap_err()
                .contains("8 MiB")
        );
    }

    #[test]
    fn known_black_and_white_pixels_have_exact_clipping_histograms_and_even_medians() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgb([0, 0, 0])
            } else {
                Rgb([255, 255, 255])
            }
        }));
        let value = statistics(&image, true).unwrap();
        assert_eq!(value["meanRGB"], json!([127.5, 127.5, 127.5]));
        assert_eq!(value["medianRGB"], json!([127.5, 127.5, 127.5]));
        assert_eq!(value["meanLuminance"], json!(0.5));
        assert_eq!(value["medianLuminance"], json!(0.5));
        for channel in ["red", "green", "blue"] {
            assert_eq!(value["clipping"][channel]["shadowPercent"], json!(50.0));
            assert_eq!(value["clipping"][channel]["highlightPercent"], json!(50.0));
            assert_eq!(value["histogram"][channel][0], json!(1));
            assert_eq!(value["histogram"][channel][255], json!(1));
        }
    }

    #[test]
    fn luminance_uses_the_srgb_transfer_and_neutral_suggestion_never_applies() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(1, 1, Rgb([128, 128, 128])));
        let value = statistics(&image, true).unwrap();
        assert!((value["meanLuminance"].as_f64().unwrap() - 0.21586050011389926).abs() < 1e-12);
        assert_eq!(value["meanLuminance"], value["medianLuminance"]);
        assert_eq!(
            white_balance(&value)["neutralRGBGainSuggestion"],
            json!([1.0, 1.0, 1.0])
        );
        assert_eq!(white_balance(&value)["applied"], json!(false));
        let black = DynamicImage::ImageRgb8(RgbImage::from_pixel(1, 1, Rgb([0, 0, 0])));
        assert_eq!(
            white_balance(&statistics(&black, true).unwrap())["available"],
            json!(false)
        );
    }

    #[test]
    fn native_regions_refuse_overflow_empty_fractional_and_excessive_requests() {
        for value in [
            json!({"x":0,"y":0,"width":0,"height":1}),
            json!({"x":0,"y":0,"width":2049,"height":1}),
            json!({"x":4294967295_u32,"y":0,"width":1,"height":1}),
            json!({"x":0.5,"y":0,"width":1,"height":1}),
        ] {
            assert!(pixel_region(&value).is_err());
        }
    }

    #[test]
    fn contact_labels_are_real_pixels_and_encoded_output_is_stable_and_bounded() {
        let images =
            vec![DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 12, Rgb([90, 30, 40]))); 6];
        let labels = ["Warm", "Cool", "Muted", "Bright", "Dark", "Neutral"].map(String::from);
        let first = contact_sheet(&images, &labels, 512).unwrap();
        let second = contact_sheet(&images, &labels, 512).unwrap();
        assert_eq!(png(&first).unwrap(), png(&second).unwrap());
        assert!(first.width() <= 512 && first.height() <= 512);
        assert!(
            first
                .to_rgb8()
                .pixels()
                .any(|p| p[0] > 200 && p[1] > 200 && p[2] > 200)
        );
        assert!(contact_sheet(&images, &vec!["東京".into(); 6], 512).is_err());
    }
}
