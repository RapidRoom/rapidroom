use crate::app_state::{AppState, LoadedImage};
use crate::file_management::parse_virtual_path;
use crate::relight::RelightTap;
use half::f16;
use half::slice::HalfFloatSliceExt;
use image::{DynamicImage, GenericImageView, Rgb32FImage, imageops};
use rayon::prelude::*;
use std::borrow::Cow;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};

const MODEL_TAG: &str = "nind-v3";
const FILE_MAGIC: &[u8; 8] = b"RRDN0002";
const DISK_CACHE_LIMIT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MEMORY_CACHE_CAPACITY: usize = 2;
const BLEND_CHUNK: usize = 3 * 16 * 1024;

struct DenoisedBase {
    key: String,
    source: String,
    fingerprint: u64,
    width: u32,
    height: u32,
    data: Vec<f16>,
}

#[derive(Default)]
struct DenoiseCache {
    dir: Option<PathBuf>,
    entries: Vec<Arc<DenoisedBase>>,
    pending: HashSet<String>,
    epoch: u64,
}

static CACHE: LazyLock<Mutex<DenoiseCache>> = LazyLock::new(Default::default);

fn cache() -> std::sync::MutexGuard<'static, DenoiseCache> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn initialize_cache_dir(cache_dir: PathBuf) {
    let dir = cache_dir.join("ai_denoise");
    let _ = fs::create_dir_all(&dir);
    cache().dir = Some(dir);
}

pub fn is_enabled(adjustments: &serde_json::Value) -> bool {
    crate::image_processing::is_effect_enabled(adjustments, "noiseReduction", "aiDenoiseEnabled")
}

/// Exports must not silently drop AI denoise. The result is generated while a photo is open in
/// the editor; a photo whose edit enables AI denoise without a cached result fails to export
/// with this message instead of exporting without denoise.
pub fn require_result(path: &str, adjustments: &serde_json::Value) -> Result<(), String> {
    if !is_enabled(adjustments) || adjustments["aiDenoiseAmount"].as_f64() == Some(0.0) {
        return Ok(());
    }
    let source = source_string(path);
    let key = entry_key(&source);
    let cached = cache()
        .get(&key)
        .is_some_and(|entry| entry.source == source)
        || read_disk_header(&key).is_some_and(|header| header.source == source);
    if cached {
        return Ok(());
    }
    let name = Path::new(&source)
        .file_name()
        .map_or_else(|| source.clone(), |n| n.to_string_lossy().into_owned());
    Err(format!(
        "AI denoise is on for {name} but has not been generated yet. Open the photo in the editor, \
         wait for AI denoise to finish, then export again."
    ))
}

pub fn cache_epoch() -> u64 {
    cache().epoch
}

impl DenoiseCache {
    fn get(&mut self, key: &str) -> Option<Arc<DenoisedBase>> {
        let pos = self.entries.iter().position(|e| e.key == key)?;
        let entry = self.entries.remove(pos);
        self.entries.push(Arc::clone(&entry));
        Some(entry)
    }

    fn insert(&mut self, entry: Arc<DenoisedBase>) {
        self.entries.retain(|e| e.key != entry.key);
        if self.entries.len() >= MEMORY_CACHE_CAPACITY {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }
}

fn source_string(path: &str) -> String {
    parse_virtual_path(path).0.to_string_lossy().to_string()
}

fn entry_key(source: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(MODEL_TAG.as_bytes());
    hasher.update(source.as_bytes());
    if let Ok(meta) = fs::metadata(source) {
        hasher.update(&meta.len().to_le_bytes());
        let modified = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .unwrap_or_default();
        hasher.update(&modified.as_nanos().to_le_bytes());
    }
    hasher.finalize().to_hex()[..32].to_string()
}

fn base_fingerprints(base: &DynamicImage) -> [u64; 2] {
    let mut hasher = blake3::Hasher::new();
    let mut legacy_hasher = blake3::Hasher::new();
    let (w, h) = base.dimensions();
    for hasher in [&mut hasher, &mut legacy_hasher] {
        hasher.update(&w.to_le_bytes());
        hasher.update(&h.to_le_bytes());
    }
    if let Some(buf) = base.as_rgb32f() {
        let raw = buf.as_raw();
        for value in raw.iter().step_by((raw.len() / 4096).max(1)) {
            hasher.update(&f16::from_f32(*value).to_bits().to_le_bytes());
            legacy_hasher.update(&value.to_bits().to_le_bytes());
        }
    }
    [hasher, legacy_hasher].map(|hasher| {
        let bytes = hasher.finalize();
        u64::from_le_bytes(bytes.as_bytes()[..8].try_into().unwrap_or_default())
    })
}

fn entry_path(key: &str) -> Option<PathBuf> {
    cache()
        .dir
        .as_ref()
        .map(|dir| dir.join(format!("{key}.rrdn")))
}

struct EntryHeader {
    source: String,
    fingerprint: u64,
    width: u32,
    height: u32,
}

fn read_header(reader: &mut impl Read) -> Option<EntryHeader> {
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic).ok()?;
    if &magic != FILE_MAGIC {
        return None;
    }
    let mut u32_buf = [0u8; 4];
    let mut u64_buf = [0u8; 8];
    reader.read_exact(&mut u32_buf).ok()?;
    let mut source = vec![0u8; u32::from_le_bytes(u32_buf) as usize];
    reader.read_exact(&mut source).ok()?;
    reader.read_exact(&mut u64_buf).ok()?;
    let fingerprint = u64::from_le_bytes(u64_buf);
    reader.read_exact(&mut u32_buf).ok()?;
    let width = u32::from_le_bytes(u32_buf);
    reader.read_exact(&mut u32_buf).ok()?;
    let height = u32::from_le_bytes(u32_buf);
    Some(EntryHeader {
        source: String::from_utf8(source).ok()?,
        fingerprint,
        width,
        height,
    })
}

fn read_disk_header(key: &str) -> Option<EntryHeader> {
    let file = File::open(entry_path(key)?).ok()?;
    read_header(&mut BufReader::new(file))
}

fn read_from_disk(key: &str, source: &str) -> Option<DenoisedBase> {
    let path = entry_path(key)?;
    let mut reader = BufReader::new(File::open(&path).ok()?);
    let header = read_header(&mut reader)?;
    if header.source != source {
        return None;
    }

    let mut data = vec![f16::ZERO; header.width as usize * header.height as usize * 3];
    reader
        .read_exact(bytemuck::cast_slice_mut(&mut data))
        .ok()?;

    if let Ok(file) = File::options().write(true).open(&path) {
        let _ = file.set_modified(SystemTime::now());
    }

    Some(DenoisedBase {
        key: key.to_string(),
        source: header.source,
        fingerprint: header.fingerprint,
        width: header.width,
        height: header.height,
        data,
    })
}

fn write_to_disk(entry: &DenoisedBase) -> Result<(), String> {
    let path = entry_path(&entry.key).ok_or("AI denoise cache directory is not initialized")?;
    let tmp_path = path.with_extension("tmp");

    let mut file = BufWriter::new(File::create(&tmp_path).map_err(|e| e.to_string())?);
    let source = entry.source.as_bytes();
    file.write_all(FILE_MAGIC)
        .and_then(|_| file.write_all(&(source.len() as u32).to_le_bytes()))
        .and_then(|_| file.write_all(source))
        .and_then(|_| file.write_all(&entry.fingerprint.to_le_bytes()))
        .and_then(|_| file.write_all(&entry.width.to_le_bytes()))
        .and_then(|_| file.write_all(&entry.height.to_le_bytes()))
        .and_then(|_| file.write_all(bytemuck::cast_slice(&entry.data)))
        .and_then(|_| file.flush())
        .map_err(|e| e.to_string())?;
    drop(file);

    fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
    evict_disk_cache(&path);
    Ok(())
}

fn evict_disk_cache(keep: &Path) {
    let Some(read_dir) = keep.parent().and_then(|dir| fs::read_dir(dir).ok()) else {
        return;
    };

    let mut entries: Vec<(PathBuf, u64, SystemTime)> = read_dir
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rrdn"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((e.path(), meta.len(), meta.modified().unwrap_or(UNIX_EPOCH)))
        })
        .collect();

    let mut total: u64 = entries.iter().map(|(_, len, _)| len).sum();
    entries.sort_by_key(|(_, _, modified)| *modified);

    for (path, len, _) in entries {
        if total <= DISK_CACHE_LIMIT_BYTES {
            break;
        }
        if path != keep && fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

fn load_entry(key: &str, source: &str) -> Option<Arc<DenoisedBase>> {
    if let Some(entry) = cache().get(key) {
        return (entry.source == source).then_some(entry);
    }

    let entry = Arc::new(read_from_disk(key, source)?);
    cache().insert(Arc::clone(&entry));
    Some(entry)
}

fn is_ready(key: &str, source: &str, fingerprints: [u64; 2]) -> bool {
    if let Some(entry) = cache().get(key) {
        return entry.source == source && fingerprints.contains(&entry.fingerprint);
    }
    read_disk_header(key)
        .is_some_and(|h| h.source == source && fingerprints.contains(&h.fingerprint))
}

pub fn apply_ai_denoise<'a>(
    base: &'a DynamicImage,
    path: &str,
    adjustments: &serde_json::Value,
) -> Cow<'a, DynamicImage> {
    if !is_enabled(adjustments) {
        return Cow::Borrowed(base);
    }

    let amount =
        (adjustments["aiDenoiseAmount"].as_f64().unwrap_or(100.0) as f32 / 100.0).clamp(0.0, 1.0);
    let detail =
        (adjustments["aiDenoiseDetail"].as_f64().unwrap_or(0.0) as f32 / 100.0).clamp(0.0, 1.0);
    if amount <= 0.0 {
        return Cow::Borrowed(base);
    }

    let source = source_string(path);
    let Some(entry) = load_entry(&entry_key(&source), &source) else {
        return Cow::Borrowed(base);
    };

    let (w, h) = base.dimensions();
    let resized: Option<Vec<f16>> = ((entry.width, entry.height) != (w, h)).then(|| {
        let mut expanded = vec![0.0f32; entry.data.len()];
        expanded
            .par_chunks_mut(BLEND_CHUNK)
            .zip(entry.data.par_chunks(BLEND_CHUNK))
            .for_each(|(dst, src)| src.convert_to_f32_slice(dst));
        let full = Rgb32FImage::from_raw(entry.width, entry.height, expanded)
            .expect("Denoised buffer matches its dimensions");
        let resized = imageops::resize(&full, w, h, imageops::FilterType::Triangle);
        resized
            .as_raw()
            .par_iter()
            .map(|v| f16::from_f32(*v))
            .collect()
    });
    let denoised: &[f16] = resized.as_deref().unwrap_or(&entry.data);

    let luma_keep = 1.0 - amount * (1.0 - detail);
    let chroma_keep = 1.0 - amount;

    let mut out = base.to_rgb32f();
    out.par_chunks_mut(BLEND_CHUNK)
        .zip(denoised.par_chunks(BLEND_CHUNK))
        .for_each(|(chunk, dn_chunk)| {
            let mut dn_values = vec![0.0f32; dn_chunk.len()];
            dn_chunk.convert_to_f32_slice(&mut dn_values);
            for (px, dn) in chunk
                .as_chunks_mut::<3>()
                .0
                .iter_mut()
                .zip(dn_values.as_chunks::<3>().0)
            {
                let r = [px[0] - dn[0], px[1] - dn[1], px[2] - dn[2]];
                let r_luma = 0.2126 * r[0] + 0.7152 * r[1] + 0.0722 * r[2];
                for c in 0..3 {
                    px[c] = dn[c] + chroma_keep * (r[c] - r_luma) + luma_keep * r_luma;
                }
            }
        });

    Cow::Owned(DynamicImage::ImageRgb32F(out))
}

fn restore_low_frequencies(denoised: &mut Rgb32FImage, source: &Rgb32FImage) {
    const FACTOR: usize = 64;
    let (w, h) = (source.width() as usize, source.height() as usize);
    let (sw, sh) = (w.div_ceil(FACTOR), h.div_ceil(FACTOR));
    if sw < 2 || sh < 2 {
        return;
    }

    let src = source.as_raw();
    let dn = denoised.as_raw();
    let planes: Vec<Vec<f32>> = (0..3)
        .map(|c| {
            let mut plane = vec![0.0f32; sw * sh];
            plane
                .par_chunks_exact_mut(sw)
                .enumerate()
                .for_each(|(sy, row)| {
                    let y_range = sy * FACTOR..((sy + 1) * FACTOR).min(h);
                    for (sx, out) in row.iter_mut().enumerate() {
                        let x_range = sx * FACTOR..((sx + 1) * FACTOR).min(w);
                        let mut acc = 0.0f32;
                        for y in y_range.clone() {
                            for x in x_range.clone() {
                                let i = (y * w + x) * 3 + c;
                                acc += src[i] - dn[i];
                            }
                        }
                        *out = acc / (y_range.len() * x_range.len()) as f32;
                    }
                });
            for _ in 0..3 {
                crate::lens_blur::dof_box_filter(&mut plane, sw, sh, 1, 3);
            }
            plane
        })
        .collect();

    denoised
        .par_chunks_exact_mut(w * 3)
        .enumerate()
        .for_each(|(y, row)| {
            let v = (y as f32 + 0.5) / h as f32;
            for x in 0..w {
                let tap = RelightTap::new((x as f32 + 0.5) / w as f32, v, sw, sh);
                for (c, plane) in planes.iter().enumerate() {
                    row[x * 3 + c] += tap.sample(plane);
                }
            }
        });
}

/// The NIND model expects [0, 1] RGB, as RapidRoom's denoise queue feeds it (nind_model_input).
fn model_input(source: &Rgb32FImage) -> Rgb32FImage {
    let mut input = source.clone();
    input.par_iter_mut().for_each(|sample| {
        *sample = if sample.is_finite() {
            sample.clamp(0.0, 1.0)
        } else {
            0.0
        };
    });
    input
}

/// Clamps the model output to its [0, 1] domain, then adds back, once, the part of each
/// source sample that lay outside that domain (highlights above 1.0, negative values).
fn restore_out_of_range(output: &mut Rgb32FImage, source: &Rgb32FImage) {
    output
        .par_iter_mut()
        .zip(source.as_raw().par_iter())
        .for_each(|(dn, &src)| {
            let excess = if src.is_finite() {
                src - src.clamp(0.0, 1.0)
            } else {
                0.0
            };
            *dn = dn.clamp(0.0, 1.0) + excess;
        });
}

fn run_model(
    base: &DynamicImage,
    session: &Mutex<ort::session::Session>,
    on_progress: &dyn Fn(f32),
) -> Result<Rgb32FImage, String> {
    let source = base.to_rgb32f();
    let input = model_input(&source);
    let mut output =
        crate::ai_processing::run_ai_denoise_with_progress(&input, 0.5, session, on_progress)?
            .into_rgb32f();
    drop(input);

    restore_out_of_range(&mut output, &source);
    restore_low_frequencies(&mut output, &source);
    Ok(output)
}

async fn generate(
    app_handle: &tauri::AppHandle,
    loaded_image: LoadedImage,
    key: String,
    source: String,
    fingerprint: u64,
) -> Result<(), String> {
    let state = app_handle.state::<AppState>();
    let session = crate::ai_processing::get_or_init_denoise_model(
        app_handle,
        &state.ai_state,
        &state.ai_init_lock,
    )
    .await
    .map_err(|e| e.to_string())?;

    let progress_handle = app_handle.clone();
    let loaded_path = loaded_image.path.clone();
    let path = loaded_path.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let image = run_model(&loaded_image.image, &session, &|progress| {
            let _ = progress_handle.emit(
                "ai-denoise-progress",
                serde_json::json!({ "path": path, "progress": progress }),
            );
        })?;

        let (width, height) = image.dimensions();
        let mut data = vec![f16::ZERO; image.as_raw().len()];
        data.par_chunks_mut(BLEND_CHUNK)
            .zip(image.as_raw().par_chunks(BLEND_CHUNK))
            .for_each(|(dst, src)| dst.convert_from_f32_slice(src));
        drop(image);

        let entry = Arc::new(DenoisedBase {
            key,
            source,
            fingerprint,
            width,
            height,
            data,
        });
        if let Err(e) = write_to_disk(&entry) {
            log::warn!("Failed to persist AI denoise cache: {}", e);
        }

        let mut cache = cache();
        cache.insert(entry);
        cache.epoch += 1;
        Ok(())
    })
    .await
    .map_err(|e| format!("AI denoise task failed: {}", e))??;

    let is_open = state
        .original_image
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|loaded| loaded.path == loaded_path);
    if !is_open {
        let thumbnail_handle = app_handle.clone();
        crate::file_management::add_to_thumbnail_queue(&state, 1, app_handle);
        tauri::async_runtime::spawn_blocking(move || {
            let settings =
                crate::app_settings::load_settings(thumbnail_handle.clone()).unwrap_or_default();
            crate::file_management::regenerate_thumbnails(
                &[loaded_path],
                &thumbnail_handle,
                &settings,
            );
        });
    }

    Ok(())
}

pub fn ensure_generated(
    app_handle: &tauri::AppHandle,
    loaded_image: &LoadedImage,
    adjustments: &serde_json::Value,
) {
    if !is_enabled(adjustments) {
        return;
    }

    let source = source_string(&loaded_image.path);
    let key = entry_key(&source);
    if cache().pending.contains(&key)
        || is_ready(&key, &source, base_fingerprints(&loaded_image.image))
        || !cache().pending.insert(key.clone())
    {
        return;
    }

    let app_handle = app_handle.clone();
    let loaded_image = loaded_image.clone();
    let path = loaded_image.path.clone();
    let [fingerprint, _] = base_fingerprints(&loaded_image.image);
    let _ = app_handle.emit(
        "ai-denoise-progress",
        serde_json::json!({ "path": path, "progress": 0.0 }),
    );

    tauri::async_runtime::spawn(async move {
        let result = generate(&app_handle, loaded_image, key.clone(), source, fingerprint).await;
        cache().pending.remove(&key);
        match result {
            Ok(()) => {
                let _ = app_handle.emit("ai-denoise-ready", &path);
            }
            Err(error) => {
                log::error!("AI denoise failed for {}: {}", path, error);
                let _ = app_handle.emit(
                    "ai-denoise-error",
                    serde_json::json!({ "path": path, "error": error }),
                );
            }
        }
    });
}

fn cache_files() -> Vec<(PathBuf, u64)> {
    let Some(read_dir) = cache().dir.clone().and_then(|dir| fs::read_dir(dir).ok()) else {
        return Vec::new();
    };
    read_dir
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "rrdn" || ext == "tmp")
        })
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.len())))
        .collect()
}

#[tauri::command]
pub fn get_ai_denoise_cache_size() -> u64 {
    cache_files().iter().map(|(_, len)| len).sum()
}

#[tauri::command]
pub fn clear_ai_denoise_cache() -> Result<(), String> {
    for (path, _) in cache_files() {
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove {}: {}", path.display(), e))?;
    }
    let mut cache = cache();
    cache.entries.clear();
    cache.epoch += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_range_samples_round_trip_exactly_through_the_model_domain() {
        let samples = [3.5f32, -0.25, 0.5, 1.0, 0.0, 12.0];
        let source = Rgb32FImage::from_raw(2, 1, samples.to_vec()).unwrap();
        let input = model_input(&source);
        assert!(input.as_raw().iter().all(|v| (0.0..=1.0).contains(v)));

        // A model that returns its input unchanged must give back the source exactly.
        let mut identity = input.clone();
        restore_out_of_range(&mut identity, &source);
        assert_eq!(identity.as_raw(), source.as_raw());

        // Model overshoot is clamped before the excess is added back, never counted twice.
        let mut overshoot = Rgb32FImage::from_raw(2, 1, vec![1.2f32; 6]).unwrap();
        restore_out_of_range(&mut overshoot, &source);
        assert_eq!(overshoot.as_raw()[0], 3.5);
        assert_eq!(overshoot.as_raw()[2], 1.0);
        assert_eq!(overshoot.as_raw()[5], 12.0);
    }

    #[test]
    #[ignore = "Local evidence: set RAPIDROOM_NIND_MODEL, RAPIDROOM_AI_DENOISE_RAW (public corpus raw) and RAPIDROOM_AI_DENOISE_OUT; run under flock"]
    fn real_nind_highlight_excess_is_added_back_once() {
        use image::DynamicImage;
        let env = |k: &str| std::env::var(k).unwrap();
        let session = Mutex::new(
            ort::session::Session::builder()
                .unwrap()
                .with_intra_threads(4)
                .unwrap()
                .commit_from_file(env("RAPIDROOM_NIND_MODEL"))
                .unwrap(),
        );
        let raw = env("RAPIDROOM_AI_DENOISE_RAW");
        let bytes = std::fs::read(&raw).unwrap();
        let base = crate::image_loader::load_and_composite_read_only(
            &bytes,
            &raw,
            &serde_json::Value::Null,
            false,
            &crate::app_settings::AppSettings::default(),
            None,
        )
        .unwrap()
        .to_rgb32f();
        // The 512 px window with the most over-range (clipped highlight) samples.
        let (w, h) = base.dimensions();
        let size = 512.min(w).min(h);
        let mut best = (0usize, 0u32, 0u32);
        for y in (0..=h - size).step_by(128) {
            for x in (0..=w - size).step_by(128) {
                let mut n = 0;
                for yy in (y..y + size).step_by(4) {
                    for xx in (x..x + size).step_by(4) {
                        n += base
                            .get_pixel(xx, yy)
                            .0
                            .iter()
                            .filter(|v| **v > 1.0)
                            .count();
                    }
                }
                if n > best.0 {
                    best = (n, x, y);
                }
            }
        }
        let source = image::imageops::crop_imm(&base, best.1, best.2, size, size).to_image();

        // Before the fix: unclamped model input, excess added on top of an unclamped output.
        let mut before =
            crate::ai_processing::run_ai_denoise_with_progress(&source, 0.5, &session, &|_| {})
                .unwrap()
                .into_rgb32f();
        before
            .iter_mut()
            .zip(source.as_raw().iter())
            .for_each(|(dn, &src)| *dn += src - src.clamp(0.0, 1.0));
        restore_low_frequencies(&mut before, &source);
        let after = run_model(
            &DynamicImage::ImageRgb32F(source.clone()),
            &session,
            &|_| {},
        )
        .unwrap();

        let over: Vec<usize> = (0..source.as_raw().len())
            .filter(|&i| source.as_raw()[i] > 1.0)
            .collect();
        let mean = |img: &Rgb32FImage| {
            over.iter()
                .map(|&i| img.as_raw()[i] - source.as_raw()[i])
                .sum::<f32>()
                / over.len().max(1) as f32
        };
        let stats = serde_json::json!({
            "raw": raw, "window": [best.1, best.2, size],
            "over_range_samples": over.len(),
            "mean_output_minus_source_on_over_range_before": mean(&before),
            "mean_output_minus_source_on_over_range_after": mean(&after),
            "max_before": before.as_raw().iter().cloned().fold(0.0f32, f32::max),
            "max_after": after.as_raw().iter().cloned().fold(0.0f32, f32::max),
            "max_source": source.as_raw().iter().cloned().fold(0.0f32, f32::max),
        });
        let out = std::path::PathBuf::from(env("RAPIDROOM_AI_DENOISE_OUT"));
        std::fs::write(
            out.join("stats.json"),
            serde_json::to_string_pretty(&stats).unwrap(),
        )
        .unwrap();
        // Display view: exposure -1.5 EV so highlights above 1.0 stay visible, then sRGB.
        let view = |img: &Rgb32FImage, name: &str| {
            let mut v = img.clone();
            v.iter_mut()
                .for_each(|s| *s = (*s * 0.35).clamp(0.0, 1.0).powf(1.0 / 2.2));
            DynamicImage::ImageRgb32F(v)
                .to_rgb8()
                .save(out.join(name))
                .unwrap();
        };
        view(&source, "source.png");
        view(&before, "before-fix.png");
        view(&after, "after-fix.png");
        assert!(
            stats["mean_output_minus_source_on_over_range_after"]
                .as_f64()
                .unwrap()
                .abs()
                < stats["mean_output_minus_source_on_over_range_before"]
                    .as_f64()
                    .unwrap()
                    .abs()
        );
    }

    #[test]
    fn non_finite_samples_never_reach_the_model() {
        let source = Rgb32FImage::from_raw(1, 1, vec![f32::NAN, f32::INFINITY, 0.25]).unwrap();
        let input = model_input(&source);
        assert_eq!(input.as_raw(), &[0.0, 0.0, 0.25]);
    }
}
