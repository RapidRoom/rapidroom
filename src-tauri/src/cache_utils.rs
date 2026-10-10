use crate::AppState;
use image::DynamicImage;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub const GEOMETRY_KEYS: &[&str] = &[
    "transformDistortion",
    "transformVertical",
    "transformHorizontal",
    "transformRotate",
    "transformAspect",
    "transformScale",
    "transformXOffset",
    "transformYOffset",
    "lensDistortionAmount",
    "lensVignetteAmount",
    "lensTcaAmount",
    "lensDistortionParams",
    "lensMaker",
    "lensModel",
    "lensDistortionEnabled",
    "lensTcaEnabled",
    "lensVignetteEnabled",
    "guidedPerspective",
];

fn hash_ai_patches(adjustments: &serde_json::Value, hasher: &mut DefaultHasher) {
    if let Some(patches_val) = adjustments.get("aiPatches")
        && let Some(patches_arr) = patches_val.as_array()
    {
        patches_arr.len().hash(hasher);

        for patch in patches_arr {
            if let Some(id) = patch.get("id").and_then(|v| v.as_str()) {
                id.hash(hasher);
            }

            let is_visible = patch
                .get("visible")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            is_visible.hash(hasher);

            if let Some(patch_data) = patch.get("patchData") {
                patch_data.to_string().hash(hasher);
            } else {
                patch["patchDataBase64"].as_str().unwrap_or("").hash(hasher);
            }

            if let Some(sub_masks_val) = patch.get("subMasks") {
                sub_masks_val.to_string().hash(hasher);
            }

            let invert = patch
                .get("invert")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            invert.hash(hasher);
        }
    }
}

fn hash_orientation(adjustments: &serde_json::Value, hasher: &mut DefaultHasher) {
    adjustments["orientationSteps"]
        .as_u64()
        .unwrap_or(0)
        .hash(hasher);
    adjustments["rotation"]
        .as_f64()
        .unwrap_or(0.0)
        .to_bits()
        .hash(hasher);
    adjustments["flipHorizontal"]
        .as_bool()
        .unwrap_or(false)
        .hash(hasher);
    adjustments["flipVertical"]
        .as_bool()
        .unwrap_or(false)
        .hash(hasher);
}

fn hash_effect_keys(adjustments: &serde_json::Value, keys: &[&str], hasher: &mut DefaultHasher) {
    for key in keys {
        if let Some(val) = adjustments.get(*key) {
            key.hash(hasher);
            match val.as_str() {
                Some(s) => s.hash(hasher),
                None => val.to_string().hash(hasher),
            }
        }
    }
}

pub fn calculate_patch_hash(adjustments: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();

    hash_ai_patches(adjustments, &mut hasher);

    let uses_generated_masks = adjustments
        .get("aiPatches")
        .and_then(|v| v.as_array())
        .is_some_and(|patches| {
            patches.iter().any(|patch| {
                patch
                    .get("patchData")
                    .and_then(|data| data.get("mask"))
                    .and_then(|mask| mask.as_str())
                    .is_none_or(|mask| mask.is_empty())
            })
        });
    uses_generated_masks.hash(&mut hasher);

    if uses_generated_masks {
        hash_orientation(adjustments, &mut hasher);

        for key in GEOMETRY_KEYS {
            if let Some(val) = adjustments.get(key) {
                key.hash(&mut hasher);
                val.to_string().hash(&mut hasher);
            }
        }
    }

    hasher.finish()
}

pub fn calculate_geometry_hash(adjustments: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();

    calculate_patch_hash(adjustments).hash(&mut hasher);

    for key in GEOMETRY_KEYS {
        if let Some(val) = adjustments.get(key) {
            key.hash(&mut hasher);
            val.to_string().hash(&mut hasher);
        }
    }

    hasher.finish()
}

pub fn calculate_patched_warped_hash(adjustments: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();

    calculate_geometry_hash(adjustments).hash(&mut hasher);
    calculate_effects_hash(adjustments).hash(&mut hasher);

    hasher.finish()
}

pub fn calculate_effects_hash(adjustments: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();

    let effects_visible = adjustments
        .get("sectionVisibility")
        .and_then(|v| v.get("effects"))
        .and_then(|s| s.as_bool())
        .unwrap_or(true);

    let blur_enabled = effects_visible && adjustments["lensBlurEnabled"].as_bool().unwrap_or(false);
    blur_enabled.hash(&mut hasher);

    if blur_enabled {
        hash_effect_keys(
            adjustments,
            &[
                "lensBlurAmount",
                "lensBlurDiffusion",
                "lensBlurShape",
                "lensBlurMinDepth",
                "lensBlurMaxDepth",
                "lensBlurMinFade",
                "lensBlurMaxFade",
                "lensBlurDepthMap",
            ],
            &mut hasher,
        );
    }

    let relight_enabled =
        effects_visible && adjustments["relightEnabled"].as_bool().unwrap_or(false);
    relight_enabled.hash(&mut hasher);

    if relight_enabled {
        hash_effect_keys(
            adjustments,
            &[
                "relightLights",
                "relightAmbient",
                "relightSoftness",
                "relightShine",
                "relightShadows",
                "relightShadowSoftness",
                "relightNormalMap",
            ],
            &mut hasher,
        );
    }

    let fog_enabled = effects_visible && adjustments["fogEnabled"].as_bool().unwrap_or(false);
    fog_enabled.hash(&mut hasher);

    if fog_enabled {
        hash_effect_keys(
            adjustments,
            &[
                "fogAmount",
                "fogStart",
                "fogDensity",
                "fogHeight",
                "fogVariation",
                "fogGlow",
                "fogTemperature",
                "fogTint",
                "fogDepthMap",
            ],
            &mut hasher,
        );
        hash_orientation(adjustments, &mut hasher);
    }

    hasher.finish()
}

pub fn calculate_thumbnail_base_hash(adjustments: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();

    calculate_patched_warped_hash(adjustments).hash(&mut hasher);

    adjustments["orientationSteps"]
        .as_u64()
        .unwrap_or(0)
        .hash(&mut hasher);

    hasher.finish()
}

pub fn calculate_visual_hash(path: &str, adjustments: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);

    if let Some(obj) = adjustments.as_object() {
        for (key, value) in obj {
            if GEOMETRY_KEYS.contains(&key.as_str()) {
                continue;
            }

            match key.as_str() {
                "crop" | "rotation" | "orientationSteps" | "flipHorizontal" | "flipVertical" => (),
                _ => {
                    key.hash(&mut hasher);
                    value.to_string().hash(&mut hasher);
                }
            }
        }
    }

    hasher.finish()
}

pub fn calculate_image_cache_hash(image_path: &str, adjustment_hash: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    image_path.hash(&mut hasher);
    adjustment_hash.hash(&mut hasher);
    hasher.finish()
}

pub fn calculate_transform_hash(adjustments: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();

    let orientation_steps = adjustments["orientationSteps"].as_u64().unwrap_or(0);
    orientation_steps.hash(&mut hasher);

    let rotation = adjustments["rotation"].as_f64().unwrap_or(0.0);
    (rotation.to_bits()).hash(&mut hasher);

    let flip_h = adjustments["flipHorizontal"].as_bool().unwrap_or(false);
    flip_h.hash(&mut hasher);

    let flip_v = adjustments["flipVertical"].as_bool().unwrap_or(false);
    flip_v.hash(&mut hasher);

    calculate_effects_hash(adjustments).hash(&mut hasher);

    if let Some(crop_val) = adjustments.get("crop")
        && !crop_val.is_null()
    {
        crop_val.to_string().hash(&mut hasher);
    }

    for key in GEOMETRY_KEYS {
        if let Some(val) = adjustments.get(key) {
            key.hash(&mut hasher);
            val.to_string().hash(&mut hasher);
        }
    }

    hash_ai_patches(adjustments, &mut hasher);

    hasher.finish()
}

pub fn calculate_full_job_hash(path: &str, adjustments: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    adjustments.to_string().hash(&mut hasher);
    hasher.finish()
}

pub struct DecodedImageCache {
    capacity: usize,
    items: Vec<(String, Arc<DynamicImage>, HashMap<String, String>)>,
}

impl DecodedImageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            items: Vec::with_capacity(capacity),
        }
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity;
        while self.items.len() > self.capacity {
            self.items.remove(0);
        }
    }

    pub fn get(&mut self, path: &str) -> Option<(Arc<DynamicImage>, HashMap<String, String>)> {
        if let Some(pos) = self.items.iter().position(|(p, _, _)| p == path) {
            let item = self.items.remove(pos);
            let result = (item.1.clone(), item.2.clone());
            self.items.push(item);
            Some(result)
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn insert(
        &mut self,
        path: String,
        image: Arc<DynamicImage>,
        exif: HashMap<String, String>,
    ) {
        if let Some(pos) = self.items.iter().position(|(p, _, _)| *p == path) {
            self.items.remove(pos);
        } else if self.items.len() >= self.capacity {
            self.items.remove(0);
        }
        self.items.push((path, image, exif));
    }
}

pub fn clear_preview_stage_caches(state: &AppState) {
    *state
        .patched_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    *state
        .patched_warped_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    *state
        .working_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    *state
        .effects_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    *state
        .full_transformed_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
}

#[tauri::command]
pub fn clear_image_caches(state: tauri::State<AppState>) {
    if let Ok(mut decoded_cache) = state.decoded_image_cache.lock() {
        decoded_cache.clear();
    }
    if let Ok(mut gpu_cache) = state.gpu_image_cache.lock() {
        *gpu_cache = None;
    }
    if let Ok(mut preview_cache) = state.cached_preview.lock() {
        *preview_cache = None;
    }
    if let Ok(mut warped_cache) = state.full_warped_cache.lock() {
        *warped_cache = None;
    }
    clear_preview_stage_caches(&state);
}

#[tauri::command]
pub fn clear_session_caches(state: tauri::State<AppState>) {
    if let Ok(mut patch_cache) = state.patch_cache.lock() {
        patch_cache.clear();
    }
    if let Ok(mut mask_cache) = state.mask_cache.lock() {
        mask_cache.clear();
    }
    if let Ok(mut geometry_cache) = state.geometry_cache.lock() {
        geometry_cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose};
    use serde_json::{Value, json};

    const RED_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAAD0lEQVR4AQEEAPv/AP8AAAMBAQCNHeWCAAAAAElFTkSuQmCC";
    const BLUE_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAAD0lEQVR4AQEEAPv/AAAA/wEDAQB8wnTbAAAAAElFTkSuQmCC";

    fn equal_length_pngs() -> (&'static str, &'static str) {
        assert_eq!(RED_PNG.len(), BLUE_PNG.len());
        for (encoded, expected) in [(RED_PNG, [255, 0, 0]), (BLUE_PNG, [0, 0, 255])] {
            let bytes = general_purpose::STANDARD.decode(encoded).unwrap();
            let image = image::load_from_memory(&bytes).unwrap().to_rgb8();
            assert_eq!(image.dimensions(), (1, 1));
            assert_eq!(image.get_pixel(0, 0).0, expected);
        }
        (RED_PNG, BLUE_PNG)
    }

    fn assert_patch_stages_invalidated(before: &Value, after: &Value) {
        for (stage, hash) in [
            ("patch", calculate_patch_hash as fn(&Value) -> u64),
            ("geometry", calculate_geometry_hash),
            ("patched/warped", calculate_patched_warped_hash),
            ("transform", calculate_transform_hash),
            ("thumbnail", calculate_thumbnail_base_hash),
        ] {
            assert_ne!(hash(before), hash(after), "stale {stage} cache");
        }
    }

    #[test]
    fn equal_length_effect_maps_invalidate_transformed_and_downstream_caches() {
        let (red, blue) = equal_length_pngs();
        for (enabled, map) in [
            ("lensBlurEnabled", "lensBlurDepthMap"),
            ("fogEnabled", "fogDepthMap"),
            ("relightEnabled", "relightNormalMap"),
        ] {
            let mut before = json!({});
            before[enabled] = json!(true);
            before[map] = json!(format!("data:image/png;base64,{red}"));
            let mut after = before.clone();
            after[map] = json!(format!("data:image/png;base64,{blue}"));
            assert_eq!(before.to_string().len(), after.to_string().len());

            for hash in [
                calculate_effects_hash as fn(&Value) -> u64,
                calculate_transform_hash,
                calculate_patched_warped_hash,
                calculate_thumbnail_base_hash,
            ] {
                assert_ne!(hash(&before), hash(&after), "stale {map} cache");
            }
            assert_eq!(calculate_patch_hash(&before), calculate_patch_hash(&after));
            assert_eq!(
                calculate_geometry_hash(&before),
                calculate_geometry_hash(&after)
            );
        }
    }

    #[test]
    fn regenerated_same_id_equal_length_patch_invalidates_every_dependent_stage() {
        let (red, blue) = equal_length_pngs();
        for field in ["color", "mask", "legacy"] {
            let mut before = json!({"aiPatches": [{
                "id": "same-patch", "visible": true,
                "patchData": {"color": red, "mask": red},
            }]});
            if field == "legacy" {
                before["aiPatches"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("patchData");
                before["aiPatches"][0]["patchDataBase64"] = json!(red);
            }
            let mut after = before.clone();
            if field == "legacy" {
                after["aiPatches"][0]["patchDataBase64"] = json!(blue);
            } else {
                after["aiPatches"][0]["patchData"][field] = json!(blue);
            }
            assert_eq!(before.to_string().len(), after.to_string().len());
            assert_patch_stages_invalidated(&before, &after);

            // Another spatial edit invalidates the outer cache, but must also
            // rebuild the patched and warped images from the replacement data.
            before["rotation"] = json!(15);
            after["rotation"] = json!(15);
            assert_patch_stages_invalidated(&before, &after);
        }
    }

    #[test]
    fn patch_placement_and_encoding_changes_invalidate_dependent_stages() {
        let before = json!({"aiPatches": [{
            "id": "same-patch", "patchData": {
                "color": RED_PNG, "mask": RED_PNG,
                "offsetX": 0, "offsetY": 0, "width": 1, "height": 1,
                "isSrgbEncoded": false,
            },
        }]});
        for field in ["offsetX", "offsetY", "width", "height", "isSrgbEncoded"] {
            let mut after = before.clone();
            after["aiPatches"][0]["patchData"][field] = if field == "isSrgbEncoded" {
                json!(true)
            } else {
                json!(2)
            };
            assert_patch_stages_invalidated(&before, &after);
        }
    }

    #[test]
    fn disabled_or_hidden_effect_maps_do_not_invalidate_spatial_caches() {
        for (enabled, map) in [
            ("lensBlurEnabled", "lensBlurDepthMap"),
            ("fogEnabled", "fogDepthMap"),
            ("relightEnabled", "relightNormalMap"),
        ] {
            for hidden in [false, true] {
                let mut before = json!({"sectionVisibility": {"effects": !hidden}});
                before[enabled] = json!(hidden);
                before[map] = json!(RED_PNG);
                let mut after = before.clone();
                after[map] = json!(BLUE_PNG);
                assert_eq!(
                    calculate_effects_hash(&before),
                    calculate_effects_hash(&after)
                );
                assert_eq!(
                    calculate_transform_hash(&before),
                    calculate_transform_hash(&after)
                );
                assert_eq!(
                    calculate_thumbnail_base_hash(&before),
                    calculate_thumbnail_base_hash(&after)
                );
            }
        }
    }

    #[test]
    fn virtual_copies_have_distinct_renderer_cache_identity() {
        let transform_hash = 42;
        let first_copy =
            calculate_image_cache_hash("/synthetic/image.jpg?vc=123abc", transform_hash);
        let second_copy =
            calculate_image_cache_hash("/synthetic/image.jpg?vc=abcdef", transform_hash);

        assert_ne!(first_copy, second_copy);
    }
}
