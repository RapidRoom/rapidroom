use image::DynamicImage;
use std::borrow::Cow;
use std::collections::HashMap;

use crate::app_state::AppState;
use crate::image_processing::{
    IntoCowImage, apply_coarse_rotation, apply_crop, apply_flip, apply_geometry_warp,
    apply_rotation, crop_in_pixels,
};

pub fn hydrate_sub_masks(
    sub_masks: &mut Vec<serde_json::Value>,
    cache: &mut HashMap<String, serde_json::Value>,
) {
    for sub_mask in sub_masks {
        let id = sub_mask
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if id.is_empty() {
            continue;
        }

        if let Some(params) = sub_mask
            .get_mut("parameters")
            .and_then(|p| p.as_object_mut())
        {
            let keys_to_check = ["mask_data_base64", "maskDataBase64"];
            for key in keys_to_check {
                if params.contains_key(key) {
                    let val = params.get(key).unwrap();
                    if !val.is_null() {
                        cache.insert(id.clone(), val.clone());
                    } else {
                        if let Some(cached_data) = cache.get(&id) {
                            params.insert(key.to_string(), cached_data.clone());
                        }
                    }
                }
            }
        }
    }
}

pub fn hydrate_adjustments(state: &tauri::State<AppState>, adjustments: &mut serde_json::Value) {
    let mut cache = state.patch_cache.lock().unwrap();

    if let Some(patches) = adjustments
        .get_mut("aiPatches")
        .and_then(|v| v.as_array_mut())
    {
        for patch in patches {
            let id = patch
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            if !id.is_empty() {
                let has_data = patch.get("patchData").is_some_and(|v| !v.is_null());

                if has_data {
                    if let Some(data) = patch.get("patchData") {
                        cache.insert(id.clone(), data.clone());
                    }
                } else {
                    if let Some(cached_data) = cache.get(&id) {
                        patch["patchData"] = cached_data.clone();
                    }
                }
            }

            if let Some(sub_masks) = patch.get_mut("subMasks").and_then(|v| v.as_array_mut()) {
                hydrate_sub_masks(sub_masks, &mut cache);
            }
        }
    }

    if let Some(masks) = adjustments.get_mut("masks").and_then(|v| v.as_array_mut()) {
        for mask_container in masks {
            if let Some(sub_masks) = mask_container
                .get_mut("subMasks")
                .and_then(|v| v.as_array_mut())
            {
                hydrate_sub_masks(sub_masks, &mut cache);
            }
        }
    }
}

pub fn apply_orientation_and_flip<'a, I: IntoCowImage<'a>>(
    image: I,
    adjustments: &serde_json::Value,
) -> Cow<'a, DynamicImage> {
    let orientation_steps = adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8;
    let flip_horizontal = adjustments["flipHorizontal"].as_bool().unwrap_or(false);
    let flip_vertical = adjustments["flipVertical"].as_bool().unwrap_or(false);

    let coarse_rotated_image = apply_coarse_rotation(image.into_cow(), orientation_steps);
    apply_flip(coarse_rotated_image, flip_horizontal, flip_vertical)
}

pub fn apply_spatial_transformations<'a, I: IntoCowImage<'a>>(
    image: I,
    adjustments: &serde_json::Value,
) -> (Cow<'a, DynamicImage>, (f32, f32)) {
    let rotation_degrees = adjustments["rotation"].as_f64().unwrap_or(0.0) as f32;

    let flipped_image = apply_orientation_and_flip(image, adjustments);
    let rotated_image = apply_rotation(flipped_image, rotation_degrees);

    let crop_data = crop_in_pixels(
        &adjustments["crop"],
        rotated_image.width() as f64,
        rotated_image.height() as f64,
    );
    let crop_json = serde_json::to_value(crop_data).unwrap_or(serde_json::Value::Null);
    let cropped_image = apply_crop(rotated_image, &crop_json);

    let unscaled_crop_offset = crop_data.map_or((0.0, 0.0), |c| (c.x as f32, c.y as f32));

    (cropped_image, unscaled_crop_offset)
}

pub fn apply_all_transformations<'a, I: IntoCowImage<'a>>(
    image: I,
    adjustments: &serde_json::Value,
) -> (Cow<'a, DynamicImage>, (f32, f32)) {
    let start_time = std::time::Instant::now();
    let image = image.into_cow();

    let warped_image = apply_geometry_warp(image, adjustments);
    let relit_image = crate::relight::apply_relight(warped_image, adjustments);
    let fogged_image = crate::fog::apply_fog(relit_image, adjustments);
    let blurred_image = crate::lens_blur::apply_lens_blur(fogged_image, adjustments);

    let (cropped_image, unscaled_crop_offset) =
        apply_spatial_transformations(blurred_image, adjustments);

    let total_duration = start_time.elapsed();
    log::info!("apply_all_transformations took {:.2?}", total_duration);

    (cropped_image, unscaled_crop_offset)
}

#[cfg(test)]
mod percent_crop_tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use serde_json::json;

    #[test]
    fn saved_percent_crop_matches_oriented_pixel_image_and_mask_offset() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(80, 60, |x, y| {
            Rgb([x as u8, y as u8, (x + y) as u8])
        }));
        for steps in 0..4 {
            let (width, height) = if steps % 2 == 1 {
                (60.0, 80.0)
            } else {
                (80.0, 60.0)
            };
            for rotation in [0.0, 5.0, -5.0] {
                let percent = json!({
                    "orientationSteps": steps, "rotation": rotation,
                    "flipHorizontal": true,
                    "crop": {"unit":"%", "x":10, "y":20, "width":50, "height":40}
                });
                let mut pixel = percent.clone();
                pixel["crop"] = json!({"unit":"px", "x":width/10.0, "y":height/5.0, "width":width/2.0, "height":height*0.4});
                let (actual, offset) = apply_spatial_transformations(&source, &percent);
                let (expected, expected_offset) = apply_spatial_transformations(&source, &pixel);
                assert_eq!(
                    actual.to_rgb8(),
                    expected.to_rgb8(),
                    "steps={steps} rotation={rotation}"
                );
                assert_eq!(offset, expected_offset);
                assert_eq!(actual.width(), (width / 2.0) as u32);
                assert_eq!(actual.height(), (height * 0.4) as u32);
                let mut implicit = pixel;
                implicit["crop"].as_object_mut().unwrap().remove("unit");
                assert_eq!(
                    apply_spatial_transformations(&source, &implicit)
                        .0
                        .to_rgb8(),
                    expected.to_rgb8()
                );
            }
        }
    }

    #[test]
    fn resized_render_retains_percent_coordinates_until_image_dimensions_are_known() {
        let source = DynamicImage::new_rgb8(40, 30);
        let percent = json!({"unit":"%", "x":10, "y":20, "width":50, "height":40});
        let pixel = json!({"unit":"px", "x":8, "y":12, "width":40, "height":24});
        let percent_scaled = crate::image_processing::scale_crop_value(&percent, 0.5);
        let pixel_scaled = crate::image_processing::scale_crop_value(&pixel, 0.5);
        let (actual, offset) =
            apply_spatial_transformations(&source, &json!({"crop":percent_scaled}));
        let (expected, expected_offset) =
            apply_spatial_transformations(&source, &json!({"crop":pixel_scaled}));
        assert_eq!(actual.to_rgb8(), expected.to_rgb8());
        assert_eq!((actual.width(), actual.height()), (20, 12));
        assert_eq!(offset, expected_offset);
        assert_eq!(
            percent,
            json!({"unit":"%", "x":10, "y":20, "width":50, "height":40})
        );
    }
}
