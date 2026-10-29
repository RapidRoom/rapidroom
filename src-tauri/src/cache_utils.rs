use crate::app_settings::AppSettings;
use crate::{AppState, PreviewTarget};
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

pub fn calculate_image_cache_hash(
    image_path: &str,
    adjustment_hash: u64,
    source_epoch: u64,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    image_path.hash(&mut hasher);
    adjustment_hash.hash(&mut hasher);
    // A half-float hit is replaced by a full decode without changing the path.
    // Preview stages key only on this hash, so the epoch has to move with the buffer.
    source_epoch.hash(&mut hasher);
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

/// Image, exif, embedded-preview marker, and whether the buffer is the original
/// f32 decode. The fourth flag is false when the image was expanded from f16.
/// Callers that only read `.0` still compile; a destructure must take the flag
/// so a lossy hit is not treated as the first-visit pixels.
pub type DecodedImageCacheHit = (Arc<DynamicImage>, HashMap<String, String>, bool, bool);

/// How many cached photos stay at full precision, including the one about to
/// be inserted. A three-photo cull then needs no extra decode. Older entries
/// are f16. `compact_except` reserves one of these slots for a path that is
/// not cached yet.
pub(crate) const FULL_PRECISION_WINDOW: usize = 3;

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
    /// Source path of the photo in the editor. Eviction may halve other
    /// entries, and it must not drop this one while it is open.
    pinned_path: Option<String>,
    items: Vec<DecodedEntry>,
}

impl DecodedImageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            pinned_path: None,
            items: Vec::with_capacity(capacity),
        }
    }

    pub fn set_pinned_path(&mut self, path: Option<String>) {
        self.pinned_path = path;
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
        let exact = matches!(entry.image, CachedImage::Full(_));
        let image = entry.image.expand();
        let exif = entry.exif.clone();
        let used_embedded_preview = entry.used_embedded_preview;
        // Put the same entry back. An expanded half buffer is not the first-visit
        // decode; storing it as Full made every later preview repeat the loss.
        self.items.push(entry);
        Some((image, exif, used_embedded_preview, exact))
    }

    /// Like `get`, but a half-float entry is left in place and reported as a miss.
    /// Callers that will decode again must not pay for an expand they discard.
    pub fn get_exact(
        &mut self,
        path: &str,
        fingerprint: &DecodeFingerprint,
    ) -> Option<DecodedImageCacheHit> {
        let pos = self.items.iter().position(|e| e.path == path)?;
        if self.items[pos].fingerprint != *fingerprint {
            self.items.remove(pos);
            return None;
        }
        if !matches!(self.items[pos].image, CachedImage::Full(_)) {
            return None;
        }
        self.get(path, fingerprint)
    }

    /// A full-precision hit that does not become the newest entry.
    /// Background work uses this so a lookup cannot push viewed photos out of
    /// the window. A half entry and a fingerprint miss stay where they are.
    pub fn peek_exact(
        &self,
        path: &str,
        fingerprint: &DecodeFingerprint,
    ) -> Option<DecodedImageCacheHit> {
        let entry = self.items.iter().find(|entry| entry.path == path)?;
        if entry.fingerprint != *fingerprint {
            return None;
        }
        let CachedImage::Full(image) = &entry.image else {
            return None;
        };
        Some((
            Arc::clone(image),
            entry.exif.clone(),
            entry.used_embedded_preview,
            true,
        ))
    }

    pub fn compact_except(&mut self, keep_path: &str) {
        let already_cached = self.items.iter().any(|e| e.path == keep_path);
        if let Some(pos) = self.items.iter().position(|e| e.path == keep_path) {
            let item = self.items.remove(pos);
            self.items.push(item);
        }
        // The caller inserts a missing photo after this returns, and that
        // insert is one of the full-precision slots.
        let keep_full = if already_cached {
            FULL_PRECISION_WINDOW
        } else {
            FULL_PRECISION_WINDOW.saturating_sub(1)
        };
        let compact_until = self.items.len().saturating_sub(keep_full);
        for item in self.items.iter_mut().take(compact_until) {
            Self::compact_item(item);
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

    fn compact_item(item: &mut DecodedEntry) {
        // Half is only a placeholder so the current image can be taken by value.
        let cached = std::mem::replace(
            &mut item.image,
            CachedImage::Half {
                width: 0,
                height: 0,
                channels: 0,
                data: Vec::new(),
            },
        );
        item.image = cached.compact();
    }

    fn total_bytes(&self) -> usize {
        self.items.iter().map(|e| e.image.byte_size()).sum()
    }

    fn evict(&mut self) {
        self.evict_with_budget(*DECODED_CACHE_BUDGET_BYTES);
    }

    /// Count comes off the oldest entry that is not the open photo. While over
    /// the byte budget, halve the oldest full image that is neither newest nor
    /// open, so a return still has an f16 frame. A shared buffer cannot be
    /// halved; drop that entry only when it is not the open photo. The newest
    /// image and the open photo stay.
    fn evict_with_budget(&mut self, budget: usize) {
        let pinned = self.pinned_path.clone();
        let is_open = |path: &str| pinned.as_deref() == Some(path);
        while self.items.len() > self.capacity.max(1) {
            let Some(pos) = self.items.iter().position(|entry| !is_open(&entry.path)) else {
                break;
            };
            self.items.remove(pos);
        }
        while self.items.len() > 1 && self.total_bytes() > budget {
            let last = self.items.len() - 1;
            let oldest_full = self.items[..last].iter().position(|entry| {
                !is_open(&entry.path) && matches!(entry.image, CachedImage::Full(_))
            });
            if let Some(pos) = oldest_full {
                let before = self.items[pos].image.byte_size();
                Self::compact_item(&mut self.items[pos]);
                if self.items[pos].image.byte_size() >= before {
                    self.items.remove(pos);
                }
            } else if let Some(pos) = self.items[..last]
                .iter()
                .position(|entry| !is_open(&entry.path))
            {
                self.items.remove(pos);
            } else {
                break;
            }
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
    /// Set by the leader before waiters are woken. A background decode that
    /// does not enter the cache still has to hand its buffer to a waiter.
    outcome: Mutex<Option<Result<Arc<SharedDecode>, String>>>,
}

/// One pristine decode shared by every caller waiting on the same path.
pub struct SharedDecode {
    pub image: Arc<DynamicImage>,
    pub exif: HashMap<String, String>,
    pub used_embedded_preview: bool,
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

impl FlightGuard<'_> {
    pub fn publish(&self, outcome: Result<Arc<SharedDecode>, String>) {
        *self
            .flight
            .outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(outcome);
    }
}

impl Flight {
    pub fn outcome(&self) -> Option<Result<Arc<SharedDecode>, String>> {
        self.outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
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

pub fn clear_preview_slot(state: &AppState, target: PreviewTarget) {
    *state
        .gpu_image_cache_for(target)
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    *state
        .cached_preview_for(target)
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
}

#[tauri::command]
pub fn clear_image_caches(state: tauri::State<AppState>) {
    if let Ok(mut decoded_cache) = state.decoded_image_cache.lock() {
        decoded_cache.clear();
    }
    for target in PreviewTarget::ALL {
        clear_preview_slot(&state, target);
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
            calculate_image_cache_hash("/synthetic/image.jpg?vc=123abc", transform_hash, 0);
        let second_copy =
            calculate_image_cache_hash("/synthetic/image.jpg?vc=abcdef", transform_hash, 0);

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

        let (_, _, used_embedded_preview, exact) = cache.get("fallback.NEF", &fingerprint).unwrap();
        assert!(used_embedded_preview);
        assert!(exact);
    }

    fn solid(value: f32) -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgb32F(image::Rgb32FImage::from_pixel(
            4,
            4,
            image::Rgb([value, value, value]),
        )))
    }

    fn fingerprint() -> DecodeFingerprint {
        DecodeFingerprint {
            len: 1,
            modified: None,
            settings_hash: 0,
        }
    }

    /// Fit-preview bytes from the CPU steps that sit on the editor path and
    /// turn an f16 rounding error into a different 8-bit value.
    fn editor_preview_bytes(image: &DynamicImage) -> Vec<u8> {
        let mut scaled = crate::image_processing::downscale_f32_image(image, 2, 2);
        crate::image_processing::apply_cpu_default_raw_processing(&mut scaled);
        let srgb = crate::image_processing::apply_linear_to_srgb(scaled);
        srgb.into_rgb8().into_raw()
    }

    #[test]
    fn return_preview_matches_the_first_visit_and_a_half_hit_does_not() {
        let value = f32::from_bits(0x3ba9_9000);
        let original = solid(value);
        let first_visit = editor_preview_bytes(&original);
        let fp = fingerprint();
        let mut cache = DecodedImageCache::new(5);

        let open = |cache: &mut DecodedImageCache, path: &str, image: Arc<DynamicImage>| {
            cache.compact_except(path);
            cache.insert(path.to_string(), fp.clone(), image, HashMap::new(), false);
        };

        open(&mut cache, "A", Arc::clone(&original));
        open(&mut cache, "B", solid(0.2));
        open(&mut cache, "C", solid(0.4));
        cache.compact_except("A");
        let (returned, _, _, exact) = cache.get("A", &fp).unwrap();
        assert!(exact);
        assert!(Arc::ptr_eq(&returned, &original));
        assert_eq!(editor_preview_bytes(&returned), first_visit);
        drop(returned);

        // Opening D keeps three full images (C, A, and D), not four.
        // The kept Arc would block compaction. A real visit drops it on the way out.
        drop(original);
        open(&mut cache, "D", solid(0.6));
        let (still_open, _, _, exact) = cache.get("A", &fp).unwrap();
        assert!(exact);
        assert_eq!(editor_preview_bytes(&still_open), first_visit);
        drop(still_open);
        let (oldest, _, _, exact) = cache.get("B", &fp).unwrap();
        assert!(!exact, "the photo before the window of three is f16");
        drop(oldest);

        // E then F push A out of the window. G is not required.
        open(&mut cache, "E", solid(0.8));
        open(&mut cache, "F", solid(0.15));
        cache.compact_except("A");
        assert!(cache.get_exact("A", &fp).is_none());
        let (provisional, _, _, exact) = cache.get("A", &fp).unwrap();
        assert!(!exact);
        assert_ne!(
            editor_preview_bytes(&provisional),
            first_visit,
            "f16 expansion must not be treated as the first-visit preview"
        );
        drop(provisional);

        // Reading the half entry must not launder the lossy buffer into a full one.
        assert!(cache.get_exact("A", &fp).is_none());
        let (still_half, _, _, exact) = cache.get("A", &fp).unwrap();
        assert!(!exact);
        drop(still_half);

        // A finished exact decode is stored by the caller. This insert stands in
        // for that store. It does not run load_image or the background re-decode.
        cache.insert(
            "A".to_string(),
            fp.clone(),
            solid(value),
            HashMap::new(),
            false,
        );
        let (restored, _, _, exact) = cache.get("A", &fp).unwrap();
        assert!(exact);
        assert_eq!(editor_preview_bytes(&restored), first_visit);
        assert_ne!(
            calculate_image_cache_hash("A", 1, 0),
            calculate_image_cache_hash("A", 1, 1)
        );
    }

    #[test]
    fn over_budget_halves_the_oldest_full_image_instead_of_dropping_it() {
        let mut cache = DecodedImageCache::new(5);
        let fp = fingerprint();
        // 4×4 RGB f32 is 192 bytes. 300 bytes holds one full image plus one half.
        cache.insert(
            "old".to_string(),
            fp.clone(),
            solid(0.2),
            HashMap::new(),
            false,
        );
        cache.insert(
            "new".to_string(),
            fp.clone(),
            solid(0.4),
            HashMap::new(),
            false,
        );
        cache.evict_with_budget(300);
        assert!(cache.get_exact("old", &fp).is_none());
        let (old, _, _, exact) = cache.get("old", &fp).unwrap();
        assert!(!exact);
        drop(old);
        let (_, _, _, exact) = cache.get("new", &fp).unwrap();
        assert!(exact);
    }

    #[test]
    fn peek_exact_does_not_make_a_background_hit_the_newest_photo() {
        let mut cache = DecodedImageCache::new(5);
        let fp = fingerprint();
        cache.insert(
            "A".to_string(),
            fp.clone(),
            solid(0.2),
            HashMap::new(),
            false,
        );
        cache.insert(
            "B".to_string(),
            fp.clone(),
            solid(0.4),
            HashMap::new(),
            false,
        );
        cache.insert(
            "C".to_string(),
            fp.clone(),
            solid(0.6),
            HashMap::new(),
            false,
        );
        let (hit, _, _, exact) = cache.peek_exact("A", &fp).unwrap();
        assert!(exact);
        drop(hit);
        // A is still the oldest. Reserving a slot for a photo that is not
        // cached halves A and leaves C full.
        cache.compact_except("incoming");
        assert!(cache.peek_exact("A", &fp).is_none());
        let (halved, _, _, exact) = cache.get("A", &fp).unwrap();
        assert!(!exact);
        drop(halved);
        let (_, _, _, exact) = cache.get("C", &fp).unwrap();
        assert!(exact);
    }

    #[test]
    fn eviction_keeps_a_pinned_open_photo_that_is_not_last() {
        let mut cache = DecodedImageCache::new(2);
        let fp = fingerprint();
        let open = solid(0.2);
        let middle = solid(0.4);
        cache.insert(
            "open".to_string(),
            fp.clone(),
            Arc::clone(&open),
            HashMap::new(),
            false,
        );
        cache.set_pinned_path(Some("open".to_string()));
        cache.insert(
            "middle".to_string(),
            fp.clone(),
            Arc::clone(&middle),
            HashMap::new(),
            false,
        );
        cache.insert(
            "newest".to_string(),
            fp.clone(),
            solid(0.6),
            HashMap::new(),
            false,
        );
        // Capacity 2 would drop the oldest unpinned entry. The open photo
        // stays even though it is first. middle is the one that goes.
        assert!(cache.peek_exact("middle", &fp).is_none());
        assert!(Arc::ptr_eq(
            &cache.peek_exact("open", &fp).unwrap().0,
            &open
        ));
        assert!(cache.peek_exact("newest", &fp).is_some());

        // A shared Arc cannot be halved. The open photo is not removed to
        // make room; a later shared entry that is not open is.
        cache.evict_with_budget(100);
        assert!(Arc::ptr_eq(
            &cache.peek_exact("open", &fp).unwrap().0,
            &open
        ));
        assert!(cache.peek_exact("middle", &fp).is_none());
        let (_, _, _, exact) = cache.peek_exact("newest", &fp).unwrap();
        assert!(exact);
        drop(open);
        drop(middle);
    }
}
