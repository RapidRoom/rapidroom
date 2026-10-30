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
    #[serde(rename = "1.91:1")]
    Landscape,
}

impl InstagramRatio {
    pub fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Portrait => (1080, 1350),
            Self::Grid => (1080, 1440),
            Self::Stories => (1080, 1920),
            Self::Landscape => (1080, 565),
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
        // Instagram framing depends on distortion, not brightness or TCA.
        // Keep canonical distortion provenance while excluding those components.
        if let Some(data) = object.get_mut("camera_data").and_then(Value::as_object_mut) {
            data.retain(|key, _| key == "distortion");
        }
        if let Some(components) = object
            .get_mut("lens_resolution")
            .and_then(|resolution| resolution.get_mut("components"))
            .and_then(Value::as_object_mut)
        {
            components.retain(|key, _| key == "distortion");
        }
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
    if adjustments["crop"]["unit"].as_str() == Some("%") {
        geometry["crop"]["unit"] = json!("%");
    }
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
    crate::file_management::apply_raw_defaults_and_resolve_auto_lens(
        &mut recipe,
        source,
        sidecar,
        settings,
        lens_db,
    );
    recipe.adjustments
}

const FRAME_ASPECT_TOLERANCE: f64 = 0.002;

pub fn centered_frame(width: u32, height: u32, ratio: InstagramRatio) -> NormalizedFrame {
    let (rw, rh) = ratio.dimensions();
    let ratio = rw as f64 / rh as f64;
    let image_ratio = width as f64 / height.max(1) as f64;
    // A crop already within the existing usable-frame tolerance needs no further crop.
    if (image_ratio - ratio).abs() < FRAME_ASPECT_TOLERANCE {
        return NormalizedFrame {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        };
    }
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
            < FRAME_ASPECT_TOLERANCE
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

impl FramingPreview {
    fn from_render(
        bytes: Vec<u8>,
        finished_dimensions: (u32, u32),
        saved: Option<InstagramFraming>,
        signature: String,
        ratio: InstagramRatio,
    ) -> Result<Self, String> {
        let image = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        let (width, height) = image.dimensions();
        let (finished_width, finished_height) = finished_dimensions;
        let framing = saved
            .filter(|f| usable_frame(f, &signature, ratio, finished_width, finished_height))
            .unwrap_or(InstagramFraming {
                version: 1,
                ratio,
                geometry_signature: signature,
                frame: centered_frame(finished_width, finished_height, ratio),
            });
        Ok(Self {
            jpeg: base64::engine::general_purpose::STANDARD.encode(bytes),
            width,
            height,
            framing,
        })
    }
}

#[tauri::command]
pub async fn get_instagram_framing_preview(
    path: String,
    ratio: InstagramRatio,
    max_size: Option<u32>,
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
    let (bytes, finished_dimensions) = crate::generate_preview_with_geometry_for_path(
        path,
        metadata.adjustments,
        max_size.unwrap_or(1280).clamp(160, 1280),
        app_handle,
    )
    .await?;
    let saved = metadata
        .export_framing
        .and_then(|v| serde_json::from_value::<InstagramFraming>(v["instagram"].clone()).ok());
    FramingPreview::from_render(bytes, finished_dimensions, saved, signature, ratio)
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
    save_framing_for_source(&sidecar, &source, &settings, lens_db.as_deref(), framing)
}

fn save_framing_for_source(
    sidecar: &std::path::Path,
    source: &std::path::Path,
    settings: &crate::app_settings::AppSettings,
    lens_db: Option<&crate::lens_correction::LensDatabase>,
    framing: InstagramFraming,
) -> Result<(), String> {
    crate::file_management::update_export_framing(sidecar, |metadata| {
        if geometry_signature(&source_adjustments(
            metadata, source, sidecar, settings, lens_db,
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

    fn encoded_preview(width: u32, height: u32) -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(image::RgbImage::new(width, height));
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90)
            .encode_image(&image)
            .unwrap();
        bytes
    }

    fn assert_resolution_independent_frames(image: &DynamicImage, adjustments: &Value) {
        let dimensions = image.dimensions();
        let signature = geometry_signature(adjustments);
        let previews = [320, 1280, dimensions.0.max(dimensions.1)]
            .into_iter()
            .map(|size| {
                let small = image.thumbnail(size, size);
                (
                    small.dimensions(),
                    encoded_preview(small.width(), small.height()),
                )
            })
            .collect::<Vec<_>>();
        for ratio in [
            InstagramRatio::Portrait,
            InstagramRatio::Grid,
            InstagramRatio::Stories,
            InstagramRatio::Landscape,
        ] {
            let centered = centered_frame(dimensions.0, dimensions.1, ratio);
            let mut offcenter = centered.clone();
            if offcenter.width < 1.0 {
                offcenter.x = (1.0 - offcenter.width) * 0.8;
            } else {
                offcenter.y = (1.0 - offcenter.height) * 0.8;
            }
            let saved = InstagramFraming {
                version: 1,
                ratio,
                geometry_signature: signature.clone(),
                frame: offcenter,
            };
            let expected_centered = apply_instagram_crop(image.clone(), None, ratio, &signature);
            let expected_offcenter =
                apply_instagram_crop(image.clone(), Some(&saved), ratio, &signature);
            for (preview_dimensions, bytes) in &previews {
                for remembered in [None, Some(saved.clone())] {
                    let preview = FramingPreview::from_render(
                        bytes.clone(),
                        dimensions,
                        remembered.clone(),
                        signature.clone(),
                        ratio,
                    )
                    .unwrap();
                    assert_eq!((preview.width, preview.height), *preview_dimensions);
                    let expected_frame = remembered.as_ref().map_or(&centered, |f| &f.frame);
                    assert_eq!(
                        serde_json::to_value(&preview.framing.frame).unwrap(),
                        serde_json::to_value(expected_frame).unwrap(),
                        "{ratio:?} at {preview_dimensions:?} must retain canonical geometry"
                    );
                    let actual = apply_instagram_crop(
                        image.clone(),
                        Some(&preview.framing),
                        ratio,
                        &signature,
                    );
                    let expected = if remembered.is_some() {
                        &expected_offcenter
                    } else {
                        &expected_centered
                    };
                    assert_eq!(actual.dimensions(), expected.dimensions());
                    assert_eq!(actual.as_bytes(), expected.as_bytes());
                }
            }
        }
    }

    #[test]
    fn matching_finished_crop_keeps_every_pixel_with_existing_tolerance() {
        let image = DynamicImage::ImageRgb8(image::RgbImage::from_fn(800, 1065, |x, y| {
            image::Rgb([(x % 251) as u8, (y % 251) as u8, ((x + y) % 251) as u8])
        }));
        for finished in [image.clone(), image.rotate90().rotate270()] {
            let cropped =
                apply_instagram_crop(finished.clone(), None, InstagramRatio::Grid, "current");
            assert_eq!(cropped.dimensions(), finished.dimensions());
            assert_eq!(cropped.to_rgb8().as_raw(), finished.to_rgb8().as_raw());
        }
        let outside = DynamicImage::new_rgb8(800, 1060);
        let cropped = apply_instagram_crop(outside, None, InstagramRatio::Grid, "current");
        assert_eq!(cropped.dimensions(), (795, 1060));
    }

    #[test]
    fn strip_active_and_full_previews_preserve_default_and_offcenter_export_pixels() {
        let image = DynamicImage::ImageRgb8(image::RgbImage::from_fn(4608, 3072, |x, y| {
            image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x ^ y) % 256) as u8])
        }));
        // This native case previously produced y330/h2412 from1280x853,
        // and rejected a valid remembered frame entirely at320x213.
        let crop = apply_instagram_crop(image.clone(), None, InstagramRatio::Landscape, "");
        assert_eq!(crop.dimensions(), (4608, 2411));
        assert_eq!(crop.get_pixel(0, 0), image.get_pixel(0, 331));
        assert_resolution_independent_frames(&image, &json!({}));
    }

    #[test]
    fn rotated_cropped_finished_geometry_is_independent_of_preview_resolution() {
        let image = DynamicImage::ImageRgb8(image::RgbImage::from_fn(4608, 3072, |x, y| {
            image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x ^ y) % 256) as u8])
        }));
        let adjustments =
            json!({"orientationSteps":1,"crop":{"x":14,"y":20,"width":2201,"height":3407}});
        let (finished, _) =
            crate::adjustment_utils::apply_all_transformations(&image, &adjustments);
        assert_eq!(finished.dimensions(), (2201, 3407));
        assert_resolution_independent_frames(&finished, &adjustments);
    }

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
                assert!(save["lensCorrectionMode"].is_null());
            }
            assert_eq!(save["baselineExposure"], true);
            assert_eq!(save["cameraExposureCompensation"], true);

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
    fn saved_auto_frame_preview_and_save_resolve_the_physical_source_without_saving_edits() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.ARW");
        let sidecar = dir.path().join("photo.ARW.rrdata");
        // A bounded physical Sony distortion tag; saved payloads below
        // deliberately describe a different lens or source.
        let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(0x7037_u16.to_le_bytes());
        bytes.extend(8_u16.to_le_bytes());
        bytes.extend(17_u32.to_le_bytes());
        bytes.extend(26_u32.to_le_bytes());
        bytes.extend(0_u32.to_le_bytes());
        bytes.extend(16_i16.to_le_bytes());
        for value in [1024_i16; 16] {
            bytes.extend(value.to_le_bytes());
        }
        std::fs::write(&raw, &bytes).unwrap();
        // The setting controls fresh files, not previously saved Auto1 edits.
        let settings = crate::app_settings::AppSettings {
            auto_lens_correction_on_new_raws: Some(false),
            ..Default::default()
        };
        for stale in [
            json!({"k1":0.123,"radius_scale":1.0}),
            json!({"camera_data":{"distortion":vec![512;16]}}),
        ] {
            for (mode, version) in [("auto", 1), ("auto", 0), ("manual", 1)] {
                let saved = crate::image_processing::ImageMetadata {
                    adjustments: json!({"lensCorrectionMode":mode,
                        "cameraLensCorrectionVersion":version,"lensDistortionParams":stale,
                        "lensDistortionAmount":25,"contrast":20}),
                    exif: Some(HashMap::from([("Make".into(), "SONY".into())])),
                    ..Default::default()
                };
                let original = serde_json::to_vec(&saved).unwrap();
                std::fs::write(&sidecar, &original).unwrap();
                let preview = crate::file_management::load_sidecar_with_raw_defaults(
                    &sidecar, &raw, &settings, None,
                );
                assert_eq!(std::fs::read(&sidecar).unwrap(), original);
                if mode == "auto" && version == 1 {
                    assert_eq!(
                        preview.adjustments["lensDistortionParams"]["camera_data"]["distortion"][0],
                        1024
                    );
                    assert_ne!(
                        geometry_signature(&saved.adjustments),
                        geometry_signature(&preview.adjustments)
                    );
                } else {
                    assert_eq!(preview.adjustments, saved.adjustments);
                }
                let framing = InstagramFraming {
                    version: 1,
                    ratio: InstagramRatio::Portrait,
                    geometry_signature: geometry_signature(&preview.adjustments),
                    frame: centered_frame(1600, 1000, InstagramRatio::Portrait),
                };
                // The same source/recipe check and atomic writer as the command.
                save_framing_for_source(&sidecar, &raw, &settings, None, framing.clone()).unwrap();
                let after: Value =
                    serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
                assert_eq!(after["adjustments"], saved.adjustments);
                assert_eq!(after["exportFraming"]["instagram"], json!(framing));
                assert_eq!(std::fs::read(&raw).unwrap(), bytes);
                let mut changed = after;
                changed["adjustments"]["rotation"] = json!(5);
                let changed_bytes = serde_json::to_vec(&changed).unwrap();
                std::fs::write(&sidecar, &changed_bytes).unwrap();
                assert_eq!(
                    save_framing_for_source(&sidecar, &raw, &settings, None, framing).unwrap_err(),
                    "Photo geometry changed; reopen Instagram framing"
                );
                assert_eq!(std::fs::read(&sidecar).unwrap(), changed_bytes);
            }
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
