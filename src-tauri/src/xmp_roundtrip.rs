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
    keys.extend(["ProcessVersion".into(), "HasSettings".into()]);
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
        if let Some(value) =
            crate::xmp::image_property(&projected, CRS, key).map_err(|e| e.to_string())?
        {
            values.push((*key, value));
        }
    }
    let xml = crate::xmp::replace_image_properties(content, CRS, "crs", &owned, &values)
        .map_err(|e| e.to_string())?;
    let mut sequences = Vec::new();
    for key in CURVES {
        if let Some(points) =
            crate::xmp::image_sequence(&projected, CRS, key).map_err(|e| e.to_string())?
        {
            sequences.push((*key, points));
        }
    }
    let xml = crate::xmp::replace_image_sequences(&xml, CRS, "crs", CURVES, &sequences)
        .map_err(|e| e.to_string())?;
    let baseline = projection(&xml, image)?;
    let xml = crate::xmp_payload::embed(
        &xml,
        &Payload {
            adjustments: adjustments.clone(),
            projection: baseline,
        },
    )?;
    let defaults = defaults()?;
    let mut losses = vec![NamedLoss {
        field: "renderingParity".into(),
        name: "Lightroom appearance".into(),
        reason: "Adobe and RapidRoom use different rendering operators; matching slider values do not guarantee matching pixels.",
        preserved_in_payload: false,
    }];
    for field in unsupported {
        let root = field.split('.').next().unwrap_or(&field);
        if adjustments.get(root) == defaults.get(root) || adjustments.get(root).is_none() {
            continue;
        }
        if field == "masks" {
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
