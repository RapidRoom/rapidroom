use base64::Engine;
use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

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
    let geometry = json!({
        "warp":warp,
        "crop":crop,
        "orientationSteps":(adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8) % 4,
        "rotation":adjustments["rotation"].as_f64().unwrap_or(0.0) as f32,
        "flipHorizontal":adjustments["flipHorizontal"].as_bool().unwrap_or(false),
        "flipVertical":adjustments["flipVertical"].as_bool().unwrap_or(false),
    });
    hex::encode(Sha256::digest(geometry.to_string().as_bytes()))
}

fn source_adjustments(metadata: &Value, source: &std::path::Path) -> Value {
    let mut adjustments = metadata["adjustments"].clone();
    if adjustments.is_null() {
        crate::raw_processing::apply_camera_crop_default_from_path(&mut adjustments, source);
    }
    adjustments
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
    let metadata = crate::exif_processing::load_sidecar_with_camera_crop(&sidecar, &source);
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
pub fn save_instagram_framing(path: String, framing: InstagramFraming) -> Result<(), String> {
    if framing.version != 1 || !framing.frame.valid() {
        return Err("Invalid Instagram frame".into());
    }
    let (source, sidecar) = crate::file_management::parse_virtual_path(&path);
    crate::file_management::update_export_framing(&sidecar, |metadata| {
        if geometry_signature(&source_adjustments(metadata, &source)) != framing.geometry_signature
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
