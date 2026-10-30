use crate::{
    file_management::Preset,
    xmp_payload::{Payload, ProjectedValue, Projection},
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;

pub(crate) static WRITE_LOCK: Mutex<()> = Mutex::new(());

const CRS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";
const CURVES: &[&str] = &[
    "ToneCurvePV2012",
    "ToneCurvePV2012Red",
    "ToneCurvePV2012Green",
    "ToneCurvePV2012Blue",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NamedLoss {
    pub field: String,
    pub name: String,
    pub reason: &'static str,
    pub preserved_in_payload: bool,
}

pub(crate) struct Document {
    pub xml: String,
    pub losses: Vec<NamedLoss>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportReport {
    pub file_path: String,
    pub losses: Vec<NamedLoss>,
    pub written: bool,
}

pub(crate) fn export(
    image_path: &str,
    file_path: &str,
    adjustments: &Value,
    write: bool,
) -> Result<ExportReport, String> {
    let (image, _) = crate::file_management::parse_virtual_path(image_path);
    let destination = Path::new(file_path);
    if !destination
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xmp"))
    {
        return Err("Choose an .xmp sidecar destination".into());
    }
    let target = if destination.exists() {
        destination.canonicalize().map_err(|e| e.to_string())?
    } else {
        destination.to_path_buf()
    };
    if !target
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xmp"))
    {
        return Err("The sidecar destination resolves to a different file type".into());
    }
    if write {
        crate::file_management::ensure_card_writable(&image)?;
        crate::file_management::ensure_card_writable(&target)?;
    }
    let _lock = WRITE_LOCK.lock().map_err(|_| "XMP writer is unavailable")?;
    let existed = target.exists();
    let original = match crate::xmp::read(&target) {
        Ok(content) => content,
        Err(_) if !target.exists() => crate::xmp::SKELETON.into(),
        Err(error) => return Err(error),
    };
    let document = serialize(&original, adjustments, &image)?;
    if write {
        if target.exists() != existed || existed && crate::xmp::read(&target)? != original {
            return Err("The sidecar changed during export; review it again before writing".into());
        }
        crate::file_management::write_file_atomically(&target, &document.xml)
            .map_err(|e| e.to_string())?;
    }
    Ok(ExportReport {
        file_path: file_path.into(),
        losses: document.losses,
        written: write,
    })
}

#[tauri::command]
pub(crate) async fn export_xmp_round_trip(
    image_path: String,
    file_path: String,
    adjustments: Value,
    write: bool,
) -> Result<ExportReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        export(&image_path, &file_path, &adjustments, write)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub(crate) fn defaults() -> Result<Value, String> {
    let schema: Value =
        serde_json::from_str(include_str!("../../rapidroom/adjustment-schema.json"))
            .map_err(|e| e.to_string())?;
    let parameters = schema["parameters"]
        .as_object()
        .ok_or("Invalid adjustment defaults")?;
    Ok(Value::Object(
        parameters
            .iter()
            .filter(|(key, _)| !key.contains('.'))
            .map(|(key, value)| (key.clone(), value["default"].clone()))
            .collect(),
    ))
}

fn scalar_keys() -> Vec<String> {
    let mut keys: Vec<String> = crate::preset_converter::BASIC_MAPPINGS
        .iter()
        .map(|(key, _)| (*key).into())
        .collect();
    keys.extend([
        "ProcessVersion".into(),
        "HasSettings".into(),
        "ToneCurveName2012".into(),
        "SplitToningBalance".into(),
    ]);
    keys.extend(
        crate::preset_converter::COLOR_GRADING_MAPPINGS
            .iter()
            .map(|(tag, _, _)| (*tag).into()),
    );
    for band in [
        "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
    ] {
        for channel in [
            "HueAdjustment",
            "SaturationAdjustment",
            "LuminanceAdjustment",
        ] {
            keys.push(format!("{channel}{band}"));
        }
    }
    keys
}

const TIFF: &str = "http://ns.adobe.com/tiff/1.0/";
const TIFF_FRAME: &[&str] = &["ImageWidth", "ImageLength", "Orientation"];

fn record_projection(content: &str) -> Result<Projection, String> {
    let mut recorded = Projection::new();
    for key in scalar_keys()
        .into_iter()
        .chain(CROP_FIELDS.iter().map(|key| key.to_string()))
    {
        if let Some(value) =
            crate::xmp::image_property(content, CRS, &key).map_err(|e| e.to_string())?
        {
            recorded.insert(
                format!("crs:{key}"),
                ProjectedValue::Scalar(value.trim().to_string()),
            );
        }
    }
    for key in CURVES {
        if let Some(points) =
            crate::xmp::image_sequence(content, CRS, key).map_err(|e| e.to_string())?
        {
            recorded.insert(format!("crs:{key}"), ProjectedValue::Sequence(points));
        }
    }
    for key in TIFF_FRAME {
        if let Some(value) =
            crate::xmp::image_property(content, TIFF, key).map_err(|e| e.to_string())?
        {
            recorded.insert(
                format!("tiff:{key}"),
                ProjectedValue::Scalar(value.trim().to_string()),
            );
        }
    }
    Ok(recorded)
}

fn projection(content: &str, image: &Path) -> Result<Value, String> {
    let canonical = crate::xmp::camera_raw_view(content).map_err(|e| e.to_string())?;
    Ok(
        crate::preset_converter::convert_xmp_sidecar_to_preset_for_image(&canonical, image)?
            .adjustments,
    )
}

fn display_name(field: &str) -> String {
    field
        .split('.')
        .map(|part| {
            if part.eq_ignore_ascii_case("hsl") || part.eq_ignore_ascii_case("lut") {
                return part.to_ascii_uppercase();
            }
            let mut words = String::new();
            for (index, letter) in part.chars().enumerate() {
                if index == 0 {
                    words.extend(letter.to_uppercase());
                } else {
                    if letter.is_uppercase() {
                        words.push(' ');
                    }
                    words.push(letter);
                }
            }
            words
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

const CROP_FIELDS: &[&str] = &[
    "HasCrop",
    "CropLeft",
    "CropTop",
    "CropRight",
    "CropBottom",
    "CropAngle",
    "CropConstrainAspectRatio",
];

fn clear_crop(content: &str) -> Result<String, String> {
    crate::xmp::replace_image_properties(
        content,
        CRS,
        "crs",
        CROP_FIELDS,
        &[("HasCrop", "False".into())],
    )
    .map_err(|e| e.to_string())
}

fn project_crop(
    content: &str,
    adjustments: &Value,
    image: &Path,
) -> Result<(String, bool), String> {
    let Some(crop) = adjustments.get("crop").filter(|value| !value.is_null()) else {
        return clear_crop(content).map(|xml| (xml, true));
    };
    if adjustments["orientationSteps"].as_u64().unwrap_or(0) != 0
        || adjustments["flipHorizontal"].as_bool().unwrap_or(false)
        || adjustments["flipVertical"].as_bool().unwrap_or(false)
    {
        return clear_crop(content).map(|xml| (xml, false));
    }
    let property = |key: &str| {
        crate::xmp::image_property(content, TIFF, key)
            .ok()
            .flatten()
            .and_then(|value| value.parse::<f64>().ok())
    };
    let frame = match (
        property("ImageWidth"),
        property("ImageLength"),
        property("Orientation"),
    ) {
        (Some(w), Some(h), Some(o)) => Some((w, h, o as u16)),
        _ => crate::preset_converter::photo_xmp_export_frame(image),
    };
    let Some((width, height, orientation)) = frame else {
        return clear_crop(content).map(|xml| (xml, false));
    };
    if !width.is_finite()
        || !height.is_finite()
        || width < 1.0
        || height < 1.0
        || !(1..=8).contains(&orientation)
    {
        return Err("Cannot project a crop with an invalid photo frame".into());
    }
    let (ow, oh) = if (5..=8).contains(&orientation) {
        (height, width)
    } else {
        (width, height)
    };
    let percent = match crop["unit"].as_str() {
        None | Some("px") => false,
        Some("%") => true,
        _ => return Err("Unsupported crop coordinate unit".into()),
    };
    let value = |key: &str, scale: f64| {
        crop[key]
            .as_f64()
            .filter(|v| v.is_finite())
            .map(|v| if percent { v * scale / 100.0 } else { v })
            .ok_or_else(|| format!("Invalid crop {key}"))
    };
    let (x, y, w, h) = (
        value("x", ow)?,
        value("y", oh)?,
        value("width", ow)?,
        value("height", oh)?,
    );
    if x < 0.0 || y < 0.0 || w <= 0.0 || h <= 0.0 || x + w > ow + 1e-6 || y + h > oh + 1e-6 {
        return Err("The crop lies outside the photo frame".into());
    }
    let angle = adjustments["rotation"].as_f64().unwrap_or(0.0);
    let first = crate::preset_converter::rotate_point_clockwise(x, y, ow / 2.0, oh / 2.0, -angle);
    let second =
        crate::preset_converter::rotate_point_clockwise(x + w, y + h, ow / 2.0, oh / 2.0, -angle);
    let inverse_orientation = match orientation {
        6 => 8,
        8 => 6,
        other => other,
    };
    let first = crate::preset_converter::orient_normalized_point(
        first.0 / ow,
        first.1 / oh,
        inverse_orientation,
    );
    let second = crate::preset_converter::orient_normalized_point(
        second.0 / ow,
        second.1 / oh,
        inverse_orientation,
    );
    let (left, top, right, bottom) = (
        first.0.min(second.0),
        first.1.min(second.1),
        first.0.max(second.0),
        first.1.max(second.1),
    );
    if ![left, top, right, bottom, angle]
        .iter()
        .all(|v| v.is_finite())
        || left < 0.0
        || top < 0.0
        || right > 1.0
        || bottom > 1.0
    {
        return clear_crop(content).map(|xml| (xml, false));
    }
    let xml = crate::xmp::replace_image_properties(
        content,
        TIFF,
        "tiff",
        &["ImageWidth", "ImageLength", "Orientation"],
        &[
            ("ImageWidth", width.to_string()),
            ("ImageLength", height.to_string()),
            ("Orientation", orientation.to_string()),
        ],
    )
    .map_err(|e| e.to_string())?;
    crate::xmp::replace_image_properties(
        &xml,
        CRS,
        "crs",
        CROP_FIELDS,
        &[
            ("HasCrop", "True".into()),
            ("CropLeft", left.to_string()),
            ("CropTop", top.to_string()),
            ("CropRight", right.to_string()),
            ("CropBottom", bottom.to_string()),
            ("CropAngle", (-angle).to_string()),
            ("CropConstrainAspectRatio", "False".into()),
        ],
    )
    .map(|xml| (xml, true))
    .map_err(|e| e.to_string())
}

fn sanitize_adjustments(adjustments: &Value) -> Result<Value, String> {
    let mut sanitized = adjustments.clone();
    if let Some(path) = sanitized.get("lutPath").and_then(Value::as_str) {
        let basename = path
            .rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
            .ok_or("The LUT path must name a file")?;
        sanitized["lutPath"] = Value::String(basename.into());
    }
    Ok(sanitized)
}

pub(crate) fn serialize(
    content: &str,
    adjustments: &Value,
    image: &Path,
) -> Result<Document, String> {
    let adjustments = sanitize_adjustments(adjustments)?;
    crate::xmp_payload::validate_edit(&adjustments)?;
    let source_sha256 = crate::xmp_payload::source_sha256(image)?;
    let preset = Preset {
        id: String::new(),
        name: String::new(),
        description: None,
        adjustments: adjustments.clone(),
        include_masks: None,
        include_crop_transform: None,
        preset_type: None,
        favorite: None,
        camera_model_restriction: None,
        unavailable_reason: None,
    };
    let (projected, unsupported) = crate::preset_xmp::serialize(&preset)?;
    let keys = scalar_keys();
    let owned: Vec<&str> = keys.iter().map(String::as_str).collect();
    let mut values = Vec::new();
    for key in &owned {
        let existing_process = if *key == "ProcessVersion" {
            crate::xmp::image_property(content, CRS, key)
                .map_err(|e| e.to_string())?
                .filter(|value| {
                    value
                        .parse::<f64>()
                        .is_ok_and(|v| v.is_finite() && v >= 11.0)
                })
        } else {
            None
        };
        if let Some(value) = existing_process
            .or(crate::xmp::image_property(&projected, CRS, key).map_err(|e| e.to_string())?)
        {
            values.push((*key, value));
        }
    }
    let xml = crate::xmp::replace_image_properties(content, CRS, "crs", &owned, &values)
        .map_err(|e| e.to_string())?;
    let has_look_curve = crate::preset_converter::has_nested_look_luma_curve(content);
    let original_luma =
        crate::xmp::image_sequence(content, CRS, CURVES[0]).map_err(|e| e.to_string())?;
    if has_look_curve
        && original_luma.as_ref().is_some_and(|points| {
            points.iter().any(|point| {
                !point.split_once(',').is_some_and(|(x, y)| {
                    x.trim().parse::<u8>().is_ok() && y.trim().parse::<u8>().is_ok()
                })
            })
        })
    {
        return Err(
            "Cannot preserve an invalid outer curve beside a Lightroom profile Look".into(),
        );
    }
    let native_luma = adjustments
        .get("curves")
        .and_then(|curves| curves.get("luma"));
    let luma_loss = has_look_curve
        && native_luma.is_some()
        && projection(content, image)?
            .get("curves")
            .and_then(|curves| curves.get("luma"))
            != native_luma;
    let mut sequences = Vec::new();
    for key in CURVES {
        let points = if *key == CURVES[0] && has_look_curve {
            original_luma.clone()
        } else {
            crate::xmp::image_sequence(&projected, CRS, key).map_err(|e| e.to_string())?
        };
        if let Some(points) = points {
            sequences.push((*key, points));
        }
    }
    let xml = crate::xmp::replace_image_sequences(&xml, CRS, "crs", CURVES, &sequences)
        .map_err(|e| e.to_string())?;
    let (xml, crop_projected) = project_crop(&xml, &adjustments, image)?;
    let projection = record_projection(&xml)?;
    let xml = crate::xmp_payload::embed(
        &xml,
        &Payload {
            adjustments: adjustments.clone(),
            source_sha256,
            projection,
        },
    )?;
    let defaults = defaults()?;
    let mut losses = Vec::new();
    if luma_loss {
        losses.push(NamedLoss {
            field: "curves.luma".into(),
            name: "Luma curve with profile Look".into(),
            reason: "This sidecar has a profile Look tone curve. Its original Lightroom luma curve is retained; the changed RapidRoom luma curve is stored only in the full edit payload to avoid applying that Look twice.",
            preserved_in_payload: true,
        });
    }
    for field in unsupported {
        let root = field.split('.').next().unwrap_or(&field);
        if adjustments.get(root) == defaults.get(root) || adjustments.get(root).is_none() {
            continue;
        }
        if field == "crop" && crop_projected
            || field == "rotation"
                && crop_projected
                && adjustments.get("crop").is_some_and(|v| !v.is_null())
        {
            continue;
        }
        if field == "aspectRatio" {
            if adjustments.get("crop").is_none_or(Value::is_null) {
                continue;
            }
            losses.push(NamedLoss {
                field,
                name: "Crop aspect lock".into(),
                reason: "The RapidRoom aspect-lock preference is stored in the full edit payload; Lightroom chooses its own crop lock.",
                preserved_in_payload: true,
            });
        } else if field == "masks" {
            if let Some(masks) = adjustments["masks"].as_array() {
                for (index, mask) in masks.iter().enumerate() {
                    losses.push(NamedLoss {
                        field: format!("masks.{index}"),
                        name: mask["name"].as_str().filter(|s| !s.is_empty()).map_or_else(|| format!("Mask {}", index + 1), str::to_owned),
                        reason: "This RapidRoom mask is stored in the full edit payload; it is not projected as a Lightroom mask.",
                        preserved_in_payload: true,
                    });
                }
            }
        } else if field == "lutPath" {
            losses.push(NamedLoss {
                name: "LUT file".into(),
                field,
                reason: "Only the LUT basename is preserved; its external file is not embedded. Keep that LUT available when reopening the edit.",
                preserved_in_payload: true,
            });
        } else {
            losses.push(NamedLoss {
                name: display_name(&field),
                field,
                reason: "This setting is preserved in the RapidRoom payload but is not represented by this Lightroom projection.",
                preserved_in_payload: true,
            });
        }
    }
    Ok(Document { xml, losses })
}

#[derive(Debug, PartialEq)]
pub(crate) enum Restore {
    Absent,
    Restored(Value),
    Ignored(String),
}

fn scalar_equal(left: &str, right: &str) -> bool {
    let left = left.trim();
    let right = right.trim();
    if left == right {
        return true;
    }
    match (left.parse::<f64>(), right.parse::<f64>()) {
        (Ok(left), Ok(right)) => (left - right).abs() <= 1e-9,
        _ => left.eq_ignore_ascii_case(right),
    }
}

fn projected_equal(left: Option<&ProjectedValue>, right: Option<&ProjectedValue>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(ProjectedValue::Scalar(left)), Some(ProjectedValue::Scalar(right))) => {
            scalar_equal(left, right)
        }
        (Some(ProjectedValue::Sequence(left)), Some(ProjectedValue::Sequence(right))) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| {
                    let left: Vec<_> = left.split(',').collect();
                    let right: Vec<_> = right.split(',').collect();
                    left.len() == right.len()
                        && left
                            .iter()
                            .zip(right)
                            .all(|(left, right)| scalar_equal(left, right))
                })
        }
        _ => false,
    }
}

fn adjustment_paths(key: &str) -> Vec<String> {
    if CROP_FIELDS.contains(&key) || TIFF_FRAME.contains(&key) {
        return vec!["crop".into(), "rotation".into()];
    }
    if let Some(curve) = CURVES.iter().position(|curve| *curve == key) {
        return vec![format!(
            "curves.{}",
            ["luma", "red", "green", "blue"][curve]
        )];
    }
    if key == "SplitToningBalance" {
        return vec!["colorGrading.balance".into()];
    }
    if let Some((_, native)) = crate::preset_converter::BASIC_MAPPINGS
        .iter()
        .find(|(crs, _)| *crs == key)
    {
        return vec![if *native == "blending" {
            "colorGrading.blending".into()
        } else {
            (*native).into()
        }];
    }
    if let Some((_, wheel, channel)) = crate::preset_converter::COLOR_GRADING_MAPPINGS
        .iter()
        .find(|(crs, _, _)| *crs == key)
    {
        return vec![format!("colorGrading.{wheel}.{channel}")];
    }
    for (prefix, channel) in [
        ("HueAdjustment", "hue"),
        ("SaturationAdjustment", "saturation"),
        ("LuminanceAdjustment", "luminance"),
    ] {
        if let Some(band) = key.strip_prefix(prefix) {
            let native = [
                "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
            ]
            .into_iter()
            .position(|expected| expected == band)
            .map(|index| {
                [
                    "reds", "oranges", "yellows", "greens", "aquas", "blues", "purples", "magentas",
                ][index]
            });
            if let Some(native) = native {
                return vec![format!("hsl.{native}.{channel}")];
            }
        }
    }
    Vec::new()
}

fn assign_adjustment(value: &mut Value, path: &str, replacement: Option<&Value>) {
    let (key, rest) = path.split_once('.').unwrap_or((path, ""));
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    let object = value.as_object_mut().expect("adjustment object");
    if rest.is_empty() {
        if let Some(replacement) = replacement {
            object.insert(key.into(), replacement.clone());
        } else {
            object.remove(key);
        }
    } else {
        assign_adjustment(
            object.entry(key).or_insert_with(|| json!({})),
            rest,
            replacement,
        );
    }
}

fn reconcile(payload: &Payload, current: &Projection, converted: &Value) -> Result<Value, String> {
    let mut restored = payload.adjustments.clone();
    let keys: BTreeSet<_> = payload.projection.keys().chain(current.keys()).collect();
    for key in keys {
        if projected_equal(payload.projection.get(key), current.get(key)) {
            continue;
        }
        let Some(native_key) = key
            .strip_prefix("crs:")
            .or_else(|| key.strip_prefix("tiff:"))
        else {
            continue;
        };
        for path in adjustment_paths(native_key) {
            assign_adjustment(&mut restored, &path, converted.get(&path));
        }
    }
    crate::assistant::adjustments::validate_adjustments(&restored)
        .map_err(|error| format!("RapidRoom full edit rejected after reconciliation: {error}"))?;
    Ok(restored)
}

pub(crate) fn restore(content: &str, image: &Path) -> Result<Restore, String> {
    let payload = match crate::xmp_payload::read(content) {
        Ok(Some(payload)) => payload,
        Ok(None) => return Ok(Restore::Absent),
        Err(error) => return Ok(Restore::Ignored(error)),
    };
    if payload.source_sha256 != crate::xmp_payload::source_sha256(image)? {
        return Ok(Restore::Ignored(
            "the source image SHA-256 does not match".into(),
        ));
    }
    crate::assistant::adjustments::validate_adjustments(&payload.adjustments)
        .map_err(|error| format!("RapidRoom full edit rejected: {error}"))?;
    let current = record_projection(content)?;
    let converted = projection(content, image)?;
    reconcile(&payload, &current, &converted).map(Restore::Restored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn synthetic_edit() -> Value {
        json!({
            "exposure": 0.1234567,
            "contrast": 12,
            "halationAmount": 42,
            "hsl": {"reds": {"hue": 17.123456, "saturation": 30}},
            "colorGrading": {"shadows": {"hue": 220, "saturation": 18, "luminance": -4}},
            "curves": {"luma": [{"x":0,"y":0},{"x":127.4,"y":139.6},{"x":255,"y":250}]},
            "masks": [{"id":"painted-sky","name":"Painted sky","subMasks":[]}],
            "lutPath": "/owned/look.cube"
        })
    }

    fn restored(content: &str, image: &Path) -> Value {
        match restore(content, image).unwrap() {
            Restore::Restored(adjustments) => adjustments,
            other => panic!("expected native restore, got {other:?}"),
        }
    }

    fn expected_native(adjustments: &Value) -> Value {
        let mut expected = adjustments.clone();
        expected["lutPath"] = json!("look.cube");
        expected
    }

    #[test]
    fn synthetic_round_trip_is_exact_and_image_mismatch_falls_back() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("photo.ARW");
        std::fs::write(&image, b"synthetic raw bytes").unwrap();
        let adjustments = synthetic_edit();
        let document = serialize(crate::xmp::SKELETON, &adjustments, &image).unwrap();
        assert_eq!(
            restored(&document.xml, &image),
            expected_native(&adjustments)
        );
        std::fs::write(&image, b"different image bytes").unwrap();
        assert!(matches!(
            restore(&document.xml, &image).unwrap(),
            Restore::Ignored(reason) if reason.contains("SHA-256")
        ));
    }

    #[test]
    fn lightroom_projection_parses_back_and_named_losses_are_specific() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("photo.ARW");
        std::fs::write(&image, b"synthetic raw bytes").unwrap();
        let adjustments = synthetic_edit();
        let document = serialize(crate::xmp::SKELETON, &adjustments, &image).unwrap();
        crate::xmp::validate(&document.xml).unwrap();
        let projected = projection(&document.xml, &image).unwrap();
        assert_eq!(projected["exposure"], adjustments["exposure"]);
        assert_eq!(projected["contrast"], adjustments["contrast"]);
        assert_eq!(
            projected["colorGrading"]["shadows"],
            adjustments["colorGrading"]["shadows"]
        );
        assert_eq!(projected["curves"]["luma"][1], json!({"x":127,"y":140}));
        assert!(
            document
                .losses
                .iter()
                .any(|loss| loss.field == "halationAmount" && loss.name == "Halation Amount")
        );
        assert!(
            document
                .losses
                .iter()
                .any(|loss| loss.name == "Painted sky")
        );
        assert!(
            document
                .losses
                .iter()
                .any(|loss| loss.name == "LUT file" && loss.reason.contains("basename"))
        );
    }

    #[test]
    fn lightroom_exposure_change_wins_and_untouched_halation_is_restored() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("photo.ARW");
        std::fs::write(&image, b"synthetic raw bytes").unwrap();
        let document = serialize(
            crate::xmp::SKELETON,
            &json!({"exposure":0.1234567,"halationAmount":42}),
            &image,
        )
        .unwrap();
        let changed = crate::xmp::replace_image_properties(
            &document.xml,
            CRS,
            "crs",
            &["Exposure2012"],
            &[("Exposure2012", "1.5".into())],
        )
        .unwrap();
        let restored = restored(&changed, &image);
        assert_eq!(restored["exposure"], json!(1.5));
        assert_eq!(restored["halationAmount"], json!(42));
    }

    #[test]
    fn lightroom_crop_change_wins() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("photo.ARW");
        std::fs::write(&image, b"synthetic raw bytes").unwrap();
        let source = format!(
            r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:tiff="{TIFF}" tiff:ImageWidth="6000" tiff:ImageLength="4000" tiff:Orientation="1"/></rdf:RDF>"#
        );
        let document = serialize(
            &source,
            &json!({"crop":{"unit":"px","x":100,"y":100,"width":1000,"height":500},"rotation":0,"halationAmount":42}),
            &image,
        )
        .unwrap();
        let changed = crate::xmp::replace_image_properties(
            &document.xml,
            CRS,
            "crs",
            &["CropLeft", "CropRight"],
            &[("CropLeft", "0.2".into()), ("CropRight", "0.8".into())],
        )
        .unwrap();
        let restored = restored(&changed, &image);
        assert_eq!(restored["crop"]["x"], json!(1200.0));
        assert_eq!(restored["crop"]["width"], json!(3600.0));
        assert_eq!(restored["crop"]["y"], json!(100.0));
        assert_eq!(restored["crop"]["height"], json!(500.0));
        assert_eq!(restored["halationAmount"], json!(42));
    }

    #[test]
    fn unprojectable_crop_clears_stale_lightroom_crop_fields() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("photo.ARW");
        std::fs::write(&image, b"synthetic raw bytes").unwrap();
        let source = format!(
            r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:crs="{CRS}" crs:HasCrop="True" crs:CropLeft="0.1" crs:CropRight="0.9"/></rdf:RDF>"#
        );
        let document = serialize(
            &source,
            &json!({"orientationSteps":1,"crop":{"unit":"px","x":100,"y":100,"width":1000,"height":500}}),
            &image,
        )
        .unwrap();
        assert_eq!(
            crate::xmp::image_property(&document.xml, CRS, "HasCrop")
                .unwrap()
                .as_deref(),
            Some("False")
        );
        assert_eq!(
            crate::xmp::image_property(&document.xml, CRS, "CropLeft").unwrap(),
            None
        );
        assert!(document.losses.iter().any(|loss| loss.field == "crop"));
    }

    #[test]
    fn lut_is_a_basename_while_prompts_and_patch_pixels_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("photo.ARW");
        std::fs::write(&image, b"synthetic raw bytes").unwrap();
        let adjustments = json!({
            "lutPath":"/private/user/look.cube",
            "aiPatches":[{"id":"patch","prompt":"remove the sign","patchData":{"color":"YWJj","mask":"YWJj"}}]
        });
        let document = serialize(crate::xmp::SKELETON, &adjustments, &image).unwrap();
        let restored = restored(&document.xml, &image);
        assert_eq!(restored["lutPath"], json!("look.cube"));
        assert_eq!(restored["aiPatches"][0]["prompt"], json!("remove the sign"));
        assert_eq!(
            restored["aiPatches"][0]["patchData"]["color"],
            json!("YWJj")
        );
        assert_eq!(restored["aiPatches"][0]["patchData"]["mask"], json!("YWJj"));
    }

    #[test]
    fn foreign_packet_bytes_and_namespace_bindings_survive() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("photo.ARW");
        std::fs::write(&image, b"synthetic raw bytes").unwrap();
        let foreign = "<other:keep xmlns:other=\"urn:other\" data=\"raw &amp; spelling\"><![CDATA[untouched <text>]]></other:keep>";
        let source = format!(
            "<?xpacket begin=\"\"?><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\">{foreign}</rdf:Description></rdf:RDF><?xpacket end=\"w\"?>"
        );
        let first = serialize(&source, &json!({"exposure":0.5}), &image).unwrap();
        assert!(first.xml.contains(foreign));
        assert!(first.xml.ends_with("<?xpacket end=\"w\"?>"));
        let second = serialize(&first.xml, &json!({"exposure":0.5}), &image).unwrap();
        assert!(second.xml.contains(foreign));
        assert_eq!(restored(&second.xml, &image), json!({"exposure":0.5}));
    }

    #[test]
    #[ignore = "set RAPIDROOM_CC0_RAW to a read-only CC0 raw file"]
    fn cc0_raw_round_trip_is_exact_and_lightroom_projection_parses() {
        let image = std::env::var("RAPIDROOM_CC0_RAW").expect("RAPIDROOM_CC0_RAW");
        let image = Path::new(&image);
        let adjustments = synthetic_edit();
        let document = serialize(crate::xmp::SKELETON, &adjustments, image).unwrap();
        assert_eq!(
            restored(&document.xml, image),
            expected_native(&adjustments)
        );
        let projected = projection(&document.xml, image).unwrap();
        assert_eq!(projected["exposure"], adjustments["exposure"]);
        assert!(
            document
                .losses
                .iter()
                .any(|loss| loss.name == "Painted sky")
        );
    }
}
