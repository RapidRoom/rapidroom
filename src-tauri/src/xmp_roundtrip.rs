use crate::{file_management::Preset, xmp_payload::Payload};
use serde::Serialize;
use serde_json::Value;
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

fn project_crop(
    content: &str,
    adjustments: &Value,
    image: &Path,
) -> Result<(String, bool), String> {
    let Some(crop) = adjustments.get("crop").filter(|value| !value.is_null()) else {
        return crate::xmp::replace_image_properties(
            content,
            CRS,
            "crs",
            CROP_FIELDS,
            &[("HasCrop", "False".into())],
        )
        .map(|xml| (xml, true))
        .map_err(|e| e.to_string());
    };
    // Other geometric edits stay named losses rather than producing a wrong crop frame.
    if adjustments["orientationSteps"].as_u64().unwrap_or(0) != 0
        || adjustments["flipHorizontal"].as_bool().unwrap_or(false)
        || adjustments["flipVertical"].as_bool().unwrap_or(false)
    {
        return Ok((content.to_owned(), false));
    }
    const TIFF: &str = "http://ns.adobe.com/tiff/1.0/";
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
        return Ok((content.to_owned(), false));
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
        return Ok((content.to_owned(), false));
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

pub(crate) fn serialize(
    content: &str,
    adjustments: &Value,
    image: &Path,
) -> Result<Document, String> {
    crate::xmp_payload::validate_edit(adjustments)?;
    let preset = Preset {
        id: String::new(),
        name: String::new(),
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
    // Only luma includes the imported Look. RGB edits must not cause that
    // already-composed curve to be written again beside the retained Look.
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
    let (xml, crop_projected) = project_crop(&xml, adjustments, image)?;
    let baseline = projection(&xml, image)?;
    let xml = crate::xmp_payload::embed(
        &xml,
        &Payload {
            adjustments: adjustments.clone(),
            projection: baseline,
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
                    losses.push(NamedLoss { field: format!("masks.{index}"), name: mask["name"].as_str().filter(|s| !s.is_empty()).map_or_else(|| format!("Mask {}", index + 1), str::to_owned),
                        reason: "This RapidRoom mask is stored in the full edit payload; it is not projected as a Lightroom mask.", preserved_in_payload: true });
                }
            }
        } else {
            if field == "lutPath" {
                losses.push(NamedLoss { name: "LUT file".into(), field,
                    reason: "The LUT path is preserved, but its external file is not embedded. Keep that LUT available when reopening the edit.", preserved_in_payload: true });
                continue;
            }
            losses.push(NamedLoss { name: display_name(&field), field,
                reason: "This setting is preserved in the RapidRoom payload but is not represented by this Lightroom projection.", preserved_in_payload: true });
        }
    }
    Ok(Document { xml, losses })
}

pub(crate) fn restore(content: &str, image: &Path) -> Result<Option<Value>, String> {
    let Some(payload) = crate::xmp_payload::read(content)? else {
        return Ok(None);
    };
    Ok(Some(crate::xmp_payload::restore(
        &payload,
        &projection(content, image)?,
        &defaults()?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    #[ignore = "requires an unmodified private Lightroom-written custom-curve/grading sidecar"]
    fn real_lightroom_custom_curve_and_grading_round_trip() {
        let fixture = std::env::var("RAPIDROOM_LIGHTROOM_XMP_FIXTURE")
            .expect("Set RAPIDROOM_LIGHTROOM_XMP_FIXTURE to the private Lightroom-written XMP");
        let original = crate::xmp::read(Path::new(&fixture)).unwrap();
        let image = Path::new("private.ARW");
        let from_lr = projection(&original, image).unwrap();
        assert_eq!(
            crate::xmp::image_property(&original, CRS, "ToneCurveName2012")
                .unwrap()
                .as_deref(),
            Some("Custom")
        );
        assert!(from_lr["curves"]["luma"].as_array().unwrap().len() > 2);
        assert!(
            from_lr["colorGrading"]["shadows"]["saturation"]
                .as_f64()
                .unwrap()
                > 0.0
        );
        assert!(
            from_lr["colorGrading"]["midtones"]["saturation"]
                .as_f64()
                .unwrap()
                > 0.0
        );
        let document = serialize(&original, &from_lr, image).unwrap();
        let imported = projection(&document.xml, image).unwrap();
        assert_eq!(imported["curves"], from_lr["curves"]);
        assert_eq!(imported["colorGrading"], from_lr["colorGrading"]);
        assert_eq!(
            crate::xmp::image_property(&document.xml, CRS, "ProcessVersion").unwrap(),
            crate::xmp::image_property(&original, CRS, "ProcessVersion").unwrap()
        );
        assert_eq!(restore(&document.xml, image).unwrap().unwrap(), from_lr);

        let native: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/xmp/j11-native-settings.json"
        ))
        .unwrap();
        let document = serialize(crate::xmp::SKELETON, &native, image).unwrap();
        let payload = crate::xmp_payload::read(&document.xml).unwrap().unwrap();
        // Use the whole actual Lightroom document, including its nested Look.
        let changed = crate::xmp_payload::embed(&original, &payload).unwrap();
        let restored = restore(&changed, image).unwrap().unwrap();
        assert_eq!(restored["curves"], from_lr["curves"]);
        assert_eq!(restored["colorGrading"], from_lr["colorGrading"]);
        assert_eq!(restored["halationAmount"], native["halationAmount"]);
    }

    const LOOKED_SIDECAR: &str = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/">
        <crs:Look><rdf:Description crs:Amount="1"><crs:Parameters><rdf:Description>
        <crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>128, 96</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012>
        </rdf:Description></crs:Parameters></rdf:Description></crs:Look>
        <crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>128, 144</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012>
        </rdf:Description></rdf:RDF>"#;

    #[test]
    fn imported_look_luma_survives_a_red_only_edit() {
        let image = Path::new("photo.ARW");
        let mut edit = projection(LOOKED_SIDECAR, image).unwrap();
        assert_eq!(edit["curves"]["luma"].as_array().unwrap().len(), 16);
        edit["curves"]["red"] = json!([{"x":0,"y":0},{"x":128,"y":176},{"x":255,"y":255}]);
        let document = serialize(LOOKED_SIDECAR, &edit, image).unwrap();
        assert_eq!(
            crate::xmp::image_sequence(&document.xml, CRS, "ToneCurvePV2012").unwrap(),
            crate::xmp::image_sequence(LOOKED_SIDECAR, CRS, "ToneCurvePV2012").unwrap()
        );
        assert_eq!(
            projection(&document.xml, image).unwrap()["curves"],
            edit["curves"]
        );
        assert!(
            !document
                .losses
                .iter()
                .any(|loss| loss.field == "curves.luma")
        );
        assert_eq!(restore(&document.xml, image).unwrap().unwrap(), edit);
    }

    #[test]
    fn changed_look_luma_is_named_payload_only_loss_not_double_projection() {
        let image = Path::new("photo.ARW");
        let original = projection(LOOKED_SIDECAR, image).unwrap();
        let mut edit = original.clone();
        edit["curves"]["luma"] = json!([{"x":0,"y":0},{"x":128,"y":176},{"x":255,"y":255}]);
        let document = serialize(LOOKED_SIDECAR, &edit, image).unwrap();
        assert_eq!(
            crate::xmp::image_sequence(&document.xml, CRS, "ToneCurvePV2012").unwrap(),
            crate::xmp::image_sequence(LOOKED_SIDECAR, CRS, "ToneCurvePV2012").unwrap()
        );
        assert_eq!(
            projection(&document.xml, image).unwrap()["curves"]["luma"],
            original["curves"]["luma"]
        );
        assert!(
            document
                .losses
                .iter()
                .any(|loss| loss.field == "curves.luma" && loss.preserved_in_payload)
        );
        assert_eq!(restore(&document.xml, image).unwrap().unwrap(), edit);
    }

    #[test]
    fn j11_fractional_curve_and_grading_project_without_native_payload() {
        let edit: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/xmp/j11-native-settings.json"
        ))
        .unwrap();
        let image = Path::new("photo.ARW");
        let document = serialize(crate::xmp::SKELETON, &edit, image).unwrap();
        let imported = projection(&document.xml, image).unwrap();
        assert_eq!(
            crate::xmp::image_property(&document.xml, CRS, "ProcessVersion")
                .unwrap()
                .as_deref(),
            Some("11.0")
        );
        assert_eq!(imported["colorGrading"], edit["colorGrading"]);
        assert_eq!(
            imported["curves"]["luma"],
            json!([
                {"x":0,"y":13}, {"x":34,"y":13}, {"x":153,"y":93},
                {"x":235,"y":224}, {"x":255,"y":224}
            ])
        );
        assert_eq!(
            imported["curves"]["red"],
            json!([
                {"x":0,"y":0}, {"x":183,"y":205}, {"x":255,"y":255}
            ])
        );
        assert_eq!(
            crate::xmp::image_property(&document.xml, CRS, "ToneCurveName2012")
                .unwrap()
                .as_deref(),
            Some("Custom")
        );
        assert_eq!(
            document
                .losses
                .iter()
                .map(|loss| loss.field.as_str())
                .collect::<Vec<_>>(),
            vec!["halationAmount"]
        );
        assert_eq!(restore(&document.xml, image).unwrap().unwrap(), edit);
    }

    #[test]
    fn photo_crop_projects_in_all_exif_frames_and_retains_only_lock_loss() {
        for orientation in 1..=8 {
            let original = format!(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:tiff="http://ns.adobe.com/tiff/1.0/" tiff:ImageWidth="6000" tiff:ImageLength="4000" tiff:Orientation="{orientation}"/></rdf:RDF>"#
            );
            let edit = json!({"crop":{"x":100,"y":200,"width":2400,"height":3200}, "aspectRatio":0.75, "rotation":0});
            let document = serialize(&original, &edit, Path::new("photo.ARW")).unwrap();
            let imported = projection(&document.xml, Path::new("photo.ARW")).unwrap();
            for key in ["x", "y", "width", "height"] {
                assert!(
                    (imported["crop"][key].as_f64().unwrap() - edit["crop"][key].as_f64().unwrap())
                        .abs()
                        <= 1.0,
                    "orientation {orientation}: {imported}"
                );
            }
            assert_eq!(
                crate::xmp::image_property(&document.xml, CRS, "HasCrop")
                    .unwrap()
                    .as_deref(),
                Some("True")
            );
            assert_eq!(document.losses.len(), 1);
            assert_eq!(document.losses[0].name, "Crop aspect lock");
            assert_eq!(
                restore(&document.xml, Path::new("photo.ARW"))
                    .unwrap()
                    .unwrap(),
                edit
            );
        }
    }

    #[test]
    fn rotated_lightroom_crop_reexports_in_its_original_frame() {
        // Same Lightroom-written _DSC1822 geometry pinned by the importer regression.
        let original = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:tiff="http://ns.adobe.com/tiff/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" tiff:ImageWidth="6000" tiff:ImageLength="4000" tiff:Orientation="1" crs:HasCrop="True" crs:CropTop="0.020203" crs:CropLeft="0.028736" crs:CropBottom="0.916557" crs:CropRight="0.971264" crs:CropAngle="3.01"/></rdf:RDF>"#;
        let image = Path::new("photo.ARW");
        let edit = projection(original, image).unwrap();
        let document = serialize(original, &edit, image).unwrap();
        let imported = projection(&document.xml, image).unwrap();
        assert_eq!(imported["crop"], edit["crop"]);
        assert_eq!(imported["rotation"], edit["rotation"]);
        for key in ["CropTop", "CropLeft", "CropBottom", "CropRight"] {
            let before: f64 = crate::xmp::image_property(original, CRS, key)
                .unwrap()
                .unwrap()
                .parse()
                .unwrap();
            let after: f64 = crate::xmp::image_property(&document.xml, CRS, key)
                .unwrap()
                .unwrap()
                .parse()
                .unwrap();
            assert!(
                (after - before).abs() < 0.0002,
                "{key}: {before} -> {after}"
            );
        }
    }

    #[test]
    fn round_trip_restores_exact_edit_and_lightroom_changes_win() {
        let image = Path::new("test.ARW");
        let adjustments = json!({"exposure":0.1234567,"halationAmount":42,"hsl":{"reds":{"hue":17.123456,"saturation":30}},"masks":[{"name":"My mask","subMasks":[]}],"curves":{"luma":[{"x":0,"y":0},{"x":255,"y":250}]}});
        let document = serialize(crate::xmp::SKELETON, &adjustments, image).unwrap();
        assert_eq!(restore(&document.xml, image).unwrap().unwrap(), adjustments);
        assert!(
            document
                .losses
                .iter()
                .any(|loss| loss.name == "My mask" && loss.preserved_in_payload)
        );
        let changed = crate::xmp::replace_image_properties(
            &document.xml,
            CRS,
            "crs",
            &["Exposure2012"],
            &[("Exposure2012", "1.5".into())],
        )
        .unwrap();
        let restored = restore(&changed, image).unwrap().unwrap();
        assert_eq!(restored["exposure"], json!(1.5));
        assert_eq!(restored["masks"], adjustments["masks"]);
        assert_eq!(restored["halationAmount"], adjustments["halationAmount"]);
    }

    #[test]
    fn removing_all_projected_hsl_resets_only_projected_native_leaves() {
        let image = Path::new("photo.ARW");
        let adjustments =
            json!({"hsl":{"blues":{"hue":90,"saturation":30,"luminance":0}},"halationAmount":20});
        let document = serialize(crate::xmp::SKELETON, &adjustments, image).unwrap();
        assert!(
            document
                .losses
                .iter()
                .any(|loss| loss.field == "hsl.blues.hue")
        );
        let keys = scalar_keys();
        let hsl_keys: Vec<&str> = keys
            .iter()
            .filter(|key| {
                key.starts_with("HueAdjustment")
                    || key.starts_with("SaturationAdjustment")
                    || key.starts_with("LuminanceAdjustment")
            })
            .map(String::as_str)
            .collect();
        let removed =
            crate::xmp::replace_image_properties(&document.xml, CRS, "crs", &hsl_keys, &[])
                .unwrap();
        let restored = restore(&removed, image).unwrap().unwrap();
        assert_eq!(restored["hsl"]["blues"]["hue"], 90);
        assert_eq!(restored["hsl"]["blues"]["saturation"], 0.0);
        assert_eq!(restored["halationAmount"], 20);
    }

    #[test]
    fn losses_name_external_lut_files_and_native_mask_names() {
        let document = serialize(crate::xmp::SKELETON, &json!({"lutPath":"/owned/look.cube","halationAmount":20,"masks":[{"name":"Painted sky","subMasks":[]}]}), Path::new("photo.ARW")).unwrap();
        let lut = document
            .losses
            .iter()
            .find(|loss| loss.field == "lutPath")
            .unwrap();
        assert_eq!(lut.name, "LUT file");
        assert!(lut.reason.contains("not embedded"));
        assert!(
            document
                .losses
                .iter()
                .any(|loss| loss.name == "Painted sky" && loss.preserved_in_payload)
        );
        assert!(
            document
                .losses
                .iter()
                .any(|loss| loss.field == "halationAmount" && loss.preserved_in_payload)
        );
    }

    #[test]
    fn preview_does_not_write_and_bad_existing_sidecars_are_never_overwritten() {
        use crate::file_management::card_mode_test_support::CardMode;
        let _mode = CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("photo.ARW");
        let destination = dir.path().join("photo.xmp");
        let image = image.to_str().unwrap();
        let file = destination.to_str().unwrap();
        let adjustments = serde_json::json!({"exposure":1.0,"halationAmount":20});
        assert!(!export(image, file, &adjustments, false).unwrap().written);
        assert!(!destination.exists());
        std::fs::write(&destination, "<broken").unwrap();
        assert!(export(image, file, &adjustments, true).is_err());
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "<broken");
        std::fs::write(&destination, crate::xmp::SKELETON).unwrap();
        assert!(export(image, file, &adjustments, true).unwrap().written);
        let xml = std::fs::read_to_string(&destination).unwrap();
        assert_eq!(
            restore(&xml, Path::new(image)).unwrap().unwrap(),
            adjustments
        );
    }

    #[cfg(unix)]
    #[test]
    fn xmp_named_symlink_cannot_overwrite_a_photo() {
        use crate::file_management::card_mode_test_support::CardMode;
        let _mode = CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("photo.ARW");
        std::fs::write(&photo, "original photo").unwrap();
        let link = dir.path().join("evil.xmp");
        std::os::unix::fs::symlink(&photo, &link).unwrap();
        assert!(
            export(
                photo.to_str().unwrap(),
                link.to_str().unwrap(),
                &json!({"exposure":1}),
                true
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(&photo).unwrap(), "original photo");
    }

    #[test]
    fn card_mode_refuses_sidecar_writes_and_keeps_every_card_byte() {
        use crate::file_management::card_mode_test_support::{CardMode, folders, snapshot};
        let fixture = folders();
        let _mode = CardMode::on(&fixture.card);
        let before = snapshot(&fixture.card);
        let image = fixture.dcim.join("IMG_0001.jpg");
        let sidecar = fixture.dcim.join("IMG_0001.xmp");
        assert!(
            export(
                image.to_str().unwrap(),
                sidecar.to_str().unwrap(),
                &json!({"exposure":1}),
                true
            )
            .is_err()
        );
        assert_eq!(snapshot(&fixture.card), before);
    }
}
