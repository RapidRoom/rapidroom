use base64::Engine;
use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tauri::Manager;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum InstagramRatio {
    #[serde(rename = "4:5")]
    Portrait,
    #[serde(rename = "3:4")]
    Grid,
    #[serde(rename = "9:16")]
    Stories,
}

impl InstagramRatio {
    pub fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Portrait => (1080, 1350),
            Self::Grid => (1080, 1440),
            Self::Stories => (1080, 1920),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NormalizedFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl NormalizedFrame {
    fn valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|n| n.is_finite())
            && self.x >= 0.0
            && self.y >= 0.0
            && self.width > 0.0
            && self.height > 0.0
            && self.x + self.width <= 1.0 + 1e-9
            && self.y + self.height <= 1.0 + 1e-9
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstagramFraming {
    pub version: u32,
    pub ratio: InstagramRatio,
    pub geometry_signature: String,
    pub frame: NormalizedFrame,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstagramExport {
    pub ratio: InstagramRatio,
    #[serde(default)]
    pub frames_by_path: HashMap<String, InstagramFraming>,
}

pub fn geometry_signature(adjustments: &Value) -> String {
    // Use the renderer's effective values, so sparse/library metadata and the
    // editor's explicit defaults describe the same geometry.
    let params = crate::image_processing::get_geometry_params_from_json(adjustments);
    let mut warp = serde_json::to_value(params).expect("Geometry parameters serialize");
    if let Some(object) = warp.as_object_mut() {
        object.retain(|key, _| {
            !key.contains("vignette")
                && !key.starts_with("vig_")
                && !key.starts_with("tca_")
                && !key.starts_with("lens_tca_")
        });
        if object
            .get("guided_perspective_enabled")
            .and_then(Value::as_bool)
            != Some(true)
        {
            object.insert("guided_lines".into(), json!([]));
        }
    }
    let crop =
        serde_json::from_value::<crate::image_processing::Crop>(adjustments["crop"].clone()).ok();
    let mut geometry = json!({
        "warp":warp,
        "crop":crop,
        "orientationSteps":(adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8) % 4,
        "rotation":adjustments["rotation"].as_f64().unwrap_or(0.0) as f32,
        "flipHorizontal":adjustments["flipHorizontal"].as_bool().unwrap_or(false),
        "flipVertical":adjustments["flipVertical"].as_bool().unwrap_or(false),
    });
    if adjustments["lensCorrectionMode"].as_str() == Some("auto")
        && adjustments["cameraLensCorrectionVersion"].as_u64() == Some(1)
    {
        // Keep every legacy signature intact; an Auto camera policy can change
        // distortion even when its stored Lensfun coefficients are identical.
        geometry["cameraLensCorrectionVersion"] = json!(1);
    }
    hex::encode(Sha256::digest(geometry.to_string().as_bytes()))
}

fn source_adjustments(
    metadata: &Value,
    source: &std::path::Path,
    sidecar: &std::path::Path,
    settings: &crate::app_settings::AppSettings,
    lens_db: Option<&crate::lens_correction::LensDatabase>,
) -> Value {
    let mut recipe = crate::image_processing::ImageMetadata {
        adjustments: metadata["adjustments"].clone(),
        exif: serde_json::from_value(metadata["exif"].clone()).unwrap_or_default(),
        ..Default::default()
    };
    crate::file_management::apply_fresh_raw_defaults(
        &mut recipe,
        source,
        sidecar,
        settings,
        lens_db,
    );
    recipe.adjustments
}

pub fn centered_frame(width: u32, height: u32, ratio: InstagramRatio) -> NormalizedFrame {
    let (rw, rh) = ratio.dimensions();
    let ratio = rw as f64 / rh as f64;
    let image_ratio = width as f64 / height.max(1) as f64;
    let (w, h) = if image_ratio > ratio {
        (ratio / image_ratio, 1.0)
    } else {
        (1.0, image_ratio / ratio)
    };
    NormalizedFrame {
        x: (1.0 - w) / 2.0,
        y: (1.0 - h) / 2.0,
        width: w,
        height: h,
    }
}

fn usable_frame(
    framing: &InstagramFraming,
    signature: &str,
    ratio: InstagramRatio,
    width: u32,
    height: u32,
) -> bool {
    let (rw, rh) = ratio.dimensions();
    framing.version == 1
        && framing.ratio == ratio
        && framing.geometry_signature == signature
        && framing.frame.valid()
        && ((framing.frame.width * width as f64) / (framing.frame.height * height as f64)
            - rw as f64 / rh as f64)
            .abs()
            < 0.002
}

pub fn apply_instagram_crop(
    image: DynamicImage,
    framing: Option<&InstagramFraming>,
    ratio: InstagramRatio,
    signature: &str,
) -> DynamicImage {
    let (width, height) = image.dimensions();
    let default = centered_frame(width, height, ratio);
    let frame = framing
        .filter(|f| usable_frame(f, signature, ratio, width, height))
        .map_or(&default, |f| &f.frame);
    let x = (frame.x * width as f64)
        .round()
        .clamp(0.0, (width - 1) as f64) as u32;
    let y = (frame.y * height as f64)
        .round()
        .clamp(0.0, (height - 1) as f64) as u32;
    let w = (frame.width * width as f64)
        .round()
        .clamp(1.0, (width - x) as f64) as u32;
    let h = (frame.height * height as f64)
        .round()
        .clamp(1.0, (height - y) as f64) as u32;
    image.crop_imm(x, y, w, h)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FramingPreview {
    jpeg: String,
    width: u32,
    height: u32,
    framing: InstagramFraming,
}

#[tauri::command]
pub async fn get_instagram_framing_preview(
    path: String,
    ratio: InstagramRatio,
    app_handle: tauri::AppHandle,
) -> Result<FramingPreview, String> {
    let (source, sidecar) = crate::file_management::parse_virtual_path(&path);
    let settings = crate::app_settings::load_settings(app_handle.clone()).unwrap_or_default();
    let state = app_handle.state::<crate::AppState>();
    let lens_db = state.lens_db.lock().unwrap().clone();
    let metadata = crate::file_management::load_sidecar_with_raw_defaults(
        &sidecar,
        &source,
        &settings,
        lens_db.as_deref(),
    );
    let signature = geometry_signature(&metadata.adjustments);
    let bytes =
        crate::generate_preview_bytes_for_path(path, metadata.adjustments, 1280, app_handle)
            .await?;
    let image = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let (width, height) = image.dimensions();
    let saved = metadata
        .export_framing
        .and_then(|v| serde_json::from_value::<InstagramFraming>(v["instagram"].clone()).ok());
    let framing = saved
        .filter(|f| usable_frame(f, &signature, ratio, width, height))
        .unwrap_or(InstagramFraming {
            version: 1,
            ratio,
            geometry_signature: signature,
            frame: centered_frame(width, height, ratio),
        });
    Ok(FramingPreview {
        jpeg: base64::engine::general_purpose::STANDARD.encode(bytes),
        width,
        height,
        framing,
    })
}

#[tauri::command]
pub fn save_instagram_framing(
    path: String,
    framing: InstagramFraming,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    if framing.version != 1 || !framing.frame.valid() {
        return Err("Invalid Instagram frame".into());
    }
    let (source, sidecar) = crate::file_management::parse_virtual_path(&path);
    let settings = crate::app_settings::load_settings(app_handle.clone()).unwrap_or_default();
    let state = app_handle.state::<crate::AppState>();
    let lens_db = state.lens_db.lock().unwrap().clone();
    crate::file_management::update_export_framing(&sidecar, |metadata| {
        if geometry_signature(&source_adjustments(
            metadata,
            &source,
            &sidecar,
            &settings,
            lens_db.as_deref(),
        )) != framing.geometry_signature
        {
            return Err("Photo geometry changed; reopen Instagram framing".into());
        }
        let mut export_framing = metadata["exportFraming"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        export_framing.insert("instagram".into(), json!(framing));
        Ok(Value::Object(export_framing))
    })
}

#[cfg(test)]
mod framing_geometry_regression {
    use super::*;

    #[test]
    fn fresh_frame_preview_and_save_use_the_same_logical_recipe_without_saving_edits() {
        for enabled in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let raw = dir.path().join("photo.ARW");
            let sidecar = dir.path().join("photo.ARW.rrdata");
            let bytes = crate::exif_processing::rating_samples::sony_arw(true, Some("4"), None);
            std::fs::write(&raw, &bytes).unwrap();
            let settings = crate::app_settings::AppSettings {
                auto_lens_correction_on_new_raws: Some(enabled),
                ..Default::default()
            };
            let preview = crate::file_management::load_sidecar_with_raw_defaults(
                &sidecar, &raw, &settings, None,
            );
            let save = source_adjustments(
                &json!({"adjustments":null}),
                &raw,
                &sidecar,
                &settings,
                None,
            );
            assert_eq!(preview.adjustments, save);
            assert_eq!(
                geometry_signature(&preview.adjustments),
                geometry_signature(&save)
            );
            assert!(!sidecar.exists());
            assert_eq!(std::fs::read(&raw).unwrap(), bytes);
            if enabled {
                assert_eq!(save["cameraLensCorrectionVersion"], 1);
            } else {
                assert!(save.is_null());
            }

            let saved = crate::image_processing::ImageMetadata {
                adjustments: json!({"lensCorrectionMode":"manual","contrast":20}),
                ..Default::default()
            };
            let original = serde_json::to_vec(&saved).unwrap();
            std::fs::write(&sidecar, &original).unwrap();
            let preview = crate::file_management::load_sidecar_with_raw_defaults(
                &sidecar, &raw, &settings, None,
            );
            let save = source_adjustments(
                &serde_json::to_value(&saved).unwrap(),
                &raw,
                &sidecar,
                &settings,
                None,
            );
            assert_eq!(preview.adjustments, saved.adjustments);
            assert_eq!(save, saved.adjustments);
            assert_eq!(std::fs::read(&sidecar).unwrap(), original);
        }
    }

    #[test]
    fn first_editor_colour_save_retains_a_sparse_sidecars_export_framing() {
        let sparse = json!({});
        let mut editor = json!({"crop":null,"orientationSteps":0,"rotation":0,"flipHorizontal":false,"flipVertical":false,
            "transformDistortion":0,"transformVertical":0,"transformHorizontal":0,"transformRotate":0,"transformAspect":0,"transformScale":100,"transformXOffset":0,"transformYOffset":0,
            "lensDistortionAmount":100,"lensDistortionEnabled":true,"lensDistortionParams":null,"lensVignetteAmount":75,"lensTcaAmount":50,
            "lensMaker":null,"lensModel":null,"guidedPerspective":{"enabled":false,"lines":[],"autoCrop":true},"exposure":1.25,"contrast":20});
        assert_eq!(
            geometry_signature(&Value::Null),
            geometry_signature(&sparse)
        );
        assert_eq!(geometry_signature(&sparse), geometry_signature(&editor));
        let remembered = InstagramFraming {
            version: 1,
            ratio: InstagramRatio::Portrait,
            geometry_signature: geometry_signature(&sparse),
            frame: NormalizedFrame {
                x: 0.1,
                y: 0.0,
                width: 0.8 / 1.5,
                height: 1.0,
            },
        };
        assert!(usable_frame(
            &remembered,
            &geometry_signature(&editor),
            InstagramRatio::Portrait,
            1200,
            800
        ));
        editor["lensCorrectionMode"] = json!("auto");
        editor["cameraLensCorrectionVersion"] = json!(0);
        assert_eq!(geometry_signature(&sparse), geometry_signature(&editor));
        editor["cameraLensCorrectionVersion"] = json!(1);
        assert!(!usable_frame(
            &remembered,
            &geometry_signature(&editor),
            InstagramRatio::Portrait,
            1200,
            800
        ));
        editor["lensCorrectionMode"] = json!("manual");
        assert_eq!(geometry_signature(&sparse), geometry_signature(&editor));
        editor["crop"] = json!({"x":10,"y":20,"width":900,"height":600});
        assert_ne!(geometry_signature(&sparse), geometry_signature(&editor));
        assert!(!usable_frame(
            &remembered,
            &geometry_signature(&editor),
            InstagramRatio::Portrait,
            1200,
            800
        ));
    }
}
