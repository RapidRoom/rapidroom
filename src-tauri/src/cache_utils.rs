use crate::AppState;
use crate::app_settings::AppSettings;
use half::f16;
use half::slice::HalfFloatSliceExt;
use image::{DynamicImage, GenericImageView, Rgb32FImage, Rgba32FImage};
use rayon::prelude::*;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::SystemTime;

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

fn hash_camera_policy(adjustments: &serde_json::Value, hasher: &mut DefaultHasher) {
    // Preserve all legacy cache identities. New camera recipes bind the actual
    // selected source/payload even if their numeric polynomial fields coincide.
    if adjustments["cameraLensCorrectionVersion"].as_u64() == Some(1) {
        "camera_lens_recipe_v1".hash(hasher);
        adjustments["lensCorrectionMode"]
            .as_str()
            .unwrap_or("manual")
            .hash(hasher);
        adjustments["lensDistortionParams"].to_string().hash(hasher);
    }
}

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

            let mask_from_sub_masks = if let Some(patch_data) = patch.get("patchData") {
                patch_data.to_string().hash(hasher);
                let has_color = patch_data
                    .get("color")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty());
                let has_mask = patch_data
                    .get("mask")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty());
                has_color && !has_mask
            } else {
                patch["patchDataBase64"].as_str().unwrap_or("").hash(hasher);
                true
            };

            if mask_from_sub_masks && let Some(sub_masks_val) = patch.get("subMasks") {
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

fn hash_ai_denoise(adjustments: &serde_json::Value, hasher: &mut DefaultHasher) {
    let enabled = crate::ai_denoise::is_enabled(adjustments);
    enabled.hash(hasher);
    if enabled {
        hash_effect_keys(
            adjustments,
            &["aiDenoiseMethod", "aiDenoiseAmount", "aiDenoiseDetail"],
            hasher,
        );
        crate::ai_denoise::cache_epoch().hash(hasher);
    }
}

fn hash_negative_conversion(adjustments: &serde_json::Value, hasher: &mut DefaultHasher) {
    let enabled = crate::negative_conversion::is_enabled(adjustments);
    enabled.hash(hasher);
    if enabled {
        hash_effect_keys(adjustments, crate::negative_conversion::PARAM_KEYS, hasher);
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
    hash_ai_denoise(adjustments, &mut hasher);
    hash_negative_conversion(adjustments, &mut hasher);

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
        hash_camera_policy(adjustments, &mut hasher);
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

    hash_camera_policy(adjustments, &mut hasher);

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

    let blur_enabled =
        crate::image_processing::is_effect_enabled(adjustments, "lensBlur", "lensBlurEnabled");
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
        crate::image_processing::is_effect_enabled(adjustments, "relight", "relightEnabled");
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

    let fog_enabled = crate::image_processing::is_effect_enabled(adjustments, "fog", "fogEnabled");
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

    hash_camera_policy(adjustments, &mut hasher);

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
    hash_ai_denoise(adjustments, &mut hasher);
    hash_negative_conversion(adjustments, &mut hasher);

    hash_camera_policy(adjustments, &mut hasher);

    hasher.finish()
}

pub fn calculate_full_job_hash(path: &str, adjustments: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    adjustments.to_string().hash(&mut hasher);
    // A freshly generated AI-denoise result changes the GPU input without changing the JSON.
    hash_ai_denoise(adjustments, &mut hasher);
    hasher.finish()
}

pub type DecodedImageCacheHit = (Arc<DynamicImage>, HashMap<String, String>, bool);

const HALF_CONVERSION_CHUNK: usize = 64 * 1024;

static DECODED_CACHE_BUDGET_BYTES: LazyLock<usize> = LazyLock::new(|| {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total = sys.total_memory() as usize;
    if total > 0 {
        total / 5
    } else {
        2 * 1024 * 1024 * 1024
    }
});

enum CachedImage {
    Full(Arc<DynamicImage>),
    Half {
        width: u32,
        height: u32,
        channels: usize,
        data: Vec<f16>,
    },
}

impl CachedImage {
    fn byte_size(&self) -> usize {
        match self {
            CachedImage::Full(image) => image.as_bytes().len(),
            CachedImage::Half { data, .. } => data.len() * std::mem::size_of::<f16>(),
        }
    }

    fn compact(self) -> Self {
        let CachedImage::Full(image) = self else {
            return self;
        };
        if Arc::strong_count(&image) > 1 {
            return CachedImage::Full(image);
        }

        let (width, height) = image.dimensions();
        let (channels, raw) = match image.as_ref() {
            DynamicImage::ImageRgb32F(buffer) => (3, buffer.as_raw()),
            DynamicImage::ImageRgba32F(buffer) => (4, buffer.as_raw()),
            _ => return CachedImage::Full(image),
        };

        let mut data = vec![f16::ZERO; raw.len()];
        data.par_chunks_mut(HALF_CONVERSION_CHUNK)
            .zip(raw.par_chunks(HALF_CONVERSION_CHUNK))
            .for_each(|(dst, src)| dst.convert_from_f32_slice(src));

        CachedImage::Half {
            width,
            height,
            channels,
            data,
        }
    }

    fn expand(&self) -> Arc<DynamicImage> {
        match self {
            CachedImage::Full(image) => Arc::clone(image),
            CachedImage::Half {
                width,
                height,
                channels,
                data,
            } => {
                let mut raw = vec![0.0f32; data.len()];
                raw.par_chunks_mut(HALF_CONVERSION_CHUNK)
                    .zip(data.par_chunks(HALF_CONVERSION_CHUNK))
                    .for_each(|(dst, src)| src.convert_to_f32_slice(dst));

                let image = if *channels == 4 {
                    DynamicImage::ImageRgba32F(
                        Rgba32FImage::from_raw(*width, *height, raw)
                            .expect("RGBA buffer matches image dimensions"),
                    )
                } else {
                    DynamicImage::ImageRgb32F(
                        Rgb32FImage::from_raw(*width, *height, raw)
                            .expect("RGB buffer matches image dimensions"),
                    )
                };
                Arc::new(image)
            }
        }
    }
}

/// A path alone goes stale: tethering reuses file names and raw settings change the pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeFingerprint {
    len: u64,
    modified: Option<SystemTime>,
    settings_hash: u64,
}

impl DecodeFingerprint {
    pub fn for_file(path: &Path, settings: &AppSettings) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        let mut hasher = DefaultHasher::new();
        settings
            .raw_highlight_compression
            .map(f32::to_bits)
            .hash(&mut hasher);
        settings.linear_raw_mode.hash(&mut hasher);
        settings
            .raw_preprocessing_color_nr
            .map(f32::to_bits)
            .hash(&mut hasher);
        settings
            .raw_preprocessing_sharpening
            .map(f32::to_bits)
            .hash(&mut hasher);
        settings.apply_preprocessing_to_non_raws.hash(&mut hasher);
        settings.use_apple_raw9.hash(&mut hasher);
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            settings_hash: hasher.finish(),
        })
    }
}

struct DecodedEntry {
    path: String,
    fingerprint: DecodeFingerprint,
    image: CachedImage,
    exif: HashMap<String, String>,
    used_embedded_preview: bool,
}

pub struct DecodedImageCache {
    capacity: usize,
    items: Vec<DecodedEntry>,
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
        self.evict();
    }

    pub fn contains(&self, path: &str, fingerprint: &DecodeFingerprint) -> bool {
        self.items
            .iter()
            .any(|e| e.path == path && e.fingerprint == *fingerprint)
    }

    pub fn get(
        &mut self,
        path: &str,
        fingerprint: &DecodeFingerprint,
    ) -> Option<DecodedImageCacheHit> {
        let pos = self.items.iter().position(|e| e.path == path)?;
        let entry = self.items.remove(pos);
        if entry.fingerprint != *fingerprint {
            return None;
        }
        let image = entry.image.expand();
        let exif = entry.exif.clone();
        let used_embedded_preview = entry.used_embedded_preview;
        self.items.push(DecodedEntry {
            image: CachedImage::Full(Arc::clone(&image)),
            ..entry
        });
        Some((image, exif, used_embedded_preview))
    }

    pub fn compact_except(&mut self, keep_path: &str) {
        if let Some(pos) = self.items.iter().position(|e| e.path == keep_path) {
            let item = self.items.remove(pos);
            self.items.push(item);
        }
        for item in self.items.iter_mut() {
            if item.path != keep_path {
                let cached = std::mem::replace(&mut item.image, CachedImage::Full(Arc::default()));
                item.image = cached.compact();
            }
        }
        self.evict();
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn insert(
        &mut self,
        path: String,
        fingerprint: DecodeFingerprint,
        image: Arc<DynamicImage>,
        exif: HashMap<String, String>,
        used_embedded_preview: bool,
    ) {
        if let Some(pos) = self.items.iter().position(|e| e.path == path) {
            self.items.remove(pos);
        }
        self.items.push(DecodedEntry {
            path,
            fingerprint,
            image: CachedImage::Full(image),
            exif,
            used_embedded_preview,
        });
        self.evict();
    }

    fn evict(&mut self) {
        while self.items.len() > self.capacity.max(1) {
            self.items.remove(0);
        }
        while self.items.len() > 1
            && self
                .items
                .iter()
                .map(|e| e.image.byte_size())
                .sum::<usize>()
                > *DECODED_CACHE_BUDGET_BYTES
        {
            self.items.remove(0);
        }
    }
}

/// Lets concurrent callers share one decode of a file instead of each decoding it.
#[derive(Default)]
pub struct DecodeFlights {
    inflight: Mutex<HashMap<String, Arc<Flight>>>,
}

#[derive(Default)]
pub struct Flight {
    done: Mutex<bool>,
    finished: Condvar,
}

impl Flight {
    pub fn wait(&self) {
        let mut done = self.done.lock().unwrap_or_else(|e| e.into_inner());
        while !*done {
            done = self.finished.wait(done).unwrap_or_else(|e| e.into_inner());
        }
    }
}

pub enum FlightRole<'a> {
    Leader(FlightGuard<'a>),
    Follower(Arc<Flight>),
}

/// Wakes waiters when dropped, including on error or panic.
pub struct FlightGuard<'a> {
    flights: &'a DecodeFlights,
    path: String,
    flight: Arc<Flight>,
}

impl Drop for FlightGuard<'_> {
    fn drop(&mut self) {
        self.flights
            .inflight
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.path);
        *self.flight.done.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.flight.finished.notify_all();
    }
}

impl DecodeFlights {
    pub fn begin(&self, path: &str) -> FlightRole<'_> {
        let mut inflight = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(flight) = inflight.get(path) {
            return FlightRole::Follower(Arc::clone(flight));
        }
        let flight = Arc::new(Flight::default());
        inflight.insert(path.to_string(), Arc::clone(&flight));
        FlightRole::Leader(FlightGuard {
            flights: self,
            path: path.to_string(),
            flight,
        })
    }
}

pub fn clear_preview_stage_caches(state: &AppState) {
    *state
        .patched_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    *state.warped_cache.lock().unwrap_or_else(|e| e.into_inner()) = None;
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
    use image::{DynamicImage, RgbImage};
    use std::{collections::HashMap, sync::Arc};

    #[test]
    fn virtual_copies_have_distinct_renderer_cache_identity() {
        let transform_hash = 42;
        let first_copy =
            calculate_image_cache_hash("/synthetic/image.jpg?vc=123abc", transform_hash);
        let second_copy =
            calculate_image_cache_hash("/synthetic/image.jpg?vc=abcdef", transform_hash);

        assert_ne!(first_copy, second_copy);
    }

    #[test]
    fn decoded_cache_preserves_embedded_preview_marker() {
        let mut cache = DecodedImageCache::new(2);
        let fingerprint = DecodeFingerprint {
            len: 1,
            modified: None,
            settings_hash: 0,
        };
        cache.insert(
            "fallback.NEF".to_string(),
            fingerprint.clone(),
            Arc::new(DynamicImage::ImageRgb8(RgbImage::new(1, 1))),
            HashMap::new(),
            true,
        );

        let (_, _, used_embedded_preview) = cache.get("fallback.NEF", &fingerprint).unwrap();
        assert!(used_embedded_preview);
    }
}
