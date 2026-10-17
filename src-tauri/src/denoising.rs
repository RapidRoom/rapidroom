#[path = "denoise_queue.rs"]
pub mod queue;
use crate::app_settings::load_settings;
use crate::app_state::AppState;
use crate::file_management::parse_virtual_path;
use crate::formats::is_raw_file;
use crate::image_loader::load_base_image_from_bytes;
use crate::image_processing::apply_cpu_default_raw_processing;
use base64::{Engine as _, engine::general_purpose};
use image::{DynamicImage, GenericImageView, ImageFormat, Rgb, Rgb32FImage};
use ort::session::{RunOptions, Session};
use rayon::prelude::*;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::Path;
use std::sync::atomic::{
    AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering as AtomicOrdering,
};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

pub const DENOISE_CANCELLED: &str = "Denoise cancelled";

type EventSink = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;
pub enum DenoisedOutput {
    Image(DynamicImage),
    Raw(Vec<u8>),
}

impl DenoisedOutput {
    fn extension(&self, is_raw: bool) -> &'static str {
        match self {
            Self::Raw(_) => "dng",
            Self::Image(_) if is_raw => "tiff",
            Self::Image(_) => "png",
        }
    }
}

pub type DenoiseResult = Arc<Mutex<Option<(u64, DenoisedOutput)>>>;

#[derive(Default)]
struct CancelToken {
    decode_cancel: Arc<AtomicUsize>,
    started: AtomicBool,
    cancelled: AtomicBool,
    run_options: Mutex<Option<Arc<RunOptions>>>,
}

impl CancelToken {
    fn cancel(&self) {
        self.cancelled.store(true, AtomicOrdering::SeqCst);
        self.decode_cancel.fetch_add(1, AtomicOrdering::SeqCst);
        if let Some(run_options) = self.run_options.lock().unwrap().as_ref() {
            let _ = run_options.terminate();
        }
    }
}

#[derive(Default)]
pub struct DenoiseJobs {
    next_id: AtomicU64,
    active: Mutex<HashMap<u64, Arc<CancelToken>>>,
}

impl DenoiseJobs {
    pub fn create(&self) -> u64 {
        let id = self.next_id.fetch_add(1, AtomicOrdering::SeqCst) + 1;
        self.active.lock().unwrap().insert(id, Arc::default());
        id
    }

    pub fn cancel(&self, id: u64) {
        if let Some(token) = self.active.lock().unwrap().remove(&id) {
            token.cancel();
        }
    }

    fn start(&self, id: u64, sink: Option<EventSink>) -> Option<DenoiseJob> {
        let token = self.active.lock().unwrap().get(&id).cloned()?;
        if token.started.swap(true, AtomicOrdering::SeqCst) {
            return None;
        }
        Some(DenoiseJob { id, token, sink })
    }

    fn finish(&self, id: u64) {
        self.active.lock().unwrap().remove(&id);
    }
}

pub struct DenoiseJob {
    pub id: u64,
    token: Arc<CancelToken>,
    sink: Option<EventSink>,
}

impl DenoiseJob {
    #[cfg(test)]
    pub(crate) fn testing() -> Self {
        Self {
            id: 1,
            token: Arc::default(),
            sink: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn testing_cancel_on_progress() -> Self {
        let token = Arc::new(CancelToken::default());
        let cancellation = token.clone();
        Self {
            id: 1,
            token,
            sink: Some(Arc::new(move |_, _| cancellation.cancel())),
        }
    }

    #[cfg(test)]
    pub(crate) fn cancel_for_test(&self) {
        self.token.cancel();
    }

    pub fn decode_cancel_token(&self) -> (Arc<AtomicUsize>, usize) {
        (self.token.decode_cancel.clone(), 0)
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.cancelled.load(AtomicOrdering::SeqCst)
    }

    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err(DENOISE_CANCELLED.to_string())
        } else {
            Ok(())
        }
    }

    fn emit(&self, event: &str, mut payload: serde_json::Value) {
        if let Some(sink) = &self.sink {
            payload["jobId"] = self.id.into();
            sink(event, payload);
        }
    }

    pub fn progress(&self, message: impl Into<String>) {
        self.emit(
            "denoise-progress",
            serde_json::json!({ "message": message.into() }),
        );
    }

    fn error(&self, message: String) {
        self.emit("denoise-error", serde_json::json!({ "message": message }));
    }

    /// Lets `cancel_denoise` terminate an ONNX run that is already in progress.
    pub fn attach_run_options(&self, run_options: Arc<RunOptions>) -> Result<(), String> {
        *self.token.run_options.lock().unwrap() = Some(run_options);
        self.check()
    }
}

struct ProgressReporter<'a> {
    counter: &'a Arc<AtomicUsize>,
    total_work: usize,
    job: Option<&'a DenoiseJob>,
}

impl ProgressReporter<'_> {
    fn is_cancelled(&self) -> bool {
        self.job.is_some_and(DenoiseJob::is_cancelled)
    }
}

const BLOCK_SIZE: usize = 8;
const BLOCK_AREA: usize = 64;
const MAX_GROUP_SIZE: usize = 16;
const STRIDE: usize = 6;
const SEARCH_WINDOW: usize = 19;
const FIXED_POINT_SCALE: f32 = 100_000.0;

#[derive(Clone, Copy)]
struct Bm3dParams {
    sigma: f32,
    hard_th_lambda: f32,
    max_dist_hard: f32,
    chroma_sigma_scale: f32,
}

impl Bm3dParams {
    fn from_intensity(i: f32) -> Self {
        let val = i.clamp(0.001, 1.0);
        Self {
            sigma: val * 80.0,
            hard_th_lambda: 2.0 + (val * 2.5),
            max_dist_hard: 3000.0 + (val * 20000.0),
            chroma_sigma_scale: 1.8,
        }
    }
}

fn event_sink(app_handle: &AppHandle) -> EventSink {
    let app_handle = app_handle.clone();
    Arc::new(move |event, payload| {
        let _ = app_handle.emit(event, payload);
    })
}

#[derive(Clone)]
enum DenoiseModels {
    Basic,
    LegacyAi(Arc<Mutex<Session>>),
    Tree {
        sessions: crate::tree_denoise::Models,
        fast: bool,
        sharpen: bool,
    },
}

struct DenoiseOptions {
    intensity: f32,
    method: String,
    sharpen: bool,
    exposure_policies: HashMap<String, serde_json::Value>,
}

fn selected_exposure_policy(
    path: &str,
    overrides: &HashMap<String, serde_json::Value>,
) -> serde_json::Value {
    overrides.get(path).cloned().unwrap_or_else(|| {
        let (_, sidecar) = parse_virtual_path(path);
        crate::exif_processing::load_sidecar(&sidecar).adjustments
    })
}

async fn denoise_session(
    method: &str,
    sharpen: bool,
    job: &DenoiseJob,
    app_handle: &AppHandle,
    state: &AppState,
) -> Result<DenoiseModels, String> {
    if matches!(method, "tree_best" | "tree_fast") {
        let fast = method == "tree_fast";
        let sessions = crate::tree_denoise::models(app_handle, state, fast, sharpen, job)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(DenoiseModels::Tree {
            sessions,
            fast,
            sharpen,
        });
    }
    if method != "ai" {
        return Ok(DenoiseModels::Basic);
    }
    let session = crate::ai_processing::get_or_init_denoise_model(
        app_handle,
        &state.ai_state,
        &state.ai_init_lock,
    )
    .await
    .map_err(|e| e.to_string())?;
    job.check()?;
    Ok(DenoiseModels::LegacyAi(session))
}

fn store_result(
    slot: &DenoiseResult,
    job: &DenoiseJob,
    image: DenoisedOutput,
) -> Result<(), String> {
    let mut slot = slot.lock().unwrap();
    job.check()?;
    if slot.as_ref().is_some_and(|(id, _)| *id > job.id) {
        return Err(DENOISE_CANCELLED.to_string());
    }
    *slot = Some((job.id, image));
    Ok(())
}

#[tauri::command]
pub fn create_denoise_job(state: tauri::State<'_, AppState>) -> u64 {
    state.denoise_jobs.create()
}

#[tauri::command]
pub fn cancel_denoise(job_id: u64, state: tauri::State<'_, AppState>) {
    state.denoise_jobs.cancel(job_id);
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn apply_denoising(
    job_id: u64,
    path: String,
    intensity: f32,
    method: String,
    sharpen: Option<bool>,
    exposure_policies: Option<HashMap<String, serde_json::Value>>,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let Some(job) = state
        .denoise_jobs
        .start(job_id, Some(event_sink(&app_handle)))
    else {
        return Ok(());
    };
    let result = run_single_denoise(
        job,
        path,
        DenoiseOptions {
            intensity,
            method,
            sharpen: sharpen.unwrap_or(true),
            exposure_policies: exposure_policies.unwrap_or_default(),
        },
        app_handle,
        &state,
    )
    .await;
    state.denoise_jobs.finish(job_id);
    match result {
        Err(e) if e == DENOISE_CANCELLED => Ok(()),
        other => other,
    }
}

async fn run_single_denoise(
    job: DenoiseJob,
    path: String,
    options: DenoiseOptions,
    app_handle: AppHandle,
    state: &AppState,
) -> Result<(), String> {
    let ai_session =
        denoise_session(&options.method, options.sharpen, &job, &app_handle, state).await?;
    let (source_path, _) = parse_virtual_path(&path);
    let path_str = source_path.to_string_lossy().to_string();
    let denoise_result_handle = state.denoise_result.clone();

    tokio::task::spawn_blocking(move || {
        let outcome = denoise_image(&path, &options, &app_handle, ai_session, &job).and_then(
            |(image, original, raw)| {
                let payload = build_previews(
                    &image,
                    original,
                    uses_default_raw_tone(is_raw_file(&path_str), &options.method),
                    &job,
                )?;
                store_result(
                    &denoise_result_handle,
                    &job,
                    raw.map_or_else(|| DenoisedOutput::Image(image), DenoisedOutput::Raw),
                )?;
                job.emit("denoise-complete", payload);
                Ok(())
            },
        );
        if let Err(e) = outcome
            && e != DENOISE_CANCELLED
        {
            job.error(e);
        }
    })
    .await
    .map_err(|e| format!("Denoising task failed: {}", e))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn batch_denoise_images(
    job_id: u64,
    paths: Vec<String>,
    intensity: f32,
    method: String,
    sharpen: Option<bool>,
    exposure_policies: Option<HashMap<String, serde_json::Value>>,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let Some(job) = state
        .denoise_jobs
        .start(job_id, Some(event_sink(&app_handle)))
    else {
        return Ok(Vec::new());
    };
    let result = run_batch_denoise(
        job,
        paths,
        DenoiseOptions {
            intensity,
            method,
            sharpen: sharpen.unwrap_or(true),
            exposure_policies: exposure_policies.unwrap_or_default(),
        },
        app_handle,
        &state,
    )
    .await;
    state.denoise_jobs.finish(job_id);
    match result {
        Err(e) if e == DENOISE_CANCELLED => Ok(Vec::new()),
        other => other,
    }
}

async fn run_batch_denoise(
    job: DenoiseJob,
    paths: Vec<String>,
    options: DenoiseOptions,
    app_handle: AppHandle,
    state: &AppState,
) -> Result<Vec<String>, String> {
    crate::file_management::ensure_card_writable_for_paths(&paths)?;
    let ai_session =
        denoise_session(&options.method, options.sharpen, &job, &app_handle, state).await?;

    tokio::task::spawn_blocking(move || {
        run_batch(&job, &paths, |path| {
            denoise_image(path, &options, &app_handle, ai_session.clone(), &job).map(
                |(image, _, raw)| {
                    raw.map_or_else(|| DenoisedOutput::Image(image), DenoisedOutput::Raw)
                },
            )
        })
    })
    .await
    .map_err(|e| format!("Batch denoising task failed: {}", e))
}

/// Stops at the first file after cancellation. Files already finished are kept; the file being
/// written goes to a hidden partial file that is removed, so no output is ever left half-written.
fn run_batch(
    job: &DenoiseJob,
    paths: &[String],
    mut denoise: impl FnMut(&str) -> Result<DenoisedOutput, String>,
) -> Vec<String> {
    let mut results = Vec::new();

    for (i, path_str) in paths.iter().enumerate() {
        if job.is_cancelled() {
            break;
        }
        job.emit(
            "denoise-batch-progress",
            serde_json::json!({
                "current": i + 1,
                "total": paths.len(),
                "path": path_str
            }),
        );

        let (source_path, source_sidecar_path) = parse_virtual_path(path_str);
        let real_path = source_path.to_string_lossy().to_string();

        let image = match denoise(path_str) {
            Ok(image) => image,
            Err(e) if e == DENOISE_CANCELLED => break,
            Err(e) => {
                job.error(format!("Failed to denoise {}: {}", real_path, e));
                continue;
            }
        };

        let parent_dir = source_path.parent().unwrap_or(Path::new(""));
        let stem = source_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();

        let is_raw = is_raw_file(&real_path);
        let output_path = parent_dir.join(format!("{}_Denoised.{}", stem, image.extension(is_raw)));
        match write_denoised_output(&image, &output_path, is_raw, job.id, Some(job)) {
            Ok(()) => {}
            Err(e) if e == DENOISE_CANCELLED => break,
            Err(e) => {
                job.error(format!("Failed to save {}: {}", real_path, e));
                continue;
            }
        }

        let _ = crate::exif_processing::write_rrexif_sidecar(&real_path, &output_path);

        if source_sidecar_path.exists()
            && let Some(output_path_str) = output_path.to_str()
        {
            let (_, dest_sidecar_path) = parse_virtual_path(output_path_str);
            if let Err(e) = copy_denoised_sidecar(&source_sidecar_path, &dest_sidecar_path) {
                log::warn!("Failed to copy sidecar file for denoised image: {}", e);
            }
        }

        results.push(output_path.to_string_lossy().to_string());
    }

    results
}

fn copy_denoised_sidecar(source: &Path, destination: &Path) -> std::io::Result<()> {
    let bytes = fs::read(source)?;
    let contents = match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(serde_json::Value::Object(mut metadata)) if metadata.contains_key("denoise") => {
            metadata.remove("denoise");
            serde_json::to_vec_pretty(&metadata).map_err(std::io::Error::other)?
        }
        _ => bytes,
    };
    // The regular atomic writer preserves any queue state already owned by the target.
    crate::file_management::write_file_atomically(destination, contents)?;
    fs::set_permissions(destination, fs::metadata(source)?.permissions())
}

fn write_denoised_output(
    output: &DenoisedOutput,
    output_path: &Path,
    is_raw: bool,
    job_id: u64,
    job: Option<&DenoiseJob>,
) -> Result<(), String> {
    let file_name = output_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let partial_path = output_path.with_file_name(format!(".{}.{}.partial", file_name, job_id));
    let result = (|| {
        match output {
            DenoisedOutput::Raw(bytes) => {
                fs::write(&partial_path, bytes).map_err(|e| e.to_string())?
            }
            DenoisedOutput::Image(image) => {
                let (image, format) = if is_raw {
                    (
                        DynamicImage::ImageRgb16(image.to_rgb16()),
                        ImageFormat::Tiff,
                    )
                } else {
                    (DynamicImage::ImageRgb8(image.to_rgb8()), ImageFormat::Png)
                };
                image
                    .save_with_format(&partial_path, format)
                    .map_err(|e| e.to_string())?;
            }
        }
        if let Some(job) = job {
            job.check()?;
        }
        fs::rename(&partial_path, output_path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial_path);
    }
    result
}

#[tauri::command]
pub async fn save_denoised_image(
    job_id: u64,
    original_path_str: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    crate::file_management::ensure_card_writable_for_paths(&[&original_path_str])?;
    let denoised_image = {
        let mut slot = state.denoise_result.lock().unwrap();
        match slot.take() {
            Some((id, image)) if id == job_id => image,
            other => {
                *slot = other;
                return Err(
                    "No denoised image found in memory. It might have already been saved or cleared."
                        .to_string(),
                );
            }
        }
    };

    let is_raw = crate::formats::is_raw_file(&original_path_str);

    let (first_path, source_sidecar_path) =
        crate::file_management::parse_virtual_path(&original_path_str);
    let parent_dir = first_path
        .parent()
        .ok_or_else(|| "Could not determine parent directory.".to_string())?;
    let stem = first_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("denoised");

    let output_path = parent_dir.join(format!(
        "{}_Denoised.{}",
        stem,
        denoised_image.extension(is_raw)
    ));
    write_denoised_output(&denoised_image, &output_path, is_raw, job_id, None)?;

    let (real_path, _) = crate::file_management::parse_virtual_path(&original_path_str);
    let _ =
        crate::exif_processing::write_rrexif_sidecar(&real_path.to_string_lossy(), &output_path);

    if source_sidecar_path.exists()
        && let Some(output_path_str) = output_path.to_str()
    {
        let (_, dest_sidecar_path) = crate::file_management::parse_virtual_path(output_path_str);
        if let Err(e) = copy_denoised_sidecar(&source_sidecar_path, &dest_sidecar_path) {
            log::warn!("Failed to copy sidecar file for denoised image: {}", e);
        }
    }

    Ok(output_path.to_string_lossy().to_string())
}

fn bm3d_denoise(
    rgb_img: &Rgb32FImage,
    intensity: f32,
    job: Option<&DenoiseJob>,
) -> Result<Rgb32FImage, String> {
    let (width, height) = rgb_img.dimensions();
    let params = Bm3dParams::from_intensity(intensity);
    let dct_tables = Arc::new(DctTables::new());
    let check = || job.map_or(Ok(()), DenoiseJob::check);

    let rgb_channels = split_channels(rgb_img);
    let (y, cb, cr) = rgb_to_ycbcr(&rgb_channels[0], &rgb_channels[1], &rgb_channels[2]);
    let original_y = y.clone();
    let channels = vec![y, cb, cr];

    let patches_x = (width as usize).saturating_sub(BLOCK_SIZE) / STRIDE + 1;
    let patches_y = (height as usize).saturating_sub(BLOCK_SIZE) / STRIDE + 1;
    let total_work_units = (patches_x * patches_y) * 2;
    let progress_counter = Arc::new(AtomicUsize::new(0));

    if let Some(job) = job {
        job.progress("Processing (Step 1/2)...");
    }

    let progress = ProgressReporter {
        counter: &progress_counter,
        total_work: total_work_units,
        job,
    };
    let mut denoised_channels =
        bm3d_process_joint(&channels, width, height, &params, &dct_tables, &progress)
            .ok_or_else(|| DENOISE_CANCELLED.to_string())?;
    check()?;

    {
        if let Some(job) = job {
            job.progress("Blending detail...");
        }
        let blurred_y = gaussian_blur_1ch(&original_y, width as usize, height as usize, 3.0);
        let detail_strength = (intensity * 0.5).min((1.0 - intensity) * 0.25).max(0.0);
        let y_ch = &mut denoised_channels[0];
        for i in 0..y_ch.len() {
            let hf = original_y[i] - blurred_y[i];
            y_ch[i] = (y_ch[i] + detail_strength * hf).clamp(0.0, 255.0);
        }
    }
    check()?;

    let (r, g, b) = ycbcr_to_rgb(
        &denoised_channels[0],
        &denoised_channels[1],
        &denoised_channels[2],
    );

    Ok(merge_channels(&[r, g, b], width, height))
}

// Both the default-developed RAW view and normal RGB files are encoded.
fn nind_model_input(image: DynamicImage) -> Result<Rgb32FImage, String> {
    let mut model_view = image.into_rgb32f();
    if model_view.as_raw().iter().any(|sample| !sample.is_finite()) {
        return Err("Legacy NIND requires finite RGB input samples".into());
    }
    // The original RGB loader normalizes uint8/uint16 files to [0, 1]. This
    // bounded model view does not change the physical source or other methods.
    for sample in model_view.as_mut() {
        *sample = sample.clamp(0.0, 1.0);
    }
    Ok(model_view)
}

fn uses_default_raw_tone(is_raw: bool, method: &str) -> bool {
    is_raw && method != "ai"
}

fn nind_raw_defaults(source_adjustments: &serde_json::Value) -> serde_json::Value {
    let mut defaults = crate::image_processing::default_adjustments();
    crate::image_processing::copy_exposure_policy(source_adjustments, &mut defaults);
    defaults
}

fn nind_raw_view(
    path: &str,
    source_adjustments: &serde_json::Value,
    image: &DynamicImage,
    settings: &crate::app_settings::AppSettings,
    app_handle: &AppHandle,
    job: &DenoiseJob,
) -> Result<DynamicImage, String> {
    use crate::gpu_processing::{RenderOutputPrecision, RenderRequest};

    job.check()?;
    job.progress("Developing legacy NIND RGB view...");
    let invalid = match image {
        DynamicImage::ImageRgb32F(rgb) => rgb.as_raw().iter().any(|v| !v.is_finite()),
        DynamicImage::ImageRgba32F(rgba) => rgba.as_raw().iter().any(|v| !v.is_finite()),
        _ => false,
    };
    if invalid {
        return Err("Legacy NIND requires finite RGB input samples".into());
    }
    let state = app_handle.state::<AppState>();
    let context = crate::gpu_processing::get_or_init_gpu_context(&state, app_handle)?;
    let (width, height) = image.dimensions();
    if width > context.limits.max_texture_dimension_2d
        || height > context.limits.max_texture_dimension_2d
    {
        return Err("Legacy NIND RAW development exceeds the GPU image limit".into());
    }
    // Reuse the RAW renderer rather than approximating its display curve in
    // the save path. NIND receives the same encoded, default-developed look
    // that the original photo displays; its TIFF needs no second tone curve.
    let default_adjustments = nind_raw_defaults(source_adjustments);
    let mut adjustments = crate::image_processing::get_all_adjustments_from_json(
        &default_adjustments,
        true,
        crate::as_shot::as_shot(path),
        crate::image_processing::resolve_render_options(settings, true),
    );
    adjustments.global.show_clipping = 0;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    "nind-default-raw-view".hash(&mut hash);
    default_adjustments.to_string().hash(&mut hash);
    job.id.hash(&mut hash);
    image.as_bytes().hash(&mut hash);
    let developed = crate::gpu_processing::process_and_get_dynamic_image_with_precision(
        &context,
        &state,
        image,
        hash.finish(),
        RenderRequest {
            adjustments,
            mask_bitmaps: &[],
            lut: None,
            roi: None,
        },
        "nind_default_raw_view",
        RenderOutputPrecision::SixteenBit,
    )?;
    job.check()?;
    Ok(developed)
}

fn denoise_image(
    image_path: &str,
    options: &DenoiseOptions,
    app_handle: &AppHandle,
    ai_session: DenoiseModels,
    job: &DenoiseJob,
) -> Result<(DynamicImage, Rgb32FImage, Option<Vec<u8>>), String> {
    denoise_image_with_settings(
        image_path,
        options,
        app_handle,
        ai_session,
        job,
        load_settings(app_handle.clone()).unwrap_or_default(),
        None,
    )
}

fn denoise_image_with_settings(
    image_path: &str,
    options: &DenoiseOptions,
    app_handle: &AppHandle,
    ai_session: DenoiseModels,
    job: &DenoiseJob,
    settings: crate::app_settings::AppSettings,
    nind_exposure_policy: Option<serde_json::Value>,
) -> Result<(DynamicImage, Rgb32FImage, Option<Vec<u8>>), String> {
    let method = options.method.as_str();
    let intensity = options.intensity;
    let (source_path, _) = parse_virtual_path(image_path);
    let path = source_path.as_path();
    let path_string = source_path.to_string_lossy();
    let path_str = path_string.as_ref();
    if !path.exists() {
        return Err("File not found".to_string());
    }

    let is_raw = is_raw_file(path_str);
    job.progress("Loading image...");

    let file_bytes = fs::read(path).map_err(|e| e.to_string())?;
    job.check()?;

    let mut original_settings = settings.clone();
    if method == "raw9" || matches!(ai_session, DenoiseModels::Tree { .. }) {
        original_settings.use_apple_raw9 = Some(false);
    }

    let dynamic_img = load_base_image_from_bytes(
        &file_bytes,
        path_str,
        false,
        &original_settings,
        Some(job.decode_cancel_token()),
    )
    .map_err(|e| e.to_string())?;
    job.check()?;

    let rgb_img_for_denoiser = if method == "ai" {
        let encoded = if is_raw {
            let source_adjustments = nind_exposure_policy.unwrap_or_else(|| {
                selected_exposure_policy(image_path, &options.exposure_policies)
            });
            nind_raw_view(
                path_str,
                &source_adjustments,
                &dynamic_img,
                &original_settings,
                app_handle,
                job,
            )?
        } else {
            dynamic_img
        };
        nind_model_input(encoded)?
    } else {
        dynamic_img.to_rgb32f()
    };

    let mut raw_output = None;
    let mut out_dynamic = if let DenoiseModels::Tree {
        sessions,
        fast,
        sharpen,
    } = &ai_session
    {
        let cached = crate::tree_denoise::cached_sensor(
            &file_bytes,
            app_handle,
            *fast,
            *sharpen,
            sessions,
            job,
        )
        .map_err(|e| {
            if job.is_cancelled() {
                DENOISE_CANCELLED.to_string()
            } else {
                e.to_string()
            }
        })?;
        let bytes = fs::read(&cached).map_err(|e| e.to_string())?;
        let developed = load_base_image_from_bytes(
            &bytes,
            &cached.to_string_lossy(),
            false,
            &original_settings,
            Some(job.decode_cancel_token()),
        )
        .map_err(|e| {
            if job.is_cancelled() {
                DENOISE_CANCELLED.to_string()
            } else {
                e.to_string()
            }
        })?;
        raw_output = Some(bytes);
        developed
    } else if method == "ai" {
        let DenoiseModels::LegacyAi(session_arc) = ai_session else {
            return Err("AI Session not provided".to_string());
        };
        crate::ai_processing::run_ai_denoise(&rgb_img_for_denoiser, intensity, &session_arc, job)?
    } else if method == "raw9" {
        if !is_raw {
            return Err("Apple RAW 9 denoising only works on RAW files.".to_string());
        }
        job.progress("Developing with Apple RAW 9...");
        crate::apple_raw::denoise_raw9(&file_bytes, path_str, intensity)
            .map_err(|e| e.to_string())?
    } else {
        DynamicImage::ImageRgb32F(bm3d_denoise(&rgb_img_for_denoiser, intensity, Some(job))?)
    };
    job.check()?;

    if uses_default_raw_tone(is_raw, method) {
        apply_cpu_default_raw_processing(&mut out_dynamic);
    }

    Ok((out_dynamic, rgb_img_for_denoiser, raw_output))
}

fn build_previews(
    out_dynamic: &DynamicImage,
    original: Rgb32FImage,
    is_raw: bool,
    job: &DenoiseJob,
) -> Result<serde_json::Value, String> {
    job.progress("Finalizing data...");
    job.progress("Generating previews...");

    let (width, height) = out_dynamic.dimensions();
    let (new_width, new_height) = if width > height {
        if width > 4000 {
            (4000, (4000.0 * height as f32 / width as f32).round() as u32)
        } else {
            (width, height)
        }
    } else {
        if height > 4000 {
            ((4000.0 * width as f32 / height as f32).round() as u32, 4000)
        } else {
            (width, height)
        }
    };

    let denoised_preview = if new_width != width {
        out_dynamic.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
    } else {
        out_dynamic.clone()
    };

    let mut buf_denoised = Cursor::new(Vec::new());
    denoised_preview
        .to_rgb8()
        .write_to(&mut buf_denoised, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode preview: {}", e))?;
    let base64_str_denoised = general_purpose::STANDARD.encode(buf_denoised.get_ref());
    let data_url_denoised = format!("data:image/png;base64,{}", base64_str_denoised);
    job.check()?;

    let mut original_dynamic = DynamicImage::ImageRgb32F(original);

    if is_raw {
        apply_cpu_default_raw_processing(&mut original_dynamic);
    }
    let original_preview = if new_width != width {
        original_dynamic.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
    } else {
        original_dynamic
    };

    let mut buf_orig = Cursor::new(Vec::new());
    original_preview
        .to_rgb8()
        .write_to(&mut buf_orig, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode original preview: {}", e))?;
    let base64_str_orig = general_purpose::STANDARD.encode(buf_orig.get_ref());
    let data_url_orig = format!("data:image/png;base64,{}", base64_str_orig);

    Ok(serde_json::json!({
        "denoised": data_url_denoised,
        "original": data_url_orig
    }))
}

fn rgb_to_ycbcr(r: &[f32], g: &[f32], b: &[f32]) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let n = r.len();
    let mut y = vec![0.0f32; n];
    let mut cb = vec![0.0f32; n];
    let mut cr = vec![0.0f32; n];
    for i in 0..n {
        let rv = r[i];
        let gv = g[i];
        let bv = b[i];
        y[i] = 0.299 * rv + 0.587 * gv + 0.114 * bv;
        cb[i] = -0.168736 * rv - 0.331264 * gv + 0.5 * bv + 128.0;
        cr[i] = 0.5 * rv - 0.418688 * gv - 0.081312 * bv + 128.0;
    }
    (y, cb, cr)
}

fn ycbcr_to_rgb(y: &[f32], cb: &[f32], cr: &[f32]) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let n = y.len();
    let mut r = vec![0.0f32; n];
    let mut g = vec![0.0f32; n];
    let mut b = vec![0.0f32; n];
    for i in 0..n {
        let yv = y[i];
        let cbv = cb[i] - 128.0;
        let crv = cr[i] - 128.0;
        r[i] = yv + 1.402 * crv;
        g[i] = yv - 0.344136 * cbv - 0.714136 * crv;
        b[i] = yv + 1.772 * cbv;
    }
    (r, g, b)
}

fn bm3d_process_joint(
    noisy_channels: &[Vec<f32>],
    width: u32,
    height: u32,
    params: &Bm3dParams,
    tables: &DctTables,
    progress: &ProgressReporter,
) -> Option<Vec<Vec<f32>>> {
    let basic_estimate = run_bm3d_step_joint(
        noisy_channels,
        noisy_channels,
        width,
        height,
        params,
        true,
        tables,
        progress,
    )?;

    run_bm3d_step_joint(
        noisy_channels,
        &basic_estimate,
        width,
        height,
        params,
        false,
        tables,
        progress,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_bm3d_step_joint(
    noisy: &[Vec<f32>],
    guide: &[Vec<f32>],
    width: u32,
    height: u32,
    params: &Bm3dParams,
    is_step_1: bool,
    tables: &DctTables,
    progress: &ProgressReporter,
) -> Option<Vec<Vec<f32>>> {
    let w = width as usize;
    let h = height as usize;
    let count = w * h;
    let num_channels = 3;

    let mut numerators = Vec::new();
    let mut denominators = Vec::new();
    for _ in 0..num_channels {
        numerators.push(Arc::new(AtomicAccumulator::new(count)));
        denominators.push(Arc::new(AtomicAccumulator::new(count)));
    }

    let mut ref_patches = Vec::with_capacity((w / STRIDE) * (h / STRIDE));
    for y in (0..h.saturating_sub(BLOCK_SIZE)).step_by(STRIDE) {
        for x in (0..w.saturating_sub(BLOCK_SIZE)).step_by(STRIDE) {
            ref_patches.push((x, y));
        }
    }

    ref_patches.par_iter().for_each(|&(rx, ry)| {
        if progress.is_cancelled() {
            return;
        }
        let c = progress.counter.fetch_add(1, AtomicOrdering::Relaxed);
        if let Some(job) = progress.job
            && c.is_multiple_of(200)
        {
            let pct = (c as f32 / progress.total_work as f32) * 100.0;
            let step_str = if is_step_1 { "Step 1/2" } else { "Step 2/2" };
            job.progress(format!("{} - {:.0}%", step_str, pct));
        }

        let mut group_locs_buf = [(0, 0); MAX_GROUP_SIZE];
        let group_size =
            block_matching_joint(guide, w, h, rx, ry, is_step_1, params, &mut group_locs_buf);
        let group_locs = &group_locs_buf[0..group_size];

        for ch in 0..num_channels {
            let guide_ch = &guide[ch];
            let noisy_ch = &noisy[ch];

            let ch_sigma = if ch == 0 {
                params.sigma
            } else {
                params.sigma * params.chroma_sigma_scale
            };

            let mut guide_stack = build_3d_group(guide_ch, w, group_locs);
            let mut noisy_stack = if is_step_1 {
                guide_stack.clone()
            } else {
                build_3d_group(noisy_ch, w, group_locs)
            };

            transform_3d(&mut guide_stack, group_size, tables);
            if !is_step_1 {
                transform_3d(&mut noisy_stack, group_size, tables);
            }

            let weight;
            if is_step_1 {
                let threshold = params.hard_th_lambda * ch_sigma;
                let nonzero = hard_threshold(&mut guide_stack, threshold);
                weight = if nonzero > 0 {
                    1.0 / (nonzero as f32)
                } else {
                    1.0
                };
                noisy_stack = guide_stack;
            } else {
                weight = wiener_filter(&mut noisy_stack, &guide_stack, ch_sigma);
            }

            inverse_transform_3d(&mut noisy_stack, group_size, tables);

            let num_acc = &numerators[ch];
            let den_acc = &denominators[ch];

            for (k, &(lx, ly)) in group_locs.iter().enumerate() {
                let patch_offset = k * BLOCK_AREA;
                for dy in 0..BLOCK_SIZE {
                    let row_global = (ly + dy) * w + lx;
                    let row_patch = dy * BLOCK_SIZE;
                    for dx in 0..BLOCK_SIZE {
                        let idx = row_global + dx;
                        let val = noisy_stack[patch_offset + row_patch + dx];
                        let w_val = tables.kaiser[row_patch + dx] * weight;
                        num_acc.add(idx, val * w_val);
                        den_acc.add(idx, w_val);
                    }
                }
            }
        }
    });

    if progress.is_cancelled() {
        return None;
    }

    let mut results = Vec::new();
    for ch in 0..num_channels {
        let num_vec = numerators[ch].to_vec();
        let den_vec = denominators[ch].to_vec();
        let final_ch = num_vec
            .iter()
            .zip(den_vec.iter())
            .zip(noisy[ch].iter())
            .map(|((&n, &d), &orig)| if d > 1e-6 { n / d } else { orig })
            .collect();
        results.push(final_ch);
    }
    Some(results)
}

fn hard_threshold(stack: &mut [f32], th: f32) -> usize {
    let mut c = 0;
    for (i, x) in stack.iter_mut().enumerate() {
        if i == 0 {
            c += 1;
            continue;
        }

        if x.abs() < th {
            *x = 0.0;
        } else {
            c += 1;
        }
    }
    c
}

fn wiener_filter(noisy: &mut [f32], guide: &[f32], sigma: f32) -> f32 {
    let mut sum = 0.0;
    let s2 = sigma * sigma;
    for (i, (n, g)) in noisy.iter_mut().zip(guide).enumerate() {
        if i == 0 {
            sum += 1.0;
            continue;
        }

        let energy = g * g;
        let coef = energy / (energy + s2 + 1e-5);
        *n *= coef;
        sum += coef * coef;
    }
    if sum > 0.0 { 1.0 / sum } else { 1.0 }
}

#[derive(Clone, Copy)]
struct Match {
    dist: f32,
    x: u16,
    y: u16,
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn block_matching_joint(
    channels: &[Vec<f32>],
    w: usize,
    h: usize,
    rx: usize,
    ry: usize,
    is_step_1: bool,
    params: &Bm3dParams,
    out_buf: &mut [(usize, usize)],
) -> usize {
    const MAX_CANDIDATES: usize = 1024;
    let mut candidates: [Match; MAX_CANDIDATES] = [Match {
        dist: f32::MAX,
        x: 0,
        y: 0,
    }; MAX_CANDIDATES];
    let mut cand_count = 0;

    let threshold = if is_step_1 {
        params.max_dist_hard
    } else {
        params.max_dist_hard * 0.5
    };

    let mut ref_r = [0.0; 64];
    let mut ref_g = [0.0; 64];
    let mut ref_b = [0.0; 64];
    extract_patch(&channels[0], w, rx, ry, &mut ref_r);
    extract_patch(&channels[1], w, rx, ry, &mut ref_g);
    extract_patch(&channels[2], w, rx, ry, &mut ref_b);

    let half_sw = SEARCH_WINDOW / 2;
    let sx_start = rx.saturating_sub(half_sw);
    let sx_end = (rx + half_sw).min(w.saturating_sub(BLOCK_SIZE));
    let sy_start = ry.saturating_sub(half_sw);
    let sy_end = (ry + half_sw).min(h.saturating_sub(BLOCK_SIZE));

    candidates[0] = Match {
        dist: 0.0,
        x: rx as u16,
        y: ry as u16,
    };
    cand_count += 1;

    for y in sy_start..=sy_end {
        for x in sx_start..=sx_end {
            if x == rx && y == ry {
                continue;
            }
            let d_r = compute_ssd_flat(&channels[0], w, x, y, &ref_r, threshold);
            if d_r > threshold {
                continue;
            }
            let d_g = compute_ssd_flat(&channels[1], w, x, y, &ref_g, threshold - d_r);
            if d_r + d_g > threshold {
                continue;
            }
            let d_b = compute_ssd_flat(&channels[2], w, x, y, &ref_b, threshold - (d_r + d_g));
            let total_dist = d_r + d_g + d_b;

            if total_dist < threshold && cand_count < MAX_CANDIDATES {
                candidates[cand_count] = Match {
                    dist: total_dist,
                    x: x as u16,
                    y: y as u16,
                };
                cand_count += 1;
            }
        }
    }

    let valid_slice = &mut candidates[0..cand_count];
    valid_slice.sort_unstable_by(|a, b| a.dist.partial_cmp(&b.dist).unwrap_or(Ordering::Equal));

    let limit = MAX_GROUP_SIZE.min(cand_count);
    let p2_limit = prev_power_of_two(limit);

    for i in 0..p2_limit {
        out_buf[i] = (valid_slice[i].x as usize, valid_slice[i].y as usize);
    }
    p2_limit
}

#[inline(always)]
fn compute_ssd_flat(
    img: &[f32],
    w: usize,
    x: usize,
    y: usize,
    ref_patch: &[f32],
    stop_thr: f32,
) -> f32 {
    let mut dist = 0.0;
    for dy in 0..8 {
        let img_base = (y + dy) * w + x;
        let ref_base = dy * 8;
        for dx in 0..8 {
            let diff = img[img_base + dx] - ref_patch[ref_base + dx];
            dist += diff * diff;
        }
        if dist > stop_thr {
            return dist;
        }
    }
    dist / BLOCK_AREA as f32
}

#[inline(always)]
fn extract_patch(img: &[f32], w: usize, x: usize, y: usize, out: &mut [f32]) {
    for dy in 0..8 {
        let src_idx = (y + dy) * w + x;
        let dst_idx = dy * 8;
        out[dst_idx..dst_idx + 8].copy_from_slice(&img[src_idx..src_idx + 8]);
    }
}

fn build_3d_group(img: &[f32], w: usize, locs: &[(usize, usize)]) -> Vec<f32> {
    let mut stack = vec![0.0; locs.len() * 64];
    for (i, &(lx, ly)) in locs.iter().enumerate() {
        let offset = i * 64;
        extract_patch(img, w, lx, ly, &mut stack[offset..offset + 64]);
    }
    stack
}

struct DctTables {
    dct_coeff: [f32; 64],
    idct_coeff: [f32; 64],
    kaiser: Vec<f32>,
}

impl DctTables {
    fn new() -> Self {
        let mut dct_coeff = [0.0; 64];
        let mut idct_coeff = [0.0; 64];
        for k in 0..8 {
            for n in 0..8 {
                let c = k as f32 * std::f32::consts::PI / 8.0;
                let val = ((n as f32 + 0.5) * c).cos();
                let scale = if k == 0 { 0.35355339 } else { 0.5 };
                dct_coeff[k * 8 + n] = val * scale;
            }
        }
        for n in 0..8 {
            for k in 0..8 {
                let theta = (std::f32::consts::PI / 8.0) * (n as f32 + 0.5) * (k as f32);
                let scale = if k == 0 { 0.35355339 } else { 0.5 };
                idct_coeff[n * 8 + k] = scale * theta.cos();
            }
        }
        let mut kaiser = vec![0.0; 64];
        for y in 0..8 {
            for x in 0..8 {
                let wx = (std::f32::consts::PI * x as f32 / 7.0).sin();
                let wy = (std::f32::consts::PI * y as f32 / 7.0).sin();
                kaiser[y * 8 + x] = wx * wy;
            }
        }
        Self {
            dct_coeff,
            idct_coeff,
            kaiser,
        }
    }
}

struct AtomicAccumulator {
    data: Vec<AtomicI64>,
}

impl AtomicAccumulator {
    fn new(size: usize) -> Self {
        let mut data = Vec::with_capacity(size);
        for _ in 0..size {
            data.push(AtomicI64::new(0));
        }
        Self { data }
    }
    #[inline(always)]
    fn add(&self, index: usize, value: f32) {
        if index < self.data.len() {
            let fixed = (value * FIXED_POINT_SCALE) as i64;
            self.data[index].fetch_add(fixed, AtomicOrdering::Relaxed);
        }
    }
    fn to_vec(&self) -> Vec<f32> {
        self.data
            .iter()
            .map(|a| a.load(AtomicOrdering::Relaxed) as f32 / FIXED_POINT_SCALE)
            .collect()
    }
}

#[inline(always)]
fn transform_3d(stack: &mut [f32], group_size: usize, tables: &DctTables) {
    for i in 0..group_size {
        let offset = i * 64;
        dct_2d_8x8(&mut stack[offset..offset + 64], &tables.dct_coeff);
    }
    for i in 0..64 {
        let mut col = [0.0; MAX_GROUP_SIZE];
        for k in 0..group_size {
            col[k] = stack[k * 64 + i];
        }
        walsh_hadamard_1d(&mut col[0..group_size]);
        for k in 0..group_size {
            stack[k * 64 + i] = col[k];
        }
    }
}

#[inline(always)]
fn inverse_transform_3d(stack: &mut [f32], group_size: usize, tables: &DctTables) {
    for i in 0..64 {
        let mut col = [0.0; MAX_GROUP_SIZE];
        for k in 0..group_size {
            col[k] = stack[k * 64 + i];
        }
        walsh_hadamard_1d(&mut col[0..group_size]);
        for k in 0..group_size {
            stack[k * 64 + i] = col[k];
        }
    }
    for i in 0..group_size {
        let offset = i * 64;
        idct_2d_8x8(&mut stack[offset..offset + 64], &tables.idct_coeff);
    }
}

#[inline]
fn dct_2d_8x8(block: &mut [f32], coeffs: &[f32; 64]) {
    for i in 0..8 {
        dct_1d_8(&mut block[i * 8..(i + 1) * 8], coeffs);
    }
    transpose_8x8(block);
    for i in 0..8 {
        dct_1d_8(&mut block[i * 8..(i + 1) * 8], coeffs);
    }
    transpose_8x8(block);
}

#[inline]
fn idct_2d_8x8(block: &mut [f32], coeffs: &[f32; 64]) {
    transpose_8x8(block);
    for i in 0..8 {
        idct_1d_8(&mut block[i * 8..(i + 1) * 8], coeffs);
    }
    transpose_8x8(block);
    for i in 0..8 {
        idct_1d_8(&mut block[i * 8..(i + 1) * 8], coeffs);
    }
}

#[inline]
fn dct_1d_8(x: &mut [f32], coeffs: &[f32; 64]) {
    let mut tmp = [0.0; 8];
    tmp.copy_from_slice(x);
    for (k, x_k) in x[..8].iter_mut().enumerate() {
        let mut s = 0.0;
        let row_start = k * 8;
        for (n, &tmp_n) in tmp.iter().enumerate() {
            s += tmp_n * coeffs[row_start + n];
        }
        *x_k = s;
    }
}

#[inline]
fn idct_1d_8(x: &mut [f32], coeffs: &[f32; 64]) {
    let mut tmp = [0.0; 8];
    tmp.copy_from_slice(x);
    for (n, x_n) in x[..8].iter_mut().enumerate() {
        let mut s = 0.0;
        let row_start = n * 8;
        for (k, &tmp_k) in tmp.iter().enumerate() {
            s += tmp_k * coeffs[row_start + k];
        }
        *x_n = s;
    }
}

#[inline]
fn transpose_8x8(b: &mut [f32]) {
    for y in 0..8 {
        for x in (y + 1)..8 {
            b.swap(y * 8 + x, x * 8 + y);
        }
    }
}

#[inline]
fn walsh_hadamard_1d(data: &mut [f32]) {
    let n = data.len();
    let mut h = 1;
    while h < n {
        for i in (0..n).step_by(h * 2) {
            for j in i..i + h {
                let x = data[j];
                let y = data[j + h];
                data[j] = x + y;
                data[j + h] = x - y;
            }
        }
        h *= 2;
    }
    let scale = 1.0 / (n as f32).sqrt();
    for x in data {
        *x *= scale;
    }
}

fn split_channels(img: &Rgb32FImage) -> Vec<Vec<f32>> {
    let (w, h) = img.dimensions();
    let size = (w * h) as usize;
    let mut r = vec![0.0; size];
    let mut g = vec![0.0; size];
    let mut b = vec![0.0; size];
    for (i, p) in img.pixels().enumerate() {
        r[i] = p[0] * 255.0;
        g[i] = p[1] * 255.0;
        b[i] = p[2] * 255.0;
    }
    vec![r, g, b]
}

fn merge_channels(channels: &[Vec<f32>], w: u32, h: u32) -> Rgb32FImage {
    let mut img = Rgb32FImage::new(w, h);
    for (i, p) in img.pixels_mut().enumerate() {
        let r = channels[0][i].clamp(0.0, 255.0) / 255.0;
        let g = channels[1][i].clamp(0.0, 255.0) / 255.0;
        let b = channels[2][i].clamp(0.0, 255.0) / 255.0;
        *p = Rgb([r, g, b]);
    }
    img
}

fn prev_power_of_two(x: usize) -> usize {
    if x == 0 {
        return 0;
    }
    let mut p = 1;
    while p * 2 <= x {
        p *= 2;
    }
    p
}

fn gaussian_blur_1ch(data: &[f32], width: usize, height: usize, sigma: f32) -> Vec<f32> {
    let radius = (3.0 * sigma).ceil() as usize;
    let klen = 2 * radius + 1;
    let mut kernel = vec![0.0f32; klen];
    let two_s2 = 2.0 * sigma * sigma;
    for (i, kernel_val) in kernel.iter_mut().enumerate() {
        let k = i as f32 - radius as f32;
        *kernel_val = (-k * k / two_s2).exp();
    }
    let ksum: f32 = kernel.iter().sum();
    for k in &mut kernel {
        *k /= ksum;
    }

    let mut tmp = vec![0.0f32; width * height];
    for y in 0..height {
        let row_in = &data[y * width..(y + 1) * width];
        let row_out = &mut tmp[y * width..(y + 1) * width];
        for (x, out_val) in row_out.iter_mut().enumerate() {
            let mut val = 0.0f32;
            let mut wsum = 0.0f32;
            let x0 = x as isize - radius as isize;
            for (ki, &kernel_val) in kernel.iter().enumerate() {
                let kx = x0 + ki as isize;
                if kx >= 0 && kx < width as isize {
                    val += row_in[kx as usize] * kernel_val;
                    wsum += kernel_val;
                }
            }
            *out_val = val / wsum;
        }
    }

    let mut out = vec![0.0f32; width * height];
    for y in 0..height {
        let y0 = y as isize - radius as isize;
        for x in 0..width {
            let mut val = 0.0f32;
            let mut wsum = 0.0f32;
            for (ki, &kernel_val) in kernel.iter().enumerate() {
                let ky = y0 + ki as isize;
                if ky >= 0 && ky < height as isize {
                    val += tmp[ky as usize * width + x] * kernel_val;
                    wsum += kernel_val;
                }
            }
            out[y * width + x] = val / wsum;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nind_preserves_prepared_encoded_rgb() {
        let encoded = Rgb32FImage::from_pixel(2, 2, Rgb([0.0, 0.01292, 0.4613561]));
        let unchanged = nind_model_input(DynamicImage::ImageRgb32F(encoded.clone())).unwrap();
        assert_eq!(unchanged, encoded);
    }

    #[test]
    fn nind_model_view_bounds_hdr_without_mutating_the_source_and_rejects_nonfinite() {
        let source = Rgb32FImage::from_pixel(2, 2, Rgb([-0.1, 1.0, 4.0]));
        let view = nind_model_input(DynamicImage::ImageRgb32F(source.clone())).unwrap();
        assert_eq!(view.get_pixel(0, 0)[0], 0.0);
        assert!((view.get_pixel(0, 0)[1] - 1.0).abs() < 1e-6);
        assert_eq!(view.get_pixel(0, 0)[2], 1.0);
        assert_eq!(source.get_pixel(0, 0).0, [-0.1, 1.0, 4.0]);
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let image = Rgb32FImage::from_pixel(1, 1, Rgb([invalid, 0.18, 0.2]));
            assert!(nind_model_input(DynamicImage::ImageRgb32F(image)).is_err());
        }
    }

    #[test]
    fn nind_prepared_shadow_save_does_not_apply_another_tone_curve() {
        let prepared = Rgb32FImage::from_pixel(2, 2, Rgb([0.0, 0.001292, 0.4613561]));
        let encoded = nind_model_input(DynamicImage::ImageRgb32F(prepared)).unwrap();
        assert!(!uses_default_raw_tone(true, "ai"));
        assert!(!uses_default_raw_tone(false, "ai"));
        assert!(uses_default_raw_tone(true, "bm3d"));
        assert!(uses_default_raw_tone(true, "raw9"));
        assert!(uses_default_raw_tone(true, "tree_best"));
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("result.tiff");
        let result = DenoisedOutput::Image(DynamicImage::ImageRgb32F(encoded));
        write_denoised_output(&result, &path, true, 1, None).unwrap();
        let pixel = *image::open(&path).unwrap().to_rgb16().get_pixel(0, 0);
        assert_eq!(pixel[0], 0);
        assert!(
            (pixel[1] as i32 - 85).abs() <= 1,
            "faint signal was crushed: {pixel:?}"
        );
        assert!(
            (pixel[2] as i32 - 30235).abs() <= 1,
            "tone curve baked into save: {pixel:?}"
        );
    }

    #[test]
    #[ignore = "Local NIND encoding integration; set RAPIDROOM_NIND_MODEL and RAPIDROOM_NIND_OUTPUT, under flock"]
    fn real_nind_encoded_rgb_smoke() {
        let model = std::env::var("RAPIDROOM_NIND_MODEL").unwrap();
        let session = Mutex::new(
            Session::builder()
                .unwrap()
                .with_intra_threads(4)
                .unwrap()
                .commit_from_file(model)
                .unwrap(),
        );
        let input = Rgb32FImage::from_fn(513, 257, |x, y| {
            Rgb([
                0.0001 + x as f32 / 6412.5,
                0.0002 + y as f32 / 3212.5,
                0.003,
            ])
        });
        let encoded = nind_model_input(DynamicImage::ImageRgb32F(input)).unwrap();
        let root = std::path::PathBuf::from(std::env::var("RAPIDROOM_NIND_OUTPUT").unwrap());
        let original = DenoisedOutput::Image(DynamicImage::ImageRgb32F(encoded.clone()));
        write_denoised_output(
            &original,
            &root.join("nind-encoded-input.tiff"),
            true,
            1,
            None,
        )
        .unwrap();
        for intensity in [0.1, 0.5, 0.9] {
            let result = crate::ai_processing::run_ai_denoise(
                &encoded,
                intensity,
                &session,
                &DenoiseJob::testing(),
            )
            .unwrap();
            let output = result.to_rgb32f();
            assert_eq!(output.dimensions(), encoded.dimensions());
            assert!(output.pixels().all(|p| p.0.iter().all(|v| v.is_finite())));
            write_denoised_output(
                &DenoisedOutput::Image(result),
                &root.join(format!("nind-encoded-{}.tiff", (intensity * 100.0) as u32)),
                true,
                1,
                None,
            )
            .unwrap();
        }
    }

    struct Lcg(u64);

    impl Lcg {
        fn uniform(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 40) as f32 + 0.5) / (1u64 << 24) as f32
        }

        fn gaussian(&mut self) -> f32 {
            let u1 = self.uniform();
            let u2 = self.uniform();
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
        }
    }

    fn clean_image(w: u32, h: u32, checker: bool) -> Rgb32FImage {
        Rgb32FImage::from_fn(w, h, |x, y| {
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;
            let square = if checker && (x / 24 + y / 24) % 2 == 0 {
                0.15
            } else {
                0.0
            };
            Rgb([
                0.25 + 0.4 * fx + square,
                0.3 + 0.3 * fy + square,
                0.55 - 0.3 * fx + square,
            ])
        })
    }

    fn add_noise(img: &Rgb32FImage, sigma: f32, seed: u64) -> Rgb32FImage {
        let mut rng = Lcg(seed);
        let mut out = img.clone();
        for p in out.pixels_mut() {
            for c in 0..3 {
                p[c] = (p[c] + rng.gaussian() * sigma).clamp(0.0, 1.0);
            }
        }
        out
    }

    fn rmse(a: &Rgb32FImage, b: &Rgb32FImage) -> f32 {
        let mut sum = 0.0f64;
        for (pa, pb) in a.pixels().zip(b.pixels()) {
            for c in 0..3 {
                let d = (pa[c] - pb[c]) as f64 * 255.0;
                sum += d * d;
            }
        }
        (sum / (a.len() as f64)).sqrt() as f32
    }

    #[test]
    fn bm3d_strength_does_not_add_noise_back() {
        let clean = clean_image(128, 128, false);
        for sigma in [20.0, 35.0] {
            let noisy = add_noise(&clean, sigma / 255.0, 27);
            let mut prev = f32::MAX;
            for step in 3..=10 {
                let intensity = step as f32 / 10.0;
                let err = rmse(&bm3d_denoise(&noisy, intensity, None).unwrap(), &clean);
                assert!(
                    err <= prev + 0.1,
                    "sigma {sigma}: RMSE rose from {prev:.2} to {err:.2} at intensity {intensity:.1}"
                );
                prev = err;
            }
        }
    }

    #[test]
    fn bm3d_output_is_closer_to_clean_than_input() {
        let clean = clean_image(128, 128, true);
        let noisy = add_noise(&clean, 20.0 / 255.0, 27);
        let noisy_err = rmse(&noisy, &clean);
        for step in 1..=10 {
            let intensity = step as f32 / 10.0;
            let err = rmse(&bm3d_denoise(&noisy, intensity, None).unwrap(), &clean);
            assert!(
                err < noisy_err,
                "intensity {intensity:.1}: RMSE {err:.2} not below noisy input {noisy_err:.2}"
            );
        }
    }

    type Events = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

    fn job_with_events(
        jobs: &Arc<DenoiseJobs>,
        cancel_when: impl Fn(&str, &serde_json::Value) -> bool + Send + Sync + 'static,
    ) -> (DenoiseJob, Events) {
        let id = jobs.create();
        let events: Events = Arc::default();
        let (jobs_ref, events_ref) = (jobs.clone(), events.clone());
        let sink: EventSink = Arc::new(move |event, payload| {
            if cancel_when(event, &payload) {
                jobs_ref.cancel(id);
            }
            events_ref
                .lock()
                .unwrap()
                .push((event.to_string(), payload));
        });
        (jobs.start(id, Some(sink)).unwrap(), events)
    }

    fn messages(events: &Events) -> Vec<String> {
        events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(_, p)| p["message"].as_str().map(str::to_string))
            .collect()
    }

    #[test]
    fn job_ids_are_unique_and_start_once() {
        let jobs = DenoiseJobs::default();
        let (a, b) = (jobs.create(), jobs.create());
        assert_ne!(a, b);
        assert!(jobs.start(a, None).is_some());
        assert!(jobs.start(a, None).is_none());
        jobs.cancel(b);
        assert!(jobs.start(b, None).is_none());
        assert!(jobs.start(999, None).is_none());
    }

    #[test]
    fn events_carry_the_job_id() {
        let jobs = Arc::new(DenoiseJobs::default());
        let (job, events) = job_with_events(&jobs, |_, _| false);
        job.progress("Loading image...");
        let events = events.lock().unwrap();
        assert_eq!(events[0].0, "denoise-progress");
        assert_eq!(events[0].1["jobId"], job.id);
        assert_eq!(events[0].1["message"], "Loading image...");
    }

    #[test]
    fn bm3d_stops_between_tiles_when_cancelled() {
        let noisy = add_noise(&clean_image(160, 160, true), 20.0 / 255.0, 27);
        let jobs = Arc::new(DenoiseJobs::default());
        let (job, events) = job_with_events(&jobs, |_, p| {
            p["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("Step 1/2 - "))
        });
        let result = bm3d_denoise(&noisy, 0.5, Some(&job));
        assert_eq!(result.unwrap_err(), DENOISE_CANCELLED);
        let messages = messages(&events);
        assert!(messages.iter().any(|m| m.starts_with("Step 1/2 - ")));
        assert!(
            !messages
                .iter()
                .any(|m| m.starts_with("Step 2/2") || m == "Blending detail..."),
            "work continued after cancellation: {messages:?}"
        );
    }

    #[test]
    fn bm3d_does_no_tile_work_when_cancelled_before_start() {
        let noisy = add_noise(&clean_image(64, 64, false), 20.0 / 255.0, 3);
        let jobs = Arc::new(DenoiseJobs::default());
        let (job, events) =
            job_with_events(&jobs, |_, p| p["message"] == "Processing (Step 1/2)...");
        assert_eq!(
            bm3d_denoise(&noisy, 0.5, Some(&job)).unwrap_err(),
            DENOISE_CANCELLED
        );
        assert_eq!(messages(&events), vec!["Processing (Step 1/2)..."]);
    }

    #[test]
    fn cancelled_or_older_jobs_cannot_overwrite_the_stored_result() {
        let slot: DenoiseResult = Arc::default();
        let jobs = Arc::new(DenoiseJobs::default());
        let (old, _) = job_with_events(&jobs, |_, _| false);
        let (new, _) = job_with_events(&jobs, |_, _| false);
        let image = || DenoisedOutput::Image(DynamicImage::new_rgb8(1, 1));

        store_result(&slot, &new, image()).unwrap();
        assert_eq!(
            store_result(&slot, &old, image()).unwrap_err(),
            DENOISE_CANCELLED
        );
        assert_eq!(slot.lock().unwrap().as_ref().unwrap().0, new.id);

        let (newest, _) = job_with_events(&jobs, |_, _| false);
        jobs.cancel(newest.id);
        assert_eq!(
            store_result(&slot, &newest, image()).unwrap_err(),
            DENOISE_CANCELLED
        );
        assert_eq!(slot.lock().unwrap().as_ref().unwrap().0, new.id);
    }

    fn batch_dir() -> (tempfile::TempDir, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let paths = ["a", "b", "c"]
            .iter()
            .map(|name| {
                let path = dir.path().join(format!("{name}.png"));
                DynamicImage::new_rgb8(4, 4).save(&path).unwrap();
                path.to_string_lossy().to_string()
            })
            .collect();
        (dir, paths)
    }

    fn partial_files(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".partial"))
            .collect()
    }

    #[test]
    fn virtual_copy_batch_keeps_its_sidecar_and_pending_exposure_policy() {
        let (dir, paths) = batch_dir();
        let original_pixels = fs::read(&paths[0]).unwrap();
        let selected = format!("{}?vc=legacy-look", paths[0]);
        let (_, base_sidecar) = parse_virtual_path(&paths[0]);
        let (_, selected_sidecar) = parse_virtual_path(&selected);
        let base = crate::image_processing::ImageMetadata {
            adjustments: serde_json::json!({
                "baselineExposure": true, "cameraExposureCompensation": true,
            }),
            ..Default::default()
        };
        let virtual_copy = crate::image_processing::ImageMetadata {
            adjustments: serde_json::json!({
                "exposure": 1.25, "baselineExposure": true,
                "cameraExposureCompensation": false,
            }),
            ..Default::default()
        };
        let base_bytes = serde_json::to_vec(&base).unwrap();
        let copy_bytes = serde_json::to_vec(&virtual_copy).unwrap();
        fs::write(&base_sidecar, &base_bytes).unwrap();
        fs::write(&selected_sidecar, &copy_bytes).unwrap();
        let pending_reset = HashMap::from([(
            selected.clone(),
            serde_json::json!({
                "baselineExposure": true, "cameraExposureCompensation": true,
            }),
        )]);
        let jobs = Arc::new(DenoiseJobs::default());
        let (job, _) = job_with_events(&jobs, |_, _| false);
        let saved = run_batch(&job, std::slice::from_ref(&selected), |path| {
            assert_eq!(
                selected_exposure_policy(path, &HashMap::new()),
                virtual_copy.adjustments
            );
            assert_eq!(
                selected_exposure_policy(path, &pending_reset),
                pending_reset[&selected]
            );
            assert_eq!(
                selected_exposure_policy(&paths[0], &pending_reset),
                base.adjustments
            );
            Ok(DenoisedOutput::Image(image::open(&paths[0]).unwrap()))
        });
        let output = dir.path().join("a_Denoised.png");
        assert_eq!(saved, vec![output.to_string_lossy().to_string()]);
        assert_eq!(
            image::open(output).unwrap().to_rgb8(),
            image::open(&paths[0]).unwrap().to_rgb8()
        );
        let (_, output_sidecar) = parse_virtual_path(&saved[0]);
        assert_eq!(fs::read(output_sidecar).unwrap(), copy_bytes);
        assert_eq!(fs::read(&base_sidecar).unwrap(), base_bytes);
        assert_eq!(fs::read(&selected_sidecar).unwrap(), copy_bytes);
        assert_eq!(fs::read(&paths[0]).unwrap(), original_pixels);
    }

    #[test]
    fn batch_stops_between_files_and_keeps_finished_and_existing_files() {
        let (dir, paths) = batch_dir();
        let existing = dir.path().join("b_Denoised.png");
        fs::write(&existing, b"user file").unwrap();

        let jobs = Arc::new(DenoiseJobs::default());
        let (job, events) = job_with_events(&jobs, |event, p| {
            event == "denoise-batch-progress" && p["current"] == 2
        });
        let mut calls = 0;
        let saved = run_batch(&job, &paths, |_| {
            calls += 1;
            job.check()?;
            Ok(DenoisedOutput::Image(DynamicImage::new_rgb8(4, 4)))
        });

        let first = dir.path().join("a_Denoised.png");
        assert_eq!(saved, vec![first.to_string_lossy().to_string()]);
        assert!(image::open(&first).is_ok());
        assert_eq!(calls, 2);
        assert_eq!(fs::read(&existing).unwrap(), b"user file");
        assert!(!dir.path().join("c_Denoised.png").exists());
        assert!(partial_files(dir.path()).is_empty());
        assert!(
            events
                .lock()
                .unwrap()
                .iter()
                .all(|(e, _)| e != "denoise-error")
        );
    }

    #[test]
    fn raw_batch_keeps_sensor_bytes_and_cancel_preserves_existing_dng() {
        let (dir, paths) = batch_dir();
        let existing = dir.path().join("b_Denoised.dng");
        fs::write(&existing, b"existing sensor").unwrap();
        let jobs = Arc::new(DenoiseJobs::default());
        let (job, _) = job_with_events(&jobs, |event, p| {
            event == "denoise-batch-progress" && p["current"] == 2
        });
        let bytes = b"cached sensor bytes";
        let saved = run_batch(&job, &paths, |_| {
            job.check()?;
            Ok(DenoisedOutput::Raw(bytes.to_vec()))
        });
        let first = dir.path().join("a_Denoised.dng");
        assert_eq!(saved, vec![first.to_string_lossy().to_string()]);
        assert_eq!(fs::read(first).unwrap(), bytes);
        assert_eq!(fs::read(existing).unwrap(), b"existing sensor");
        assert!(!dir.path().join("c_Denoised.dng").exists());
        assert!(partial_files(dir.path()).is_empty());
    }

    #[test]
    fn batch_cancelled_while_writing_removes_the_partial_file_only() {
        let (dir, paths) = batch_dir();
        let existing = dir.path().join("a_Denoised.png");
        fs::write(&existing, b"user file").unwrap();

        let jobs = Arc::new(DenoiseJobs::default());
        let (job, _) = job_with_events(&jobs, |_, _| false);
        let saved = run_batch(&job, &paths, |_| {
            jobs.cancel(job.id);
            Ok(DenoisedOutput::Image(DynamicImage::new_rgb8(4, 4)))
        });

        assert!(saved.is_empty());
        assert_eq!(fs::read(&existing).unwrap(), b"user file");
        assert!(!dir.path().join("b_Denoised.png").exists());
        assert!(partial_files(dir.path()).is_empty());
    }
}
