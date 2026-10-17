use memmap2::{Mmap, MmapOptions};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock, Mutex, RwLock};

pub(crate) static SIDECAR_METADATA_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
use std::thread;

use anyhow::Result;
use chrono::{DateTime, Utc};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, ImageBuffer, Luma};
use rayon::prelude::*;
use regex::regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sysinfo::Disks;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::AppState;
use crate::PendingMetadata;
#[cfg(target_os = "android")]
use crate::android_integration::*;
use crate::app_settings::*;
use crate::as_shot::as_shot;
use crate::baseline_exposure::BaselineOptions;
use crate::batch_rename::{RenameOptions, RenameOutcome, RenamePreview, UndoInfo, plan_rename};
use crate::exif_processing;
use crate::formats::{is_raw_file, is_supported_image_file};
use crate::gpu_processing;
use crate::image_loader;
use crate::image_processing::GpuContext;
use crate::image_processing::{
    Crop, ImageFlag, ImageMetadata, apply_coarse_rotation, apply_cpu_default_raw_processing,
    apply_crop, apply_exposure, apply_flip, apply_geometry_warp, apply_rotation,
    auto_results_to_json, default_adjustments, effective_baseline_exposure,
    get_all_adjustments_from_json, perform_auto_analysis, resolve_render_options,
};

use crate::lrtemplate;
use crate::mask_generation::MaskDefinition;
use crate::preset_converter;
use crate::tagging::COLOR_TAG_PREFIX;

pub const CARD_READ_ONLY_ERROR: &str =
    "Card mode is read-only: RapidRoom never writes to the card. Import the photos to keep edits.";

struct CardBrowseRoot {
    path: PathBuf,
    canonical: Option<PathBuf>,
}

static CARD_BROWSE_ROOT: LazyLock<RwLock<Option<CardBrowseRoot>>> =
    LazyLock::new(|| RwLock::new(None));

/// Resolves symlinks and `..` through the deepest ancestor that exists, so a
/// file that hasn't been created yet still maps onto its real folder.
fn resolve_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        if let Ok(canonical) = fs::canonicalize(current) {
            return Some(
                missing
                    .iter()
                    .rev()
                    .fold(canonical, |acc, part| acc.join(part)),
            );
        }
        missing.push(current.file_name()?.to_os_string());
        current = current.parent()?;
    }
}

fn with_card_root<T>(f: impl FnOnce(&CardBrowseRoot) -> T) -> Option<T> {
    CARD_BROWSE_ROOT.read().ok()?.as_ref().map(f)
}

pub fn is_card_read_only_path(path: &Path) -> bool {
    with_card_root(|root| {
        path.starts_with(&root.path)
            || root.canonical.as_ref().is_some_and(|canonical_root| {
                resolve_existing_ancestor(path).is_some_and(|p| p.starts_with(canonical_root))
            })
    })
    .unwrap_or(false)
}

/// True if `path` is on the card or contains it, e.g. deleting a parent folder.
pub fn touches_card_tree(path: &Path) -> bool {
    is_card_read_only_path(path)
        || with_card_root(|root| {
            root.path.starts_with(path)
                || root.canonical.as_ref().is_some_and(|canonical_root| {
                    resolve_existing_ancestor(path).is_some_and(|p| canonical_root.starts_with(p))
                })
        })
        .unwrap_or(false)
}

pub fn ensure_card_writable(path: &Path) -> Result<(), String> {
    if is_card_read_only_path(path) {
        Err(CARD_READ_ONLY_ERROR.to_string())
    } else {
        Ok(())
    }
}

/// Checks image paths (virtual copies included) and so the folders next to them.
pub fn ensure_card_writable_for_paths<S: AsRef<str>>(paths: &[S]) -> Result<(), String> {
    for path in paths {
        ensure_card_writable(&parse_virtual_path(path.as_ref()).0)?;
    }
    Ok(())
}

pub fn ensure_card_tree_writable(path: &Path) -> Result<(), String> {
    if touches_card_tree(path) {
        Err(CARD_READ_ONLY_ERROR.to_string())
    } else {
        Ok(())
    }
}

fn card_read_only_io_error() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::PermissionDenied, CARD_READ_ONLY_ERROR)
}

#[tauri::command]
pub fn set_card_browse_root(path: Option<String>) -> Result<(), String> {
    let root = path.filter(|p| !p.is_empty()).map(|p| {
        let path = PathBuf::from(p);
        let canonical = fs::canonicalize(&path).ok();
        CardBrowseRoot { path, canonical }
    });
    let mut guard = CARD_BROWSE_ROOT
        .write()
        .map_err(|_| "Failed to update Card mode".to_string())?;
    *guard = root;
    Ok(())
}

fn resolve_thumbnail_cache_dir(app_handle: &AppHandle) -> std::result::Result<PathBuf, String> {
    let cache_dir = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?;
    let thumb_cache_dir = cache_dir.join("thumbnails");
    if !thumb_cache_dir.exists() {
        fs::create_dir_all(&thumb_cache_dir).map_err(|e| e.to_string())?;
    }
    Ok(thumb_cache_dir)
}

fn emit_thumbnail_cache_setup_error(app_handle: &AppHandle, path: &str, reason: &str) {
    let _ = app_handle.emit(
        "thumbnail-generation-error",
        serde_json::json!({ "path": path, "reason": reason }),
    );
}

fn compute_thumbnail_cache_hash(path_str: &str, adjustments_bytes: &[u8]) -> Option<String> {
    let (source_path, _) = parse_virtual_path(path_str);
    let img_mod_time = thumbnail_mtime(&source_path)?;
    Some(thumbnail_cache_hash(
        path_str,
        img_mod_time,
        adjustments_bytes,
    ))
}

fn thumbnail_mtime(source_path: &Path) -> Option<u64> {
    Some(
        fs::metadata(source_path)
            .ok()?
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs(),
    )
}

fn thumbnail_cache_hash(path_str: &str, img_mod_time: u64, adjustments_bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(path_str.as_bytes());
    hasher.update(&img_mod_time.to_le_bytes());
    hasher.update(adjustments_bytes);
    hasher.finalize().to_hex().to_string()
}

fn thumbnail_adjustment_bytes(sidecar_path: &Path) -> Vec<u8> {
    fs::read_to_string(sidecar_path)
        .ok()
        .and_then(|content| serde_json::from_str::<ImageMetadata>(&content).ok())
        .map(|meta| serde_json::to_vec(&meta.adjustments).unwrap_or_default())
        .unwrap_or_default()
}

struct ImageFileMetadata {
    is_edited: bool,
    tags: Option<Vec<String>>,
    rating: u8,
    flag: Option<ImageFlag>,
    is_raw: bool,
    stack: Option<crate::photo_stacks::StackMember>,
}

fn resolve_image_metadata(
    image_path: &Path,
    sidecar_path: &Path,
    enable_xmp_sync: bool,
    settings: &AppSettings,
) -> ImageFileMetadata {
    let _metadata_lock = SIDECAR_METADATA_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut metadata = crate::exif_processing::load_sidecar_unlocked(sidecar_path);

    if enable_xmp_sync
        && sync_metadata_from_xmp(image_path, sidecar_path, &mut metadata)
        && let Ok(json) = serde_json::to_string_pretty(&metadata)
    {
        let _ = write_file_atomically(sidecar_path, json);
    }

    drop(_metadata_lock);
    let is_raw = crate::formats::is_raw_file(image_path);
    let tm_override = crate::image_processing::resolve_tonemapper_override(settings, is_raw);
    let is_edited =
        crate::image_processing::is_image_edited(&metadata.adjustments, is_raw, tm_override);
    ImageFileMetadata {
        is_edited,
        rating: crate::exif_processing::resolve_rating(image_path, &metadata),
        tags: metadata.tags,
        flag: metadata.flag,
        stack: metadata.stack.clone(),
        is_raw,
    }
}

fn emit_image_metadata_loaded(
    app_handle: &AppHandle,
    path: &str,
    rating: u8,
    flag: Option<ImageFlag>,
    is_edited: bool,
    tags: &Option<Vec<String>>,
    stack: &Option<crate::photo_stacks::StackMember>,
) {
    let _ = app_handle.emit(
        "image-metadata-loaded",
        serde_json::json!({ "path": path, "rating": rating, "flag": flag, "is_edited": is_edited, "tags": tags, "stack": if path.contains("?vc=") { None } else { stack.as_ref() } }),
    );
}

fn enqueue_metadata(
    app_handle: &AppHandle,
    virtual_path: String,
    image_path: PathBuf,
    sidecar_path: PathBuf,
) {
    let state = app_handle.state::<crate::AppState>();
    let manager = &state.metadata_manager;

    let mut pending = manager.pending.lock().unwrap();
    if !pending.insert(sidecar_path.clone()) {
        return;
    }
    drop(pending);

    manager.queue.lock().unwrap().push_back(PendingMetadata {
        virtual_path,
        image_path,
        sidecar_path,
    });
    manager.cvar.notify_one();
}

// Not compute-heavy — these threads mostly block waiting on iCloud to
// materialize a file, not burning CPU — so a small fixed pool is enough and
// doesn't need a user-facing setting the way thumbnail_worker_threads does.
const METADATA_WORKER_THREADS: usize = 4;

pub fn start_metadata_workers(app_handle: tauri::AppHandle) {
    let state = app_handle.state::<crate::AppState>();
    let manager = state.metadata_manager.clone();

    for _ in 0..METADATA_WORKER_THREADS {
        let app_clone = app_handle.clone();
        let manager_clone = manager.clone();

        std::thread::spawn(move || {
            loop {
                let item = {
                    let mut queue = manager_clone.queue.lock().unwrap();
                    while queue.is_empty() {
                        queue = manager_clone.cvar.wait(queue).unwrap();
                    }
                    queue.pop_front().unwrap()
                };

                let settings = load_settings(app_clone.clone()).unwrap_or_default();
                let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);

                let metadata = resolve_image_metadata(
                    &item.image_path,
                    &item.sidecar_path,
                    enable_xmp_sync,
                    &settings,
                );

                emit_image_metadata_loaded(
                    &app_clone,
                    &item.virtual_path,
                    metadata.rating,
                    metadata.flag,
                    metadata.is_edited,
                    &metadata.tags,
                    &metadata.stack,
                );

                manager_clone
                    .pending
                    .lock()
                    .unwrap()
                    .remove(&item.sidecar_path);
            }
        });
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Preset {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub adjustments: Value,
    #[serde(rename = "includeMasks", skip_serializing_if = "Option::is_none")]
    pub include_masks: Option<bool>,
    #[serde(
        rename = "includeCropTransform",
        skip_serializing_if = "Option::is_none"
    )]
    pub include_crop_transform: Option<bool>,
    #[serde(rename = "presetType", skip_serializing_if = "Option::is_none")]
    pub preset_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,
    #[serde(
        rename = "cameraModelRestriction",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub camera_model_restriction: Option<String>,
    #[serde(
        rename = "unavailableReason",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub unavailable_reason: Option<String>,
}

#[derive(Serialize)]
struct ExportPresetFile<'a> {
    creator: &'a str,
    presets: &'a [PresetItem],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PresetFolder {
    pub id: String,
    pub name: String,
    pub children: Vec<Preset>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PresetItem {
    Preset(Preset),
    Folder(PresetFolder),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PresetFile {
    pub presets: Vec<PresetItem>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PresetImportFailure {
    pub file_name: String,
    pub error: String,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PresetImportWarning {
    pub file_name: String,
    pub message: String,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PresetImportResult {
    pub presets: Vec<PresetItem>,
    pub failures: Vec<PresetImportFailure>,
    pub warnings: Vec<PresetImportWarning>,
}

#[derive(Debug)]
pub enum ReadFileError {
    Io(std::io::Error),
    Locked,
    Empty,
    NotFound,
    Invalid,
}

impl fmt::Display for ReadFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadFileError::Io(err) => write!(f, "IO error: {}", err),
            ReadFileError::Locked => write!(f, "File is locked"),
            ReadFileError::Empty => write!(f, "File is empty"),
            ReadFileError::NotFound => write!(f, "File not found"),
            ReadFileError::Invalid => write!(f, "Invalid file"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImageFile {
    pub path: String,
    modified: u64,
    is_edited: bool,
    rating: u8,
    flag: Option<ImageFlag>,
    tags: Option<Vec<String>>,
    exif: Option<HashMap<String, String>>,
    is_virtual_copy: bool,
    is_cloud_placeholder: bool,
    is_raw: bool,
    group_id: Option<String>,
    #[serde(default)]
    stack: Option<crate::photo_stacks::StackMember>,
}

fn make_group_key(source_path: &Path) -> String {
    let parent = source_path.parent().unwrap_or(Path::new(""));
    let stem = source_path.file_stem().unwrap_or_default();
    format!("{}/{}", parent.to_string_lossy(), stem.to_string_lossy())
}

fn assign_group_ids(files: &mut [ImageFile], settings: &crate::app_settings::AppSettings) {
    let require_matching_exif = settings.require_matching_exif.unwrap_or(false);
    let group_edited_files = settings.group_edited_files.unwrap_or(true);

    #[derive(Clone)]
    struct Candidate {
        index: usize,
        source_path: PathBuf,
        key: String,
    }

    let candidates: Vec<Candidate> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| !file.is_virtual_copy && (group_edited_files || !file.is_edited))
        .map(|(index, file)| {
            let (source_path, _) = parse_virtual_path(&file.path);
            let key = make_group_key(&source_path);
            Candidate {
                index,
                source_path,
                key,
            }
        })
        .collect();

    let mut stem_groups: HashMap<String, Vec<Candidate>> = HashMap::new();
    for candidate in candidates {
        stem_groups
            .entry(candidate.key.clone())
            .or_default()
            .push(candidate);
    }

    if require_matching_exif {
        let groupable_paths: Vec<PathBuf> = stem_groups
            .values()
            .filter(|candidates| candidates.len() >= 2)
            .flat_map(|candidates| candidates.iter().map(|c| c.source_path.clone()))
            .collect();
        let exif_dates: HashMap<PathBuf, Option<chrono::DateTime<chrono::Utc>>> = groupable_paths
            .par_iter()
            .map(|p| {
                (
                    p.clone(),
                    crate::exif_processing::try_get_exif_creation_date(p),
                )
            })
            .collect();

        stem_groups.retain(|_, candidates| {
            if candidates.len() < 2 {
                return false;
            }
            let first = exif_dates
                .get(&candidates[0].source_path)
                .copied()
                .flatten();
            if first.is_none() {
                return false;
            }
            candidates
                .iter()
                .skip(1)
                .all(|c| exif_dates.get(&c.source_path).copied().flatten() == first)
        });
    } else {
        stem_groups.retain(|_, candidates| candidates.len() >= 2);
    }

    for (key, candidates) in stem_groups {
        for candidate in candidates {
            files[candidate.index].group_id = Some(key.clone());
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImportSettings {
    pub filename_template: String,
    pub organize_by_date: bool,
    pub date_folder_format: String,
    pub delete_after_import: bool,
}

#[derive(Debug)]
struct ImportedXmpSidecar {
    sidecar_path: PathBuf,
    metadata: ImageMetadata,
    not_transferred: Vec<&'static str>,
    adjustments_applied: bool,
    metadata_applied: bool,
    already_applied: bool,
    draft_preserved: bool,
    settings_report: crate::lightroom_report::Report,
    unchanged: bool,
    brush_table_refusals: Vec<crate::lightroom_masks::BrushTableRefusal>,
    mask_failures: Vec<crate::lightroom_masks::MaskFailure>,
}

const NO_SUPPORTED_XMP_CONTENT_ERROR: &str =
    "No supported Lightroom adjustments or metadata were found in the selected XMP file.";

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct XmpSidecarImportResult {
    pub matched: usize,
    pub imported: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub failed: usize,
    pub failures: Vec<String>,
    pub imported_paths: Vec<String>,
    pub adjustments_imported_paths: Vec<String>,
    pub already_applied_paths: Vec<String>,
    pub preserved_draft_paths: Vec<String>,
    pub unchanged_paths: Vec<String>,
    pub not_transferred: Vec<XmpNotTransferred>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct XmpNotTransferred {
    pub path: String,
    pub items: Vec<&'static str>,
    pub settings_report: crate::lightroom_report::Report,
    pub brush_table_refusals: Vec<crate::lightroom_masks::BrushTableRefusal>,
    pub mask_failures: Vec<crate::lightroom_masks::MaskFailure>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct XmpImageImportResult {
    #[serde(flatten)]
    pub metadata: ImageMetadata,
    pub not_transferred: Vec<&'static str>,
    pub adjustments_applied: bool,
    pub already_applied: bool,
    pub draft_preserved: bool,
    pub settings_report: crate::lightroom_report::Report,
    pub unchanged: bool,
    pub brush_table_refusals: Vec<crate::lightroom_masks::BrushTableRefusal>,
    pub mask_failures: Vec<crate::lightroom_masks::MaskFailure>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmpEditorDraft {
    path: String,
    adjustments: Value,
}

pub fn parse_virtual_path(virtual_path: &str) -> (PathBuf, PathBuf) {
    let (source_path_str, copy_id) = if let Some((base, id)) = virtual_path.rsplit_once("?vc=") {
        (base.to_string(), Some(id.to_string()))
    } else {
        (virtual_path.to_string(), None)
    };

    let source_path = PathBuf::from(source_path_str);

    let sidecar_filename = if let Some(id) = copy_id {
        format!(
            "{}.{}.rrdata",
            source_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            id
        )
    } else {
        format!(
            "{}.rrdata",
            source_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        )
    };

    let sidecar_path = source_path.with_file_name(sidecar_filename);
    (source_path, sidecar_path)
}

#[tauri::command]
pub async fn read_exif_for_paths(
    paths: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<HashMap<String, HashMap<String, String>>, String> {
    let is_hdd = state
        .thumbnail_manager
        .rotational_disk
        .load(Ordering::Relaxed);

    tauri::async_runtime::spawn_blocking(move || {
        let process_path = |virtual_path: &String| {
            let (source_path, _) = parse_virtual_path(virtual_path);
            let source_path_str = source_path.to_string_lossy().to_string();

            let map = if let Some(sidecar_exif) =
                crate::exif_processing::read_rrexif_sidecar(&source_path)
            {
                sidecar_exif
            } else if is_cloud_placeholder(&source_path) {
                HashMap::new()
            } else if let Ok(mmap) = read_file_mapped(&source_path) {
                crate::exif_processing::read_exif_data(&source_path_str, &mmap)
            } else if let Ok(bytes) = fs::read(&source_path) {
                crate::exif_processing::read_exif_data(&source_path_str, &bytes)
            } else {
                HashMap::new()
            };

            if map.is_empty() {
                None
            } else {
                Some((virtual_path.clone(), map))
            }
        };

        let exif_data: HashMap<String, HashMap<String, String>> = if is_hdd {
            paths.iter().filter_map(process_path).collect()
        } else {
            paths.par_iter().filter_map(process_path).collect()
        };

        Ok(exif_data)
    })
    .await
    .unwrap_or_else(|e| Err(format!("Task failed: {}", e)))
}

#[tauri::command]
pub async fn update_exif_fields(
    paths: Vec<String>,
    updates: HashMap<String, String>,
) -> Result<(), String> {
    ensure_card_writable_for_paths(&paths)?;
    tauri::async_runtime::spawn_blocking(move || {
        paths.par_iter().for_each(|path| {
            let original_path = Path::new(&path);
            let primary_path = crate::exif_processing::get_primary_sidecar_path(original_path);
            let temp_metadata = crate::exif_processing::load_sidecar(&primary_path);

            let exif_data = temp_metadata.exif.unwrap_or_else(|| {
                if let Some(existing) = crate::exif_processing::read_rrexif_sidecar(original_path) {
                    existing
                } else if let Ok(mmap) = read_file_mapped(original_path) {
                    crate::exif_processing::read_exif_data_from_bytes(path, &mmap)
                } else if let Ok(bytes) = fs::read(original_path) {
                    crate::exif_processing::read_exif_data_from_bytes(path, &bytes)
                } else {
                    HashMap::new()
                }
            });

            if let Err(error) = update_sidecar(&primary_path, |current| {
                let exif = current.exif.get_or_insert_with(|| exif_data.clone());
                for (k, v) in &updates {
                    let trimmed = v.trim();
                    if trimmed.is_empty() {
                        exif.remove(k);
                    } else {
                        exif.insert(k.clone(), trimmed.to_string());
                    }
                }
            }) {
                log::error!("Failed to save EXIF edits: {error}");
            }
        });
        Ok(())
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

fn match_disk_kind(disks: &Disks, canonical: &Path) -> Option<bool> {
    let mut best_match: Option<(&Path, bool)> = None;

    for disk in disks.list() {
        let mount_point = disk.mount_point();
        if canonical.starts_with(mount_point) {
            let is_longer_match = best_match
                .map(|(current, _)| mount_point.as_os_str().len() > current.as_os_str().len())
                .unwrap_or(true);
            if is_longer_match {
                best_match = Some((mount_point, disk.kind() == sysinfo::DiskKind::HDD));
            }
        }
    }

    best_match.map(|(_, is_hdd)| is_hdd)
}

fn update_rotational_disk_flag(path: &str, app_handle: &AppHandle) {
    let state = app_handle.state::<crate::AppState>();
    let Ok(canonical) = Path::new(path).canonicalize() else {
        return;
    };

    let cached_match = {
        let cache = state.disks_cache.lock().unwrap();
        cache
            .as_ref()
            .and_then(|disks| match_disk_kind(disks, &canonical))
    };

    match cached_match {
        Some(is_hdd) => {
            state
                .thumbnail_manager
                .rotational_disk
                .store(is_hdd, Ordering::Relaxed);
        }
        None => {
            if !state.disks_cache_refreshing.swap(true, Ordering::Relaxed) {
                let refresh_app_handle = app_handle.clone();
                thread::spawn(move || {
                    let disks = Disks::new_with_refreshed_list();
                    let state = refresh_app_handle.state::<crate::AppState>();
                    *state.disks_cache.lock().unwrap() = Some(disks);
                    state.disks_cache_refreshing.store(false, Ordering::Relaxed);
                });
            }
        }
    }
}

#[tauri::command]
pub fn list_images_in_dir(path: String, app_handle: AppHandle) -> Result<Vec<ImageFile>, String> {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);

    update_rotational_disk_flag(&path, &app_handle);

    let entries = fs::read_dir(&path).map_err(|e| e.to_string())?;
    let mut images = Vec::new();
    let mut sidecars_by_filename: HashMap<String, Vec<Option<String>>> = HashMap::new();

    for entry in entries.filter_map(Result::ok) {
        let entry_path = entry.path();
        let file_name = entry
            .file_name()
            .into_string()
            .unwrap_or_else(|os| os.to_string_lossy().into_owned());

        if file_name.ends_with(".rrdata") {
            let base = &file_name[..file_name.len() - 7];

            let (source_filename, copy_id) =
                if base.len() >= 7 && base.as_bytes()[base.len() - 7] == b'.' {
                    let id = &base[base.len() - 6..];
                    if id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
                        (&base[..base.len() - 7], Some(id.to_string()))
                    } else {
                        (base, None)
                    }
                } else {
                    (base, None)
                };

            sidecars_by_filename
                .entry(source_filename.to_string())
                .or_default()
                .push(copy_id);
        } else if is_supported_image_file(&file_name) {
            images.push((file_name, entry_path));
        }
    }

    let tasks: Vec<_> = images
        .into_iter()
        .map(|(file_name, path_buf)| {
            let sidecars = sidecars_by_filename
                .remove(&file_name)
                .unwrap_or_else(|| vec![None]);
            let path_str = path_buf.to_string_lossy().into_owned();
            (path_str, file_name, path_buf, sidecars)
        })
        .collect();

    let mut result_list: Vec<ImageFile> = tasks
        .into_par_iter()
        .flat_map(|(path_str, file_name, path_buf, sidecars)| {
            let modified = fs::metadata(&path_buf)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let is_cloud_placeholder = is_cloud_placeholder(&path_buf);

            let mut file_results = Vec::with_capacity(sidecars.len());

            for copy_id_opt in sidecars {
                let (virtual_path, is_virtual_copy, sidecar_filename) = match copy_id_opt {
                    Some(id) => (
                        format!("{}?vc={}", path_str, id),
                        true,
                        format!("{}.{}.rrdata", file_name, id),
                    ),
                    None => (path_str.clone(), false, format!("{}.rrdata", file_name)),
                };

                let sidecar_path = path_buf.with_file_name(sidecar_filename);

                let xmp_is_placeholder = enable_xmp_sync
                    && resolve_xmp_path(&path_buf)
                        .is_some_and(|p| crate::file_management::is_cloud_placeholder(&p));

                let metadata = if crate::file_management::is_cloud_placeholder(&sidecar_path)
                    || xmp_is_placeholder
                {
                    enqueue_metadata(
                        &app_handle,
                        virtual_path.clone(),
                        path_buf.clone(),
                        sidecar_path.clone(),
                    );
                    ImageFileMetadata {
                        is_edited: false,
                        tags: None,
                        rating: 0,
                        flag: None,
                        stack: None,
                        is_raw: crate::formats::is_raw_file(&path_buf),
                    }
                } else {
                    resolve_image_metadata(&path_buf, &sidecar_path, enable_xmp_sync, &settings)
                };

                file_results.push(ImageFile {
                    path: virtual_path,
                    modified,
                    is_edited: metadata.is_edited,
                    tags: metadata.tags,
                    exif: None,
                    is_virtual_copy,
                    is_raw: metadata.is_raw,
                    group_id: None,
                    rating: metadata.rating,
                    flag: metadata.flag,
                    stack: if is_virtual_copy {
                        None
                    } else {
                        metadata.stack.clone()
                    },
                    is_cloud_placeholder,
                });
            }

            file_results
        })
        .collect();

    assign_group_ids(&mut result_list, &settings);
    Ok(result_list)
}

#[tauri::command]
pub fn list_images_recursive(
    path: String,
    app_handle: AppHandle,
) -> Result<Vec<ImageFile>, String> {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);

    update_rotational_disk_flag(&path, &app_handle);

    let root_path = Path::new(&path);
    let mut images = Vec::new();

    let mut sidecars_by_path: HashMap<PathBuf, Vec<Option<String>>> = HashMap::new();

    for entry in WalkDir::new(root_path).into_iter().filter_map(Result::ok) {
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }

        let file_name = entry_path.file_name().unwrap_or_default().to_string_lossy();
        if let Some(base) = file_name.strip_suffix(".rrdata") {
            let (source_filename, copy_id) =
                if base.len() >= 7 && base.as_bytes()[base.len() - 7] == b'.' {
                    let id = &base[base.len() - 6..];
                    if id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
                        (&base[..base.len() - 7], Some(id.to_string()))
                    } else {
                        (base, None)
                    }
                } else {
                    (base, None)
                };

            if let Some(parent) = entry_path.parent() {
                sidecars_by_path
                    .entry(parent.join(source_filename))
                    .or_default()
                    .push(copy_id);
            }
        } else if is_supported_image_file(entry_path.to_string_lossy().as_ref()) {
            images.push(entry_path.to_path_buf());
        }
    }

    let tasks: Vec<_> = images
        .into_iter()
        .map(|path_buf| {
            let sidecars = sidecars_by_path
                .remove(&path_buf)
                .unwrap_or_else(|| vec![None]);
            let path_str = path_buf.to_string_lossy().into_owned();
            let file_name = path_buf
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (path_str, file_name, path_buf, sidecars)
        })
        .collect();

    let mut result_list: Vec<ImageFile> = tasks
        .into_par_iter()
        .flat_map(|(path_str, file_name, path_buf, sidecars)| {
            let modified = fs::metadata(&path_buf)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let is_cloud_placeholder = is_cloud_placeholder(&path_buf);

            let mut file_results = Vec::with_capacity(sidecars.len());

            for copy_id_opt in sidecars {
                let (virtual_path, is_virtual_copy, sidecar_filename) = match copy_id_opt {
                    Some(id) => (
                        format!("{}?vc={}", path_str, id),
                        true,
                        format!("{}.{}.rrdata", file_name, id),
                    ),
                    None => (path_str.clone(), false, format!("{}.rrdata", file_name)),
                };

                let sidecar_path = path_buf.with_file_name(sidecar_filename);

                let xmp_is_placeholder = enable_xmp_sync
                    && resolve_xmp_path(&path_buf)
                        .is_some_and(|p| crate::file_management::is_cloud_placeholder(&p));

                let metadata = if crate::file_management::is_cloud_placeholder(&sidecar_path)
                    || xmp_is_placeholder
                {
                    enqueue_metadata(
                        &app_handle,
                        virtual_path.clone(),
                        path_buf.clone(),
                        sidecar_path.clone(),
                    );
                    ImageFileMetadata {
                        is_edited: false,
                        tags: None,
                        rating: 0,
                        flag: None,
                        stack: None,
                        is_raw: crate::formats::is_raw_file(&path_buf),
                    }
                } else {
                    resolve_image_metadata(&path_buf, &sidecar_path, enable_xmp_sync, &settings)
                };

                file_results.push(ImageFile {
                    path: virtual_path,
                    modified,
                    is_edited: metadata.is_edited,
                    tags: metadata.tags,
                    exif: None,
                    is_virtual_copy,
                    is_raw: metadata.is_raw,
                    group_id: None,
                    rating: metadata.rating,
                    flag: metadata.flag,
                    stack: if is_virtual_copy {
                        None
                    } else {
                        metadata.stack.clone()
                    },
                    is_cloud_placeholder,
                });
            }

            file_results
        })
        .collect();

    assign_group_ids(&mut result_list, &settings);
    Ok(result_list)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AlbumItem {
    Album {
        id: String,
        name: String,
        icon: Option<String>,
        images: Vec<String>,
    },
    Group {
        id: String,
        name: String,
        icon: Option<String>,
        children: Vec<AlbumItem>,
    },
}

fn get_albums_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let albums_dir = data_dir.join("albums");
    if !albums_dir.exists() {
        fs::create_dir_all(&albums_dir).map_err(|e| e.to_string())?;
    }
    Ok(albums_dir.join("albums.json"))
}

pub fn sort_album_tree(items: &mut [AlbumItem]) {
    items.sort_by(|a, b| {
        let get_sort_key = |item: &AlbumItem| match item {
            AlbumItem::Group { name, .. } => (0, name.to_lowercase()),
            AlbumItem::Album { name, .. } => (1, name.to_lowercase()),
        };

        let key_a = get_sort_key(a);
        let key_b = get_sort_key(b);

        key_a.cmp(&key_b)
    });

    for item in items.iter_mut() {
        if let AlbumItem::Group { children, .. } = item {
            sort_album_tree(children);
        }
    }
}

#[tauri::command]
pub fn get_albums(app_handle: AppHandle) -> Result<Vec<AlbumItem>, String> {
    let path = get_albums_path(&app_handle)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut items: Vec<AlbumItem> = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    sort_album_tree(&mut items);
    Ok(items)
}

#[tauri::command]
pub fn save_albums(mut tree: Vec<AlbumItem>, app_handle: AppHandle) -> Result<(), String> {
    let path = get_albums_path(&app_handle)?;
    sort_album_tree(&mut tree);
    let json_string = serde_json::to_string_pretty(&tree).map_err(|e| e.to_string())?;
    write_file_atomically(path, json_string).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_to_album(
    album_id: String,
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let mut tree = get_albums(app_handle.clone())?;

    fn add_recursive(items: &mut [AlbumItem], target_id: &str, paths_to_add: &Vec<String>) -> bool {
        for item in items.iter_mut() {
            #[allow(clippy::collapsible_match)]
            match item {
                AlbumItem::Album { id, images, .. } if id == target_id => {
                    for p in paths_to_add {
                        if !images.contains(p) {
                            images.push(p.clone());
                        }
                    }
                    return true;
                }
                AlbumItem::Group { children, .. } => {
                    if add_recursive(children, target_id, paths_to_add) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    if add_recursive(&mut tree, &album_id, &paths) {
        save_albums(tree, app_handle)?;
    }
    Ok(())
}

fn sync_album_path_changes(
    app_handle: &AppHandle,
    renames: Option<&HashMap<String, String>>,
    deletions: Option<&HashSet<String>>,
    folder_rename: Option<(&str, &str)>,
) {
    if let Ok(mut tree) = get_albums(app_handle.clone()) {
        let mut changed = false;

        fn process_nodes(
            nodes: &mut [AlbumItem],
            renames: Option<&HashMap<String, String>>,
            deletions: Option<&HashSet<String>>,
            folder_rename: Option<(&str, &str)>,
            changed: &mut bool,
        ) {
            for node in nodes.iter_mut() {
                match node {
                    AlbumItem::Album { images, .. } => {
                        let mut new_images = Vec::new();

                        for img in images.drain(..) {
                            let mut current_img = img;

                            if let Some((old_folder, new_folder)) = folder_rename {
                                let img_path = Path::new(&current_img);
                                let old_path = Path::new(old_folder);
                                if let Ok(stripped) = img_path.strip_prefix(old_path) {
                                    let new_img_path = Path::new(new_folder).join(stripped);
                                    current_img = new_img_path.to_string_lossy().into_owned();
                                    *changed = true;
                                }
                            }

                            if let Some(r) = renames {
                                if let Some(new_path) = r.get(&current_img) {
                                    current_img = new_path.clone();
                                    *changed = true;
                                } else if let Some((base_path, vc_id)) =
                                    current_img.rsplit_once("?vc=")
                                    && let Some(new_base) = r.get(base_path)
                                {
                                    current_img = format!("{}?vc={}", new_base, vc_id);
                                    *changed = true;
                                }
                            }

                            let mut is_deleted = false;
                            if let Some(d) = deletions {
                                if d.contains(&current_img) {
                                    is_deleted = true;
                                } else {
                                    let img_path = Path::new(&current_img);
                                    for del_path_str in d {
                                        let del_path = Path::new(del_path_str);
                                        if img_path.starts_with(del_path) {
                                            is_deleted = true;
                                            break;
                                        }

                                        if let Some((base_path, _)) =
                                            current_img.rsplit_once("?vc=")
                                            && base_path == del_path_str
                                        {
                                            is_deleted = true;
                                            break;
                                        }
                                    }
                                }
                            }

                            if !is_deleted {
                                new_images.push(current_img);
                            } else {
                                *changed = true;
                            }
                        }
                        *images = new_images;
                    }
                    AlbumItem::Group { children, .. } => {
                        process_nodes(children, renames, deletions, folder_rename, changed);
                    }
                }
            }
        }

        process_nodes(&mut tree, renames, deletions, folder_rename, &mut changed);

        if changed {
            let _ = save_albums(tree, app_handle.clone());
        }
    }
}

#[tauri::command]
pub fn get_album_images(
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<Vec<ImageFile>, String> {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);

    let mut result_list: Vec<ImageFile> = paths
        .into_par_iter()
        .filter_map(|virtual_path| {
            let (source_path, sidecar_path) = parse_virtual_path(&virtual_path);
            if !source_path.exists() {
                return None;
            }

            let modified = fs::metadata(&source_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let is_virtual_copy = virtual_path.contains("?vc=");
            let is_cloud_placeholder = is_cloud_placeholder(&source_path);

            let xmp_is_placeholder = enable_xmp_sync
                && resolve_xmp_path(&source_path)
                    .is_some_and(|p| crate::file_management::is_cloud_placeholder(&p));

            let metadata = if crate::file_management::is_cloud_placeholder(&sidecar_path)
                || xmp_is_placeholder
            {
                enqueue_metadata(
                    &app_handle,
                    virtual_path.clone(),
                    source_path.clone(),
                    sidecar_path.clone(),
                );
                ImageFileMetadata {
                    is_edited: false,
                    tags: None,
                    rating: 0,
                    flag: None,
                    stack: None,
                    is_raw: crate::formats::is_raw_file(&source_path),
                }
            } else {
                resolve_image_metadata(&source_path, &sidecar_path, enable_xmp_sync, &settings)
            };

            Some(ImageFile {
                path: virtual_path.clone(),
                modified,
                is_edited: metadata.is_edited,
                tags: metadata.tags,
                exif: None,
                is_virtual_copy,
                is_raw: metadata.is_raw,
                group_id: None,
                rating: metadata.rating,
                flag: metadata.flag,
                stack: if is_virtual_copy {
                    None
                } else {
                    metadata.stack.clone()
                },
                is_cloud_placeholder,
            })
        })
        .collect();

    assign_group_ids(&mut result_list, &settings);
    Ok(result_list)
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FolderNode {
    pub name: String,
    pub path: String,
    pub children: Vec<FolderNode>,
    pub is_dir: bool,
    pub image_count: usize,
    pub has_subdirs: bool,
    pub modified: u64,
    pub created: u64,
}

fn has_subdirs(path: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(Result::ok) {
            if let Ok(file_type) = entry.file_type()
                && file_type.is_dir()
            {
                let name = entry.file_name();
                if !name.to_string_lossy().starts_with('.') {
                    return true;
                }
            }
        }
    }
    false
}

fn scan_dir_lazy(
    path: &Path,
    expanded_folders: &HashSet<&str>,
    show_image_counts: bool,
    prefetch_one_level: bool,
) -> Result<(Vec<FolderNode>, usize), std::io::Error> {
    let mut children_folders = Vec::new();
    let mut current_dir_image_count = 0;

    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("Could not scan directory '{}': {}", path.display(), e);
            return Ok((Vec::new(), 0));
        }
    };

    for entry in entries.filter_map(Result::ok) {
        let current_path = entry.path();
        let (file_type, modified, created) = match entry.metadata() {
            Ok(meta) => {
                let ft = meta.file_type();
                let mod_time = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let cre_time = meta.created().unwrap_or(mod_time);

                (
                    ft,
                    mod_time
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    cre_time
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                )
            }
            Err(_) => continue,
        };

        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        if name_str.starts_with('.') {
            continue;
        }

        if file_type.is_dir() {
            let path_str = current_path.to_string_lossy().into_owned();
            let is_expanded = expanded_folders.contains(path_str.as_str());

            let should_scan = is_expanded || prefetch_one_level;
            let next_prefetch = is_expanded;

            let (grand_children, sub_dir_own_images) = if should_scan {
                scan_dir_lazy(
                    &current_path,
                    expanded_folders,
                    show_image_counts,
                    next_prefetch,
                )?
            } else {
                let count = if show_image_counts {
                    crate::folder_counts::count_photos(&current_path, false)
                        .map(|result| result.count)
                        .unwrap_or(0)
                } else {
                    0
                };
                (Vec::new(), count)
            };

            let has_any_subdirs = if should_scan {
                grand_children.iter().any(|c| c.is_dir)
            } else {
                has_subdirs(&current_path)
            };

            children_folders.push(FolderNode {
                name: name_str.into_owned(),
                path: path_str,
                children: grand_children,
                is_dir: true,
                image_count: sub_dir_own_images,
                has_subdirs: has_any_subdirs,
                modified,
                created,
            });
        } else if show_image_counts
            && file_type.is_file()
            && crate::formats::is_supported_image_file(&current_path)
        {
            current_dir_image_count += 1;
        }
    }

    children_folders.sort_by_key(|a| a.name.to_lowercase());

    Ok((children_folders, current_dir_image_count))
}

fn get_folder_tree_sync(
    path: String,
    expanded_folders: Vec<String>,
    show_image_counts: bool,
) -> Result<FolderNode, String> {
    let root_path = Path::new(&path);
    if !root_path.is_dir() {
        return Err(format!("Directory does not exist: {}", path));
    }

    let (modified, created) = root_path
        .metadata()
        .map(|m| {
            let mod_time = m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let cre_time = m.created().unwrap_or(mod_time);
            (
                mod_time
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                cre_time
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            )
        })
        .unwrap_or((0, 0));

    let expanded_set: HashSet<&str> = expanded_folders.iter().map(|s| s.as_str()).collect();

    let (children, own_count) = scan_dir_lazy(root_path, &expanded_set, show_image_counts, true)
        .map_err(|e| e.to_string())?;

    let has_subdirs = children.iter().any(|c| c.is_dir);

    let name = match root_path.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => {
            let trimmed = path.trim_end_matches(&['/', '\\'][..]);
            if trimmed.is_empty() {
                path.clone()
            } else {
                trimmed.to_string()
            }
        }
    };

    Ok(FolderNode {
        name,
        path: path.clone(),
        children,
        is_dir: true,
        image_count: own_count,
        has_subdirs,
        modified,
        created,
    })
}

#[tauri::command]
pub async fn get_folder_children(
    path: String,
    show_image_counts: bool,
) -> Result<Vec<FolderNode>, String> {
    match tauri::async_runtime::spawn_blocking(move || {
        let root_path = Path::new(&path);
        if !root_path.is_dir() {
            return Err(format!("Directory does not exist: {}", path));
        }
        let empty_set = HashSet::new();
        let (children, _) = scan_dir_lazy(root_path, &empty_set, show_image_counts, false)
            .map_err(|e| e.to_string())?;

        Ok(children)
    })
    .await
    {
        Ok(Ok(children)) => Ok(children),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(format!("Task failed: {}", e)),
    }
}

#[tauri::command]
pub async fn get_folder_tree(
    path: String,
    expanded_folders: Vec<String>,
    show_image_counts: bool,
) -> Result<FolderNode, String> {
    match tauri::async_runtime::spawn_blocking(move || {
        get_folder_tree_sync(path, expanded_folders, show_image_counts)
    })
    .await
    {
        Ok(Ok(folder_node)) => Ok(folder_node),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(format!("Failed to execute folder tree task: {}", e)),
    }
}

#[tauri::command]
pub async fn get_pinned_folder_trees(
    paths: Vec<String>,
    expanded_folders: Vec<String>,
    show_image_counts: bool,
) -> Result<Vec<FolderNode>, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let results: Vec<Result<FolderNode, String>> = paths
            .par_iter()
            .map(|path| {
                get_folder_tree_sync(path.clone(), expanded_folders.clone(), show_image_counts)
            })
            .collect();

        let mut folder_nodes = Vec::new();
        for result in results {
            match result {
                Ok(node) => folder_nodes.push(node),
                Err(e) => log::warn!("Failed to get tree for pinned folder: {}", e),
            }
        }
        folder_nodes
    })
    .await;

    match result {
        Ok(nodes) => Ok(nodes),
        Err(e) => Err(format!("Task failed: {}", e)),
    }
}

/// Checks if the given path exists and is an iCloud placeholder file on macOS.
#[cfg(target_os = "macos")]
pub fn is_cloud_placeholder(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    const SF_DATALESS: u32 = 0x4000_0000;

    let c_path = match std::ffi::CString::new(path.as_os_str().as_bytes()) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let mut stat_buf: libc::stat = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::lstat(c_path.as_ptr(), &mut stat_buf) };
    ret == 0 && (stat_buf.st_flags & SF_DATALESS) != 0
}

#[cfg(not(target_os = "macos"))]
pub fn is_cloud_placeholder(_path: &Path) -> bool {
    false
}

/// Replaces `path` so that a crash, power loss, or full disk leaves either the
/// previous file or the new one, never a truncated edit. The replacement keeps
/// the existing file's permissions, and a symlink is written through.
/// Refuses any path on a card opened in Card mode.
pub fn write_file_atomically(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> std::io::Result<()> {
    let path = path.as_ref();
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rrdata"))
    {
        return write_file_atomically_with_backend(
            path,
            contents.as_ref(),
            &mut TempfileAtomicWriteBackend,
        );
    }
    let _guard = lock_sidecar(path)?;
    let mut bytes = contents.as_ref().to_vec();
    if let Ok(existing) = fs::read(path)
        && let Ok(current) = serde_json::from_slice::<Value>(&existing)
        && SIDECAR_OWNED_FIELDS
            .iter()
            .any(|key| current.get(key).is_some())
    {
        // Ordinary edit/EXIF/rating writers can hold stale snapshots. Each dedicated
        // transaction owns its destination field, including an explicit null clear.
        let mut incoming: Value = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
        let object = incoming
            .as_object_mut()
            .ok_or_else(|| std::io::Error::other("Sidecar must be a JSON object"))?;
        preserve_sidecar_owned_fields(&current, object);
        bytes = serde_json::to_vec_pretty(&incoming).map_err(std::io::Error::other)?;
    }
    // Retain the exact-byte/non-JSON contract when there are no owned fields.
    write_file_atomically_unmerged(path, bytes)
}

pub(crate) const SIDECAR_OWNED_FIELDS: [&str; 3] = ["denoise", "appliedPreset", "exportFraming"];

pub(crate) fn preserve_sidecar_owned_fields(
    current: &Value,
    incoming: &mut serde_json::Map<String, Value>,
) {
    for key in SIDECAR_OWNED_FIELDS {
        if let Some(value) = current.get(key) {
            incoming.insert(key.into(), value.clone());
        }
    }
}

static SIDECAR_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) struct SidecarWriteGuard {
    _file: fs::File,
    _mutex: std::sync::MutexGuard<'static, ()>,
}

// All ordinary and dedicated writers acquire in this order. The file guard also
// coordinates the headless CLI with a GUI running in another process.
pub(crate) fn lock_sidecar(path: &Path) -> std::io::Result<SidecarWriteGuard> {
    if is_card_read_only_path(path) {
        return Err(card_read_only_io_error());
    }
    let mutex = SIDECAR_WRITE_LOCK
        .lock()
        .map_err(|_| std::io::Error::other("Sidecar write lock poisoned"))?;
    let file = lock_sidecar_file(path)?;
    Ok(SidecarWriteGuard {
        _file: file,
        _mutex: mutex,
    })
}

fn lock_sidecar_file(path: &Path) -> std::io::Result<fs::File> {
    let target = if path.is_symlink() {
        fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other("Missing filename"))?
        .to_string_lossy();
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(target.with_file_name(format!(".{name}.rapidroom-lock")))?;
    file.lock()?;
    Ok(file)
}

#[cfg(test)]
pub(crate) fn lock_sidecar_as_external_writer(path: &Path) -> std::io::Result<fs::File> {
    // Simulate a different process: it has its own mutex but shares the OS lock.
    lock_sidecar_file(path)
}

// Only call while holding lock_sidecar, or for a non-sidecar target.
pub(crate) fn write_file_atomically_unmerged(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> std::io::Result<()> {
    write_file_atomically_with_backend(
        path.as_ref(),
        contents.as_ref(),
        &mut TempfileAtomicWriteBackend,
    )
}

pub(crate) fn update_applied_preset(path: &Path, record: Value) -> Result<(), String> {
    let _guard = lock_sidecar(path).map_err(|e| e.to_string())?;
    let mut metadata = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            serde_json::to_value(ImageMetadata::default()).map_err(|e| e.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    };
    metadata
        .as_object_mut()
        .ok_or("Invalid photo metadata")?
        .insert("appliedPreset".into(), record);
    let bytes = serde_json::to_vec_pretty(&metadata).map_err(|e| e.to_string())?;
    write_file_atomically_unmerged(path, bytes).map_err(|e| e.to_string())
}

pub(crate) fn update_export_framing(
    path: &Path,
    update: impl FnOnce(&Value) -> Result<Value, String>,
) -> Result<(), String> {
    let _guard = lock_sidecar(path).map_err(|e| e.to_string())?;
    let mut metadata = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            serde_json::to_value(ImageMetadata::default()).map_err(|e| e.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    };
    let framing = update(&metadata)?;
    metadata
        .as_object_mut()
        .ok_or("Invalid photo metadata")?
        .insert("exportFraming".into(), framing);
    let bytes = serde_json::to_vec_pretty(&metadata).map_err(|e| e.to_string())?;
    write_file_atomically_unmerged(path, bytes).map_err(|e| e.to_string())
}

trait AtomicWriteTemp: std::io::Write {
    fn path(&self) -> &Path;
    fn sync_all(&mut self) -> std::io::Result<()>;
    fn persist(self, target: &Path) -> std::io::Result<()>
    where
        Self: Sized;
}

trait AtomicWriteBackend {
    type Temp: AtomicWriteTemp;

    fn create_temp(&mut self, parent: &Path) -> std::io::Result<Self::Temp>;
}

struct TempfileAtomicWriteBackend;

impl AtomicWriteBackend for TempfileAtomicWriteBackend {
    type Temp = tempfile::NamedTempFile;

    fn create_temp(&mut self, parent: &Path) -> std::io::Result<Self::Temp> {
        tempfile::Builder::new()
            .prefix(".rapidraw-")
            .suffix(".tmp")
            .tempfile_in(parent)
    }
}

impl AtomicWriteTemp for tempfile::NamedTempFile {
    fn path(&self) -> &Path {
        tempfile::NamedTempFile::path(self)
    }

    fn sync_all(&mut self) -> std::io::Result<()> {
        self.as_file_mut().sync_all()
    }

    fn persist(self, target: &Path) -> std::io::Result<()> {
        tempfile::NamedTempFile::persist(self, target)
            .map(|_| ())
            .map_err(|error| error.error)
    }
}

fn write_file_atomically_with_backend<B: AtomicWriteBackend>(
    path: &Path,
    contents: &[u8],
    backend: &mut B,
) -> std::io::Result<()> {
    use std::io::Write;

    if is_card_read_only_path(path) {
        return Err(card_read_only_io_error());
    }
    let target = match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => fs::canonicalize(path)?,
        _ => path.to_path_buf(),
    };
    if is_card_read_only_path(&target) {
        return Err(card_read_only_io_error());
    }
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = backend.create_temp(parent)?;
    temp.write_all(contents.as_ref())?;
    match fs::metadata(&target) {
        Ok(existing) => fs::set_permissions(temp.path(), existing.permissions())?,
        #[cfg(unix)]
        Err(_) => {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o644))?;
        }
        #[cfg(not(unix))]
        Err(_) => {}
    }
    temp.sync_all()?;
    temp.persist(&target)?;
    Ok(())
}

pub fn read_file_mapped(path: &Path) -> Result<Mmap, ReadFileError> {
    if !path.is_file() {
        return Err(ReadFileError::Invalid);
    }
    if !path.exists() {
        return Err(ReadFileError::NotFound);
    }
    if path.metadata().map_err(ReadFileError::Io)?.len() == 0 {
        return Err(ReadFileError::Empty);
    }
    let file = fs::File::open(path).map_err(ReadFileError::Io)?;
    if file.try_lock_shared().is_err() {
        return Err(ReadFileError::Locked);
    }
    let mmap = unsafe {
        MmapOptions::new()
            .len(file.metadata().map_err(ReadFileError::Io)?.len() as usize)
            .map(&file)
            .map_err(ReadFileError::Io)?
    };
    Ok(mmap)
}

fn can_use_embedded_preview(
    adjustments: &serde_json::Value,
    settings: &AppSettings,
    file_bytes: &[u8],
) -> bool {
    if adjustments.is_null() {
        return true;
    }
    if !adjustments.is_object() {
        return false;
    }
    let tm = crate::image_processing::resolve_tonemapper_override(settings, true);
    if crate::image_processing::is_image_edited(adjustments, true, tm)
        || adjustments["lensBlurEnabled"].as_bool().unwrap_or(false)
    {
        return false;
    }
    if adjustments["crop"].is_null() {
        return true;
    }
    let Ok(crop) = serde_json::from_value::<Crop>(adjustments["crop"].clone()) else {
        return false;
    };
    let Some((w, h, _)) = crate::raw_processing::get_raw_dimensions(file_bytes) else {
        return false;
    };
    full_size_crop(&crop, w, h)
}

fn thumbnail_embedded_preview(
    adjustments: &serde_json::Value,
    settings: &AppSettings,
    file_bytes: &[u8],
    path: &str,
) -> Option<DynamicImage> {
    if settings.always_decode_raw_thumbnails.unwrap_or(false)
        || !can_use_embedded_preview(adjustments, settings, file_bytes)
    {
        return None;
    }
    let resolution = settings.medium_thumbnail_resolution.unwrap_or(1280);
    let preview = image_loader::safe_embedded_preview(file_bytes, path, Some(resolution))?;
    (preview.width().max(preview.height()) >= (resolution as f32 * 0.95) as u32).then_some(preview)
}

fn full_size_crop(crop: &Crop, width: u32, height: u32) -> bool {
    let near = |a: f64, b: u32| a.is_finite() && a > 0.0 && (a - b as f64).abs() <= 1.0;
    crop.x.abs() <= 0.1
        && crop.y.abs() <= 0.1
        && ((near(crop.width, width) && near(crop.height, height))
            || (near(crop.width, height) && near(crop.height, width)))
}

fn has_ai_patches(adjustments: &serde_json::Value) -> bool {
    adjustments["aiPatches"]
        .as_array()
        .is_some_and(|patches| !patches.is_empty())
}

fn thumbnail_proxy_min_dim(
    file_bytes: &[u8],
    target_res: u32,
    crop: Option<&Crop>,
) -> Option<usize> {
    let (full_w, full_h, is_linear) = crate::raw_processing::get_raw_dimensions(file_bytes)?;
    if !is_linear {
        return None;
    }
    let full_max_dim = full_w.max(full_h) as f64;
    let needed = match crop {
        Some(c) if c.width > 0.0 && c.height > 0.0 => {
            target_res as f64 * full_max_dim / c.width.max(c.height)
        }
        _ => target_res as f64,
    };
    Some(needed.min(full_max_dim).ceil() as usize)
}

pub fn generate_thumbnail_data(
    path_str: &str,
    gpu_context: Option<&GpuContext>,
    preloaded_image: Option<&DynamicImage>,
    app_handle: &AppHandle,
) -> anyhow::Result<DynamicImage> {
    let (source_path, sidecar_path) = parse_virtual_path(path_str);
    let source_path_str = source_path.to_string_lossy().to_string();
    let is_raw = is_raw_file(&source_path_str);

    let metadata: Option<ImageMetadata> = if is_cloud_placeholder(&sidecar_path) {
        enqueue_metadata(
            app_handle,
            path_str.to_string(),
            source_path.clone(),
            sidecar_path.clone(),
        );
        None
    } else {
        fs::read_to_string(&sidecar_path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
    };

    let adjustments = metadata
        .as_ref()
        .map_or(serde_json::Value::Null, |m| m.adjustments.clone());

    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    if is_raw
        && preloaded_image.is_none()
        && let Ok(mmap) = read_file_mapped(&source_path)
        && let Some(preview) =
            thumbnail_embedded_preview(&adjustments, &settings, &mmap, &source_path_str)
    {
        return Ok(preview);
    }

    if let (Some(context), Some(meta)) = (gpu_context, metadata)
        && !meta.adjustments.is_null()
    {
        let state = app_handle.state::<AppState>();
        let target_res = settings.medium_thumbnail_resolution.unwrap_or(1280);

        let base_cache_hash = crate::cache_utils::calculate_thumbnail_base_hash(&meta.adjustments);

        let crop_data: Option<Crop> = serde_json::from_value(meta.adjustments["crop"].clone()).ok();

        let cached_base: Option<(Arc<DynamicImage>, f32)> = {
            let cache = state.thumbnail_geometry_cache.lock().unwrap();
            if let Some((cached_hash, img, scale)) = cache.get(path_str) {
                let mut sufficient_resolution = true;
                if let Some(c) = &crop_data
                    && c.width > 0.0
                    && c.height > 0.0
                {
                    let final_crop_max_dim =
                        (c.width as f32 * *scale).max(c.height as f32 * *scale);
                    if final_crop_max_dim < (target_res as f32 * 0.95) {
                        sufficient_resolution = false;
                    }
                }

                if *cached_hash == base_cache_hash && sufficient_resolution {
                    Some((Arc::clone(img), *scale))
                } else {
                    None
                }
            } else {
                None
            }
        };

        let (processing_base_arc, total_scale) = if let Some((arc_img, scale)) = cached_base {
            (arc_img, scale)
        } else {
            let mut raw_scale_factor = 1.0f32;

            let composite_image = if let Some(img) = preloaded_image {
                image_loader::composite_patches_on_image(img, &adjustments)?
            } else {
                let mmap_guard;
                let vec_guard;

                let file_slice: &[u8] = match read_file_mapped(&source_path) {
                    Ok(mmap) => {
                        mmap_guard = Some(mmap);
                        mmap_guard.as_ref().unwrap()
                    }
                    Err(e) => {
                        log::warn!("Fallback read for {}: {}", source_path_str, e);
                        let bytes = fs::read(&source_path).map_err(|io_err| {
                            anyhow::anyhow!(
                                "Fallback read failed for {}: {}",
                                source_path_str,
                                io_err
                            )
                        })?;
                        vec_guard = Some(bytes);
                        vec_guard.as_ref().unwrap()
                    }
                };

                let proxy_min_dim = if is_raw && !has_ai_patches(&adjustments) {
                    thumbnail_proxy_min_dim(file_slice, target_res, crop_data.as_ref())
                } else {
                    None
                };
                // Saved repair offsets and bitmaps are in native pixels.
                // Composite before the thumbnail resize, without fast RAW shrinking.
                let base = image_loader::load_base_image_with_proxy(
                    file_slice,
                    &source_path_str,
                    !has_ai_patches(&adjustments),
                    &settings,
                    None,
                    proxy_min_dim,
                )?;
                let img = image_loader::composite_patches_on_image(&base, &adjustments)?;

                if is_raw {
                    raw_scale_factor = crate::raw_processing::get_fast_demosaic_scale_factor(
                        file_slice,
                        img.width(),
                        img.height(),
                    );
                }
                img
            };

            let warped_image =
                apply_geometry_warp(Cow::Borrowed(&composite_image), &meta.adjustments);

            let relit_image = crate::relight::apply_relight(warped_image, &meta.adjustments);
            let fogged_image = crate::fog::apply_fog(relit_image, &meta.adjustments);
            let blurred_image = crate::lens_blur::apply_lens_blur(fogged_image, &meta.adjustments);

            let orientation_steps =
                meta.adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8;
            let coarse_rotated_image = apply_coarse_rotation(blurred_image, orientation_steps);

            let (full_w, full_h) = coarse_rotated_image.dimensions();

            let mut processing_dim = target_res;
            if let Some(c) = &crop_data
                && c.width > 0.0
                && c.height > 0.0
            {
                let crop_max_dim_loaded = c.width.max(c.height) * raw_scale_factor as f64;
                let full_max_dim = full_w.max(full_h) as f64;
                if crop_max_dim_loaded > 0.0 {
                    processing_dim = ((target_res as f64 * full_max_dim / crop_max_dim_loaded)
                        .round() as u32)
                        .min(full_w.max(full_h));
                }
            }

            let (base, gpu_scale) = if full_w > processing_dim || full_h > processing_dim {
                let base = crate::image_processing::downscale_f32_image(
                    &coarse_rotated_image,
                    processing_dim,
                    processing_dim,
                );
                let scale = if full_w > 0 {
                    base.width() as f32 / full_w as f32
                } else {
                    1.0
                };
                (base, scale)
            } else {
                (coarse_rotated_image.into_owned(), 1.0)
            };

            let total_scale = gpu_scale * raw_scale_factor;

            let mut cache = state.thumbnail_geometry_cache.lock().unwrap();
            if cache.len() >= 8 {
                let key_to_remove = cache.keys().next().cloned();
                if let Some(key) = key_to_remove {
                    cache.remove(&key);
                }
            }

            let base_arc = Arc::new(base);
            cache.insert(
                path_str.to_string(),
                (base_cache_hash, Arc::clone(&base_arc), total_scale),
            );

            (base_arc, total_scale)
        };

        let rotation_degrees = meta.adjustments["rotation"].as_f64().unwrap_or(0.0) as f32;
        let flip_horizontal = meta.adjustments["flipHorizontal"]
            .as_bool()
            .unwrap_or(false);
        let flip_vertical = meta.adjustments["flipVertical"].as_bool().unwrap_or(false);

        let flipped_image = apply_flip(
            Cow::Borrowed(&*processing_base_arc),
            flip_horizontal,
            flip_vertical,
        );
        let rotated_image = apply_rotation(flipped_image, rotation_degrees);

        let scaled_crop_json = if let Some(c) = &crop_data {
            serde_json::to_value(Crop {
                x: c.x * total_scale as f64,
                y: c.y * total_scale as f64,
                width: c.width * total_scale as f64,
                height: c.height * total_scale as f64,
            })
            .unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };

        let cropped_preview = apply_crop(rotated_image, &scaled_crop_json);
        let (preview_w, preview_h) = cropped_preview.dimensions();
        let unscaled_crop_offset = crop_data.map_or((0.0, 0.0), |c| (c.x as f32, c.y as f32));

        let mask_definitions: Vec<MaskDefinition> =
            crate::mask_generation::parse_mask_definitions(&meta.adjustments);

        let mask_bitmaps: Vec<ImageBuffer<Luma<u8>, Vec<u8>>> = mask_definitions
            .iter()
            .filter_map(|def| {
                crate::get_cached_or_generate_mask(
                    &state,
                    path_str,
                    def,
                    preview_w,
                    preview_h,
                    total_scale,
                    (
                        unscaled_crop_offset.0 * total_scale,
                        unscaled_crop_offset.1 * total_scale,
                    ),
                    &meta.adjustments,
                )
            })
            .collect();

        let gpu_adjustments = get_all_adjustments_from_json(
            &meta.adjustments,
            is_raw,
            as_shot(&source_path_str),
            resolve_render_options(&settings, is_raw),
        );
        let lut_path = meta.adjustments["lutPath"].as_str();
        let lut = lut_path.and_then(|p| {
            let mut cache = state.lut_cache.lock().unwrap();
            if let Some(cached_lut) = cache.get(p) {
                return Some(cached_lut.clone());
            }
            if let Ok(loaded_lut) = crate::lut_processing::parse_lut_file(p) {
                let arc_lut = Arc::new(loaded_lut);
                cache.insert(p.to_string(), arc_lut.clone());
                return Some(arc_lut);
            }
            None
        });

        let mut hasher = DefaultHasher::new();
        path_str.hash(&mut hasher);
        meta.adjustments.to_string().hash(&mut hasher);
        let unique_hash = hasher.finish();

        if let Ok(processed_image) = gpu_processing::process_and_get_dynamic_image(
            context,
            &state,
            cropped_preview.as_ref(),
            unique_hash,
            gpu_processing::RenderRequest {
                adjustments: gpu_adjustments,
                mask_bitmaps: &mask_bitmaps,
                lut,
                roi: None,
            },
            "generate_thumbnail_data",
        ) {
            return Ok(processed_image);
        } else {
            return Ok(cropped_preview.into_owned());
        }
    }

    let mut final_image = if let Some(img) = preloaded_image {
        image_loader::composite_patches_on_image(img, &adjustments)?
    } else {
        match read_file_mapped(&source_path) {
            Ok(mmap) => image_loader::load_and_composite(
                &mmap,
                &source_path_str,
                &adjustments,
                !has_ai_patches(&adjustments),
                &settings,
                None,
            )?,
            Err(e) => {
                log::warn!("Fallback read for {}: {}", source_path_str, e);
                let bytes = fs::read(&source_path)?;
                image_loader::load_and_composite(
                    &bytes,
                    &source_path_str,
                    &adjustments,
                    !has_ai_patches(&adjustments),
                    &settings,
                    None,
                )?
            }
        }
    };

    if adjustments.is_null() {
        let render = resolve_render_options(&settings, is_raw);
        final_image = apply_exposure(
            final_image,
            effective_baseline_exposure(&adjustments, as_shot(&source_path_str), render.baseline),
        );
        let use_agx = render.tonemapper_override == Some(1);

        if use_agx {
            if !is_raw {
                final_image = crate::image_processing::apply_srgb_to_linear(final_image);
            }
            crate::image_processing::apply_cpu_agx_tonemap(&mut final_image);
        } else if is_raw {
            apply_cpu_default_raw_processing(&mut final_image);
        }
    }

    let fallback_orientation_steps = adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8;
    Ok(apply_coarse_rotation(Cow::Owned(final_image), fallback_orientation_steps).into_owned())
}

fn encode_thumbnail(image: &DynamicImage, target_width: u32) -> Result<Vec<u8>> {
    let thumbnail = crate::image_processing::downscale_f32_image(image, target_width, target_width);
    let mut buf = Cursor::new(Vec::new());
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, 75);
    encoder.encode_image(&thumbnail.to_rgb8())?;
    Ok(buf.into_inner())
}

fn generate_single_thumbnail_and_cache(
    path_str: &str,
    thumb_cache_dir: &Path,
    gpu_context: Option<&GpuContext>,
    preloaded_image: Option<&DynamicImage>,
    force_regenerate: bool,
    app_handle: &AppHandle,
    settings: &AppSettings,
) -> Option<(String, String, u8, bool)> {
    generate_single_thumbnail_and_cache_with(
        path_str,
        thumb_cache_dir,
        gpu_context,
        preloaded_image,
        force_regenerate,
        app_handle,
        settings,
        |_| {},
    )
}

#[allow(clippy::too_many_arguments)]
fn generate_single_thumbnail_and_cache_with(
    path_str: &str,
    thumb_cache_dir: &Path,
    gpu_context: Option<&GpuContext>,
    preloaded_image: Option<&DynamicImage>,
    force_regenerate: bool,
    app_handle: &AppHandle,
    settings: &AppSettings,
    before_generate: impl FnOnce(bool),
) -> Option<(String, String, u8, bool)> {
    let (source_path, sidecar_path) = parse_virtual_path(path_str);

    let (rating, is_edited, adjustments_bytes) = if is_cloud_placeholder(&sidecar_path) {
        enqueue_metadata(
            app_handle,
            path_str.to_string(),
            source_path.clone(),
            sidecar_path.clone(),
        );
        (0, false, Vec::new())
    } else if let Some(meta) = fs::read_to_string(&sidecar_path)
        .ok()
        .and_then(|content| serde_json::from_str::<ImageMetadata>(&content).ok())
    {
        let is_raw = crate::formats::is_raw_file(path_str);
        let tm = crate::image_processing::resolve_tonemapper_override(settings, is_raw);
        (
            crate::exif_processing::resolve_rating(&source_path, &meta),
            crate::image_processing::is_image_edited(&meta.adjustments, is_raw, tm),
            serde_json::to_vec(&meta.adjustments).unwrap_or_default(),
        )
    } else {
        (
            crate::exif_processing::resolve_rating(&source_path, &ImageMetadata::default()),
            false,
            Vec::new(),
        )
    };

    let cache_hash = compute_thumbnail_cache_hash(path_str, &adjustments_bytes)?;

    let small_path = thumb_cache_dir.join(format!("{}_small.jpg", cache_hash));
    let medium_path = thumb_cache_dir.join(format!("{}_medium.jpg", cache_hash));

    if !force_regenerate && small_path.exists() && medium_path.exists() {
        return Some((
            small_path.to_string_lossy().into_owned(),
            medium_path.to_string_lossy().into_owned(),
            rating,
            is_edited,
        ));
    }

    if is_cloud_placeholder(&source_path) {
        return None;
    }

    before_generate(is_edited);

    let target_width_small = settings.small_thumbnail_resolution.unwrap_or(480);
    let target_width_medium = settings.medium_thumbnail_resolution.unwrap_or(1280);

    if let Ok(thumb_image) =
        generate_thumbnail_data(path_str, gpu_context, preloaded_image, app_handle)
        && let (Ok(small_data), Ok(medium_data)) = (
            encode_thumbnail(&thumb_image, target_width_small),
            encode_thumbnail(&thumb_image, target_width_medium),
        )
    {
        let _ = fs::write(&small_path, &small_data);
        let _ = fs::write(&medium_path, &medium_data);
        return Some((
            small_path.to_string_lossy().into_owned(),
            medium_path.to_string_lossy().into_owned(),
            rating,
            is_edited,
        ));
    }
    None
}

const RAW_HEADER_PREFETCH_BYTES: u64 = 8 * 1024 * 1024;

fn prefetch_source_file(path_str: &str, is_edited: bool, always_decode_raw: bool) {
    let (source_path, _) = parse_virtual_path(path_str);
    let Ok(mut file) = std::fs::File::open(&source_path) else {
        return;
    };
    let source_path_str = source_path.to_string_lossy().into_owned();
    if is_raw_file(&source_path_str) {
        let mut head = Vec::new();
        if (&mut file)
            .take(RAW_HEADER_PREFETCH_BYTES)
            .read_to_end(&mut head)
            .is_err()
        {
            return;
        }
        if !is_edited && !always_decode_raw {
            return;
        }
        let is_dng = source_path_str.to_lowercase().ends_with(".dng");
        let is_linear_dng = is_dng
            && crate::raw_processing::get_raw_dimensions(&head)
                .is_some_and(|(_, _, is_linear)| is_linear);
        if is_linear_dng && !always_decode_raw {
            return;
        }
    }
    let _ = std::io::copy(&mut file, &mut std::io::sink());
}

pub fn start_thumbnail_workers(app_handle: tauri::AppHandle) {
    let state = app_handle.state::<crate::AppState>();
    let manager = state.thumbnail_manager.clone();
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let thread_count = settings.thumbnail_worker_threads.unwrap_or(4).clamp(1, 16);

    for _ in 0..thread_count {
        let app_clone = app_handle.clone();
        let manager_clone = manager.clone();

        std::thread::spawn(move || {
            loop {
                let path_to_process: String = {
                    let mut queue = manager_clone.queue.lock().unwrap();
                    while queue.is_empty() {
                        queue = manager_clone.cvar.wait(queue).unwrap();
                    }
                    let path = queue.pop_back().unwrap();

                    let mut processing = manager_clone.processing_now.lock().unwrap();
                    if processing.contains(&path) {
                        let state = app_clone.state::<crate::AppState>();
                        increment_thumbnail_progress(&state, &app_clone);
                        continue;
                    }
                    processing.insert(path.clone());
                    path
                };

                let state = app_clone.state::<crate::AppState>();
                let gpu_context =
                    crate::gpu_processing::get_or_init_gpu_context(&state, &app_clone).ok();

                let current_settings = load_settings(app_clone.clone()).unwrap_or_default();

                if let Ok(cache_dir) = get_thumb_cache_dir(&app_clone) {
                    let always_decode_raw = current_settings
                        .always_decode_raw_thumbnails
                        .unwrap_or(false);
                    let result = generate_single_thumbnail_and_cache_with(
                        &path_to_process,
                        &cache_dir,
                        gpu_context.as_ref(),
                        None,
                        false,
                        &app_clone,
                        &current_settings,
                        |is_edited| {
                            if manager_clone.rotational_disk.load(Ordering::Relaxed) {
                                let _io_permit = manager_clone.io_gate.lock().unwrap();
                                prefetch_source_file(
                                    &path_to_process,
                                    is_edited,
                                    always_decode_raw,
                                );
                            }
                        },
                    );

                    if let Some((small_path, medium_path, rating, is_edited)) = result {
                        emit_thumbnail_generated(
                            &app_clone,
                            &path_to_process,
                            &small_path,
                            &medium_path,
                            rating,
                            is_edited,
                        );
                    }
                    increment_thumbnail_progress(&state, &app_clone);
                }
                manager_clone
                    .processing_now
                    .lock()
                    .unwrap()
                    .remove(&path_to_process);
            }
        });
    }
}

#[tauri::command]
pub fn update_thumbnail_queue(
    paths: Vec<String>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let state = app_handle.state::<crate::AppState>();

    let mut queue = state.thumbnail_manager.queue.lock().unwrap();

    if paths.is_empty() {
        queue.clear();
        let mut tracker = state.thumbnail_progress.lock().unwrap();
        tracker.total = 0;
        tracker.completed = 0;
        drop(tracker);

        let _ = app_handle.emit(
            "thumbnail-progress",
            serde_json::json!({ "current": 0, "total": 0 }),
        );
        state.thumbnail_manager.cvar.notify_all();
        return Ok(());
    }

    let mut unique_paths = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        if seen.insert(path.clone()) {
            unique_paths.push(path);
        }
    }

    queue.retain(|p| !seen.contains(p));

    while queue.len() + unique_paths.len() > 500 {
        queue.pop_front();
    }

    if state
        .thumbnail_manager
        .rotational_disk
        .load(Ordering::Relaxed)
    {
        unique_paths.sort();
        for path in unique_paths.into_iter().rev() {
            queue.push_back(path);
        }
    } else {
        for path in unique_paths {
            queue.push_back(path);
        }
    }

    let queue_len = queue.len();
    drop(queue);

    let mut tracker = state.thumbnail_progress.lock().unwrap();
    tracker.total = tracker.completed + queue_len;

    let current = tracker.completed;
    let total = tracker.total;
    drop(tracker);

    let _ = app_handle.emit(
        "thumbnail-progress",
        serde_json::json!({ "current": current, "total": total }),
    );

    state.thumbnail_manager.cvar.notify_all();
    Ok(())
}

pub fn add_to_thumbnail_queue(state: &AppState, count: usize, app_handle: &AppHandle) {
    let mut tracker = state.thumbnail_progress.lock().unwrap();
    tracker.total += count;
    let current = tracker.completed;
    let total = tracker.total;
    drop(tracker);

    let _ = app_handle.emit(
        "thumbnail-progress",
        serde_json::json!({ "current": current, "total": total }),
    );
}

pub fn increment_thumbnail_progress(state: &AppState, app_handle: &AppHandle) {
    let mut tracker = state.thumbnail_progress.lock().unwrap();
    tracker.completed += 1;
    let current = tracker.completed;
    let total = tracker.total;

    if current >= total {
        tracker.total = 0;
        tracker.completed = 0;
        drop(tracker);

        let _ = app_handle.emit(
            "thumbnail-progress",
            serde_json::json!({ "current": 0, "total": 0 }),
        );
        let _ = app_handle.emit("thumbnail-generation-complete", true);
    } else {
        drop(tracker);
        let _ = app_handle.emit(
            "thumbnail-progress",
            serde_json::json!({ "current": current, "total": total }),
        );
    }
}

fn emit_thumbnail_generated(
    app_handle: &AppHandle,
    path: &str,
    small_thumbnail_path: &str,
    medium_thumbnail_path: &str,
    rating: u8,
    is_edited: bool,
) {
    let _ = app_handle.emit(
        "thumbnail-generated",
        serde_json::json!({
            "path": path,
            "thumbnailPath": small_thumbnail_path,
            "previewPath": medium_thumbnail_path,
            "rating": rating,
            "is_edited": is_edited
        }),
    );
}

pub fn resolve_lens_params_in_adjustments(
    adjustments: &mut Value,
    exif_data: &Option<HashMap<String, String>>,
    lens_db: Option<&crate::lens_correction::LensDatabase>,
) {
    if let Some(map) = adjustments.as_object_mut() {
        let mode = map
            .get("lensCorrectionMode")
            .and_then(|v| v.as_str())
            .unwrap_or("manual");

        if mode == "auto" {
            if let Some(exif) = exif_data {
                let exif_maker = exif.get("Make").map(|s| s.as_str()).unwrap_or("");
                let exif_model = exif.get("LensModel").map(|s| s.as_str()).unwrap_or("");
                let exif_camera_model = exif.get("Model").map(|s| s.as_str()).unwrap_or("");
                if let Some(db) = lens_db {
                    if let Some((detected_maker, detected_model)) =
                        crate::lens_correction::find_best_lens_match(
                            db,
                            exif_maker,
                            exif_model,
                            exif_camera_model,
                        )
                    {
                        map.insert(
                            "lensMaker".to_string(),
                            serde_json::to_value(&detected_maker).unwrap(),
                        );
                        map.insert(
                            "lensModel".to_string(),
                            serde_json::to_value(&detected_model).unwrap(),
                        );
                    } else {
                        map.remove("lensMaker");
                        map.remove("lensModel");
                    }
                }
            } else {
                map.remove("lensMaker");
                map.remove("lensModel");
            }
        }

        // Lens values from an older version carry no radius scale. They keep
        // the old evaluation when the edit is saved again, so that the image
        // does not change. Choosing a lens again gives the current one.
        let legacy = map
            .get("lensDistortionParams")
            .and_then(|v| v.as_object())
            .is_some_and(|p| !p.contains_key("radius_scale"));

        if let Some(db) = lens_db {
            let has_valid_lens = match (
                map.get("lensMaker").and_then(|v| v.as_str()),
                map.get("lensModel").and_then(|v| v.as_str()),
            ) {
                (Some(maker), Some(model)) if !maker.is_empty() && !model.is_empty() => {
                    let mut focal_length = 50.0;
                    let mut aperture = None;
                    let mut distance = None;

                    if let Some(exif) = exif_data {
                        if let Some(fl_str) = exif
                            .get("FocalLength")
                            .or(exif.get("FocalLengthIn35mmFilm"))
                            && let Ok(fl) = fl_str.replace(" mm", "").trim().parse::<f32>()
                        {
                            focal_length = fl;
                        }
                        if let Some(ap_str) = exif.get("ApertureValue").or(exif.get("FNumber"))
                            && let Ok(ap) = ap_str.replace("f/", "").trim().parse::<f32>()
                        {
                            aperture = Some(ap);
                        }
                        if let Some(dist_str) = exif.get("SubjectDistance")
                            && let Ok(dist) = dist_str.replace(" m", "").trim().parse::<f32>()
                        {
                            distance = Some(dist);
                        }
                    }

                    let camera_crop = exif_data.as_ref().and_then(|exif| {
                        crate::lens_correction::camera_crop_factor(
                            db,
                            exif.get("Make").map(|s| s.as_str()).unwrap_or(""),
                            exif.get("Model").map(|s| s.as_str()).unwrap_or(""),
                        )
                    });

                    let resolved = if legacy {
                        crate::lens_correction::resolve_legacy_lens_params(
                            db,
                            maker,
                            model,
                            focal_length,
                            aperture,
                            distance,
                        )
                    } else {
                        crate::lens_correction::resolve_lens_params(
                            db,
                            maker,
                            model,
                            focal_length,
                            aperture,
                            distance,
                            camera_crop,
                        )
                    };

                    if let Some(mut params) = resolved {
                        if let Some(saved) = map.get("lensDistortionParams") {
                            params.preserve_saved_tca(saved);
                        }
                        map.insert(
                            "lensDistortionParams".to_string(),
                            serde_json::to_value(params).unwrap(),
                        );
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if !has_valid_lens {
                map.remove("lensDistortionParams");
            }
        }
    }
}

#[tauri::command]
pub fn get_supported_file_types() -> Result<serde_json::Value, String> {
    let raw_extensions: Vec<&str> = crate::formats::RAW_EXTENSIONS
        .iter()
        .map(|(ext, _)| *ext)
        .collect();
    let non_raw_extensions: Vec<&str> = crate::formats::NON_RAW_EXTENSIONS.to_vec();

    Ok(serde_json::json!({
        "raw": raw_extensions,
        "nonRaw": non_raw_extensions
    }))
}

#[tauri::command]
pub fn create_folder(path: String) -> Result<(), String> {
    let path_obj = Path::new(&path);
    ensure_card_writable(path_obj)?;
    if let (Some(parent), Some(new_folder_name_os)) = (path_obj.parent(), path_obj.file_name())
        && let Some(new_folder_name) = new_folder_name_os.to_str()
        && parent.exists()
    {
        for entry in fs::read_dir(parent).map_err(|e| e.to_string())? {
            if let Ok(entry) = entry
                && entry.file_name().to_string_lossy().to_lowercase()
                    == new_folder_name.to_lowercase()
            {
                return Err("A folder with that name already exists.".to_string());
            }
        }
    }
    fs::create_dir_all(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_folder(path: String, new_name: String, app_handle: AppHandle) -> Result<(), String> {
    let new_folder_str = rename_folder_on_disk(&path, &new_name)?;
    sync_album_path_changes(&app_handle, None, None, Some((&path, &new_folder_str)));
    Ok(())
}

fn rename_folder_on_disk(path: &str, new_name: &str) -> Result<String, String> {
    let p = Path::new(path);
    ensure_card_tree_writable(p)?;
    if !p.is_dir() {
        return Err("Path is not a directory.".to_string());
    }
    if let Some(parent) = p.parent() {
        for entry in fs::read_dir(parent).map_err(|e| e.to_string())? {
            if let Ok(entry) = entry
                && entry.file_name().to_string_lossy().to_lowercase() == new_name.to_lowercase()
                && entry.path() != p
            {
                return Err("A folder with that name already exists.".to_string());
            }
        }
        let new_path = parent.join(new_name);
        fs::rename(p, &new_path).map_err(|e| e.to_string())?;
        Ok(new_path.to_string_lossy().into_owned())
    } else {
        Err("Could not determine parent directory.".to_string())
    }
}

#[tauri::command]
pub fn delete_folder(path: String, app_handle: AppHandle) -> Result<(), String> {
    delete_folder_on_disk(&path)?;

    let mut deletions = HashSet::new();
    deletions.insert(path);
    sync_album_path_changes(&app_handle, None, Some(&deletions), None);

    Ok(())
}

fn delete_folder_on_disk(path: &str) -> Result<(), String> {
    ensure_card_tree_writable(Path::new(path))?;
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        if let Err(trash_error) = trash::delete(path) {
            log::warn!(
                "Failed to move folder to trash: {}. Falling back to permanent delete.",
                trash_error
            );
            fs::remove_dir_all(path).map_err(|e| e.to_string())?;
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn duplicate_file(
    path: String,
    target_album_id: Option<String>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let dest_path_str = duplicate_file_on_disk(&path)?;

    if let Some(album_id) = target_album_id {
        let _ = add_to_album(album_id, vec![dest_path_str.clone()], app_handle);
    }

    Ok(dest_path_str)
}

fn duplicate_file_on_disk(path: &str) -> Result<String, String> {
    let (source_path, source_sidecar_path) = parse_virtual_path(path);
    ensure_card_writable(&source_path)?;
    if !source_path.is_file() {
        return Err("Source path is not a file.".to_string());
    }

    let parent = source_path
        .parent()
        .ok_or("Could not get parent directory")?;
    let stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Could not get file stem")?;
    let extension = source_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let mut counter = 1;
    let mut dest_path;
    loop {
        let new_stem = if counter == 1 {
            format!("{}_copy", stem)
        } else {
            format!("{}_copy_{}", stem, counter - 1)
        };
        dest_path = parent.join(format!("{}.{}", new_stem, extension));
        if !dest_path.exists() {
            break;
        }
        counter += 1;
    }

    fs::copy(&source_path, &dest_path).map_err(|e| e.to_string())?;

    if source_sidecar_path.exists()
        && let Some(dest_str) = dest_path.to_str()
    {
        let (_, dest_sidecar_path) = parse_virtual_path(dest_str);
        fs::copy(&source_sidecar_path, &dest_sidecar_path).map_err(|e| e.to_string())?;
    }

    let mut source_rrexif_name = source_path.file_name().unwrap().to_os_string();
    source_rrexif_name.push(".rrexif");
    let source_rrexif = source_path.with_file_name(source_rrexif_name);

    if source_rrexif.exists() {
        let mut dest_rrexif_name = dest_path.file_name().unwrap().to_os_string();
        dest_rrexif_name.push(".rrexif");
        let dest_rrexif = dest_path.with_file_name(dest_rrexif_name);
        let _ = fs::copy(&source_rrexif, &dest_rrexif);
    }

    Ok(dest_path.to_string_lossy().into_owned())
}

fn find_all_associated_files(source_image_path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut associated_files = vec![source_image_path.to_path_buf()];

    let mut rrexif_name = source_image_path
        .file_name()
        .unwrap_or_default()
        .to_os_string();
    rrexif_name.push(".rrexif");
    let rrexif_path = source_image_path.with_file_name(rrexif_name);

    if rrexif_path.exists() {
        associated_files.push(rrexif_path);
    }

    let parent_dir = source_image_path
        .parent()
        .ok_or("Could not determine parent directory")?;
    let source_filename = source_image_path
        .file_name()
        .ok_or("Could not get source filename")?
        .to_string_lossy();

    let primary_sidecar_name = format!("{}.rrdata", source_filename);
    let virtual_copy_prefix = format!("{}.", source_filename);

    if let Ok(entries) = fs::read_dir(parent_dir) {
        for entry in entries.filter_map(Result::ok) {
            let entry_path = entry.path();
            if !entry_path.is_file() {
                continue;
            }

            let entry_os_filename = entry.file_name();
            let entry_filename = entry_os_filename.to_string_lossy();

            if entry_filename == primary_sidecar_name
                || (entry_filename.starts_with(&virtual_copy_prefix)
                    && entry_filename.ends_with(".rrdata"))
            {
                associated_files.push(entry_path);
            }
        }
    }

    Ok(associated_files)
}

#[tauri::command]
pub fn copy_files(
    source_paths: Vec<String>,
    destination_folder: String,
) -> Result<crate::file_transfer::TransferSummary, String> {
    let dest_path = Path::new(&destination_folder);
    ensure_card_writable(dest_path)?;
    if !dest_path.is_dir() {
        return Err(format!(
            "Destination is not a folder: {}",
            destination_folder
        ));
    }
    let mut seen = HashSet::new();
    let mut summary = crate::file_transfer::TransferSummary::default();
    for source in source_paths.iter().map(|p| parse_virtual_path(p).0) {
        if !seen.insert(source.clone()) {
            continue;
        }
        let result = (|| {
            let filename = source.file_name().ok_or("Source has no filename")?;
            let in_place = source.parent().and_then(|p| p.canonicalize().ok())
                == dest_path.canonicalize().ok();
            crate::file_transfer::transfer(&source, &dest_path.join(filename), in_place)
        })();
        summary.record(result);
    }
    Ok(summary)
}

#[tauri::command]
pub fn move_files(
    source_paths: Vec<String>,
    destination_folder: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    let renames = move_files_on_disk(&source_paths, &destination_folder)?;
    sync_album_path_changes(&app_handle, Some(&renames), None, None);
    Ok(())
}

fn move_files_on_disk(
    source_paths: &[String],
    destination_folder: &str,
) -> Result<HashMap<String, String>, String> {
    let dest_path = Path::new(destination_folder);
    ensure_card_writable(dest_path)?;
    for path in source_paths {
        ensure_card_tree_writable(&parse_virtual_path(path).0)?;
    }
    if !dest_path.is_dir() {
        return Err(format!(
            "Destination is not a folder: {}",
            destination_folder
        ));
    }

    let unique_source_images: HashSet<PathBuf> = source_paths
        .iter()
        .map(|p| parse_virtual_path(p).0)
        .collect();

    let mut operations_to_perform = Vec::new();
    let mut renames = HashMap::new();

    for source_image_path in &unique_source_images {
        let source_parent = source_image_path
            .parent()
            .ok_or("Could not get parent directory")?;

        if source_parent == dest_path {
            return Err("Cannot move files into the same folder they are already in.".to_string());
        }

        let all_files_to_move = find_all_associated_files(source_image_path)?;

        for file_to_move in &all_files_to_move {
            if let Some(file_name) = file_to_move.file_name() {
                let dest_file_path = dest_path.join(file_name);

                if dest_file_path.exists() {
                    return Err(format!(
                        "Move aborted: File already exists at destination: {}",
                        dest_file_path.display()
                    ));
                }

                operations_to_perform.push((file_to_move.clone(), dest_file_path));
            }
        }

        let dest_image_path = dest_path.join(source_image_path.file_name().unwrap());
        renames.insert(
            source_image_path.to_string_lossy().into_owned(),
            dest_image_path.to_string_lossy().into_owned(),
        );
    }

    for (source, dest) in operations_to_perform {
        if fs::rename(&source, &dest).is_err() {
            fs::copy(&source, &dest)
                .map_err(|e| format!("Move failed during copy for {}: {}", source.display(), e))?;

            if let Err(e) = fs::remove_file(&source) {
                log::warn!(
                    "Moved file successfully, but failed to delete original {}: {}",
                    source.display(),
                    e
                );
            }
        }
    }

    Ok(renames)
}

#[tauri::command]
pub fn save_metadata_and_update_thumbnail(
    path: String,
    adjustments: Value,
    app_handle: AppHandle,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let (source_path, sidecar_path) = parse_virtual_path(&path);
    ensure_card_writable(&source_path)?;

    let mut metadata = crate::exif_processing::load_sidecar_with_exif(&sidecar_path, &source_path);

    let mut final_adjustments = adjustments;
    {
        let lens_db_guard = state.lens_db.lock().unwrap();
        resolve_lens_params_in_adjustments(
            &mut final_adjustments,
            &metadata.exif,
            lens_db_guard.as_deref(),
        );
    }

    metadata = update_sidecar(&sidecar_path, |current| {
        current.adjustments = final_adjustments.clone();
        if current.exif.is_none() {
            current.exif = metadata.exif.clone();
        }
    })?;

    if let Ok(settings) = load_settings(app_handle.clone())
        && settings.enable_xmp_sync.unwrap_or(false)
    {
        let create_if_missing = settings.create_xmp_if_missing.unwrap_or(false);
        sync_metadata_to_xmp(&source_path, &sidecar_path, &metadata, create_if_missing);
    }

    queue_import_thumbnail_refresh(path, app_handle, &state);
    Ok(())
}

fn queue_import_thumbnail_refresh(
    path: String,
    app_handle: AppHandle,
    state: &tauri::State<'_, AppState>,
) {
    let loaded_image_lock = state.original_image.lock().unwrap();
    let preloaded_image_option = if let Some(loaded_image) = loaded_image_lock.as_ref() {
        if loaded_image.path == path {
            Some(loaded_image.image.clone())
        } else {
            None
        }
    } else {
        None
    };
    drop(loaded_image_lock);

    let gpu_context = gpu_processing::get_or_init_gpu_context(state, &app_handle).ok();
    let app_handle_clone = app_handle.clone();
    let path_clone = path.clone();

    add_to_thumbnail_queue(state, 1, &app_handle);

    thread::spawn(move || {
        let state = app_handle_clone.state::<AppState>();
        let settings = load_settings(app_handle_clone.clone()).unwrap_or_default();

        let thumb_cache_dir = match resolve_thumbnail_cache_dir(&app_handle_clone) {
            Ok(dir) => dir,
            Err(e) => {
                log::warn!(
                    "Unable to initialize thumbnail cache directory for '{}': {}",
                    path_clone,
                    e
                );
                emit_thumbnail_cache_setup_error(&app_handle_clone, &path_clone, &e);
                increment_thumbnail_progress(&state, &app_handle_clone);
                return;
            }
        };

        let result = generate_single_thumbnail_and_cache(
            &path_clone,
            &thumb_cache_dir,
            gpu_context.as_ref(),
            preloaded_image_option.as_deref(),
            true,
            &app_handle_clone,
            &settings,
        );

        if let Some((small_path, medium_path, rating, is_edited)) = result {
            emit_thumbnail_generated(
                &app_handle_clone,
                &path_clone,
                &small_path,
                &medium_path,
                rating,
                is_edited,
            );
        }

        increment_thumbnail_progress(&state, &app_handle_clone);
    });
}

fn is_xmp_sidecar_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xmp"))
}

fn find_matching_xmp_sidecar(path: &str) -> Option<PathBuf> {
    let (source_path, _) = parse_virtual_path(path);
    resolve_xmp_path(&source_path)
}

fn find_matching_xmp_sidecars_recursive(
    folder: &Path,
) -> (Vec<(PathBuf, PathBuf)>, usize, Vec<String>) {
    let mut matches = Vec::new();
    let mut skipped = 0;
    let mut failures = Vec::new();

    for entry in WalkDir::new(folder) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                failures.push(format!("Failed to inspect folder entry: {}", error));
                continue;
            }
        };

        let path = entry.path();
        if !entry.file_type().is_file() || !is_supported_image_file(path) {
            continue;
        }

        if let Some(xmp_path) = resolve_xmp_path(path) {
            matches.push((path.to_path_buf(), xmp_path));
        } else {
            skipped += 1;
        }
    }

    matches.sort_by(|(left, _), (right, _)| left.cmp(right));
    (matches, skipped, failures)
}

fn read_sidecar_report_snapshot(path: &Path) -> Result<ImageMetadata, String> {
    const MAX_REPORT_SIDECAR_BYTES: usize = 16 * 1024 * 1024;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ImageMetadata::default());
        }
        Err(_) => return Err("Could not read native edits for the import report".into()),
    };
    let metadata = file
        .metadata()
        .map_err(|_| "Could not inspect native edits for the import report")?;
    if !metadata.is_file() || metadata.len() > MAX_REPORT_SIDECAR_BYTES as u64 {
        return Err("Import reports require a regular native sidecar of at most 16 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_REPORT_SIDECAR_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read native edits for the import report")?;
    if bytes.len() > MAX_REPORT_SIDECAR_BYTES {
        return Err("Native sidecar exceeds the import report size limit".into());
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| "Could not parse native edits for the import report".into())
}

fn merge_imported_adjustments(existing: &mut Value, imported: Value) {
    if let (Some(target), Some(source)) = (existing.as_object_mut(), imported.as_object()) {
        for (key, value) in source {
            if let Some(current) = target.get_mut(key) {
                merge_imported_adjustments(current, value.clone());
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    } else {
        *existing = imported;
    }
}

#[cfg(test)]
fn import_xmp_adjustments_to_sidecar(
    path: &str,
    xmp_path: &Path,
    lens_db: Option<&crate::lens_correction::LensDatabase>,
) -> Result<ImportedXmpSidecar, String> {
    import_xmp_adjustments_to_sidecar_with_draft(
        path,
        xmp_path,
        lens_db,
        None,
        &AppSettings::default(),
    )
}

fn import_xmp_adjustments_to_sidecar_with_draft(
    path: &str,
    xmp_path: &Path,
    lens_db: Option<&crate::lens_correction::LensDatabase>,
    pending_adjustments: Option<&Value>,
    settings: &AppSettings,
) -> Result<ImportedXmpSidecar, String> {
    if !is_xmp_sidecar_path(xmp_path) {
        return Err("Selected file is not an XMP sidecar.".to_string());
    }

    let (source_path, sidecar_path) = parse_virtual_path(path);
    ensure_card_writable(&source_path)?;
    let xmp_content = crate::xmp::read(xmp_path)
        .map_err(|error| format!("Failed to read XMP file: {}", error))?;
    let (converted_preset, mask_summary) =
        preset_converter::convert_xmp_sidecar_to_preset_for_image_with_mask_report(
            &xmp_content,
            &source_path,
        )?;
    let already_applied = preset_converter::adobe_camera_raw_already_applied(&xmp_content)?;

    let has_supported_adjustments = converted_preset
        .adjustments
        .as_object()
        .is_some_and(|adjustments| !adjustments.is_empty());
    let has_supported_metadata = extract_xmp_rating(&xmp_content).is_some()
        || extract_xmp_label(&xmp_content).is_some()
        || !extract_xmp_tags(&xmp_content).is_empty();
    let settings_report = crate::lightroom_report::settings_report(&xmp_content, &converted_preset);
    let unchanged = !has_supported_adjustments && !has_supported_metadata;
    let not_transferred = preset_converter::lightroom_settings_not_transferred_with_mask_summary(
        &xmp_content,
        &converted_preset,
        &mask_summary,
    );
    if unchanged
        && !already_applied
        && settings_report.settings.is_empty()
        && not_transferred.is_empty()
        && mask_summary.brush_table_refusals.is_empty()
        && mask_summary.failures.is_empty()
    {
        return Err(NO_SUPPORTED_XMP_CONTENT_ERROR.to_string());
    }
    if unchanged {
        let metadata = read_sidecar_report_snapshot(&sidecar_path)?;
        return Ok(ImportedXmpSidecar {
            sidecar_path,
            metadata,
            not_transferred,
            brush_table_refusals: mask_summary.brush_table_refusals,
            mask_failures: mask_summary.failures,
            adjustments_applied: false,
            metadata_applied: false,
            already_applied,
            draft_preserved: false,
            settings_report,
            unchanged,
        });
    }
    let mut metadata = crate::exif_processing::load_sidecar(&sidecar_path);
    if let Some(pending) = pending_adjustments {
        if !pending.is_object() {
            return Err("Pending editor adjustments must be an object".to_string());
        }
        metadata.adjustments = pending.clone();
    }
    if has_supported_adjustments {
        apply_fresh_raw_defaults(
            &mut metadata,
            &source_path,
            &sidecar_path,
            settings,
            lens_db,
        );
        let before = metadata.adjustments.clone();
        let imported = converted_preset.adjustments;
        merge_imported_adjustments(&mut metadata.adjustments, imported.clone());
        normalize_lens_policy_version(&before, &imported, &mut metadata.adjustments, 100.0);
        let imported_exposure_without_policy = imported.get("exposure").is_some()
            && imported.get("cameraExposureCompensation").is_none();
        if imported_exposure_without_policy
            && metadata
                .adjustments
                .get("cameraExposureCompensation")
                .and_then(Value::as_bool)
                == Some(true)
        {
            metadata.adjustments["cameraExposureCompensation"] = Value::Bool(false);
        }
        resolve_lens_params_in_adjustments(&mut metadata.adjustments, &metadata.exif, lens_db);
    }
    let draft_preserved = !has_supported_adjustments && pending_adjustments.is_some();
    let preserved_adjustments =
        (!has_supported_adjustments).then(|| std::mem::take(&mut metadata.adjustments));
    merge_xmp_metadata_fields(&xmp_content, &mut metadata);
    if let Some(preserved) = preserved_adjustments {
        metadata.adjustments = preserved;
    }

    if has_supported_adjustments || has_supported_metadata || draft_preserved {
        let json_string =
            serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?;
        write_file_atomically(&sidecar_path, json_string).map_err(|error| error.to_string())?;
    }

    Ok(ImportedXmpSidecar {
        sidecar_path,
        metadata,
        not_transferred,
        brush_table_refusals: mask_summary.brush_table_refusals,
        mask_failures: mask_summary.failures,
        adjustments_applied: has_supported_adjustments,
        metadata_applied: has_supported_metadata,
        already_applied,
        draft_preserved,
        settings_report,
        unchanged,
    })
}

fn hydrate_imported_xmp_exif(
    path: &str,
    imported: &mut ImportedXmpSidecar,
    lens_db: Option<&crate::lens_correction::LensDatabase>,
) -> Result<(), String> {
    if !imported.adjustments_applied {
        return Ok(());
    }
    // Keep the single-photo import's existing EXIF/lens resolution without
    // invoking ordinary autosave, which would sync into the incoming XMP.
    let (source_path, _) = parse_virtual_path(path);
    let mut metadata =
        crate::exif_processing::load_sidecar_with_exif(&imported.sidecar_path, &source_path);
    resolve_lens_params_in_adjustments(&mut metadata.adjustments, &metadata.exif, lens_db);
    let json = serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?;
    write_file_atomically(&imported.sidecar_path, json).map_err(|error| error.to_string())?;
    imported.metadata = metadata;
    Ok(())
}

#[tauri::command]
pub fn import_xmp_adjustments_for_image(
    path: String,
    xmp_path: Option<String>,
    pending_adjustments: Option<Value>,
    app_handle: AppHandle,
    state: tauri::State<AppState>,
) -> Result<XmpImageImportResult, String> {
    ensure_card_writable_for_paths(&[&path])?;
    let xmp_path = xmp_path
        .map(PathBuf::from)
        .or_else(|| find_matching_xmp_sidecar(&path))
        .ok_or_else(|| "No matching XMP sidecar was found next to this image.".to_string())?;
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let lens_db = state.lens_db.lock().unwrap().clone();
    let mut imported = import_xmp_adjustments_to_sidecar_with_draft(
        &path,
        &xmp_path,
        lens_db.as_deref(),
        pending_adjustments.as_ref(),
        &settings,
    )?;
    hydrate_imported_xmp_exif(&path, &mut imported, lens_db.as_deref())?;
    let sidecar_path = imported.sidecar_path.clone();
    let not_transferred = imported.not_transferred;
    let brush_table_refusals = imported.brush_table_refusals;
    let mask_failures = imported.mask_failures;
    let adjustments_applied = imported.adjustments_applied;
    let already_applied = imported.already_applied;
    let settings_report = imported.settings_report;
    let unchanged = imported.unchanged;

    let draft_preserved = imported.draft_preserved;
    if adjustments_applied || imported.metadata_applied || draft_preserved {
        queue_import_thumbnail_refresh(path, app_handle, &state);
    }

    log::info!(
        "Imported XMP adjustments from {} to {}",
        xmp_path.display(),
        sidecar_path.display()
    );

    Ok(XmpImageImportResult {
        metadata: imported.metadata,
        not_transferred,
        brush_table_refusals,
        mask_failures,
        adjustments_applied,
        already_applied,
        draft_preserved,
        settings_report,
        unchanged,
    })
}

#[tauri::command]
pub async fn import_matching_xmp_sidecars_in_folder(
    folder_path: String,
    pending_draft: Option<XmpEditorDraft>,
    app_handle: AppHandle,
) -> Result<XmpSidecarImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let folder = PathBuf::from(&folder_path);
        if !folder.is_dir() {
            return Err(format!("Folder not found: {}", folder.display()));
        }
        ensure_card_tree_writable(&folder)?;

        let (matches, skipped, traversal_failures) = find_matching_xmp_sidecars_recursive(&folder);
        let total = matches.len();
        let settings = load_settings(app_handle.clone()).unwrap_or_default();
        let lens_db = app_handle
            .state::<AppState>()
            .lens_db
            .lock()
            .unwrap()
            .clone();

        let mut result = XmpSidecarImportResult {
            matched: total,
            imported: 0,
            unchanged: 0,
            skipped,
            failed: traversal_failures.len(),
            failures: traversal_failures,
            imported_paths: Vec::new(),
            adjustments_imported_paths: Vec::new(),
            already_applied_paths: Vec::new(),
            preserved_draft_paths: Vec::new(),
            unchanged_paths: Vec::new(),
            not_transferred: Vec::new(),
        };

        let emit_progress = |current: usize, result: &XmpSidecarImportResult| {
            let _ = app_handle.emit(
                "xmp-sidecar-import-progress",
                serde_json::json!({
                    "folderPath": folder_path,
                    "current": current,
                    "total": total,
                    "imported": result.imported,
                    "unchanged": result.unchanged,
                    "failed": result.failed,
                }),
            );
        };
        emit_progress(0, &result);

        for (index, (path, xmp_path)) in matches.into_iter().enumerate() {
            let path_string = path.to_string_lossy().to_string();
            let pending = pending_draft
                .as_ref()
                .filter(|draft| draft.path == path_string)
                .map(|draft| &draft.adjustments);
            match import_xmp_adjustments_to_sidecar_with_draft(
                &path_string,
                &xmp_path,
                lens_db.as_deref(),
                pending,
                &settings,
            ) {
                Ok(imported) => {
                    if imported.already_applied {
                        result.already_applied_paths.push(path_string.clone());
                    }
                    if imported.draft_preserved {
                        result.preserved_draft_paths.push(path_string.clone());
                        let state = app_handle.state::<AppState>();
                        queue_import_thumbnail_refresh(
                            path_string.clone(),
                            app_handle.clone(),
                            &state,
                        );
                    }
                    if !imported.adjustments_applied && !imported.metadata_applied {
                        result.unchanged += 1;
                        result.unchanged_paths.push(path_string.clone());
                    } else {
                        result.imported += 1;
                        result.imported_paths.push(path_string.clone());
                        if imported.adjustments_applied {
                            result
                                .adjustments_imported_paths
                                .push(path.to_string_lossy().to_string());
                        }
                    }
                    if !imported.not_transferred.is_empty()
                        || !imported.settings_report.settings.is_empty()
                        || !imported.mask_failures.is_empty()
                    {
                        result.not_transferred.push(XmpNotTransferred {
                            path: path_string.clone(),
                            items: imported.not_transferred,
                            brush_table_refusals: imported.brush_table_refusals,
                            mask_failures: imported.mask_failures,
                            settings_report: imported.settings_report,
                        });
                    }
                }
                Err(error) if error == NO_SUPPORTED_XMP_CONTENT_ERROR => {
                    result.unchanged += 1;
                    result.unchanged_paths.push(path_string);
                    log::debug!(
                        "Skipping unchanged Lightroom XMP sidecar for {}",
                        path.display()
                    );
                }
                Err(error) => {
                    result.failed += 1;
                    log::warn!(
                        "Failed to import Lightroom XMP sidecar {} for {}: {}",
                        xmp_path.display(),
                        path.display(),
                        error
                    );
                    result
                        .failures
                        .push(format!("{}: {}", path.display(), error));
                }
            }
            emit_progress(index + 1, &result);
        }

        if !result.imported_paths.is_empty() {
            let imported_paths = result.imported_paths.clone();
            let app_handle_clone = app_handle.clone();
            let state = app_handle.state::<AppState>();
            add_to_thumbnail_queue(&state, imported_paths.len(), &app_handle);

            thread::spawn(move || {
                let state = app_handle_clone.state::<AppState>();
                let settings = load_settings(app_handle_clone.clone()).unwrap_or_default();
                let thumbnail_cache_dir = match resolve_thumbnail_cache_dir(&app_handle_clone) {
                    Ok(directory) => directory,
                    Err(error) => {
                        log::warn!("Unable to initialize thumbnail cache directory: {}", error);
                        for path in &imported_paths {
                            emit_thumbnail_cache_setup_error(&app_handle_clone, path, &error);
                            increment_thumbnail_progress(&state, &app_handle_clone);
                        }
                        return;
                    }
                };
                let gpu_context =
                    gpu_processing::get_or_init_gpu_context(&state, &app_handle_clone).ok();

                imported_paths.par_iter().for_each(|path| {
                    let generated = generate_single_thumbnail_and_cache(
                        path,
                        &thumbnail_cache_dir,
                        gpu_context.as_ref(),
                        None,
                        true,
                        &app_handle_clone,
                        &settings,
                    );

                    if let Some((small_path, medium_path, rating, is_edited)) = generated {
                        emit_thumbnail_generated(
                            &app_handle_clone,
                            path,
                            &small_path,
                            &medium_path,
                            rating,
                            is_edited,
                        );
                    }

                    increment_thumbnail_progress(&state, &app_handle_clone);
                });
            });
        }

        Ok(result)
    })
    .await
    .unwrap_or_else(|error| Err(format!("Task failed: {}", error)))
}

#[tauri::command]
pub async fn apply_adjustments_to_paths(
    paths: Vec<String>,
    adjustments: Value,
    app_handle: AppHandle,
) -> Result<(), String> {
    ensure_card_writable_for_paths(&paths)?;
    let state = app_handle.state::<AppState>();
    add_to_thumbnail_queue(&state, paths.len(), &app_handle);

    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(app_handle.clone()).unwrap_or_default();
        let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);
        let create_xmp_if_missing = settings.create_xmp_if_missing.unwrap_or(false);

        let lens_db = app_handle
            .state::<AppState>()
            .lens_db
            .lock()
            .unwrap()
            .clone();

        paths.par_iter().for_each(|path| {
            let (source_path, sidecar_path) = parse_virtual_path(path);

            let mut existing_metadata = load_sidecar_with_raw_defaults(
                &sidecar_path,
                &source_path,
                &settings,
                lens_db.as_deref(),
            );
            if existing_metadata.exif.is_none() {
                existing_metadata.exif =
                    crate::exif_processing::load_sidecar_with_exif(&sidecar_path, &source_path)
                        .exif;
            }

            let mut new_adjustments = existing_metadata.adjustments;
            if new_adjustments.is_null() {
                new_adjustments = default_adjustments();
            }
            let before = new_adjustments.clone();

            if let (Some(new_map), Some(pasted_map)) =
                (new_adjustments.as_object_mut(), adjustments.as_object())
            {
                // A pasted lens is a new choice of lens, so its values are
                // resolved afresh instead of keeping the old evaluation.
                if (pasted_map.contains_key("lensMaker") || pasted_map.contains_key("lensModel"))
                    && !pasted_map.contains_key("lensDistortionParams")
                {
                    new_map.remove("lensDistortionParams");
                }
                for (k, v) in pasted_map {
                    new_map.insert(k.clone(), v.clone());
                }
            }

            normalize_lens_policy_version(&before, &adjustments, &mut new_adjustments, 100.0);
            resolve_lens_params_in_adjustments(
                &mut new_adjustments,
                &existing_metadata.exif,
                lens_db.as_deref(),
            );

            existing_metadata.adjustments = new_adjustments;

            if let Ok(json_string) = serde_json::to_string_pretty(&existing_metadata) {
                let _ = write_file_atomically(&sidecar_path, json_string);
            }

            if enable_xmp_sync {
                let source_path = parse_virtual_path(path).0;
                sync_metadata_to_xmp(
                    &source_path,
                    &sidecar_path,
                    &existing_metadata,
                    create_xmp_if_missing,
                );
            }
        });

        regenerate_thumbnails(&paths, &app_handle, &settings);
    });

    Ok(())
}

#[tauri::command]
pub async fn reset_adjustments_for_paths(
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    ensure_card_writable_for_paths(&paths)?;
    let state = app_handle.state::<AppState>();
    add_to_thumbnail_queue(&state, paths.len(), &app_handle);

    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(app_handle.clone()).unwrap_or_default();
        let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);
        let create_xmp_if_missing = settings.create_xmp_if_missing.unwrap_or(false);

        let lens_db = app_handle
            .state::<AppState>()
            .lens_db
            .lock()
            .unwrap()
            .clone();
        paths.par_iter().for_each(|path| {
            let (source_path, sidecar_path) = parse_virtual_path(path);

            let mut existing_metadata = crate::exif_processing::load_sidecar(&sidecar_path);

            existing_metadata.adjustments = default_adjustments();
            apply_default_raw_lens_policy(
                &mut existing_metadata,
                &source_path,
                &sidecar_path,
                &settings,
                lens_db.as_deref(),
            );

            if let Ok(json_string) = serde_json::to_string_pretty(&existing_metadata) {
                let _ = write_file_atomically(&sidecar_path, json_string);
            }

            if enable_xmp_sync {
                let source_path = parse_virtual_path(path).0;
                sync_metadata_to_xmp(
                    &source_path,
                    &sidecar_path,
                    &existing_metadata,
                    create_xmp_if_missing,
                );
            }
        });

        regenerate_thumbnails(&paths, &app_handle, &settings);
    });

    Ok(())
}

fn regenerate_thumbnails(paths: &[String], app_handle: &AppHandle, settings: &AppSettings) {
    let state = app_handle.state::<AppState>();
    let thumb_cache_dir = match resolve_thumbnail_cache_dir(app_handle) {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!("Unable to initialize thumbnail cache directory: {}", e);
            for path in paths {
                emit_thumbnail_cache_setup_error(app_handle, path, &e);
            }
            for _ in 0..paths.len() {
                increment_thumbnail_progress(&state, app_handle);
            }
            return;
        }
    };

    let gpu_context = gpu_processing::get_or_init_gpu_context(&state, app_handle).ok();

    paths.par_iter().for_each(|path_str| {
        let result = generate_single_thumbnail_and_cache(
            path_str,
            &thumb_cache_dir,
            gpu_context.as_ref(),
            None,
            true,
            app_handle,
            settings,
        );

        if let Some((small_path, medium_path, rating, is_edited)) = result {
            emit_thumbnail_generated(
                app_handle,
                path_str,
                &small_path,
                &medium_path,
                rating,
                is_edited,
            );
        }

        increment_thumbnail_progress(&state, app_handle);
    });
}

#[tauri::command]
pub async fn apply_auto_lens_correction_to_paths(
    paths: Vec<String>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    ensure_card_writable_for_paths(&paths)?;
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);
    let create_xmp_if_missing = settings.create_xmp_if_missing.unwrap_or(false);
    let lens_db = app_handle
        .state::<AppState>()
        .lens_db
        .lock()
        .unwrap()
        .clone();

    let sidecar_paths = paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        sidecar_paths.par_iter().for_each(|path| {
            let (source_path, sidecar_path) = parse_virtual_path(path);
            let mut existing_metadata =
                crate::exif_processing::load_sidecar_with_exif(&sidecar_path, &source_path);

            if existing_metadata.adjustments.is_null() {
                existing_metadata.adjustments = default_adjustments();
            }

            if let Some(obj) = existing_metadata.adjustments.as_object_mut() {
                obj.insert("lensCorrectionMode".to_string(), serde_json::json!("auto"));
                obj.insert(
                    "cameraLensCorrectionVersion".to_string(),
                    serde_json::json!(1),
                );
                obj.insert("lensDistortionEnabled".to_string(), serde_json::json!(true));
                obj.insert("lensTcaEnabled".to_string(), serde_json::json!(true));
                obj.insert("lensVignetteEnabled".to_string(), serde_json::json!(true));
                obj.remove("lensDistortionParams");
            }

            resolve_lens_params_in_adjustments(
                &mut existing_metadata.adjustments,
                &existing_metadata.exif,
                lens_db.as_deref(),
            );

            if let Ok(json_string) = serde_json::to_string_pretty(&existing_metadata) {
                let _ = write_file_atomically(&sidecar_path, json_string);
            }

            if enable_xmp_sync {
                sync_metadata_to_xmp(
                    &source_path,
                    &sidecar_path,
                    &existing_metadata,
                    create_xmp_if_missing,
                );
            }
        });
    })
    .await
    .map_err(|e| e.to_string())?;

    add_to_thumbnail_queue(&app_handle.state::<AppState>(), paths.len(), &app_handle);
    tauri::async_runtime::spawn_blocking(move || {
        regenerate_thumbnails(&paths, &app_handle, &settings)
    });

    Ok(())
}

#[tauri::command]
pub async fn apply_auto_adjustments_to_paths(
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    ensure_card_writable_for_paths(&paths)?;
    let state = app_handle.state::<AppState>();
    add_to_thumbnail_queue(&state, paths.len(), &app_handle);

    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings(app_handle.clone()).unwrap_or_default();
        let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);
        let create_xmp_if_missing = settings.create_xmp_if_missing.unwrap_or(false);

        let state = app_handle.state::<AppState>();
        let lens_db = state.lens_db.lock().unwrap().clone();
        let thumb_cache_dir = match resolve_thumbnail_cache_dir(&app_handle) {
            Ok(dir) => dir,
            Err(e) => {
                log::warn!("Unable to initialize thumbnail cache directory: {}", e);
                for path in &paths {
                    emit_thumbnail_cache_setup_error(&app_handle, path, &e);
                }
                for _ in 0..paths.len() {
                    increment_thumbnail_progress(&state, &app_handle);
                }
                return;
            }
        };

        let gpu_context = gpu_processing::get_or_init_gpu_context(&state, &app_handle).ok();

        paths.par_iter().for_each(|path| {
            let loaded_image: Option<DynamicImage> =
                (|| -> Result<Option<DynamicImage>, String> {
                    let (source_path, sidecar_path) = parse_virtual_path(path);
                    let source_path_str = source_path.to_string_lossy().to_string();

                    let file_bytes = fs::read(&source_path).map_err(|e| e.to_string())?;
                    let image = image_loader::load_base_image_from_bytes(
                        &file_bytes,
                        &source_path_str,
                        true,
                        &settings,
                        None,
                    )
                    .map_err(|e| e.to_string())?;

                    let mut existing_metadata = crate::exif_processing::load_sidecar(&sidecar_path);

                    if existing_metadata.adjustments.is_null() {
                        existing_metadata.adjustments = default_adjustments();
                        crate::raw_processing::apply_camera_crop_default_from_path(
                            &mut existing_metadata.adjustments,
                            &source_path,
                        );
                        apply_default_raw_lens_policy(
                            &mut existing_metadata,
                            &source_path,
                            &sidecar_path,
                            &settings,
                            lens_db.as_deref(),
                        );
                    }

                    let auto_results = perform_auto_analysis(
                        &image,
                        effective_baseline_exposure(
                            &existing_metadata.adjustments,
                            as_shot(&source_path_str),
                            BaselineOptions::from_settings(&settings),
                        ),
                    );
                    let auto_adjustments_json = auto_results_to_json(&auto_results);

                    let mut active_tools =
                        crate::image_processing::active_tools(&existing_metadata.adjustments);

                    if let (Some(existing_map), Some(auto_map)) = (
                        existing_metadata.adjustments.as_object_mut(),
                        auto_adjustments_json.as_object(),
                    ) {
                        for (k, v) in auto_map {
                            if k == "activeTools" {
                                if let Some(auto_tools) = v.as_object() {
                                    active_tools.extend(auto_tools.clone());
                                }
                                existing_map.insert(
                                    k.clone(),
                                    serde_json::Value::Object(std::mem::take(&mut active_tools)),
                                );
                            } else {
                                existing_map.insert(k.clone(), v.clone());
                            }
                        }
                    }

                    if let Ok(json_string) = serde_json::to_string_pretty(&existing_metadata) {
                        let _ = write_file_atomically(&sidecar_path, json_string);
                    }

                    if enable_xmp_sync {
                        sync_metadata_to_xmp(
                            &source_path,
                            &sidecar_path,
                            &existing_metadata,
                            create_xmp_if_missing,
                        );
                    }
                    // Auto analysis keeps the fast base, but native repair coordinates
                    // require a fresh full-resolution base for the thumbnail.
                    Ok((!has_ai_patches(&existing_metadata.adjustments)).then_some(image))
                })()
                .map_err(|e| eprintln!("Failed to apply auto adjustments to {}: {}", path, e))
                .ok()
                .flatten();

            let result = generate_single_thumbnail_and_cache(
                path,
                &thumb_cache_dir,
                gpu_context.as_ref(),
                loaded_image.as_ref(),
                true,
                &app_handle,
                &settings,
            );

            if let Some((small_path, medium_path, rating, is_edited)) = result {
                emit_thumbnail_generated(
                    &app_handle,
                    path,
                    &small_path,
                    &medium_path,
                    rating,
                    is_edited,
                );
            }

            increment_thumbnail_progress(&state, &app_handle);
        });
    });

    Ok(())
}

pub(crate) fn update_sidecar(
    sidecar_path: &Path,
    update: impl Fn(&mut ImageMetadata),
) -> Result<ImageMetadata, String> {
    let _lock = SIDECAR_METADATA_LOCK.lock().map_err(|e| e.to_string())?;
    let mut metadata = crate::exif_processing::load_sidecar_unlocked(sidecar_path);
    update(&mut metadata);
    let json_string = serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?;
    write_file_atomically(sidecar_path, json_string).map_err(|error| {
        format!(
            "Failed to save metadata to {}: {error}",
            sidecar_path.display()
        )
    })?;
    Ok(metadata)
}

fn update_metadata_for_paths(
    paths: &[String],
    app_handle: &AppHandle,
    update: impl Fn(&mut ImageMetadata) + Sync,
) -> Result<(), String> {
    ensure_card_writable_for_paths(paths)?;
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let xmp_sync = settings
        .enable_xmp_sync
        .unwrap_or(false)
        .then(|| settings.create_xmp_if_missing.unwrap_or(false));
    update_metadata(paths, xmp_sync, update)
}

/// `xmp_sync` is `Some(create_if_missing)` when XMP sync is on.
fn update_metadata(
    paths: &[String],
    xmp_sync: Option<bool>,
    update: impl Fn(&mut ImageMetadata) + Sync,
) -> Result<(), String> {
    ensure_card_writable_for_paths(paths)?;
    paths.par_iter().try_for_each(|path| {
        let (source_path, sidecar_path) = parse_virtual_path(path);

        let metadata = update_sidecar(&sidecar_path, &update)?;

        if let Some(create_xmp_if_missing) = xmp_sync {
            sync_metadata_to_xmp(
                &source_path,
                &sidecar_path,
                &metadata,
                create_xmp_if_missing,
            );
        }
        Ok(())
    })
}

#[tauri::command]
pub fn set_color_label_for_paths(
    paths: Vec<String>,
    color: Option<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    update_metadata_for_paths(&paths, &app_handle, |metadata| {
        let mut tags = metadata.tags.take().unwrap_or_default();
        tags.retain(|tag| !tag.starts_with(COLOR_TAG_PREFIX));

        if let Some(c) = &color
            && !c.is_empty()
        {
            tags.push(format!("{}{}", COLOR_TAG_PREFIX, c));
        }

        if !tags.is_empty() {
            metadata.tags = Some(tags);
        }
    })
}

fn apply_user_rating(metadata: &mut ImageMetadata, rating: u8) {
    metadata.rating = rating;
    metadata.rating_is_explicit = true;
    if rating > 0 && metadata.flag == Some(ImageFlag::Reject) {
        apply_user_flag(metadata, None);
    }
}

/// Like `rating_is_explicit`: a flag set or removed in RapidRoom wins over a
/// reject in the .xmp, so a removed reject doesn't come back from XMP sync.
fn apply_user_flag(metadata: &mut ImageMetadata, flag: Option<ImageFlag>) {
    metadata.flag = flag;
    metadata.flag_is_explicit = true;
}

#[cfg(test)]
fn store_user_rating(sidecar_path: &Path, rating: u8) -> ImageMetadata {
    update_sidecar(sidecar_path, |metadata| apply_user_rating(metadata, rating)).unwrap()
}

#[tauri::command]
pub fn set_rating_for_paths(
    paths: Vec<String>,
    rating: u8,
    apply_to_stack: Option<bool>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let paths = if apply_to_stack.unwrap_or(false) {
        crate::photo_stacks::metadata_paths(&paths)?
    } else {
        paths
    };
    update_metadata_for_paths(&paths, &app_handle, |metadata| {
        apply_user_rating(metadata, rating)
    })
}

#[tauri::command]
pub fn set_flag_for_paths(
    paths: Vec<String>,
    flag: Option<ImageFlag>,
    apply_to_stack: Option<bool>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let paths = if apply_to_stack.unwrap_or(false) {
        crate::photo_stacks::metadata_paths(&paths)?
    } else {
        paths
    };
    update_metadata_for_paths(&paths, &app_handle, |metadata| {
        apply_user_flag(metadata, flag)
    })
}

pub(crate) fn normalize_lens_policy_version(
    target: &Value,
    source: &Value,
    merged: &mut Value,
    amount: f64,
) {
    let Some(map) = merged.as_object_mut() else {
        return;
    };
    let replacing = source
        .as_object()
        .is_some_and(|source| source.contains_key("lensCorrectionMode"))
        && amount >= 100.0;
    let version = if replacing { source } else { target }.get("cameraLensCorrectionVersion");
    if version.and_then(Value::as_u64) == Some(1) {
        map.insert("cameraLensCorrectionVersion".into(), serde_json::json!(1));
    } else if target.get("cameraLensCorrectionVersion").is_some() {
        map.insert("cameraLensCorrectionVersion".into(), serde_json::json!(0));
    } else {
        map.remove("cameraLensCorrectionVersion");
    }
}

pub(crate) fn apply_default_raw_lens_policy(
    metadata: &mut ImageMetadata,
    source_path: &Path,
    sidecar_path: &Path,
    settings: &AppSettings,
    lens_db: Option<&crate::lens_correction::LensDatabase>,
) {
    if !crate::formats::is_raw_file(source_path)
        || !settings.auto_lens_correction_on_new_raws.unwrap_or(true)
    {
        return;
    }
    if metadata.exif.is_none() {
        metadata.exif =
            crate::exif_processing::load_sidecar_with_exif(sidecar_path, source_path).exif;
    }
    if !metadata.adjustments.is_object() {
        metadata.adjustments = default_adjustments();
    }
    metadata.adjustments["lensCorrectionMode"] = serde_json::json!("auto");
    metadata.adjustments["cameraLensCorrectionVersion"] = serde_json::json!(1);
    resolve_lens_params_in_adjustments(&mut metadata.adjustments, &metadata.exif, lens_db);
}

pub(crate) fn apply_fresh_raw_defaults(
    metadata: &mut ImageMetadata,
    source_path: &Path,
    sidecar_path: &Path,
    settings: &AppSettings,
    lens_db: Option<&crate::lens_correction::LensDatabase>,
) {
    if metadata.adjustments.is_null() {
        metadata.adjustments = default_adjustments();
        crate::raw_processing::apply_camera_crop_default_from_path(
            &mut metadata.adjustments,
            source_path,
        );
        apply_default_raw_lens_policy(metadata, source_path, sidecar_path, settings, lens_db);
    }
}

pub(crate) fn load_sidecar_with_raw_defaults(
    sidecar_path: &Path,
    source_path: &Path,
    settings: &AppSettings,
    lens_db: Option<&crate::lens_correction::LensDatabase>,
) -> ImageMetadata {
    let mut metadata = crate::exif_processing::load_sidecar(sidecar_path);
    apply_fresh_raw_defaults(&mut metadata, source_path, sidecar_path, settings, lens_db);
    metadata
}

#[tauri::command]
pub async fn get_default_raw_lens_adjustments(
    path: String,
    app_handle: AppHandle,
) -> Result<Value, String> {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let lens_db = app_handle
        .state::<AppState>()
        .lens_db
        .lock()
        .unwrap()
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (source_path, sidecar_path) = parse_virtual_path(&path);
        let mut metadata = ImageMetadata::default();
        apply_default_raw_lens_policy(
            &mut metadata,
            &source_path,
            &sidecar_path,
            &settings,
            lens_db.as_deref(),
        );
        if let Some(patch) = metadata.adjustments.as_object_mut() {
            patch.retain(|key, _| key.starts_with("lens") || key == "cameraLensCorrectionVersion");
            metadata.adjustments
        } else {
            serde_json::json!({})
        }
    })
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn load_metadata(path: String, app_handle: AppHandle) -> Result<ImageMetadata, String> {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let enable_xmp_sync = settings.enable_xmp_sync.unwrap_or(false);

    let (source_path, sidecar_path) = parse_virtual_path(&path);
    let lens_db = app_handle
        .state::<AppState>()
        .lens_db
        .lock()
        .unwrap()
        .clone();
    let mut metadata =
        load_sidecar_with_raw_defaults(&sidecar_path, &source_path, &settings, lens_db.as_deref());

    if enable_xmp_sync
        && sync_metadata_from_xmp(&source_path, &sidecar_path, &mut metadata)
        && let Ok(json) = serde_json::to_string_pretty(&metadata)
    {
        let _ = write_file_atomically(&sidecar_path, json);
    }

    Ok(metadata)
}

fn get_presets_path(app_handle: &AppHandle) -> Result<std::path::PathBuf, String> {
    let presets_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("presets");

    if !presets_dir.exists() {
        fs::create_dir_all(&presets_dir).map_err(|e| e.to_string())?;
    }

    Ok(presets_dir.join("presets.json"))
}

#[tauri::command]
pub fn load_presets(app_handle: AppHandle) -> Result<Vec<PresetItem>, String> {
    let path = get_presets_path(&app_handle)?;
    let marker = path.with_file_name(".rapidroom-styles-v1");
    let replacement_marker = path.with_file_name(".rapidroom-styles-replacement-v1");
    let mut presets = if path.exists() {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())?
    } else if !marker.exists() {
        crate::starter_presets::library()?
    } else {
        Vec::new()
    };
    if !marker.exists() || !replacement_marker.exists() {
        if !replacement_marker.exists() {
            crate::starter_presets::retire_untouched_stock(&mut presets)?;
        }
        if !marker.exists() {
            crate::starter_presets::include_styles(&mut presets)?;
        }
        // Persist before either marker; an existing Styles marker also protects
        // a user's deletion when retiring the old stock pack on upgrade.
        let content = serde_json::to_string_pretty(&presets).map_err(|e| e.to_string())?;
        write_file_atomically(&path, content).map_err(|e| e.to_string())?;
        write_file_atomically(&replacement_marker, "1\n").map_err(|e| e.to_string())?;
        write_file_atomically(&marker, "1\n").map_err(|e| e.to_string())?;
    }
    Ok(presets)
}

#[tauri::command]
pub fn save_presets(presets: Vec<PresetItem>, app_handle: AppHandle) -> Result<(), String> {
    let path = get_presets_path(&app_handle)?;
    let json_string = serde_json::to_string_pretty(&presets).map_err(|e| e.to_string())?;
    fs::write(path, json_string).map_err(|e| e.to_string())
}

fn get_internal_library_root_path(app_handle: &AppHandle) -> Result<std::path::PathBuf, String> {
    #[cfg(not(target_os = "android"))]
    {
        let library_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("library");

        if !library_dir.exists() {
            fs::create_dir_all(&library_dir).map_err(|e| e.to_string())?;
        }
        Ok(library_dir)
    }
    #[cfg(target_os = "android")]
    {
        crate::android_integration::get_android_internal_library_root()
    }
}

#[tauri::command]
pub fn get_or_create_internal_library_root(app_handle: AppHandle) -> Result<String, String> {
    let library_root = get_internal_library_root_path(&app_handle)?;

    Ok(library_root.to_string_lossy().to_string())
}

fn preset_file_display_name(file_path: &str) -> String {
    Path::new(file_path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string())
}

fn collect_top_level_preset_names(items: &[PresetItem]) -> HashSet<String> {
    items
        .iter()
        .map(|item| match item {
            PresetItem::Preset(p) => p.name.clone(),
            PresetItem::Folder(f) => f.name.clone(),
        })
        .collect()
}

fn parse_preset_file(file_path: &str) -> Result<(Vec<PresetItem>, Vec<String>), String> {
    let lower_path = file_path.to_lowercase();
    let is_legacy = lower_path.ends_with(".xmp") || lower_path.ends_with(".lrtemplate");

    if !is_legacy {
        let content = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read preset file: {}", e))?;
        let preset_file: PresetFile = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse preset file: {}", e))?;
        return Ok((preset_file.presets, Vec::new()));
    }

    let content = String::from_utf8(crate::xmp::read_bytes(Path::new(file_path))?)
        .map_err(|e| format!("Failed to read legacy preset file: {}", e))?;

    let mut not_imported = Vec::new();
    let xmp_content = if lower_path.ends_with(".lrtemplate") {
        if let Some(caps) = regex!(r#"(?s)s.xmp = "(.*)""#).captures(&content) {
            caps.get(1)
                .map(|m| m.as_str().replace(r#"\""#, r#"""#))
                .unwrap_or(content)
        } else {
            let converted = lrtemplate::lrtemplate_to_xmp(&content)?;
            not_imported = converted.unsupported;
            converted.xmp
        }
    } else {
        content
    };

    let mut converted_preset = preset_converter::convert_xmp_to_preset(&xmp_content)?;
    if let Some(reason) = crate::preset_xmp::unavailable_reason(&xmp_content)? {
        converted_preset.unavailable_reason = Some(reason);
    }
    let exact = crate::preset_xmp::exact_adjustments(&xmp_content)?;
    let is_exact = exact.is_some();
    if let Some(exact) = exact {
        converted_preset.adjustments = exact;
    }
    for item in
        preset_converter::lightroom_settings_not_transferred(&xmp_content, &converted_preset)
    {
        if !not_imported.iter().any(|existing| existing == item) {
            not_imported.push(item.to_string());
        }
    }
    if is_exact {
        not_imported.clear();
    }
    if !not_imported.is_empty() && converted_preset.unavailable_reason.is_none() {
        converted_preset.unavailable_reason = Some(format!(
            "Settings not supported: {}",
            not_imported.join(", ")
        ));
    }
    let mut warnings = if not_imported.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "Settings not imported: {}",
            not_imported.join(", ")
        )]
    };
    if !is_exact {
        warnings.extend(crate::lightroom_report::warning_lines(
            &crate::lightroom_report::settings_report(&xmp_content, &converted_preset),
        ));
    }
    let item = if let Some(name) = crate::preset_xmp::group_name(&xmp_content) {
        PresetItem::Folder(PresetFolder {
            id: format!("xmp-group:{name}"),
            name,
            children: vec![converted_preset],
        })
    } else {
        PresetItem::Preset(converted_preset)
    };
    Ok((vec![item], warnings))
}

fn merge_imported_items(
    target: &mut Vec<PresetItem>,
    taken_names: &mut HashSet<String>,
    imported: Vec<PresetItem>,
) {
    for mut imported_item in imported {
        if let PresetItem::Folder(incoming) = &mut imported_item
            && incoming.id.starts_with("xmp-group:")
            && let Some(PresetItem::Folder(existing)) = target
                .iter_mut()
                .find(|item| matches!(item, PresetItem::Folder(f) if f.name == incoming.name))
        {
            let mut names: HashSet<String> =
                existing.children.iter().map(|p| p.name.clone()).collect();
            for mut preset in incoming.children.drain(..) {
                preset.id = Uuid::new_v4().to_string();
                let name = preset.name.clone();
                let mut suffix = 1;
                while names.contains(&preset.name) {
                    preset.name = format!("{name} ({suffix})");
                    suffix += 1;
                }
                names.insert(preset.name.clone());
                existing.children.push(preset);
            }
            continue;
        }
        let original_name = match &mut imported_item {
            PresetItem::Preset(p) => {
                p.id = Uuid::new_v4().to_string();
                p.name.clone()
            }
            PresetItem::Folder(f) => {
                f.id = Uuid::new_v4().to_string();
                for child in &mut f.children {
                    child.id = Uuid::new_v4().to_string();
                }
                f.name.clone()
            }
        };

        let mut new_name = original_name.clone();
        let mut counter = 1;
        while taken_names.contains(&new_name) {
            new_name = format!("{} ({})", original_name, counter);
            counter += 1;
        }

        match &mut imported_item {
            PresetItem::Preset(p) => p.name = new_name.clone(),
            PresetItem::Folder(f) => f.name = new_name.clone(),
        }

        taken_names.insert(new_name);
        target.push(imported_item);
    }
}

fn import_preset_file_into_library(
    file_path: &str,
    app_handle: AppHandle,
) -> Result<Vec<PresetItem>, String> {
    let (imported, warnings) = parse_preset_file(file_path)?;
    for warning in warnings {
        log::warn!("{}: {}", preset_file_display_name(file_path), warning);
    }

    let mut current_presets = load_presets(app_handle.clone())?;
    let mut taken_names = collect_top_level_preset_names(&current_presets);
    merge_imported_items(&mut current_presets, &mut taken_names, imported);

    save_presets(current_presets.clone(), app_handle)?;
    Ok(current_presets)
}

#[tauri::command]
pub fn handle_import_presets_from_file(
    file_path: String,
    app_handle: AppHandle,
) -> Result<Vec<PresetItem>, String> {
    import_preset_file_into_library(&file_path, app_handle)
}

#[tauri::command]
pub fn handle_import_legacy_presets_from_file(
    file_path: String,
    app_handle: AppHandle,
) -> Result<Vec<PresetItem>, String> {
    import_preset_file_into_library(&file_path, app_handle)
}

#[tauri::command]
pub fn handle_import_presets_from_files(
    file_paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<PresetImportResult, String> {
    let mut current_presets = load_presets(app_handle.clone())?;
    let mut taken_names = collect_top_level_preset_names(&current_presets);

    let mut failures: Vec<PresetImportFailure> = Vec::new();
    let mut warnings: Vec<PresetImportWarning> = Vec::new();
    let mut library_changed = false;

    for file_path in &file_paths {
        match parse_preset_file(file_path) {
            Ok((imported, file_warnings)) => {
                warnings.extend(
                    file_warnings
                        .into_iter()
                        .map(|message| PresetImportWarning {
                            file_name: preset_file_display_name(file_path),
                            message,
                        }),
                );
                library_changed |= !imported.is_empty();
                merge_imported_items(&mut current_presets, &mut taken_names, imported);
            }
            Err(error) => failures.push(PresetImportFailure {
                file_name: preset_file_display_name(file_path),
                error,
            }),
        }
    }

    if library_changed {
        save_presets(current_presets.clone(), app_handle)?;
    }

    Ok(PresetImportResult {
        presets: current_presets,
        failures,
        warnings,
    })
}

#[tauri::command]
pub fn handle_export_presets_to_file(
    presets_to_export: Vec<PresetItem>,
    file_path: String,
) -> Result<(), String> {
    ensure_card_writable(Path::new(&file_path))?;
    let preset_file = ExportPresetFile {
        creator: "Anonymous",
        presets: &presets_to_export,
    };

    let json_string = serde_json::to_string_pretty(&preset_file)
        .map_err(|e| format!("Failed to serialize presets: {}", e))?;
    fs::write(file_path, json_string).map_err(|e| format!("Failed to write preset file: {}", e))
}

#[tauri::command]
pub fn export_lightroom_preset(preset: Preset, file_path: String) -> Result<Vec<String>, String> {
    ensure_card_writable(Path::new(&file_path))?;
    let (xml, unsupported) = crate::preset_xmp::serialize(&preset)?;
    fs::write(file_path, xml).map_err(|e| e.to_string())?;
    Ok(unsupported)
}

#[tauri::command]
pub fn save_community_preset(
    name: String,
    adjustments: Value,
    app_handle: AppHandle,
    include_masks: Option<bool>,
    include_crop_transform: Option<bool>,
    preset_type: Option<String>,
) -> Result<(), String> {
    let mut current_presets = load_presets(app_handle.clone())?;

    let community_folder_name = "Community";
    let community_folder_id = match current_presets.iter_mut().find(|item| {
        if let PresetItem::Folder(f) = item {
            f.name == community_folder_name
        } else {
            false
        }
    }) {
        Some(PresetItem::Folder(folder)) => folder.id.clone(),
        _ => {
            let new_folder_id = Uuid::new_v4().to_string();
            let new_folder = PresetItem::Folder(PresetFolder {
                id: new_folder_id.clone(),
                name: community_folder_name.to_string(),
                children: Vec::new(),
            });
            current_presets.insert(0, new_folder);
            new_folder_id
        }
    };

    let new_preset = Preset {
        id: Uuid::new_v4().to_string(),
        name,
        description: None,
        adjustments,
        include_masks,
        include_crop_transform,
        preset_type: preset_type.or(Some("style".to_string())),
        favorite: None,
        camera_model_restriction: None,
        unavailable_reason: None,
    };

    if let Some(PresetItem::Folder(folder)) = current_presets.iter_mut().find(|item| {
        if let PresetItem::Folder(f) = item {
            f.id == community_folder_id
        } else {
            false
        }
    }) {
        folder.children.retain(|p| p.name != new_preset.name);
        folder.children.push(new_preset);
    }

    save_presets(current_presets, app_handle)
}

#[tauri::command]
pub fn clear_all_sidecars(root_path: String) -> Result<usize, String> {
    ensure_card_writable(Path::new(&root_path))?;
    if !Path::new(&root_path).exists() {
        return Err(format!("Root path does not exist: {}", root_path));
    }

    let mut deleted_count = 0;
    let walker = WalkDir::new(root_path).into_iter();

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file()
            && let Some(extension) = path.extension()
            && (extension == "rrdata" || extension == "rrexif")
            && !is_card_read_only_path(path)
        {
            if fs::remove_file(path).is_ok() {
                deleted_count += 1;
            } else {
                eprintln!("Failed to delete sidecar file: {:?}", path);
            }
        }
    }

    Ok(deleted_count)
}

#[derive(Serialize)]
pub struct ThumbnailCacheUsage {
    bytes: u64,
    files: u64,
}

#[tauri::command]
pub async fn get_thumbnail_cache_usage(
    app_handle: AppHandle,
) -> Result<ThumbnailCacheUsage, String> {
    let directory = app_handle
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("thumbnails");
    tauri::async_runtime::spawn_blocking(move || {
        let mut usage = ThumbnailCacheUsage { bytes: 0, files: 0 };
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(usage),
            Err(error) => return Err(error.to_string()),
        };
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let metadata = match fs::symlink_metadata(entry.path()) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            if metadata.is_file() {
                usage.bytes = usage.bytes.saturating_add(metadata.len());
                usage.files += 1;
            }
        }
        Ok(usage)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn clear_thumbnail_cache(app_handle: AppHandle) -> Result<(), String> {
    let cache_dir = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?;
    let thumb_cache_dir = cache_dir.join("thumbnails");

    if thumb_cache_dir.exists() {
        fs::remove_dir_all(&thumb_cache_dir)
            .map_err(|e| format!("Failed to remove thumbnail cache: {}", e))?;
    }

    fs::create_dir_all(&thumb_cache_dir)
        .map_err(|e| format!("Failed to recreate thumbnail cache directory: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn instagram_export_folder(
    folder: Option<String>,
    create: bool,
    app_handle: AppHandle,
) -> Result<String, String> {
    let path = if let Some(folder) = folder.filter(|s| !s.trim().is_empty()) {
        PathBuf::from(folder)
    } else {
        app_handle
            .path()
            .picture_dir()
            .map_err(|e| e.to_string())?
            .join("Instagram")
            .join(chrono::Local::now().format("%Y-%m-%d").to_string())
    };
    if !path.is_absolute() {
        return Err("Export folder must be an absolute path".into());
    }
    if create {
        ensure_card_writable(&path)?;
        fs::create_dir_all(&path).map_err(|e| format!("Could not create export folder: {e}"))?;
    }
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn open_export_folder(path: String) -> Result<(), String> {
    let path = Path::new(&path);
    if !path.is_dir() {
        return Err("Export folder no longer exists".into());
    }
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        command.arg(path).spawn().map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err("Opening an export folder is unavailable on this platform".into())
    }
}

#[tauri::command]
pub fn show_in_finder(path: String) -> Result<(), String> {
    let (source_path, _) = parse_virtual_path(&path);

    #[cfg(target_os = "windows")]
    {
        let source_path_str = source_path.to_string_lossy().to_string();
        Command::new("explorer")
            .args(["/select,", &source_path_str])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        let source_path_str = source_path.to_string_lossy().to_string();
        Command::new("open")
            .args(["-R", &source_path_str])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = source_path.parent() {
            Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            return Err("Could not get parent directory".into());
        }
    }

    #[cfg(target_os = "android")]
    {
        return Err("Show in File Manager is not natively supported via CLI on Android.".into());
    }

    #[cfg(target_os = "ios")]
    {
        return Err("Show in File Manager is not supported on iOS.".into());
    }

    Ok(())
}

#[tauri::command]
pub fn delete_files_from_disk(paths: Vec<String>, app_handle: AppHandle) -> Result<(), String> {
    let (final_paths_to_delete, deletions) = plan_file_deletions(paths)?;
    if final_paths_to_delete.is_empty() {
        return Ok(());
    }

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    if let Err(trash_error) = trash::delete_all(&final_paths_to_delete) {
        log::warn!(
            "Failed to move files to trash: {}. Falling back to permanent delete.",
            trash_error
        );
        for path in final_paths_to_delete {
            if path.is_file() {
                if let Err(e) = fs::remove_file(&path) {
                    log::warn!("Failed to delete file {}: {}", path.display(), e);
                }
            } else if path.is_dir()
                && let Err(e) = fs::remove_dir_all(&path)
            {
                log::warn!("Failed to delete directory {}: {}", path.display(), e);
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    for path in final_paths_to_delete {
        if path.is_file() {
            if let Err(e) = fs::remove_file(&path) {
                log::warn!("Failed to delete file {}: {}", path.display(), e);
            }
        } else if path.is_dir() {
            if let Err(e) = fs::remove_dir_all(&path) {
                log::warn!("Failed to delete directory {}: {}", path.display(), e);
            }
        }
    }

    sync_album_path_changes(&app_handle, None, Some(&deletions), None);

    Ok(())
}

type DeletionPlan = (Vec<PathBuf>, HashSet<String>);

fn plan_file_deletions(paths: Vec<String>) -> Result<DeletionPlan, String> {
    ensure_card_writable_for_paths(&paths)?;
    let mut files_to_trash = HashSet::new();
    let mut deletions = HashSet::new();

    for path_str in paths {
        let (source_path, sidecar_path) = parse_virtual_path(&path_str);
        deletions.insert(path_str.clone());

        if path_str.contains("?vc=") {
            if sidecar_path.exists() {
                files_to_trash.insert(sidecar_path);
            }
        } else {
            if source_path.exists() {
                match find_all_associated_files(&source_path) {
                    Ok(associated_files) => {
                        for file in associated_files {
                            files_to_trash.insert(file);
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Could not find associated files for {}: {}",
                            source_path.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    Ok((files_to_trash.into_iter().collect(), deletions))
}

fn deletion_stem_for(filename: &str) -> Option<&str> {
    let image_filename = if filename.ends_with(".rrdata") {
        let without_rrdata = filename.trim_end_matches(".rrdata");
        if let Some(dot_pos) = without_rrdata.rfind('.') {
            let suffix = &without_rrdata[dot_pos + 1..];
            if suffix.len() == 6 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
                &without_rrdata[..dot_pos]
            } else {
                without_rrdata
            }
        } else {
            without_rrdata
        }
    } else if filename.ends_with(".rrexif") {
        filename.trim_end_matches(".rrexif")
    } else if is_supported_image_file(filename) {
        filename
    } else {
        return None;
    };
    Path::new(image_filename)
        .file_stem()
        .and_then(|s| s.to_str())
}

#[tauri::command]
pub fn delete_files_with_associated(
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let (final_paths_to_delete, deletions) = plan_associated_deletions(&paths)?;
    if final_paths_to_delete.is_empty() {
        return Ok(());
    }

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    if let Err(trash_error) = trash::delete_all(&final_paths_to_delete) {
        log::warn!(
            "Failed to move files to trash: {}. Falling back to permanent delete.",
            trash_error
        );
        for path in final_paths_to_delete {
            if path.is_file()
                && let Err(e) = fs::remove_file(&path)
            {
                log::warn!("Failed to delete file {}: {}", path.display(), e);
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    for path in final_paths_to_delete {
        if path.is_file() {
            if let Err(e) = fs::remove_file(&path) {
                log::warn!("Failed to delete file {}: {}", path.display(), e);
            }
        }
    }

    sync_album_path_changes(&app_handle, None, Some(&deletions), None);

    Ok(())
}

fn plan_associated_deletions(paths: &[String]) -> Result<DeletionPlan, String> {
    ensure_card_writable_for_paths(paths)?;

    let mut stems_to_delete = HashSet::new();
    let mut parent_dirs = HashSet::new();
    let mut deletions = HashSet::new();

    for path_str in paths {
        deletions.insert(path_str.clone());
        let (source_path, _) = parse_virtual_path(path_str);
        if let Some(stem) = source_path.file_stem().and_then(|s| s.to_str()) {
            stems_to_delete.insert(stem.to_string());
        }
        if let Some(parent) = source_path.parent() {
            parent_dirs.insert(parent.to_path_buf());
        }
    }

    if stems_to_delete.is_empty() {
        return Ok((Vec::new(), deletions));
    }

    let mut files_to_trash = HashSet::new();

    for parent_dir in parent_dirs {
        if let Ok(entries) = fs::read_dir(parent_dir) {
            for entry in entries.filter_map(Result::ok) {
                let entry_path = entry.path();
                if !entry_path.is_file() {
                    continue;
                }

                let entry_filename = entry.file_name();
                let entry_filename_str = entry_filename.to_string_lossy();

                if let Some(stem) = deletion_stem_for(&entry_filename_str)
                    && stems_to_delete.contains(stem)
                {
                    files_to_trash.insert(entry_path);
                }
            }
        }
    }

    Ok((files_to_trash.into_iter().collect(), deletions))
}

pub fn get_thumb_cache_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let cache_dir = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?;
    let thumb_cache_dir = cache_dir.join("thumbnails");
    if !thumb_cache_dir.exists() {
        fs::create_dir_all(&thumb_cache_dir).map_err(|e| e.to_string())?;
    }
    Ok(thumb_cache_dir)
}

pub fn get_cache_key_hash(path_str: &str) -> Option<String> {
    let (_, sidecar_path) = parse_virtual_path(path_str);
    compute_thumbnail_cache_hash(path_str, &thumbnail_adjustment_bytes(&sidecar_path))
}

pub fn get_cached_or_generate_thumbnail_image(
    path_str: &str,
    app_handle: &AppHandle,
    gpu_context: Option<&GpuContext>,
) -> Result<DynamicImage> {
    let thumb_cache_dir = get_thumb_cache_dir(app_handle).map_err(|e| anyhow::anyhow!(e))?;
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let target_width_small = settings.small_thumbnail_resolution.unwrap_or(480);
    let target_width_medium = settings.medium_thumbnail_resolution.unwrap_or(1280);

    if let Some(cache_hash) = get_cache_key_hash(path_str) {
        let cache_path = thumb_cache_dir.join(format!("{}_medium.jpg", cache_hash));

        if cache_path.exists() {
            if let Ok(image) = image::open(&cache_path) {
                return Ok(image);
            }
            eprintln!(
                "Could not open cached thumbnail, regenerating: {:?}",
                cache_path
            );
        }

        let thumb_image = generate_thumbnail_data(path_str, gpu_context, None, app_handle)?;
        if let (Ok(small_data), Ok(medium_data)) = (
            encode_thumbnail(&thumb_image, target_width_small),
            encode_thumbnail(&thumb_image, target_width_medium),
        ) {
            let _ = fs::write(
                thumb_cache_dir.join(format!("{}_small.jpg", cache_hash)),
                &small_data,
            );
            let _ = fs::write(
                thumb_cache_dir.join(format!("{}_medium.jpg", cache_hash)),
                &medium_data,
            );
        }

        Ok(thumb_image)
    } else {
        generate_thumbnail_data(path_str, gpu_context, None, app_handle)
    }
}

fn ensure_import_writable(
    source_paths: &[String],
    destination_folder: &str,
    delete_after_import: bool,
) -> Result<(), String> {
    ensure_card_writable(Path::new(destination_folder))?;
    if delete_after_import {
        ensure_card_writable_for_paths(source_paths)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn import_files(
    source_paths: Vec<String>,
    destination_folder: String,
    settings: ImportSettings,
    app_handle: AppHandle,
) -> Result<(), String> {
    ensure_import_writable(
        &source_paths,
        &destination_folder,
        settings.delete_after_import,
    )?;
    let total_files = source_paths.len();
    let _ = app_handle.emit("import-start", serde_json::json!({ "total": total_files }));

    tauri::async_runtime::spawn_blocking(move || {
        let mut summary = crate::file_transfer::TransferSummary::default();
        for (i, source_path_str) in source_paths.iter().enumerate() {
            let _ = app_handle.emit(
                "import-progress",
                serde_json::json!({ "current": i, "total": total_files, "path": source_path_str }),
            );

            let import_result: Result<crate::file_transfer::TransferOutcome, String> = (|| {
                #[cfg(target_os = "android")]
                if is_android_content_uri(source_path_str) {
                    let resolved_name = resolve_android_content_uri_name(source_path_str)?;
                    let source_bytes = read_android_content_uri(source_path_str)?;
                    let source_name_path = Path::new(&resolved_name);
                    let file_date = exif_processing::get_creation_date_from_bytes(
                        &resolved_name,
                        &source_bytes,
                    );

                    let mut final_dest_folder = PathBuf::from(&destination_folder);
                    if settings.organize_by_date {
                        let date_format_str = settings
                            .date_folder_format
                            .replace("YYYY", "%Y")
                            .replace("MM", "%m")
                            .replace("DD", "%d");
                        let subfolder = file_date.format(&date_format_str).to_string();
                        final_dest_folder.push(subfolder);
                    }

                    fs::create_dir_all(&final_dest_folder)
                        .map_err(|e| format!("Failed to create destination folder: {}", e))?;

                    let new_stem = generate_filename_from_template(
                        &settings.filename_template,
                        source_name_path,
                        i + 1,
                        total_files,
                        &file_date,
                    );
                    let extension = source_name_path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    let new_filename = if extension.is_empty() {
                        new_stem
                    } else {
                        format!("{new_stem}.{extension}")
                    };
                    let dest_file_path = final_dest_folder.join(new_filename);

                    let outcome =
                        crate::file_transfer::transfer_bytes(&source_bytes, &dest_file_path)?;

                    if settings.delete_after_import {
                        log::info!(
                            "Skipping delete_after_import for Android content URI source: {}",
                            source_path_str
                        );
                    }

                    return Ok(outcome);
                }

                let (source_path, _source_sidecar) = parse_virtual_path(source_path_str);
                if !source_path.exists() {
                    return Err(format!("Source file not found: {}", source_path_str));
                }

                let file_date = exif_processing::get_creation_date_from_path(&source_path);

                let mut final_dest_folder = PathBuf::from(&destination_folder);
                if settings.organize_by_date {
                    let date_format_str = settings
                        .date_folder_format
                        .replace("YYYY", "%Y")
                        .replace("MM", "%m")
                        .replace("DD", "%d");
                    let subfolder = file_date.format(&date_format_str).to_string();
                    final_dest_folder.push(subfolder);
                }

                fs::create_dir_all(&final_dest_folder)
                    .map_err(|e| format!("Failed to create destination folder: {}", e))?;

                let new_stem = generate_filename_from_template(
                    &settings.filename_template,
                    &source_path,
                    i + 1,
                    total_files,
                    &file_date,
                );
                let extension = source_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                let new_filename = if extension.is_empty() {
                    new_stem
                } else {
                    format!("{new_stem}.{extension}")
                };
                let dest_file_path = final_dest_folder.join(new_filename);

                let outcome = crate::file_transfer::transfer(&source_path, &dest_file_path, false)?;
                if settings.delete_after_import
                    && let crate::file_transfer::TransferOutcome::Copied { files, .. } = &outcome
                {
                    crate::file_transfer::remove_import_sources(files, |source| {
                        #[cfg(any(
                            target_os = "windows",
                            target_os = "macos",
                            target_os = "linux"
                        ))]
                        trash::delete(source).map_err(|e| {
                            format!("Copied, but could not trash {}: {e}", source.display())
                        })?;
                        #[cfg(not(any(
                            target_os = "windows",
                            target_os = "macos",
                            target_os = "linux"
                        )))]
                        fs::remove_file(source).map_err(|e| e.to_string())?;
                        Ok(())
                    })?;
                }
                Ok(outcome)
            })(
            );

            if let Err(e) = import_result {
                eprintln!("Failed to import {}: {}", source_path_str, e);
                let _ = app_handle.emit("import-error", e.clone());
                summary.record(Err(e));
            } else {
                summary.record(import_result);
            }
        }

        let _ = app_handle.emit(
            "import-progress",
            serde_json::json!({ "current": total_files, "total": total_files, "path": "" }),
        );
        let _ = app_handle.emit("import-complete", &summary);
    });

    Ok(())
}

pub fn generate_filename_from_template(
    template: &str,
    original_path: &std::path::Path,
    sequence: usize,
    total: usize,
    file_date: &DateTime<Utc>,
) -> String {
    let facts = crate::file_naming::PhotoFacts::new(original_path);
    crate::file_naming::render_lenient(
        template,
        &crate::file_naming::NamingContext {
            source_path: original_path,
            sequence,
            total,
            date: *file_date,
            group: None,
            group_count: 0,
            member_count: 0,
            facts: &facts,
        },
    )
}

/// Resolve a single export filename stem from a template for one image. Used by
/// the export panel to build the suggested name shown in the save dialog, so the
/// same tokens (dates, metadata, original filename) work for single-image export
/// as for batch export. Falls back to the original stem if the result is empty.
#[tauri::command]
pub fn generate_export_filename(path: String, template: String) -> String {
    let (source_path, _) = parse_virtual_path(&path);
    let file_date = crate::exif_processing::get_creation_date_from_path(&source_path);
    let stem = generate_filename_from_template(&template, &source_path, 1, 1, &file_date);
    let trimmed = stem.trim();
    if trimmed.is_empty() {
        source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string()
    } else {
        trimmed.to_string()
    }
}

#[tauri::command]
pub fn preview_rename_files(
    paths: Vec<String>,
    name_template: String,
    options: Option<RenameOptions>,
) -> Result<RenamePreview, String> {
    Ok(plan_rename(&paths, &name_template, &options.unwrap_or_default())?.preview)
}

#[tauri::command]
pub fn rename_files(
    paths: Vec<String>,
    name_template: String,
    options: Option<RenameOptions>,
    app_handle: AppHandle,
) -> Result<RenameOutcome, String> {
    let outcome = rename_files_on_disk(&paths, &name_template, &options.unwrap_or_default())?;
    match rename_journal_path(&app_handle)
        .and_then(|journal| crate::batch_rename::save_journal(&journal, &outcome))
    {
        Ok(()) => {}
        Err(e) => log::warn!("Could not save the rename for undo: {}", e),
    }
    update_references_after_rename(&app_handle, &outcome);
    Ok(outcome)
}

#[tauri::command]
pub fn get_last_rename(app_handle: AppHandle) -> Option<UndoInfo> {
    crate::batch_rename::undo_info(&rename_journal_path(&app_handle).ok()?)
}

#[tauri::command]
pub fn undo_last_rename(app_handle: AppHandle) -> Result<RenameOutcome, String> {
    let outcome = crate::batch_rename::undo_from_journal(&rename_journal_path(&app_handle)?)?;
    update_references_after_rename(&app_handle, &outcome);
    Ok(outcome)
}

fn rename_journal_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("last_rename.json"))
}

fn update_references_after_rename(app_handle: &AppHandle, outcome: &RenameOutcome) {
    if outcome.files.is_empty() {
        return;
    }
    sync_album_path_changes(app_handle, Some(&outcome.image_map()), None, None);
    for change in &outcome.files {
        exif_processing::rename_cached_exif(Path::new(&change.from), Path::new(&change.to));
    }
    if let Ok(thumb_cache_dir) = get_thumb_cache_dir(app_handle) {
        migrate_thumbnail_cache(&thumb_cache_dir, &outcome.images);
    }
}

/// Cached thumbnails are keyed by path, so move them to the new names instead
/// of regenerating every renamed image.
fn migrate_thumbnail_cache(thumb_cache_dir: &Path, images: &[crate::batch_rename::PathChange]) {
    for change in images {
        let (source_path, sidecar_path) = parse_virtual_path(&change.to);
        let Some(mtime) = thumbnail_mtime(&source_path) else {
            continue;
        };
        let adjustments = thumbnail_adjustment_bytes(&sidecar_path);
        let old_hash = thumbnail_cache_hash(&change.from, mtime, &adjustments);
        let new_hash = thumbnail_cache_hash(&change.to, mtime, &adjustments);
        for size in ["small", "medium"] {
            let old = thumb_cache_dir.join(format!("{}_{}.jpg", old_hash, size));
            if old.exists() {
                let _ = fs::rename(
                    &old,
                    thumb_cache_dir.join(format!("{}_{}.jpg", new_hash, size)),
                );
            }
        }
    }
}

/// The single entry point for renaming images on disk: plans the rename (pairs,
/// sidecars, tokens, collisions) and carries it out in two phases.
fn rename_files_on_disk(
    paths: &[String],
    name_template: &str,
    options: &RenameOptions,
) -> Result<RenameOutcome, String> {
    ensure_card_writable_for_paths(paths)?;
    if paths.is_empty() {
        return Ok(RenameOutcome::default());
    }
    crate::batch_rename::apply_plan(&plan_rename(paths, name_template, options)?)
}

#[tauri::command]
pub fn create_virtual_copy(
    source_virtual_path: String,
    target_album_id: Option<String>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let new_virtual_path = create_virtual_copy_on_disk(&source_virtual_path)?;

    if let Some(album_id) = target_album_id {
        let _ = add_to_album(album_id, vec![new_virtual_path.clone()], app_handle);
    }

    Ok(new_virtual_path)
}

/// Snapshot the active draft without flushing it over the source photo's sidecar.
/// Keep all other sidecar metadata, including fields introduced by newer versions.
#[cfg(feature = "mcp")]
pub(crate) fn create_virtual_copy_from_draft(
    path: &str,
    adjustments: Value,
) -> Result<String, String> {
    use std::io::Write;
    let (source, sidecar) = parse_virtual_path(path);
    ensure_card_writable(&source)?;
    if !source.is_file() {
        return Err("Virtual copy source is not a file".into());
    }
    let mut metadata = match fs::read(&sidecar) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            serde_json::to_value(ImageMetadata::default()).map_err(|e| e.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    };
    let object = metadata
        .as_object_mut()
        .ok_or("Invalid source sidecar metadata")?;
    object.insert("adjustments".into(), adjustments);
    let bytes = serde_json::to_vec_pretty(&metadata).map_err(|e| e.to_string())?;
    for _ in 0..16 {
        let id = Uuid::new_v4().to_string()[..6].to_string();
        let copy = format!("{}?vc={id}", source.to_string_lossy());
        let (_, target) = parse_virtual_path(&copy);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
                    let _ = fs::remove_file(&target);
                    return Err(error.to_string());
                }
                return Ok(copy);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("Could not allocate a unique virtual copy ID".into())
}

#[cfg(feature = "mcp")]
pub(crate) fn set_assistant_metadata(
    paths: &[String],
    changes: &Value,
    app: &AppHandle,
) -> Result<(), String> {
    update_metadata_for_paths(paths, app, |metadata| {
        if let Some(rating) = changes.get("rating").and_then(Value::as_u64) {
            apply_user_rating(metadata, rating as u8);
        }
        if let Some(flag) = changes.get("flag") {
            apply_user_flag(
                metadata,
                match flag.as_str() {
                    Some("pick") => Some(ImageFlag::Pick),
                    Some("reject") => Some(ImageFlag::Reject),
                    _ => None,
                },
            );
        }
        if let Some(color) = changes.get("colorLabel") {
            let mut tags = metadata.tags.take().unwrap_or_default();
            tags.retain(|tag| !tag.starts_with(COLOR_TAG_PREFIX));
            if let Some(color) = color.as_str() {
                tags.push(format!("{COLOR_TAG_PREFIX}{color}"));
            }
            metadata.tags = (!tags.is_empty()).then_some(tags);
        }
    })
}

fn create_virtual_copy_on_disk(source_virtual_path: &str) -> Result<String, String> {
    let (source_path, source_sidecar_path) = parse_virtual_path(source_virtual_path);
    ensure_card_writable(&source_path)?;

    let new_copy_id = Uuid::new_v4().to_string()[..6].to_string();
    let new_virtual_path = format!("{}?vc={}", source_path.to_string_lossy(), new_copy_id);
    let (_, new_sidecar_path) = parse_virtual_path(&new_virtual_path);

    let _metadata_lock = SIDECAR_METADATA_LOCK.lock().map_err(|e| e.to_string())?;
    let mut value = if source_sidecar_path.exists() {
        let bytes = fs::read(&source_sidecar_path).map_err(|e| e.to_string())?;
        serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())?
    } else {
        serde_json::to_value(ImageMetadata::default()).map_err(|e| e.to_string())?
    };
    value
        .as_object_mut()
        .ok_or("Virtual-copy sidecar must be an object")?
        .remove("stack");
    let json_string = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    write_file_atomically(&new_sidecar_path, json_string).map_err(|e| e.to_string())?;

    Ok(new_virtual_path)
}

pub fn extract_xmp_rating(content: &str) -> Option<i8> {
    crate::xmp::metadata(content).rating
}

const XMP_REJECTED_RATING: i8 = -1;

pub fn xmp_rating(metadata: &ImageMetadata) -> String {
    if metadata.flag == Some(ImageFlag::Reject) {
        XMP_REJECTED_RATING.to_string()
    } else {
        metadata.rating.to_string()
    }
}

pub fn xmp_label(tags: &[String]) -> Option<String> {
    tags.iter()
        .rev()
        .find_map(|t| t.strip_prefix(COLOR_TAG_PREFIX))
        .map(|color| {
            let mut c = color.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
}

pub fn xmp_subject_bag<S: AsRef<str>>(items: &[S]) -> String {
    let mut bag = String::from("<dc:subject>\n    <rdf:Bag>\n");
    for item in items {
        bag.push_str(&format!("     <rdf:li>{}</rdf:li>\n", item.as_ref()));
    }
    bag.push_str("    </rdf:Bag>\n   </dc:subject>");
    bag
}

pub fn extract_xmp_label(content: &str) -> Option<String> {
    crate::xmp::metadata(content).label
}

pub fn extract_xmp_tags(content: &str) -> Vec<String> {
    crate::xmp::metadata(content).tags
}

pub fn resolve_xmp_path(image_path: &Path) -> Option<PathBuf> {
    let xmp_path = image_path.with_extension("xmp");
    let xmp_path_upper = image_path.with_extension("XMP");
    if xmp_path.exists() {
        Some(xmp_path)
    } else if xmp_path_upper.exists() {
        Some(xmp_path_upper)
    } else {
        let parent = image_path.parent()?;
        let image_stem = image_path.file_stem()?;
        fs::read_dir(parent)
            .ok()?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|candidate| {
                candidate.file_stem() == Some(image_stem) && is_xmp_sidecar_path(candidate)
            })
    }
}

fn merge_xmp_metadata_fields(content: &str, metadata: &mut ImageMetadata) {
    let xmp_rating = extract_xmp_rating(content);
    if xmp_rating == Some(XMP_REJECTED_RATING) {
        apply_user_flag(metadata, Some(ImageFlag::Reject));
    }
    if let Some(rating) = xmp_rating.and_then(|r| u8::try_from(r).ok()) {
        apply_user_rating(metadata, rating);
        if let Some(adjustments) = metadata.adjustments.as_object_mut() {
            adjustments.insert("rating".to_string(), serde_json::json!(rating));
        } else {
            metadata.adjustments = serde_json::json!({ "rating": rating });
        }
    }

    let xmp_label = extract_xmp_label(content);
    let xmp_tags = extract_xmp_tags(content);
    if xmp_label.is_none() && xmp_tags.is_empty() {
        return;
    }

    let mut current_tags = metadata.tags.clone().unwrap_or_default();
    for tag in xmp_tags {
        if !current_tags.contains(&tag) {
            current_tags.push(tag);
        }
    }

    if let Some(label) = xmp_label {
        let label_tag = format!("{}{}", COLOR_TAG_PREFIX, label.to_lowercase());
        if !current_tags.contains(&label_tag) {
            current_tags.retain(|tag| !tag.starts_with(COLOR_TAG_PREFIX));
            current_tags.push(label_tag);
        }
    }

    if !current_tags.is_empty() {
        metadata.tags = Some(current_tags);
    }
}

/// A virtual copy shares the original's .xmp, so the .xmp's xmp:Rating="-1"
/// belongs to the original image only.
fn is_virtual_copy_sidecar(source_path: &Path, sidecar_path: &Path) -> bool {
    parse_virtual_path(&source_path.to_string_lossy()).1 != sidecar_path
}

pub fn sync_metadata_from_xmp(
    source_path: &Path,
    sidecar_path: &Path,
    metadata: &mut ImageMetadata,
) -> bool {
    let actual_xmp = resolve_xmp_path(source_path);

    let mut changed = false;

    if let Some(xmp_file) = actual_xmp
        && let Ok(content) = crate::xmp::read(&xmp_file)
    {
        let xmp_rating = extract_xmp_rating(&content);

        if xmp_rating == Some(XMP_REJECTED_RATING)
            && metadata.flag.is_none()
            && !metadata.flag_is_explicit
            && !is_virtual_copy_sidecar(source_path, sidecar_path)
        {
            metadata.flag = Some(ImageFlag::Reject);
            changed = true;
        }

        if metadata.rating == 0
            && !metadata.rating_is_explicit
            && let Some(rating) = xmp_rating.and_then(|r| u8::try_from(r).ok())
            && rating != 0
        {
            metadata.rating = rating;
            if !metadata.adjustments.is_null() && !metadata.adjustments.is_object() {
                metadata.adjustments = default_adjustments();
            }
            if let Some(obj) = metadata.adjustments.as_object_mut() {
                obj.insert("rating".to_string(), serde_json::json!(rating));
            }
            changed = true;
        }

        let xmp_label = extract_xmp_label(&content);
        let xmp_tags = extract_xmp_tags(&content);

        let mut current_tags = metadata.tags.clone().unwrap_or_default();
        let original_len = current_tags.len();
        let had_no_tags = metadata.tags.is_none();

        for tag in xmp_tags {
            if !current_tags.contains(&tag) {
                current_tags.push(tag);
            }
        }

        if let Some(label) = xmp_label {
            let label_tag = format!("{}{}", COLOR_TAG_PREFIX, label.to_lowercase());
            if !current_tags.contains(&label_tag) {
                current_tags.retain(|t| !t.starts_with(COLOR_TAG_PREFIX));
                current_tags.push(label_tag);
            }
        }

        if current_tags.len() != original_len || (had_no_tags && !current_tags.is_empty()) {
            metadata.tags = Some(current_tags);
            changed = true;
        }
    }
    changed
}

fn backup_malformed_xmp(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    // Never replace an earlier backup, including one from another process.
    for index in 0..100 {
        let mut name = path.as_os_str().to_os_string();
        name.push(if index == 0 {
            ".bak".into()
        } else {
            format!(".bak.{index}")
        });
        let backup = PathBuf::from(name);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&backup) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                    let _ = fs::remove_file(&backup);
                    return Err(error.to_string());
                }
                #[cfg(unix)]
                fs::File::open(
                    path.parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or(Path::new(".")),
                )
                .and_then(|parent| parent.sync_all())
                .map_err(|e| e.to_string())?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("No free XMP backup name".into())
}

pub fn sync_metadata_to_xmp(
    source_path: &Path,
    sidecar_path: &Path,
    metadata: &ImageMetadata,
    create_if_missing: bool,
) {
    if ensure_card_writable(source_path).is_err() {
        return;
    }
    let existing = resolve_xmp_path(source_path);
    if existing.is_none() && !create_if_missing {
        return;
    }
    let path = existing
        .clone()
        .unwrap_or_else(|| source_path.with_extension("xmp"));
    if ensure_card_writable(&path).is_err() {
        return;
    }
    let bytes = if existing.is_some() {
        match crate::xmp::read_bytes(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::error!("XMP write refused: {}", error);
                return;
            }
        }
    } else {
        crate::xmp::SKELETON.as_bytes().to_vec()
    };
    let parsed =
        std::str::from_utf8(&bytes).map_err(|e| crate::xmp::Error::Malformed(e.to_string()));
    let validation = parsed.and_then(|content| crate::xmp::validate(content).map(|()| content));
    let content = match validation {
        Ok(content) => content,
        Err(crate::xmp::Error::Unsafe(reason)) => {
            log::error!("XMP write refused: {}", reason);
            return;
        }
        Err(error) => {
            if existing.is_some()
                && let Err(backup_error) = backup_malformed_xmp(&path, &bytes)
            {
                log::error!("XMP repair refused: backup failed: {}", backup_error);
                return;
            }
            log::warn!("{}; original backed up before repair", error);
            crate::xmp::SKELETON
        }
    };
    let rejected = if is_virtual_copy_sidecar(source_path, sidecar_path) {
        extract_xmp_rating(content) == Some(XMP_REJECTED_RATING)
    } else {
        metadata.flag == Some(ImageFlag::Reject)
    };
    let rating = if rejected {
        i16::from(XMP_REJECTED_RATING)
    } else {
        i16::from(metadata.rating)
    };
    let mut label = None;
    let mut tags = Vec::new();
    for tag in metadata.tags.as_deref().unwrap_or_default() {
        if let Some(color) = tag.strip_prefix(COLOR_TAG_PREFIX) {
            let mut chars = color.chars();
            label = Some(chars.next().map_or_else(String::new, |c| {
                c.to_uppercase().collect::<String>() + chars.as_str()
            }));
        } else {
            tags.push(tag.clone());
        }
    }
    match crate::xmp::update(content, rating, label.as_deref(), &tags) {
        Ok(updated) => {
            if updated.as_bytes() != bytes
                && let Err(error) = write_file_atomically(&path, updated)
            {
                log::error!("Failed to write XMP: {}", error);
            }
        }
        Err(error) => log::error!("XMP write refused: {}", error),
    }
}

#[cfg(test)]
mod xmp_safety_tests {
    use super::*;

    fn write_metadata(path: &Path) {
        let metadata = ImageMetadata {
            rating: 3,
            tags: Some(vec![
                "family & <friends>".into(),
                format!("{COLOR_TAG_PREFIX}red"),
            ]),
            ..Default::default()
        };
        sync_metadata_to_xmp(path, &path.with_extension("arw.rrdata"), &metadata, true);
    }

    #[test]
    fn backs_up_malformed_bytes_before_repair_without_replacing_old_backup() {
        let _card = card_mode_test_support::CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("sample.arw");
        let xmp = raw.with_extension("xmp");
        let broken = b"<rdf:Description xmp:Rating='2'><broken>";
        fs::write(&xmp, broken).unwrap();
        fs::write(dir.path().join("sample.xmp.bak"), "previous").unwrap();
        write_metadata(&raw);
        assert_eq!(
            fs::read(dir.path().join("sample.xmp.bak.1")).unwrap(),
            broken
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("sample.xmp.bak")).unwrap(),
            "previous"
        );
        let content = crate::xmp::read(&xmp).unwrap();
        assert_eq!(extract_xmp_rating(&content), Some(3));
        assert!(extract_xmp_tags(&content).contains(&"family & <friends>".into()));
    }

    #[test]
    fn unsafe_sidecars_are_never_overwritten_or_backed_up() {
        let _card = card_mode_test_support::CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("sample.arw");
        let xmp = raw.with_extension("xmp");
        for unsafe_xmp in [
            "<!DOCTYPE",
            "<!DOCTYPE a><a/>",
            &format!("{}{}", "<a>".repeat(65), "</a>".repeat(65)),
        ] {
            fs::write(&xmp, unsafe_xmp).unwrap();
            write_metadata(&raw);
            assert_eq!(fs::read_to_string(&xmp).unwrap(), unsafe_xmp);
            assert!(!dir.path().join("sample.xmp.bak").exists());
        }
        fs::File::create(&xmp)
            .unwrap()
            .set_len(crate::xmp::MAX_BYTES as u64 + 1)
            .unwrap();
        write_metadata(&raw);
        assert_eq!(
            fs::metadata(&xmp).unwrap().len(),
            crate::xmp::MAX_BYTES as u64 + 1
        );
        assert!(!dir.path().join("sample.xmp.bak").exists());
    }

    #[test]
    fn failed_backup_keeps_malformed_original() {
        let _card = card_mode_test_support::CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("sample.arw");
        let xmp = raw.with_extension("xmp");
        fs::write(&xmp, "<broken>").unwrap();
        for index in 0..100 {
            fs::write(
                dir.path().join(if index == 0 {
                    "sample.xmp.bak".into()
                } else {
                    format!("sample.xmp.bak.{index}")
                }),
                "existing backup",
            )
            .unwrap();
        }
        write_metadata(&raw);
        assert_eq!(fs::read_to_string(&xmp).unwrap(), "<broken>");
    }
}

#[cfg(test)]
mod atomic_write_tests {
    use super::{
        AtomicWriteBackend, AtomicWriteTemp, write_file_atomically,
        write_file_atomically_with_backend,
    };
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};

    #[derive(Clone, Copy)]
    enum InjectedFailure {
        DiskFullAfter(usize),
        Sync,
        Persist,
    }

    struct InjectedBackend {
        failure: InjectedFailure,
    }

    struct InjectedTemp {
        path: PathBuf,
        file: File,
        failure: InjectedFailure,
        written: usize,
    }

    impl Write for InjectedTemp {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if let InjectedFailure::DiskFullAfter(limit) = self.failure {
                if self.written >= limit {
                    return Err(io::Error::new(
                        io::ErrorKind::StorageFull,
                        "injected disk full",
                    ));
                }
                let count = buffer.len().min(limit - self.written);
                let written = self.file.write(&buffer[..count])?;
                self.written += written;
                return Ok(written);
            }
            self.file.write(buffer)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }

    impl AtomicWriteTemp for InjectedTemp {
        fn path(&self) -> &Path {
            &self.path
        }

        fn sync_all(&mut self) -> io::Result<()> {
            if matches!(self.failure, InjectedFailure::Sync) {
                return Err(io::Error::other("injected interrupted finalization"));
            }
            self.file.sync_all()
        }

        fn persist(self, target: &Path) -> io::Result<()> {
            if matches!(self.failure, InjectedFailure::Persist) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "injected final-name collision",
                ));
            }
            fs::rename(&self.path, target)
        }
    }

    impl Drop for InjectedTemp {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    impl AtomicWriteBackend for InjectedBackend {
        type Temp = InjectedTemp;

        fn create_temp(&mut self, parent: &Path) -> io::Result<Self::Temp> {
            let path = parent.join(".rapidraw-injected.tmp");
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            Ok(InjectedTemp {
                path,
                file,
                failure: self.failure,
                written: 0,
            })
        }
    }

    fn leftover_temp_files(dir: &std::path::Path) -> usize {
        fs::read_dir(dir)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".rapidraw-")
            })
            .count()
    }

    fn assert_injected_failure_preserves_existing_file(failure: InjectedFailure) -> io::Error {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("photo.cr3.rrdata");
        fs::write(&target, "original").unwrap();
        let error = write_file_atomically_with_backend(
            &target,
            b"replacement contents",
            &mut InjectedBackend { failure },
        )
        .unwrap_err();

        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(leftover_temp_files(dir.path()), 0);
        error
    }

    #[test]
    fn replaces_contents_and_keeps_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("photo.cr3.rrdata");
        write_file_atomically(&sidecar, "{\"rating\":1}").unwrap();
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), "{\"rating\":1}");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&sidecar).unwrap().permissions().mode() & 0o777,
                0o644
            );
            fs::set_permissions(&sidecar, fs::Permissions::from_mode(0o664)).unwrap();
        }
        write_file_atomically(&sidecar, "{\"rating\":5}").unwrap();
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), "{\"rating\":5}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&sidecar).unwrap().permissions().mode() & 0o777,
                0o664
            );
        }
        assert_eq!(leftover_temp_files(dir.path()), 0);
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_a_symlinked_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("shared.rrdata");
        let link = dir.path().join("photo.cr3.rrdata");
        fs::write(&target, "old").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_file_atomically(&link, "new").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn failed_replace_keeps_the_existing_entry() {
        let dir = tempfile::tempdir().unwrap();
        // A directory cannot be replaced by a file, so the final rename fails.
        let occupied = dir.path().join("photo.cr3.rrdata");
        fs::create_dir(&occupied).unwrap();
        fs::write(occupied.join("keep"), "kept").unwrap();

        assert!(write_file_atomically(&occupied, "new").is_err());
        assert_eq!(fs::read_to_string(occupied.join("keep")).unwrap(), "kept");
        assert_eq!(leftover_temp_files(dir.path()), 0);
    }

    #[test]
    fn disk_full_during_a_partial_write_keeps_old_contents_and_cleans_temp() {
        let error =
            assert_injected_failure_preserves_existing_file(InjectedFailure::DiskFullAfter(4));
        assert_eq!(error.kind(), io::ErrorKind::StorageFull);
    }

    #[test]
    fn interrupted_sync_keeps_old_contents_and_cleans_temp() {
        let error = assert_injected_failure_preserves_existing_file(InjectedFailure::Sync);
        assert!(error.to_string().contains("interrupted finalization"));
    }

    #[test]
    fn final_name_collision_keeps_old_contents_and_cleans_temp() {
        let error = assert_injected_failure_preserves_existing_file(InjectedFailure::Persist);
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }
}

#[cfg(test)]
pub(crate) mod card_mode_test_support {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard};

    use super::set_card_browse_root;

    static CARD_MODE_LOCK: Mutex<()> = Mutex::new(());

    /// Holds the global Card mode root for one test and clears it on drop.
    pub struct CardMode {
        _lock: MutexGuard<'static, ()>,
    }

    impl CardMode {
        pub fn on(root: &Path) -> Self {
            let lock = CARD_MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            set_card_browse_root(Some(root.to_string_lossy().into_owned())).unwrap();
            CardMode { _lock: lock }
        }

        pub fn off() -> Self {
            let lock = CARD_MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            set_card_browse_root(None).unwrap();
            CardMode { _lock: lock }
        }
    }

    impl Drop for CardMode {
        fn drop(&mut self) {
            let _ = set_card_browse_root(None);
        }
    }

    /// A temp folder with a `card` (DCIM layout) and a `library` next to it.
    pub struct Folders {
        _dir: tempfile::TempDir,
        pub card: PathBuf,
        pub dcim: PathBuf,
        pub library: PathBuf,
    }

    pub fn folders() -> Folders {
        let dir = tempfile::tempdir().unwrap();
        let card = dir.path().join("card");
        let dcim = card.join("DCIM").join("100CANON");
        let library = dir.path().join("library");
        fs::create_dir_all(&dcim).unwrap();
        fs::create_dir_all(&library).unwrap();
        for folder in [&dcim, &library] {
            fs::write(folder.join("IMG_0001.jpg"), b"jpeg bytes").unwrap();
            fs::write(
                folder.join("IMG_0001.jpg.rrdata"),
                r#"{"version":1,"rating":2,"adjustments":{},"tags":["ai:dog","user:keep","color:red"]}"#,
            )
            .unwrap();
            fs::write(
                folder.join("IMG_0001.xmp"),
                r#"<x:xmpmeta><rdf:RDF><rdf:Description xmp:Rating="2"></rdf:Description></rdf:RDF></x:xmpmeta>"#,
            )
            .unwrap();
        }
        Folders {
            _dir: dir,
            card,
            dcim,
            library,
        }
    }

    /// Every file and folder under `root` with its contents, to prove nothing changed.
    pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .map(|entry| {
                let contents = entry
                    .file_type()
                    .is_file()
                    .then(|| fs::read(entry.path()).unwrap());
                (entry.path().to_path_buf(), contents)
            })
            .collect()
    }

    pub fn path_str(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod card_mode_tests {
    use super::card_mode_test_support::{CardMode, folders, path_str, snapshot};
    use super::*;

    fn assert_read_only<T: std::fmt::Debug>(result: Result<T, String>) {
        assert_eq!(result.unwrap_err(), CARD_READ_ONLY_ERROR);
    }

    #[test]
    fn guard_covers_the_card_and_nothing_else() {
        let f = folders();
        let _mode = CardMode::on(&f.card);

        assert!(is_card_read_only_path(&f.card));
        assert!(is_card_read_only_path(&f.dcim.join("IMG_0001.jpg")));
        assert!(is_card_read_only_path(&f.dcim.join("new.jpg.rrdata")));
        assert!(is_card_read_only_path(
            &f.library.join("..").join("card").join("x.jpg")
        ));
        assert!(!is_card_read_only_path(&f.library.join("IMG_0001.jpg")));
        let sibling = f.card.with_file_name("card2");
        fs::create_dir_all(&sibling).unwrap();
        assert!(!is_card_read_only_path(&sibling.join("x.jpg")));

        let virtual_copy = format!("{}?vc=abc123", path_str(&f.dcim.join("IMG_0001.jpg")));
        assert_read_only(ensure_card_writable_for_paths(&[virtual_copy]));
        assert!(
            ensure_card_writable_for_paths(&[path_str(&f.library.join("IMG_0001.jpg"))]).is_ok()
        );

        let parent = f.card.parent().unwrap();
        assert!(touches_card_tree(parent));
        assert!(!is_card_read_only_path(parent));
        assert!(!touches_card_tree(&f.library));
    }

    #[cfg(unix)]
    #[test]
    fn guard_follows_symlinks_into_the_card() {
        let f = folders();
        let link = f.library.join("card-link");
        std::os::unix::fs::symlink(&f.dcim, &link).unwrap();
        let _mode = CardMode::on(&f.card);
        let before = snapshot(&f.card);

        assert!(is_card_read_only_path(&link.join("IMG_0001.jpg.rrdata")));
        assert!(write_file_atomically(link.join("IMG_0001.jpg.rrdata"), "{}").is_err());
        assert!(write_file_atomically(link.join("new.jpg.rrdata"), "{}").is_err());
        assert_eq!(snapshot(&f.card), before);
    }

    #[test]
    fn clearing_card_mode_makes_the_card_writable_again() {
        let f = folders();
        {
            let _mode = CardMode::on(&f.card);
            assert!(is_card_read_only_path(&f.dcim));
        }
        let _mode = CardMode::off();
        assert!(!is_card_read_only_path(&f.dcim));
    }

    #[test]
    fn sidecar_writes_refuse_the_card() {
        let f = folders();
        let _mode = CardMode::on(&f.card);
        let before = snapshot(&f.card);

        let existing = write_file_atomically(f.dcim.join("IMG_0001.jpg.rrdata"), "{}");
        assert_eq!(
            existing.unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert!(write_file_atomically(f.dcim.join("IMG_0002.jpg.rrdata"), "{}").is_err());
        assert_eq!(snapshot(&f.card), before);

        write_file_atomically(f.library.join("IMG_0001.jpg.rrdata"), "{}").unwrap();
        assert_eq!(
            fs::read_to_string(f.library.join("IMG_0001.jpg.rrdata")).unwrap(),
            "{}"
        );
    }

    fn rated(rating: u8) -> ImageMetadata {
        ImageMetadata {
            rating,
            ..Default::default()
        }
    }

    #[test]
    fn xmp_write_back_refuses_the_card() {
        let f = folders();
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            sync_metadata_to_xmp(
                &f.dcim.join("IMG_0001.jpg"),
                &f.dcim.join("IMG_0001.jpg.rrdata"),
                &rated(5),
                true,
            );
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        sync_metadata_to_xmp(
            &f.library.join("IMG_0001.jpg"),
            &f.library.join("IMG_0001.jpg.rrdata"),
            &rated(5),
            true,
        );
        let xmp = fs::read_to_string(f.library.join("IMG_0001.xmp")).unwrap();
        assert!(xmp.contains("xmp:Rating=\"5\""));
    }

    #[test]
    fn creating_a_missing_xmp_refuses_the_card() {
        let f = folders();
        fs::remove_file(f.dcim.join("IMG_0001.xmp")).unwrap();
        fs::remove_file(f.library.join("IMG_0001.xmp")).unwrap();
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            sync_metadata_to_xmp(
                &f.dcim.join("IMG_0001.jpg"),
                &f.dcim.join("IMG_0001.jpg.rrdata"),
                &rated(4),
                true,
            );
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        sync_metadata_to_xmp(
            &f.library.join("IMG_0001.jpg"),
            &f.library.join("IMG_0001.jpg.rrdata"),
            &rated(4),
            true,
        );
        assert_eq!(
            extract_xmp_rating(&fs::read_to_string(f.library.join("IMG_0001.xmp")).unwrap()),
            Some(4)
        );
    }

    #[test]
    fn exif_refresh_refuses_the_card() {
        let f = folders();
        let updates = HashMap::from([("Artist".to_string(), "Tomas".to_string())]);
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            let result = tauri::async_runtime::block_on(update_exif_fields(
                vec![path_str(&f.dcim.join("IMG_0001.jpg"))],
                updates.clone(),
            ));
            assert_read_only(result);
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        tauri::async_runtime::block_on(update_exif_fields(
            vec![path_str(&f.library.join("IMG_0001.jpg"))],
            updates,
        ))
        .unwrap();
        let sidecar = fs::read_to_string(f.library.join("IMG_0001.jpg.rrdata")).unwrap();
        assert!(sidecar.contains("\"Artist\": \"Tomas\""));
    }

    #[test]
    fn create_folder_refuses_the_card() {
        let f = folders();
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            assert_read_only(create_folder(path_str(&f.dcim.join("Picks"))));
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        create_folder(path_str(&f.library.join("Picks"))).unwrap();
        assert!(f.library.join("Picks").is_dir());
    }

    #[test]
    fn rename_folder_refuses_the_card_and_its_parents() {
        let f = folders();
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            assert_read_only(rename_folder_on_disk(&path_str(&f.dcim), "renamed"));
            let parent = f.card.parent().unwrap();
            assert_read_only(rename_folder_on_disk(&path_str(parent), "renamed"));
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        let renamed = rename_folder_on_disk(&path_str(&f.library), "renamed").unwrap();
        assert!(Path::new(&renamed).join("IMG_0001.jpg").is_file());
    }

    #[test]
    fn delete_folder_refuses_the_card_and_its_parents() {
        let f = folders();
        let _mode = CardMode::on(&f.card);
        let before = snapshot(&f.card);
        assert_read_only(delete_folder_on_disk(&path_str(&f.dcim)));
        assert_read_only(delete_folder_on_disk(&path_str(&f.card)));
        assert_read_only(delete_folder_on_disk(&path_str(f.card.parent().unwrap())));
        assert_eq!(snapshot(&f.card), before);
    }

    #[test]
    fn copy_into_the_card_is_refused_and_copy_out_works() {
        let f = folders();
        let _mode = CardMode::on(&f.card);
        let before = snapshot(&f.card);

        assert_read_only(copy_files(
            vec![path_str(&f.library.join("IMG_0001.jpg"))],
            path_str(&f.dcim),
        ));
        assert_eq!(snapshot(&f.card), before);

        let out = f.library.join("from-card");
        fs::create_dir(&out).unwrap();
        copy_files(vec![path_str(&f.dcim.join("IMG_0001.jpg"))], path_str(&out)).unwrap();
        assert!(out.join("IMG_0001.jpg").is_file());
        assert!(out.join("IMG_0001.jpg.rrdata").is_file());
        assert_eq!(snapshot(&f.card), before);
    }

    #[test]
    fn move_refuses_the_card_both_ways() {
        let f = folders();
        let elsewhere = f.library.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            assert_read_only(move_files_on_disk(
                &[path_str(&f.dcim.join("IMG_0001.jpg"))],
                &path_str(&elsewhere),
            ));
            assert_read_only(move_files_on_disk(
                &[path_str(&f.library.join("IMG_0001.jpg"))],
                &path_str(&f.dcim),
            ));
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        let renames = move_files_on_disk(
            &[path_str(&f.library.join("IMG_0001.jpg"))],
            &path_str(&elsewhere),
        )
        .unwrap();
        assert_eq!(renames.len(), 1);
        assert!(elsewhere.join("IMG_0001.jpg").is_file());
        assert!(elsewhere.join("IMG_0001.jpg.rrdata").is_file());
    }

    #[test]
    fn duplicate_and_virtual_copy_refuse_the_card() {
        let f = folders();
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            let photo = path_str(&f.dcim.join("IMG_0001.jpg"));
            assert_read_only(duplicate_file_on_disk(&photo));
            assert_read_only(create_virtual_copy_on_disk(&photo));
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        let photo = path_str(&f.library.join("IMG_0001.jpg"));
        assert!(Path::new(&duplicate_file_on_disk(&photo).unwrap()).is_file());
        let copy = create_virtual_copy_on_disk(&photo).unwrap();
        assert!(parse_virtual_path(&copy).1.is_file());
    }

    #[cfg(feature = "mcp")]
    #[test]
    fn assistant_copy_snapshots_unsaved_draft_and_preserves_source_and_other_metadata() {
        let f = folders();
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            assert_read_only(create_virtual_copy_from_draft(
                &path_str(&f.dcim.join("IMG_0001.jpg")),
                serde_json::json!({"exposure":1.25}),
            ));
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        let photo = path_str(&f.library.join("IMG_0001.jpg"));
        let (_, source_sidecar) = parse_virtual_path(&photo);
        let raw_before = fs::read(&photo).unwrap();
        let metadata = serde_json::json!({"version":1,"rating":4,"tags":["keep"],"adjustments":{"exposure":0.2},"futureField":{"retained":true}});
        fs::write(
            &source_sidecar,
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
        let source_before = fs::read(&source_sidecar).unwrap();
        let draft = serde_json::json!({"exposure":1.25,"masks":[]});
        let copy = create_virtual_copy_from_draft(&photo, draft.clone()).unwrap();
        let copied: Value =
            serde_json::from_slice(&fs::read(parse_virtual_path(&copy).1).unwrap()).unwrap();
        assert_eq!(copied["adjustments"], draft);
        assert_eq!(copied["rating"], 4);
        assert_eq!(copied["futureField"]["retained"], true);
        assert_eq!(fs::read(&source_sidecar).unwrap(), source_before);
        assert_eq!(fs::read(&photo).unwrap(), raw_before);
        assert_ne!(create_virtual_copy_from_draft(&photo, draft).unwrap(), copy);
        fs::write(&source_sidecar, b"malformed").unwrap();
        let before = snapshot(&f.library);
        assert!(create_virtual_copy_from_draft(&photo, serde_json::json!({})).is_err());
        assert_eq!(snapshot(&f.library), before);
    }

    #[test]
    fn rename_files_refuses_the_card() {
        let f = folders();
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            assert_read_only(rename_files_on_disk(
                &[path_str(&f.dcim.join("IMG_0001.jpg"))],
                "trip_{sequence}",
                &RenameOptions::default(),
            ));
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        let outcome = rename_files_on_disk(
            &[path_str(&f.library.join("IMG_0001.jpg"))],
            "trip",
            &RenameOptions::default(),
        )
        .unwrap();
        assert_eq!(outcome.images[0].to, path_str(&f.library.join("trip.jpg")));
        assert!(f.library.join("trip.jpg.rrdata").is_file());
    }

    #[test]
    fn deleting_files_refuses_the_card() {
        let f = folders();
        {
            let _mode = CardMode::on(&f.card);
            let photo = path_str(&f.dcim.join("IMG_0001.jpg"));
            assert_read_only(plan_file_deletions(vec![photo.clone()]));
            assert_read_only(plan_associated_deletions(&[photo]));
        }
        let _mode = CardMode::off();
        let photo = path_str(&f.library.join("IMG_0001.jpg"));
        let (files, _) = plan_file_deletions(vec![photo.clone()]).unwrap();
        assert!(files.contains(&f.library.join("IMG_0001.jpg.rrdata")));
        let (files, _) = plan_associated_deletions(&[photo]).unwrap();
        assert!(files.contains(&f.library.join("IMG_0001.jpg")));
    }

    #[test]
    fn clearing_sidecars_never_touches_the_card() {
        let f = folders();
        let _mode = CardMode::on(&f.card);
        let before = snapshot(&f.card);

        assert_read_only(clear_all_sidecars(path_str(&f.dcim)));
        let parent = f.card.parent().unwrap();
        assert_eq!(clear_all_sidecars(path_str(parent)).unwrap(), 1);
        assert!(!f.library.join("IMG_0001.jpg.rrdata").exists());
        assert_eq!(snapshot(&f.card), before);
    }

    #[test]
    fn import_refuses_writing_to_or_deleting_from_the_card() {
        let f = folders();
        let _mode = CardMode::on(&f.card);
        let from_card = vec![path_str(&f.dcim.join("IMG_0001.jpg"))];
        let from_library = vec![path_str(&f.library.join("IMG_0001.jpg"))];

        assert_read_only(ensure_import_writable(
            &from_library,
            &path_str(&f.dcim),
            false,
        ));
        assert_read_only(ensure_import_writable(
            &from_card,
            &path_str(&f.library),
            true,
        ));
        assert!(ensure_import_writable(&from_card, &path_str(&f.library), false).is_ok());
    }

    #[test]
    fn preset_export_refuses_the_card() {
        let f = folders();
        {
            let _mode = CardMode::on(&f.card);
            let before = snapshot(&f.card);
            assert_read_only(handle_export_presets_to_file(
                Vec::new(),
                path_str(&f.dcim.join("presets.rrpreset")),
            ));
            assert_eq!(snapshot(&f.card), before);
        }
        let _mode = CardMode::off();
        let target = f.library.join("presets.rrpreset");
        handle_export_presets_to_file(Vec::new(), path_str(&target)).unwrap();
        assert!(target.is_file());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires RAPIDROOM_THUMBNAIL_QA_INPUT pointing to a local sample"]
    fn real_raw_thumbnail_probe() {
        let path = std::env::var("RAPIDROOM_THUMBNAIL_QA_INPUT").unwrap();
        let io_read_bytes = || {
            fs::read_to_string("/proc/self/io").ok().and_then(|s| {
                s.lines().find_map(|line| {
                    line.strip_prefix("read_bytes: ")
                        .and_then(|v| v.parse::<u64>().ok())
                })
            })
        };
        let io_before = io_read_bytes();
        let _file = fs::File::open(&path).unwrap();
        #[cfg(target_os = "linux")]
        if std::env::var("RAPIDROOM_THUMBNAIL_QA_COLD").as_deref() == Ok("1") {
            use std::os::fd::AsRawFd;
            // Only evict this read-only sample's clean pages, never the system cache.
            assert_eq!(
                unsafe { libc::posix_fadvise(_file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) },
                0
            );
        }
        let mmap = read_file_mapped(Path::new(&path)).unwrap();
        let settings = AppSettings::default();
        let input_dimensions = crate::raw_processing::get_raw_dimensions(&mmap).unwrap();
        let start = std::time::Instant::now();
        let preview = thumbnail_embedded_preview(&serde_json::Value::Null, &settings, &mmap, &path);
        let seconds = start.elapsed().as_secs_f64();
        let dimensions = preview.as_ref().map(|p| (p.width(), p.height()));
        let defaults = thumbnail_embedded_preview(&serde_json::json!({}), &settings, &mmap, &path);
        assert_eq!(
            dimensions,
            defaults.as_ref().map(|p| (p.width(), p.height()))
        );
        assert!(
            thumbnail_embedded_preview(
                &serde_json::json!({"exposure": 0.5}),
                &settings,
                &mmap,
                &path
            )
            .is_none()
        );
        assert!(
            thumbnail_embedded_preview(
                &serde_json::json!({"aiPatches": [{}]}),
                &settings,
                &mmap,
                &path
            )
            .is_none()
        );
        let mut force_raw = settings.clone();
        force_raw.always_decode_raw_thumbnails = Some(true);
        assert!(
            thumbnail_embedded_preview(&serde_json::Value::Null, &force_raw, &mmap, &path)
                .is_none()
        );
        let fraction = std::env::var("RAPIDROOM_THUMBNAIL_QA_CROP_FRACTION")
            .ok()
            .map(|v| v.parse::<f64>().unwrap())
            .unwrap_or(1.0);
        assert!(fraction > 0.0 && fraction <= 1.0);
        let crop = Crop {
            x: 0.0,
            y: 0.0,
            width: f64::from(input_dimensions.0) * fraction,
            height: f64::from(input_dimensions.1) * fraction,
        };
        let proxy_min = thumbnail_proxy_min_dim(&mmap, 1280, Some(&crop));
        let proxy = if input_dimensions.2 && proxy_min.is_some() {
            let image = crate::raw_processing::develop_raw_image(
                &mmap,
                true,
                0.0,
                "linear".into(),
                None,
                proxy_min,
            )
            .unwrap();
            assert!(image.to_rgb32f().as_raw().iter().all(|v| v.is_finite()));
            let scale = crate::raw_processing::get_fast_demosaic_scale_factor(
                &mmap,
                image.width(),
                image.height(),
            );
            let ratio = image.width().max(image.height()) as f32
                / input_dimensions.0.max(input_dimensions.1) as f32;
            assert!((scale - if ratio > 0.97 { 1.0 } else { ratio }).abs() < 1e-5);
            Some((image.width(), image.height()))
        } else {
            None
        };
        println!(
            "THUMBNAIL_QA {}",
            serde_json::json!({"input": input_dimensions, "preview": dimensions, "preview_seconds": seconds, "physical_read_bytes": io_read_bytes().zip(io_before).map(|(a,b)| a.saturating_sub(b)), "proxy_min_dim": proxy_min, "crop_fraction": fraction, "proxy": proxy})
        );
    }

    #[test]
    fn embedded_preview_keeps_defaults_and_rejects_user_edits() {
        let settings = AppSettings::default();
        for value in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({"exposure": 0.0}),
        ] {
            assert!(can_use_embedded_preview(&value, &settings, b""));
        }
        for value in [
            serde_json::json!({"exposure": 0.5}),
            serde_json::json!({"lensBlurEnabled": true}),
            serde_json::json!({"aiPatches": [{}]}),
            serde_json::json!({"masks": [{}]}),
            serde_json::json!({"orientationSteps": 1}),
            serde_json::json!({"rotation": 0.5}),
            serde_json::json!({"crop": {}}),
            serde_json::json!(7),
        ] {
            assert!(!can_use_embedded_preview(&value, &settings, b""), "{value}");
        }
    }

    #[test]
    fn a_small_real_crop_is_not_discarded_as_a_full_size_crop() {
        let full = Crop {
            x: 0.0,
            y: 0.0,
            width: 7008.0,
            height: 4672.0,
        };
        assert!(full_size_crop(&full, 7008, 4672));
        assert!(full_size_crop(&full, 4672, 7008));
        assert!(full_size_crop(
            &Crop {
                width: 7007.5,
                ..full
            },
            7008,
            4672
        ));
        for crop in [
            Crop {
                width: 7006.0,
                ..full
            },
            Crop { x: 1.0, ..full },
            Crop {
                height: 4640.0,
                ..full
            },
            Crop {
                width: f64::NAN,
                ..full
            },
            Crop {
                width: -7008.0,
                ..full
            },
        ] {
            assert!(!full_size_crop(&crop, 7008, 4672));
        }
    }

    #[test]
    fn imports_crop_only_lightroom_xmp_to_sidecar() {
        let test_directory =
            std::env::temp_dir().join(format!("rapidraw-xmp-crop-{}", Uuid::new_v4()));
        fs::create_dir_all(&test_directory).unwrap();

        let source_path = test_directory.join("sample.jpg");
        let xmp_path = test_directory.join("sample.xmp");
        fs::write(
            &xmp_path,
            r#"<?xpacket begin="﻿"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description
   xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   xmlns:tiff="http://ns.adobe.com/tiff/1.0/"
   xmlns:xmp="http://ns.adobe.com/xap/1.0/"
   xmlns:dc="http://purl.org/dc/elements/1.1/"
   crs:HasCrop="True"
   crs:AlreadyApplied="False"
   crs:CropTop="0.078"
   crs:CropLeft="0"
   crs:CropBottom="0.922"
   crs:CropRight="1"
   crs:CropAngle="0"
   tiff:ImageWidth="6000"
   tiff:ImageLength="4000"
   tiff:Orientation="1"
   xmp:Rating="4">
   <dc:subject><rdf:Bag><rdf:li>crop-test</rdf:li></rdf:Bag></dc:subject>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        )
        .unwrap();

        let imported = import_xmp_adjustments_to_sidecar(
            source_path.to_string_lossy().as_ref(),
            &xmp_path,
            None,
        )
        .unwrap();

        assert_eq!(
            imported.metadata.adjustments["crop"],
            serde_json::json!({
                "x": 0.0,
                "y": 312.0,
                "width": 6000.0,
                "height": 3376.0
            })
        );
        assert_eq!(imported.metadata.rating, 4);
        assert_eq!(imported.metadata.tags, Some(vec!["crop-test".to_string()]));
        assert!(imported.sidecar_path.exists());

        let saved = crate::exif_processing::load_sidecar(&imported.sidecar_path);
        assert_eq!(saved.adjustments, imported.metadata.adjustments);

        fs::remove_dir_all(test_directory).unwrap();
    }

    #[test]
    fn imports_metadata_only_lightroom_xmp_to_sidecar() {
        let test_directory =
            std::env::temp_dir().join(format!("rapidraw-xmp-metadata-{}", Uuid::new_v4()));
        fs::create_dir_all(&test_directory).unwrap();

        let source_path = test_directory.join("sample.jpg");
        let xmp_path = test_directory.join("sample.xmp");
        fs::write(
            &xmp_path,
            r#"<rdf:Description xmp:Rating="3" xmp:Label="Red">
                <dc:subject><rdf:Bag><rdf:li>portfolio</rdf:li></rdf:Bag></dc:subject>
            </rdf:Description>"#,
        )
        .unwrap();

        let imported = import_xmp_adjustments_to_sidecar(
            source_path.to_string_lossy().as_ref(),
            &xmp_path,
            None,
        )
        .unwrap();

        assert_eq!(imported.metadata.rating, 3);
        assert_eq!(
            imported.metadata.tags,
            Some(vec!["portfolio".to_string(), "color:red".to_string()])
        );
        assert_eq!(imported.metadata.adjustments, Value::Null);

        fs::remove_dir_all(test_directory).unwrap();
    }

    #[test]
    fn unsupported_only_import_reports_settings_without_writing_or_erasing_edits() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("sample.jpg");
        let sidecar = dir.path().join("sample.jpg.rrdata");
        let xmp = dir.path().join("sample.xmp");
        let existing = ImageMetadata {
            adjustments: serde_json::json!({"exposure":2,"aiPatches":[{"id":"keep"}]}),
            ..ImageMetadata::default()
        };
        let original = serde_json::to_string(&existing).unwrap();
        fs::write(&sidecar, &original).unwrap();
        fs::write(
            &xmp,
            r#"<root crs:SharpenRadius="0" crs:PerspectiveVertical="2"/>"#,
        )
        .unwrap();
        let imported =
            import_xmp_adjustments_to_sidecar(source.to_str().unwrap(), &xmp, None).unwrap();
        assert!(imported.unchanged);
        assert_eq!(imported.settings_report.ignored, 2);
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), original);
        fs::remove_file(&sidecar).unwrap();
        assert!(
            import_xmp_adjustments_to_sidecar(source.to_str().unwrap(), &xmp, None)
                .unwrap()
                .unchanged
        );
        assert!(!sidecar.exists());
    }

    #[test]
    fn unsupported_only_mask_import_reuses_existing_report_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("sample.jpg");
        let sidecar = dir.path().join("sample.jpg.rrdata");
        let xmp = dir.path().join("sample.xmp");
        fs::write(&xmp, r#"<root><crs:PaintBasedCorrections><rdf:Seq><rdf:li>unsupported</rdf:li></rdf:Seq></crs:PaintBasedCorrections></root>"#).unwrap();
        let imported =
            import_xmp_adjustments_to_sidecar(source.to_str().unwrap(), &xmp, None).unwrap();
        assert!(imported.unchanged);
        assert_eq!(imported.not_transferred, vec!["masks"]);
        assert!(imported.settings_report.settings.is_empty());
        assert!(!sidecar.exists());
    }

    #[test]
    fn report_only_import_does_not_heal_large_exif_or_rewrite_sidecar_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("sample.jpg");
        let sidecar = dir.path().join("sample.jpg.rrdata");
        let xmp = dir.path().join("sample.xmp");
        let metadata = ImageMetadata {
            exif: Some(HashMap::from([(
                "SyntheticLongExif".into(),
                "X".repeat(700),
            )])),
            adjustments: serde_json::json!({"exposure":2}),
            ..ImageMetadata::default()
        };
        let original = serde_json::to_string(&metadata).unwrap();
        fs::write(&sidecar, &original).unwrap();
        fs::write(&xmp, r#"<root crs:SharpenRadius="0"/>"#).unwrap();
        let imported =
            import_xmp_adjustments_to_sidecar(source.to_str().unwrap(), &xmp, None).unwrap();
        assert!(imported.unchanged);
        assert_eq!(
            imported.metadata.exif.as_ref().unwrap()["SyntheticLongExif"].len(),
            700
        );
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), original);
        let snapshot = read_sidecar_report_snapshot(&sidecar).unwrap();
        assert_eq!(snapshot.exif.unwrap()["SyntheticLongExif"].len(), 700);
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), original);
        fs::write(&sidecar, "invalid JSON").unwrap();
        assert!(read_sidecar_report_snapshot(&sidecar).is_err());
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), "invalid JSON");
    }

    #[test]
    fn report_only_with_pending_draft_preserves_unknown_sidecar_bytes_and_does_not_consume_draft() {
        let _mode = super::card_mode_test_support::CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.NEF");
        let sidecar = dir.path().join("photo.NEF.rrdata");
        let xmp = dir.path().join("photo.xmp");
        fs::write(&source, b"untouched raw").unwrap();
        let saved = ImageMetadata {
            adjustments: serde_json::json!({"exposure":2,"futureNativeControl":{"opaque":true}}),
            ..ImageMetadata::default()
        };
        let mut native = serde_json::to_value(&saved).unwrap();
        native["futureTopLevel"] = serde_json::json!({"mustSurvive":"unrecognized"});
        let original = serde_json::to_string_pretty(&native).unwrap();
        fs::write(&sidecar, &original).unwrap();
        let pending = serde_json::json!({"exposure":0.75,"contrast":12});
        for packet in [
            r#"<root crs:PerspectiveVertical="2"/>"#,
            r#"<root crs:AlreadyApplied="True" crs:Exposure2012="4"/>"#,
        ] {
            fs::write(&xmp, packet).unwrap();
            let imported = import_xmp_adjustments_to_sidecar_with_draft(
                source.to_str().unwrap(),
                &xmp,
                None,
                Some(&pending),
                &AppSettings::default(),
            )
            .unwrap();
            assert!(imported.unchanged);
            assert!(!imported.adjustments_applied);
            assert!(!imported.draft_preserved);
            assert_eq!(imported.metadata.adjustments, saved.adjustments);
            assert_eq!(fs::read_to_string(&sidecar).unwrap(), original);
            assert_eq!(fs::read_to_string(&xmp).unwrap(), packet);
            assert_eq!(fs::read(&source).unwrap(), b"untouched raw");
            assert_eq!(pending["exposure"], 0.75);
        }
    }

    #[test]
    fn partial_import_preserves_unrelated_native_and_nested_controls() {
        let mut existing = serde_json::json!({"exposure":2,"aiPatches":[{"id":"keep"}],"hsl":{"reds":{"hue":4,"saturation":8},"blues":{"hue":2}}});
        merge_imported_adjustments(
            &mut existing,
            serde_json::json!({"exposure":0,"hsl":{"reds":{"hue":10}}}),
        );
        assert_eq!(existing["exposure"], 0);
        assert_eq!(existing["hsl"]["reds"]["hue"], 10);
        assert_eq!(existing["hsl"]["reds"]["saturation"], 8);
        assert_eq!(existing["hsl"]["blues"]["hue"], 2);
        assert_eq!(existing["aiPatches"][0]["id"], "keep");
    }

    #[test]
    fn treats_default_only_lightroom_xmp_as_unchanged() {
        let test_directory =
            std::env::temp_dir().join(format!("rapidraw-xmp-unchanged-{}", Uuid::new_v4()));
        fs::create_dir_all(&test_directory).unwrap();

        let source_path = test_directory.join("sample.cr2");
        let xmp_path = test_directory.join("sample.xmp");
        fs::write(
            &xmp_path,
            r#"<rdf:Description
                tiff:ImageWidth="4752"
                tiff:ImageLength="3168"
                crs:CropTop="0"
                crs:CropLeft="0"
                crs:CropBottom="1"
                crs:CropRight="1"
                crs:CropAngle="0" />"#,
        )
        .unwrap();

        let error = import_xmp_adjustments_to_sidecar(
            source_path.to_string_lossy().as_ref(),
            &xmp_path,
            None,
        )
        .unwrap_err();

        assert_eq!(error, NO_SUPPORTED_XMP_CONTENT_ERROR);
        assert!(
            !parse_virtual_path(source_path.to_string_lossy().as_ref())
                .1
                .exists()
        );

        fs::remove_dir_all(test_directory).unwrap();
    }

    #[test]
    fn finds_matching_xmp_sidecars_in_nested_folders() {
        let test_directory =
            std::env::temp_dir().join(format!("rapidraw-xmp-recursive-{}", Uuid::new_v4()));
        let first_directory = test_directory.join("2020").join("06");
        let second_directory = test_directory.join("2021").join("07");
        fs::create_dir_all(&first_directory).unwrap();
        fs::create_dir_all(&second_directory).unwrap();

        let first_image = first_directory.join("duplicate.jpg");
        let first_xmp = first_directory.join("duplicate.xmp");
        let second_image = second_directory.join("duplicate.jpg");
        let second_xmp = second_directory.join("duplicate.XmP");
        fs::write(&first_image, []).unwrap();
        fs::write(&first_xmp, []).unwrap();
        fs::write(&second_image, []).unwrap();
        fs::write(&second_xmp, []).unwrap();
        fs::write(second_directory.join("without-sidecar.jpg"), []).unwrap();
        fs::write(second_directory.join("ignored.txt"), []).unwrap();

        let (matches, skipped, failures) = find_matching_xmp_sidecars_recursive(&test_directory);

        assert_eq!(matches.len(), 2);
        for (image_path, xmp_path) in &matches {
            assert!(xmp_path.is_file());
            assert_eq!(image_path.parent(), xmp_path.parent());
            assert_eq!(image_path.file_stem(), xmp_path.file_stem());
        }
        assert!(
            matches
                .iter()
                .any(|(image_path, _)| image_path == &first_image)
        );
        assert!(
            matches
                .iter()
                .any(|(image_path, _)| image_path == &second_image)
        );
        assert_eq!(skipped, 1);
        assert!(failures.is_empty());

        fs::remove_dir_all(test_directory).unwrap();
    }
}

#[cfg(test)]
mod lightroom_xmp_import_tests {
    use super::card_mode_test_support::{CardMode, folders, path_str, snapshot};
    use super::*;

    #[test]
    fn refused_table_only_import_reports_groups_without_replacing_edits() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("sample.ARW");
        let xmp = raw.with_extension("xmp");
        let (_, sidecar) = parse_virtual_path(raw.to_str().unwrap());
        let existing = ImageMetadata {
            adjustments: serde_json::json!({"exposure":1.25}),
            ..Default::default()
        };
        let original = serde_json::to_vec_pretty(&existing).unwrap();
        fs::write(&sidecar, &original).unwrap();
        let source = r#"<rdf:Description xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
          xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" xmlns:tiff="http://ns.adobe.com/tiff/1.0/"
          tiff:ImageWidth="6000" tiff:ImageLength="4000">
          <crs:MaskGroupBasedCorrections><rdf:Seq><rdf:li rdf:parseType="Resource">
          <crs:CorrectionMasks><rdf:Seq><rdf:li rdf:parseType="Resource"
            crs:What="Mask/Aggregate" crs:MaskName="Brush group"
            crs:MaskBrushTable="00000000000000000000000000000000" crs:MaskBrushUncompressedBytes="185" />
          </rdf:Seq></crs:CorrectionMasks></rdf:li></rdf:Seq></crs:MaskGroupBasedCorrections>
          </rdf:Description>"#;
        fs::write(&xmp, source).unwrap();
        let imported =
            import_xmp_adjustments_to_sidecar(raw.to_str().unwrap(), &xmp, None).unwrap();
        assert!(imported.unchanged);
        assert_eq!(imported.metadata.adjustments, existing.adjustments);
        assert_eq!(fs::read(&sidecar).unwrap(), original);
        assert_eq!(fs::read_to_string(&xmp).unwrap(), source);
        let report = serde_json::to_value(XmpNotTransferred {
            path: raw.to_string_lossy().to_string(),
            items: imported.not_transferred,
            brush_table_refusals: imported.brush_table_refusals,
            mask_failures: imported.mask_failures,
            settings_report: imported.settings_report,
        })
        .unwrap();
        assert_eq!(
            report["items"],
            serde_json::json!(["masks", "maskBrushTable"])
        );
        assert_eq!(report["brushTableRefusals"][0]["name"], "Brush group");
    }

    fn lightroom_xmp(attributes: &str) -> String {
        format!(
            r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
   xmlns:xmp="http://ns.adobe.com/xap/1.0/"
   xmlns:tiff="http://ns.adobe.com/tiff/1.0/"
   xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   {attributes}>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#
        )
    }

    fn adjustment(metadata: &ImageMetadata, key: &str) -> f64 {
        metadata.adjustments[key]
            .as_f64()
            .unwrap_or_else(|| panic!("{key} missing from {}", metadata.adjustments))
    }

    fn leftover_temp_files(folder: &Path) -> Vec<PathBuf> {
        fs::read_dir(folder)
            .unwrap()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(".rapidraw-"))
            })
            .collect()
    }

    #[test]
    fn single_photo_import_retains_source_exif_without_syncing_input() {
        let _mode = CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.NEF");
        let xmp = dir.path().join("photo.xmp");
        fs::write(&raw, b"raw bytes").unwrap();
        fs::write(
            crate::exif_processing::get_rrexif_path(&raw),
            r#"{"Make":"Nikon","Model":"Synthetic cached camera"}"#,
        )
        .unwrap();
        let xml = lightroom_xmp(r#"crs:Exposure2012="1.25""#);
        fs::write(&xmp, &xml).unwrap();
        let mut imported = import_xmp_adjustments_to_sidecar(&path_str(&raw), &xmp, None).unwrap();
        hydrate_imported_xmp_exif(&path_str(&raw), &mut imported, None).unwrap();
        assert_eq!(
            imported
                .metadata
                .exif
                .as_ref()
                .unwrap()
                .get("Model")
                .unwrap(),
            "Synthetic cached camera"
        );
        assert_eq!(imported.metadata.adjustments["exposure"], 1.25);
        assert_eq!(
            crate::exif_processing::load_sidecar(&imported.sidecar_path).exif,
            imported.metadata.exif
        );
        assert_eq!(fs::read_to_string(&xmp).unwrap(), xml);
        assert_eq!(fs::read(&raw).unwrap(), b"raw bytes");
    }

    #[test]
    fn metadata_only_import_persists_the_live_draft_without_rewriting_input() {
        let _mode = CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.NEF");
        let sidecar = dir.path().join("photo.NEF.rrdata");
        let xmp = dir.path().join("photo.xmp");
        fs::write(&raw, b"raw bytes").unwrap();
        let pending = serde_json::json!({"exposure": 0.75, "lensDistortion": 12,
            "curves": {"luma": [{"x":0,"y":0},{"x":255,"y":220}]},
            "masks": [{"id":"pending-mask","subMasks":[]}]});
        for flag in ["", "crs:AlreadyApplied=\"True\" crs:Exposure2012=\"4\""] {
            let xml = lightroom_xmp(&format!("xmp:Rating=\"5\" {flag}"));
            fs::write(&xmp, &xml).unwrap();
            let imported = import_xmp_adjustments_to_sidecar_with_draft(
                &path_str(&raw),
                &xmp,
                None,
                Some(&pending),
                &AppSettings::default(),
            )
            .unwrap();
            assert!(imported.draft_preserved);
            assert!(!imported.adjustments_applied);
            assert_eq!(imported.metadata.adjustments, pending);
            assert_eq!(imported.metadata.rating, 5);
            assert_eq!(
                crate::exif_processing::load_sidecar(&sidecar).adjustments,
                pending
            );
            assert_eq!(fs::read_to_string(&xmp).unwrap(), xml);
            assert_eq!(fs::read(&raw).unwrap(), b"raw bytes");
        }
    }

    #[test]
    fn partial_photo_import_merges_live_controls_and_reports_unsupported_fields_read_only() {
        let _mode = CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.NEF");
        let sidecar = dir.path().join("photo.NEF.rrdata");
        let xmp = dir.path().join("photo.xmp");
        fs::write(&raw, b"raw bytes").unwrap();
        let pending = serde_json::json!({"exposure": 0.75, "contrast": 12,
            "aiPatches": [{"id":"live-repair"}], "hsl": {"reds": {"hue": 4}}});
        let xml = lightroom_xmp(r#"crs:Exposure2012="1.25" crs:PerspectiveVertical="2""#);
        fs::write(&xmp, &xml).unwrap();
        let imported = import_xmp_adjustments_to_sidecar_with_draft(
            &path_str(&raw),
            &xmp,
            None,
            Some(&pending),
            &AppSettings::default(),
        )
        .unwrap();
        let mut expected = pending;
        expected["exposure"] = serde_json::json!(1.25);
        assert!(imported.adjustments_applied);
        assert!(!imported.unchanged);
        assert_eq!(imported.metadata.adjustments, expected);
        assert_eq!(
            crate::exif_processing::load_sidecar(&sidecar).adjustments,
            expected
        );
        assert!(
            imported
                .settings_report
                .settings
                .iter()
                .any(|setting| setting.field == "PerspectiveVertical"
                    && setting.reason == "perspective")
        );
        assert_eq!(fs::read_to_string(&xmp).unwrap(), xml);
        assert_eq!(fs::read(&raw).unwrap(), b"raw bytes");
    }

    #[test]
    fn metadata_only_xmp_import_preserves_saved_adjustments() {
        let _mode = CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.NEF");
        let sidecar = dir.path().join("photo.NEF.rrdata");
        let xmp = dir.path().join("photo.xmp");
        let metadata = ImageMetadata {
            adjustments: serde_json::json!({
                "exposure": 0.75,
                "baselineExposure": true,
                "cameraExposureCompensation": true,
                "curves": {"luma": [{"x": 0, "y": 0}, {"x": 255, "y": 220}]},
                "masks": [{"id": "kept-mask", "subMasks": []}],
                "lensCorrectionMode": "manual",
                "lensDistortion": 12
            }),
            ..Default::default()
        };
        fs::write(&sidecar, serde_json::to_string(&metadata).unwrap()).unwrap();
        fs::write(&raw, b"untouched raw bytes").unwrap();
        let xml = lightroom_xmp(r#"xmp:Rating="5""#);
        fs::write(&xmp, &xml).unwrap();

        let imported = import_xmp_adjustments_to_sidecar(&path_str(&raw), &xmp, None).unwrap();
        assert_eq!(imported.metadata.adjustments, metadata.adjustments);
        assert_eq!(imported.metadata.rating, 5);
        assert_eq!(
            crate::exif_processing::load_sidecar(&sidecar).adjustments,
            metadata.adjustments
        );
        assert_eq!(fs::read(&raw).unwrap(), b"untouched raw bytes");
        assert_eq!(fs::read_to_string(&xmp).unwrap(), xml);
        assert!(leftover_temp_files(dir.path()).is_empty());
    }

    #[test]
    fn already_applied_photo_without_metadata_keeps_sidecar_bytes() {
        let _mode = CardMode::off();
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.NEF");
        let sidecar = dir.path().join("photo.NEF.rrdata");
        let xmp = dir.path().join("photo.xmp");
        let metadata = ImageMetadata {
            adjustments: serde_json::json!({"exposure": 0.75, "contrast": 12}),
            ..Default::default()
        };
        let original = serde_json::to_string(&metadata).unwrap();
        fs::write(&sidecar, &original).unwrap();
        fs::write(&raw, b"untouched raw bytes").unwrap();
        let xml = lightroom_xmp(r#"crs:AlreadyApplied="True" crs:Exposure2012="4""#);
        fs::write(&xmp, &xml).unwrap();

        let imported = import_xmp_adjustments_to_sidecar(&path_str(&raw), &xmp, None).unwrap();
        assert!(imported.already_applied);
        assert!(!imported.adjustments_applied);
        assert!(!imported.metadata_applied);
        assert!(imported.not_transferred.is_empty());
        assert_eq!(imported.metadata.adjustments, metadata.adjustments);
        assert_eq!(fs::read_to_string(&sidecar).unwrap(), original);
        assert_eq!(fs::read(&raw).unwrap(), b"untouched raw bytes");
        assert_eq!(fs::read_to_string(&xmp).unwrap(), xml);
    }

    #[test]
    fn imports_pv2012_basic_adjustments_into_the_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("DSC_0042.NEF");
        let xmp = dir.path().join("DSC_0042.xmp");
        fs::write(&raw, b"raw bytes").unwrap();
        fs::write(
            &xmp,
            lightroom_xmp(
                r#"crs:Version="9.12"
   crs:ProcessVersion="6.7"
   crs:WhiteBalance="As Shot"
   crs:Exposure2012="+0.65"
   crs:Contrast2012="+12"
   crs:Highlights2012="-48"
   crs:Shadows2012="+35"
   crs:Whites2012="+8"
   crs:Blacks2012="-14"
   crs:Clarity2012="+10"
   crs:Dehaze="+5"
   crs:Vibrance="+18"
   crs:Saturation="-6"
   crs:HasCrop="False"
   crs:AlreadyApplied="False""#,
            ),
        )
        .unwrap();

        let imported = import_xmp_adjustments_to_sidecar(&path_str(&raw), &xmp, None).unwrap();

        for (key, expected) in [
            ("exposure", 0.65),
            ("contrast", 12.0),
            ("highlights", -48.0),
            ("shadows", 35.0),
            ("whites", 8.0),
            ("blacks", -14.0),
            ("clarity", 10.0),
            ("dehaze", 5.0),
            ("vibrance", 18.0),
            ("saturation", -6.0),
        ] {
            assert_eq!(adjustment(&imported.metadata, key), expected, "{key}");
        }
        assert!(imported.metadata.adjustments.get("crop").is_none());
        assert!(imported.metadata.adjustments.get("temperature").is_none());

        assert_eq!(
            imported.sidecar_path,
            dir.path().join("DSC_0042.NEF.rrdata")
        );
        let saved = crate::exif_processing::load_sidecar(&imported.sidecar_path);
        assert_eq!(saved.adjustments, imported.metadata.adjustments);
        assert!(leftover_temp_files(dir.path()).is_empty());
        assert_eq!(fs::read(&raw).unwrap(), b"raw bytes");
    }

    #[test]
    fn explicit_xmp_exposure_opts_out_of_a_fresh_camera_baseline_including_zero() {
        let _mode = CardMode::off();
        for exposure in ["0", "1.25"] {
            let dir = tempfile::tempdir().unwrap();
            let raw = dir.path().join("photo.ARW");
            let sidecar = dir.path().join("photo.ARW.rrdata");
            let xmp = dir.path().join("photo.xmp");
            fs::write(&raw, b"untouched raw bytes").unwrap();
            let metadata = ImageMetadata {
                adjustments: crate::image_processing::default_adjustments(),
                ..Default::default()
            };
            fs::write(&sidecar, serde_json::to_string(&metadata).unwrap()).unwrap();
            let xml = lightroom_xmp(&format!(r#"crs:Exposure2012="{exposure}""#));
            fs::write(&xmp, &xml).unwrap();
            let imported = import_xmp_adjustments_to_sidecar(&path_str(&raw), &xmp, None).unwrap();
            assert_eq!(
                adjustment(&imported.metadata, "exposure"),
                exposure.parse::<f64>().unwrap()
            );
            assert_eq!(
                imported.metadata.adjustments["cameraExposureCompensation"],
                false
            );
            assert_eq!(
                crate::exif_processing::load_sidecar(&sidecar).adjustments,
                imported.metadata.adjustments
            );
            assert_eq!(fs::read_to_string(&xmp).unwrap(), xml);
            assert_eq!(fs::read(&raw).unwrap(), b"untouched raw bytes");
        }
    }

    #[test]
    fn imports_a_rotated_crop_in_post_rotation_pixels() {
        // A 4800x3200 crop at (600, 400), straightened by 2.5 degrees. Lightroom
        // stores the crop corners in the unrotated image, normalised.
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("IMG_1234.CR2");
        let xmp = dir.path().join("IMG_1234.xmp");
        fs::write(
            &xmp,
            lightroom_xmp(
                r#"crs:ProcessVersion="11.0"
   tiff:ImageWidth="6000"
   tiff:ImageLength="4000"
   tiff:Orientation="1"
   crs:HasCrop="True"
   crs:AlreadyApplied="False"
   crs:CropLeft="0.112013"
   crs:CropTop="0.074209"
   crs:CropRight="0.887987"
   crs:CropBottom="0.925791"
   crs:CropAngle="2.5"
   crs:CropConstrainToWarp="0""#,
            ),
        )
        .unwrap();

        let imported = import_xmp_adjustments_to_sidecar(&path_str(&raw), &xmp, None).unwrap();

        assert_eq!(
            imported.metadata.adjustments["crop"],
            serde_json::json!({ "x": 600.0, "y": 400.0, "width": 4800.0, "height": 3200.0 })
        );
        assert_eq!(adjustment(&imported.metadata, "rotation"), -2.5);
        assert_eq!(adjustment(&imported.metadata, "aspectRatio"), 1.5);

        let saved = crate::exif_processing::load_sidecar(&imported.sidecar_path);
        assert_eq!(
            saved.adjustments["crop"],
            imported.metadata.adjustments["crop"]
        );
        assert_eq!(saved.adjustments["rotation"], serde_json::json!(-2.5));
    }

    #[test]
    fn imported_lightroom_rating_wins_over_the_camera_rating() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("DSC00001.ARW");
        let sidecar = dir.path().join("DSC00001.ARW.rrdata");
        let xmp = dir.path().join("DSC00001.xmp");
        fs::write(
            &raw,
            crate::exif_processing::rating_samples::sony_arw(true, Some("4"), None),
        )
        .unwrap();
        let shown = |xmp_sync: bool| {
            resolve_image_metadata(
                &raw,
                &sidecar,
                xmp_sync,
                &crate::app_settings::AppSettings::default(),
            )
            .rating
        };
        let import = |attributes: &str| {
            fs::write(&xmp, lightroom_xmp(attributes)).unwrap();
            import_xmp_adjustments_to_sidecar(&path_str(&raw), &xmp, None).unwrap()
        };
        assert_eq!(shown(false), 4);

        let imported = import(r#"crs:Exposure2012="+0.30""#);
        assert!(!imported.metadata.rating_is_explicit);
        assert_eq!(shown(false), 4, "no xmp:Rating keeps the camera rating");

        let imported = import(r#"xmp:Rating="0" crs:Exposure2012="+0.30""#);
        assert!(imported.metadata.rating_is_explicit);
        assert_eq!(
            shown(false),
            0,
            "a rating cleared in Lightroom stays cleared"
        );
        assert_eq!(shown(true), 0);

        import(r#"xmp:Rating="2" crs:Exposure2012="+0.30""#);
        assert_eq!(shown(false), 2);
        assert_eq!(shown(true), 2);
    }

    #[test]
    fn import_into_a_read_only_card_folder_is_refused() {
        let f = folders();
        let edited = lightroom_xmp(r#"crs:Exposure2012="+1.00" crs:Contrast2012="+20""#);
        fs::write(f.dcim.join("IMG_0001.xmp"), &edited).unwrap();
        fs::write(f.dcim.join("IMG_0002.jpg"), b"jpeg bytes").unwrap();
        fs::write(f.dcim.join("IMG_0002.xmp"), &edited).unwrap();
        fs::write(f.library.join("IMG_0001.xmp"), &edited).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let picked_xmp = outside.path().join("picked.xmp");
        fs::write(&picked_xmp, &edited).unwrap();

        let _mode = CardMode::on(&f.card);
        let before = snapshot(&f.card);

        for (image, xmp) in [
            (f.dcim.join("IMG_0001.jpg"), f.dcim.join("IMG_0001.xmp")),
            (f.dcim.join("IMG_0002.jpg"), f.dcim.join("IMG_0002.xmp")),
            (f.dcim.join("IMG_0002.jpg"), picked_xmp.clone()),
        ] {
            let error = import_xmp_adjustments_to_sidecar(&path_str(&image), &xmp, None)
                .expect_err("import onto the card must be refused");
            assert_eq!(error, CARD_READ_ONLY_ERROR);
        }
        let virtual_copy = format!("{}?vc=abc123", path_str(&f.dcim.join("IMG_0001.jpg")));
        assert_eq!(
            import_xmp_adjustments_to_sidecar(&virtual_copy, &picked_xmp, None).unwrap_err(),
            CARD_READ_ONLY_ERROR
        );

        // The folder import runs every match through the same function.
        let (matches, _, _) = find_matching_xmp_sidecars_recursive(f.card.parent().unwrap());
        let mut imported = Vec::new();
        for (image, xmp) in &matches {
            match import_xmp_adjustments_to_sidecar(&path_str(image), xmp, None) {
                Ok(_) => imported.push(image.clone()),
                Err(error) => {
                    assert!(image.starts_with(&f.card), "{}: {error}", image.display());
                    assert_eq!(error, CARD_READ_ONLY_ERROR);
                }
            }
        }
        assert_eq!(imported, vec![f.library.join("IMG_0001.jpg")]);
        assert_eq!(snapshot(&f.card), before);

        let library_sidecar =
            crate::exif_processing::load_sidecar(&f.library.join("IMG_0001.jpg.rrdata"));
        assert!(library_sidecar.adjustments.get("contrast").is_some());
        assert_eq!(library_sidecar.rating, 2);
        assert!(
            library_sidecar
                .tags
                .is_some_and(|tags| tags.contains(&"user:keep".to_string()))
        );
    }

    #[test]
    fn imports_lightroom_masks_atomically_and_never_onto_the_card() {
        let with_masks = crate::lightroom_masks::tests::sidecar_with_corrections(
            "",
            r#"<rdf:li>
      <rdf:Description crs:What="Correction" crs:CorrectionName="Sky" crs:LocalExposure2012="-0.125">
      <crs:CorrectionMasks>
       <rdf:Seq>
        <rdf:li crs:What="Mask/Gradient" crs:MaskValue="1"
         crs:ZeroX="0.5" crs:ZeroY="0.6" crs:FullX="0.5" crs:FullY="0.2"/>
       </rdf:Seq>
      </crs:CorrectionMasks>
      </rdf:Description>
     </rdf:li>"#,
        );
        let f = folders();
        fs::write(f.library.join("IMG_0001.xmp"), &with_masks).unwrap();
        fs::write(f.dcim.join("IMG_0001.xmp"), &with_masks).unwrap();

        let _mode = CardMode::on(&f.card);
        let before = snapshot(&f.card);
        assert_eq!(
            import_xmp_adjustments_to_sidecar(
                &path_str(&f.dcim.join("IMG_0001.jpg")),
                &f.dcim.join("IMG_0001.xmp"),
                None,
            )
            .unwrap_err(),
            CARD_READ_ONLY_ERROR
        );
        assert_eq!(snapshot(&f.card), before);

        let imported = import_xmp_adjustments_to_sidecar(
            &path_str(&f.library.join("IMG_0001.jpg")),
            &f.library.join("IMG_0001.xmp"),
            None,
        )
        .unwrap();
        assert!(imported.not_transferred.is_empty());
        assert!(leftover_temp_files(&f.library).is_empty());

        let sidecar = crate::exif_processing::load_sidecar(&f.library.join("IMG_0001.jpg.rrdata"));
        let masks = sidecar.adjustments["masks"].as_array().unwrap();
        assert_eq!(masks.len(), 1);
        assert_eq!(masks[0]["name"], "Sky");
        assert_eq!(masks[0]["adjustments"]["exposure"], -0.5);
        assert_eq!(masks[0]["subMasks"][0]["type"], "linear");
        // RapidRAW's renderer must accept every imported mask, or it drops them all.
        assert_eq!(
            crate::mask_generation::parse_mask_definitions(&sidecar.adjustments).len(),
            1
        );
    }
}

#[cfg(test)]
mod embedded_rating_tests {
    use super::{resolve_image_metadata, store_user_rating};
    use crate::app_settings::AppSettings;
    use crate::exif_processing::rating_samples::{sony_arw, xmp_packet};
    use std::fs;
    use std::path::PathBuf;

    struct Shot {
        _dir: tempfile::TempDir,
        raw: PathBuf,
        sidecar: PathBuf,
        xmp: PathBuf,
    }

    fn camera_rated(stars: &str) -> Shot {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("DSC00001.ARW");
        fs::write(&raw, sony_arw(true, Some(stars), None)).unwrap();
        Shot {
            sidecar: dir.path().join("DSC00001.ARW.rrdata"),
            xmp: dir.path().join("DSC00001.xmp"),
            raw,
            _dir: dir,
        }
    }

    fn shown_rating(shot: &Shot, xmp_sync: bool) -> u8 {
        resolve_image_metadata(&shot.raw, &shot.sidecar, xmp_sync, &AppSettings::default()).rating
    }

    #[test]
    fn embedded_rating_without_sidecar() {
        let shot = camera_rated("4");
        assert_eq!(shown_rating(&shot, false), 4);
        assert_eq!(shown_rating(&shot, true), 4);
        assert!(
            !shot.sidecar.exists(),
            "reading a camera rating must not create a sidecar"
        );
    }

    #[test]
    fn sidecar_rating_overrides_embedded() {
        let shot = camera_rated("4");
        fs::write(
            &shot.sidecar,
            r#"{"version":1,"rating":2,"adjustments":{}}"#,
        )
        .unwrap();
        assert_eq!(shown_rating(&shot, false), 2);

        let shot = camera_rated("4");
        fs::write(&shot.xmp, xmp_packet("5")).unwrap();
        assert_eq!(shown_rating(&shot, true), 5);
        // Folder metadata reads must not turn a fresh photo into a saved
        // manual recipe before the editor/default policy sees it.
        assert!(
            crate::exif_processing::load_sidecar(&shot.sidecar)
                .adjustments
                .is_null()
        );

        let shot = camera_rated("4");
        store_user_rating(&shot.sidecar, 1);
        fs::write(&shot.xmp, xmp_packet("5")).unwrap();
        assert_eq!(shown_rating(&shot, true), 1);
    }

    #[test]
    fn skeleton_xmp_sidecar_does_not_hide_embedded_rating() {
        let shot = camera_rated("4");
        fs::write(&shot.xmp, xmp_packet("0")).unwrap();
        assert_eq!(shown_rating(&shot, true), 4);
    }

    #[test]
    fn user_cleared_rating_stays_cleared() {
        let shot = camera_rated("4");
        assert_eq!(shown_rating(&shot, false), 4);
        store_user_rating(&shot.sidecar, 0);
        assert_eq!(shown_rating(&shot, false), 0);

        fs::write(&shot.xmp, xmp_packet("5")).unwrap();
        assert_eq!(shown_rating(&shot, true), 0);

        // Other sidecar writes (edits, tags) keep the flag.
        let mut metadata = crate::exif_processing::load_sidecar(&shot.sidecar);
        metadata.tags = Some(vec!["portrait".into()]);
        fs::write(&shot.sidecar, serde_json::to_string(&metadata).unwrap()).unwrap();
        assert_eq!(shown_rating(&shot, true), 0);
    }

    #[test]
    fn unflagged_zero_rating_falls_back_to_embedded() {
        // Sidecars written by edits (or before this flag existed) say nothing about the rating.
        let shot = camera_rated("3");
        fs::write(
            &shot.sidecar,
            r#"{"version":1,"rating":0,"adjustments":{"exposure":0.5}}"#,
        )
        .unwrap();
        assert_eq!(shown_rating(&shot, false), 3);
        assert!(
            !fs::read_to_string(&shot.sidecar)
                .unwrap()
                .contains("rating_is_explicit")
        );
    }

    #[test]
    fn malformed_files_show_no_rating() {
        let shot = camera_rated("4");
        fs::write(&shot.raw, b"II*\0\xff\xff\xff\xff garbage").unwrap();
        assert_eq!(shown_rating(&shot, true), 0);

        fs::write(&shot.sidecar, "{ not json").unwrap();
        fs::write(&shot.xmp, "<xmp:Rating>banana</xmp:Rating>").unwrap();
        assert_eq!(shown_rating(&shot, true), 0);

        fs::remove_file(&shot.raw).unwrap();
        assert_eq!(shown_rating(&shot, false), 0);
    }
}

#[cfg(test)]
mod lens_params_tests {
    use super::*;
    use crate::lens_correction::{Lens, LensDatabase};

    fn canon_db() -> LensDatabase {
        let mut db = LensDatabase {
            cameras: Vec::new(),
            lenses: Vec::new(),
        };
        for file in ["slr-canon.xml", "mil-canon.xml"] {
            let path = format!("{}/lensfun_db/{}", env!("CARGO_MANIFEST_DIR"), file);
            let xml = fs::read_to_string(path).expect("part of the bundled database");
            let mut part: LensDatabase = quick_xml::de::from_str(&xml).expect("parses");
            db.cameras.append(&mut part.cameras);
            db.lenses.append(&mut part.lenses);
        }
        db
    }

    /// The name the lens picker shows for the full frame entry of the
    /// Canon EF 50mm f/1.8 STM.
    fn ef_50_name(db: &LensDatabase) -> String {
        let canon: Vec<&Lens> = db
            .lenses
            .iter()
            .filter(|l| l.get_maker() == "Canon")
            .collect();
        canon
            .iter()
            .find(|l| {
                l.get_canonical_model_name() == "Canon EF 50mm f/1.8 STM"
                    && l.cropfactor == Some(1.0)
            })
            .expect("full frame entry")
            .get_display_name(&canon)
    }

    fn exif() -> Option<HashMap<String, String>> {
        Some(HashMap::from([
            ("Make".to_string(), "Canon".to_string()),
            ("Model".to_string(), "Canon EOS M6 Mark II".to_string()),
            ("FocalLength".to_string(), "50.0 mm".to_string()),
            ("FNumber".to_string(), "1.8".to_string()),
        ]))
    }

    fn resolve(db: &LensDatabase, params: Option<Value>) -> Value {
        let mut adjustments = serde_json::json!({
            "lensCorrectionMode": "manual",
            "lensMaker": "Canon",
            "lensModel": ef_50_name(db),
        });
        if let Some(params) = params {
            adjustments["lensDistortionParams"] = params;
        }
        resolve_lens_params_in_adjustments(&mut adjustments, &exif(), Some(db));
        adjustments["lensDistortionParams"].clone()
    }

    #[test]
    fn saving_an_old_edit_keeps_the_old_values() {
        let db = canon_db();
        // As an older version wrote it: the raw ptlens terms, no radius scale.
        let old = serde_json::json!({
            "k1": 0.0061844f32 as f64, "k2": -0.0313122f32 as f64, "k3": 0.0314815f32 as f64,
            "model": 1, "tca_vr": 1.0000409f32 as f64, "tca_vb": 0.9999893f32 as f64,
            "vig_k1": -1.5829f32 as f64, "vig_k2": 1.2949f32 as f64, "vig_k3": -0.5012f32 as f64,
        });
        let resolved = resolve(&db, Some(old.clone()));
        assert_eq!(resolved, old);
    }

    #[test]
    fn polynomial_tca_saved_marker_survives_the_real_sidecar_resolver() {
        let db: LensDatabase = quick_xml::de::from_str(include_str!(
            "../../rapidroom/validation/lensfun-tca/fixture.xml"
        ))
        .unwrap();
        let base = serde_json::json!({"lensCorrectionMode":"manual","lensMaker":"RapidRoom test fixture","lensModel":"Polynomial TCA reference (synthetic)"});
        let exif = Some(HashMap::from([(
            "FocalLength".to_string(),
            "35 mm".to_string(),
        )]));
        let mut fresh = base.clone();
        resolve_lens_params_in_adjustments(&mut fresh, &exif, Some(&db));
        assert!(fresh["lensDistortionParams"]["tca_poly3"].is_object());
        let snapshot = fresh.clone();
        resolve_lens_params_in_adjustments(&mut fresh, &exif, Some(&db));
        assert_eq!(fresh, snapshot);
        let mut unresolved = base.clone();
        unresolved["lensDistortionParams"] = serde_json::Value::Null;
        resolve_lens_params_in_adjustments(&mut unresolved, &exif, Some(&db));
        assert_eq!(unresolved, snapshot);
        for radius in [false, true] {
            let mut old = base.clone();
            old["lensDistortionParams"] = serde_json::json!({"tca_vr":1.003,"tca_vb":0.998});
            if radius {
                old["lensDistortionParams"]["radius_scale"] = serde_json::json!(1.8);
            }
            resolve_lens_params_in_adjustments(&mut old, &exif, Some(&db));
            assert!(old["lensDistortionParams"].get("tca_poly3").is_none());
            assert_eq!(old["lensDistortionParams"]["tca_vr"], 1.003);
            assert_eq!(old["lensDistortionParams"]["tca_vb"], 0.998);
        }
    }

    #[test]
    fn a_new_edit_gets_the_lensfun_evaluation() {
        let db = canon_db();
        for existing in [
            None,
            Some(serde_json::json!({"k1": 0.1, "radius_scale": 1.0})),
        ] {
            let resolved = resolve(&db, existing);
            let scale = resolved["radius_scale"].as_f64().expect("radius scale");
            assert!((scale - 1.5f64.hypot(1.0) / (1.613f32 as f64)).abs() < 1e-9);
            // The rescaled ptlens term, c / d^2, not the raw a.
            assert!((resolved["k1"].as_f64().unwrap() - 0.0061844).abs() > 1e-3);
        }
    }
}

#[cfg(test)]
mod flag_tests {
    use super::card_mode_test_support::{CardMode, folders, path_str, snapshot};
    use super::{
        CARD_READ_ONLY_ERROR, apply_user_flag, apply_user_rating,
        import_xmp_adjustments_to_sidecar, resolve_image_metadata, update_metadata,
    };
    use crate::app_settings::AppSettings;
    use crate::exif_processing::load_sidecar;
    use crate::exif_processing::rating_samples::{sony_arw, xmp_packet};
    use crate::image_processing::ImageFlag;
    use std::fs;
    use std::path::{Path, PathBuf};

    struct Shot {
        dir: tempfile::TempDir,
        raw: PathBuf,
        sidecar: PathBuf,
        xmp: PathBuf,
    }

    fn camera_rated(stars: &str) -> Shot {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("DSC00001.ARW");
        fs::write(&raw, sony_arw(true, Some(stars), None)).unwrap();
        Shot {
            sidecar: dir.path().join("DSC00001.ARW.rrdata"),
            xmp: dir.path().join("DSC00001.xmp"),
            raw,
            dir,
        }
    }

    fn set_flag(path: &Path, flag: Option<ImageFlag>, xmp_sync: Option<bool>) {
        update_metadata(&[path_str(path)], xmp_sync, |m| apply_user_flag(m, flag)).unwrap();
    }

    fn set_rating(path: &Path, rating: u8, xmp_sync: Option<bool>) {
        update_metadata(&[path_str(path)], xmp_sync, |m| {
            apply_user_rating(m, rating)
        })
        .unwrap();
    }

    fn shown(raw: &Path, sidecar: &Path, xmp_sync: bool) -> (u8, Option<ImageFlag>) {
        let metadata = resolve_image_metadata(raw, sidecar, xmp_sync, &AppSettings::default());
        (metadata.rating, metadata.flag)
    }

    fn xmp_rating(xmp: &Path) -> String {
        let content = fs::read_to_string(xmp).unwrap();
        super::extract_xmp_rating(&content).unwrap().to_string()
    }

    #[test]
    fn a_failed_sidecar_write_returns_an_error_without_syncing_xmp() {
        let shot = camera_rated("4");
        fs::create_dir(&shot.sidecar).unwrap();
        fs::write(&shot.xmp, xmp_packet("3")).unwrap();
        let before = fs::read(&shot.xmp).unwrap();

        let error = update_metadata(&[path_str(&shot.raw)], Some(true), |metadata| {
            apply_user_flag(metadata, Some(ImageFlag::Reject));
        })
        .unwrap_err();

        assert!(error.contains("Failed to save metadata"), "{error}");
        assert!(error.contains(&path_str(&shot.sidecar)), "{error}");
        assert!(shot.sidecar.is_dir());
        assert_eq!(fs::read(&shot.xmp).unwrap(), before);
    }

    #[test]
    fn flag_is_stored_next_to_the_rating() {
        let shot = camera_rated("4");
        set_rating(&shot.raw, 3, None);
        set_flag(&shot.raw, Some(ImageFlag::Pick), None);

        let json = fs::read_to_string(&shot.sidecar).unwrap();
        assert!(json.contains(r#""flag": "pick""#), "{json}");
        let saved = load_sidecar(&shot.sidecar);
        assert_eq!(saved.rating, 3);
        assert_eq!(saved.flag, Some(ImageFlag::Pick));
        assert_eq!(
            shown(&shot.raw, &shot.sidecar, false),
            (3, Some(ImageFlag::Pick))
        );

        set_flag(&shot.raw, Some(ImageFlag::Reject), None);
        assert_eq!(load_sidecar(&shot.sidecar).flag, Some(ImageFlag::Reject));
        assert_eq!(
            load_sidecar(&shot.sidecar).rating,
            3,
            "a reject keeps the stars"
        );

        set_flag(&shot.raw, None, None);
        let json = fs::read_to_string(&shot.sidecar).unwrap();
        assert!(!json.contains(r#""flag":"#), "{json}");
        assert!(json.contains(r#""flag_is_explicit": true"#), "{json}");
        assert_eq!(shown(&shot.raw, &shot.sidecar, false), (3, None));
    }

    #[test]
    fn untouched_sidecars_get_no_flag_fields() {
        let json =
            serde_json::to_string(&crate::image_processing::ImageMetadata::default()).unwrap();
        assert!(!json.contains("flag"), "{json}");

        // Tags and ratings written by other commands don't add them either.
        let shot = camera_rated("4");
        set_rating(&shot.raw, 2, None);
        assert!(!fs::read_to_string(&shot.sidecar).unwrap().contains("flag"));
    }

    #[test]
    fn unknown_flag_values_read_as_unflagged() {
        let shot = camera_rated("4");
        for flag in [r#""maybe""#, "7", "null", r#"{"a":1}"#, r#""PICK""#] {
            fs::write(
                &shot.sidecar,
                format!(
                    r#"{{"version":1,"rating":2,"flag":{flag},"adjustments":{{"exposure":0.5}}}}"#
                ),
            )
            .unwrap();
            let saved = load_sidecar(&shot.sidecar);
            assert_eq!(saved.flag, None, "{flag}");
            assert_eq!(
                saved.rating, 2,
                "{flag}: the rest of the sidecar still loads"
            );
            assert_eq!(saved.adjustments["exposure"], serde_json::json!(0.5));
        }
    }

    #[test]
    fn flag_writes_keep_the_rest_of_the_sidecar() {
        let shot = camera_rated("4");
        fs::write(
            &shot.sidecar,
            r#"{"version":1,"rating":5,"adjustments":{"exposure":1.25},"tags":["user:keep"]}"#,
        )
        .unwrap();
        set_flag(&shot.raw, Some(ImageFlag::Pick), None);

        let saved = load_sidecar(&shot.sidecar);
        assert_eq!(saved.adjustments["exposure"], serde_json::json!(1.25));
        assert_eq!(saved.tags, Some(vec!["user:keep".to_string()]));
        assert_eq!(saved.rating, 5);
        let leftovers: Vec<_> = fs::read_dir(shot.dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn card_mode_refuses_flag_writes() {
        let f = folders();
        let _mode = CardMode::on(&f.card);
        let before = snapshot(&f.card);

        let card_image = f.dcim.join("IMG_0001.jpg");
        for paths in [
            vec![path_str(&card_image)],
            vec![format!("{}?vc=abc123", path_str(&card_image))],
            // One card path refuses the whole batch, so a selection is never half-flagged.
            vec![
                path_str(&f.library.join("IMG_0001.jpg")),
                path_str(&card_image),
            ],
        ] {
            let error = update_metadata(&paths, Some(true), |m| {
                apply_user_flag(m, Some(ImageFlag::Reject))
            })
            .unwrap_err();
            assert_eq!(error, CARD_READ_ONLY_ERROR);
        }
        assert_eq!(snapshot(&f.card), before);
        assert_eq!(
            load_sidecar(&f.library.join("IMG_0001.jpg.rrdata")).flag,
            None
        );

        set_flag(
            &f.library.join("IMG_0001.jpg"),
            Some(ImageFlag::Pick),
            Some(true),
        );
        assert_eq!(
            load_sidecar(&f.library.join("IMG_0001.jpg.rrdata")).flag,
            Some(ImageFlag::Pick)
        );
    }

    #[test]
    fn reject_round_trips_through_xmp_rating() {
        let shot = camera_rated("4");
        set_rating(&shot.raw, 3, Some(true));
        assert_eq!(xmp_rating(&shot.xmp), "3");

        set_flag(&shot.raw, Some(ImageFlag::Reject), Some(true));
        assert_eq!(xmp_rating(&shot.xmp), "-1");
        assert_eq!(load_sidecar(&shot.sidecar).rating, 3);

        // A pick is RapidRoom-only; the .xmp keeps the stars.
        set_flag(&shot.raw, Some(ImageFlag::Pick), Some(true));
        assert_eq!(xmp_rating(&shot.xmp), "3");

        set_flag(&shot.raw, Some(ImageFlag::Reject), Some(true));
        set_flag(&shot.raw, None, Some(true));
        assert_eq!(xmp_rating(&shot.xmp), "3", "unflagging restores the stars");
    }

    #[test]
    fn reject_from_another_app_is_read_from_xmp() {
        let shot = camera_rated("4");
        fs::write(&shot.xmp, xmp_packet("-1")).unwrap();

        assert_eq!(
            shown(&shot.raw, &shot.sidecar, false),
            (4, None),
            "XMP sync off"
        );
        assert_eq!(
            shown(&shot.raw, &shot.sidecar, true),
            (4, Some(ImageFlag::Reject)),
            "the camera's stars still show; -1 is not a star rating"
        );
        assert_eq!(load_sidecar(&shot.sidecar).flag, Some(ImageFlag::Reject));
    }

    #[test]
    fn removed_reject_does_not_come_back_from_xmp() {
        let shot = camera_rated("4");
        fs::write(&shot.xmp, xmp_packet("-1")).unwrap();
        assert_eq!(
            shown(&shot.raw, &shot.sidecar, true).1,
            Some(ImageFlag::Reject)
        );

        // Unflagged with XMP sync off, so the .xmp still says -1.
        set_flag(&shot.raw, None, None);
        assert_eq!(xmp_rating(&shot.xmp), "-1");
        assert_eq!(shown(&shot.raw, &shot.sidecar, true), (4, None));

        // Same for a reject cleared by giving the photo stars.
        let shot = camera_rated("4");
        fs::write(&shot.xmp, xmp_packet("-1")).unwrap();
        assert_eq!(
            shown(&shot.raw, &shot.sidecar, true).1,
            Some(ImageFlag::Reject)
        );
        set_rating(&shot.raw, 2, None);
        assert_eq!(shown(&shot.raw, &shot.sidecar, true), (2, None));
    }

    #[test]
    fn stars_clear_a_reject_but_zero_stars_do_not() {
        let shot = camera_rated("4");
        set_flag(&shot.raw, Some(ImageFlag::Reject), Some(true));
        set_rating(&shot.raw, 0, Some(true));
        assert_eq!(
            shown(&shot.raw, &shot.sidecar, true),
            (0, Some(ImageFlag::Reject))
        );
        assert_eq!(xmp_rating(&shot.xmp), "-1");

        set_rating(&shot.raw, 5, Some(true));
        assert_eq!(shown(&shot.raw, &shot.sidecar, true), (5, None));
        assert_eq!(xmp_rating(&shot.xmp), "5");
    }

    #[test]
    fn a_virtual_copy_never_rejects_the_original() {
        let shot = camera_rated("4");
        let copy = format!("{}?vc=abc123", path_str(&shot.raw));
        let copy_sidecar = shot.dir.path().join("DSC00001.ARW.abc123.rrdata");

        update_metadata(std::slice::from_ref(&copy), Some(true), |m| {
            apply_user_flag(m, Some(ImageFlag::Reject))
        })
        .unwrap();
        assert_eq!(load_sidecar(&copy_sidecar).flag, Some(ImageFlag::Reject));
        assert_ne!(xmp_rating(&shot.xmp), "-1");
        assert_eq!(shown(&shot.raw, &shot.sidecar, true).1, None);

        // The original's reject isn't undone by writes from the copy...
        set_flag(&shot.raw, Some(ImageFlag::Reject), Some(true));
        update_metadata(std::slice::from_ref(&copy), Some(true), |m| {
            apply_user_flag(m, None);
            apply_user_rating(m, 3);
        })
        .unwrap();
        assert_eq!(xmp_rating(&shot.xmp), "-1");
        assert_eq!(
            shown(&shot.raw, &shot.sidecar, true).1,
            Some(ImageFlag::Reject)
        );

        // ...and isn't copied onto a copy that was never flagged.
        let fresh_copy_sidecar = shot.dir.path().join("DSC00001.ARW.def456.rrdata");
        assert_eq!(shown(&shot.raw, &fresh_copy_sidecar, true).1, None);
        assert!(load_sidecar(&fresh_copy_sidecar).flag.is_none());
    }

    #[test]
    fn lightroom_reject_imports_as_a_reject_flag() {
        let shot = camera_rated("4");
        let lr_xmp = shot.dir.path().join("lightroom.xmp");
        fs::write(
            &lr_xmp,
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="-1"/></rdf:RDF></x:xmpmeta>"#,
        )
        .unwrap();

        let imported =
            import_xmp_adjustments_to_sidecar(&path_str(&shot.raw), &lr_xmp, None).unwrap();
        assert_eq!(imported.metadata.flag, Some(ImageFlag::Reject));
        assert!(
            !imported.metadata.rating_is_explicit,
            "-1 is not a star rating"
        );
        assert_eq!(
            shown(&shot.raw, &shot.sidecar, false),
            (4, Some(ImageFlag::Reject))
        );
    }
}

#[cfg(test)]
mod rename_cache_tests {
    use super::*;
    use crate::batch_rename::PathChange;

    #[test]
    fn renamed_thumbnails_are_found_under_the_new_path() {
        let dir = tempfile::tempdir().unwrap();
        let photos = dir.path().join("photos");
        let cache = dir.path().join("thumbs");
        fs::create_dir_all(&photos).unwrap();
        fs::create_dir_all(&cache).unwrap();
        let old = photos.join("IMG_1.jpg");
        fs::write(&old, "jpg").unwrap();
        fs::write(
            photos.join("IMG_1.jpg.abc.rrdata"),
            r#"{"version":1,"rating":0,"adjustments":{"exposure":1.0}}"#,
        )
        .unwrap();
        let old_paths = [
            old.to_string_lossy().into_owned(),
            format!("{}?vc=abc", old.to_string_lossy()),
        ];
        for path in &old_paths {
            let hash = get_cache_key_hash(path).unwrap();
            fs::write(cache.join(format!("{}_small.jpg", hash)), path).unwrap();
            fs::write(cache.join(format!("{}_medium.jpg", hash)), path).unwrap();
        }

        let new = photos.join("trip_1.jpg");
        fs::rename(&old, &new).unwrap();
        fs::rename(
            photos.join("IMG_1.jpg.abc.rrdata"),
            photos.join("trip_1.jpg.abc.rrdata"),
        )
        .unwrap();
        let new_paths = [
            new.to_string_lossy().into_owned(),
            format!("{}?vc=abc", new.to_string_lossy()),
        ];
        let changes: Vec<PathChange> = old_paths
            .iter()
            .zip(&new_paths)
            .map(|(from, to)| PathChange {
                from: from.clone(),
                to: to.clone(),
            })
            .collect();
        migrate_thumbnail_cache(&cache, &changes);

        for (old_path, new_path) in old_paths.iter().zip(&new_paths) {
            let hash = get_cache_key_hash(new_path).unwrap();
            for size in ["small", "medium"] {
                let cached = cache.join(format!("{}_{}.jpg", hash, size));
                assert_eq!(fs::read_to_string(cached).unwrap(), *old_path);
            }
        }
    }
}

#[cfg(test)]
mod xmp_preset_browser_tests {
    use super::*;

    #[test]
    fn xmp_groups_merge_and_keep_profile_compatibility() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preset.xmp");
        fs::write(&path, r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Exposure2012="1" crs:CameraModelRestriction="Sony" crs:CameraProfile="Adobe Standard"><crs:Group><rdf:Alt><rdf:li>Color &amp; Tone</rdf:li></rdf:Alt></crs:Group><crs:Name><rdf:Alt><rdf:li>Warm</rdf:li></rdf:Alt></crs:Name></rdf:Description></rdf:RDF></x:xmpmeta>"#).unwrap();
        let (items, _) = parse_preset_file(path.to_str().unwrap()).unwrap();
        let mut library = Vec::new();
        let mut names = HashSet::new();
        merge_imported_items(&mut library, &mut names, items.clone());
        merge_imported_items(&mut library, &mut names, items);
        assert_eq!(library.len(), 1);
        let PresetItem::Folder(group) = &library[0] else {
            panic!("missing group")
        };
        assert_eq!(group.name, "Color & Tone");
        assert_eq!(group.children.len(), 2);
        assert_ne!(group.children[0].id, group.children[1].id);
        assert_eq!(group.children[1].name, "Warm (1)");
        assert_eq!(
            group.children[0].camera_model_restriction.as_deref(),
            Some("Sony")
        );
        assert!(
            group.children[0]
                .unavailable_reason
                .as_ref()
                .unwrap()
                .contains("Adobe Standard")
        );
    }

    #[test]
    fn presets_show_individual_ignored_fields_and_approximations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preset.xmp");
        fs::write(&path, r#"<root crs:Sharpness="0" crs:SharpenRadius="1"/>"#).unwrap();
        let (_, warnings) = parse_preset_file(path.to_str().unwrap()).unwrap();
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("SharpenRadius: ignored") && w.contains("radius/detail"))
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("Sharpness: approximated") && w.contains("0-150"))
        );
    }

    #[test]
    fn own_xmp_export_reimports_exact_sparse_adjustments_and_applicability() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preset.xmp");
        let mut preset = preset_converter::convert_xmp_to_preset(
            r#"<rdf:Description crs:Exposure2012="1.25"/>"#,
        )
        .unwrap();
        preset.adjustments = serde_json::json!({"exposure":1.25,"brightness":8,"cameraExposureCompensation":true,"hsl":{"reds":{"hue":17}}});
        preset.camera_model_restriction = Some("Sony & Fujifilm".into());
        preset.unavailable_reason = Some("Requires camera profile: Adobe Standard".into());
        let (xml, missing) = crate::preset_xmp::serialize(&preset).unwrap();
        assert!(missing.contains(&"brightness".to_string()));
        assert!(xml.contains("crs:CameraModelRestriction=\"Sony &amp; Fujifilm\""));
        assert!(xml.contains("crs:CameraProfile=\"Adobe Standard\""));
        fs::write(&path, xml).unwrap();
        let (items, warnings) = parse_preset_file(path.to_str().unwrap()).unwrap();
        let PresetItem::Folder(group) = &items[0] else {
            panic!("missing group")
        };
        assert_eq!(group.children[0].adjustments, preset.adjustments);
        assert_eq!(
            group.children[0].camera_model_restriction,
            preset.camera_model_restriction
        );
        assert_eq!(
            group.children[0].unavailable_reason,
            preset.unavailable_reason
        );
        assert!(warnings.is_empty());
        preset.unavailable_reason = Some("Settings not supported: proprietary look".into());
        let (xml, missing) = crate::preset_xmp::serialize(&preset).unwrap();
        assert!(missing.contains(&"unavailableReason".to_string()));
        fs::write(&path, xml).unwrap();
        let (items, _) = parse_preset_file(path.to_str().unwrap()).unwrap();
        let PresetItem::Folder(group) = &items[0] else {
            panic!("missing group")
        };
        assert_eq!(
            group.children[0].unavailable_reason,
            preset.unavailable_reason
        );
        assert_eq!(
            group.children[0].camera_model_restriction,
            preset.camera_model_restriction
        );
    }
}

#[cfg(test)]
mod export_framing_sidecar_transactions {
    use super::*;
    use serde_json::json;

    #[test]
    fn framing_and_stale_edit_saves_preserve_each_others_owned_fields() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("photo.jpg");
        let (_, primary) = parse_virtual_path(raw.to_str().unwrap());
        let (_, copy) = parse_virtual_path(&format!("{}?vc=1", raw.display()));
        let original = json!({"version":1,"adjustments":{"exposure":0,"crop":{"x":12,"y":8,"width":400,"height":300}},"rating":4,"flag":"pick","tags":["trip"],"unknown":{"preserve":true}});
        write_file_atomically(&primary, serde_json::to_vec(&original).unwrap()).unwrap();
        write_file_atomically(&copy, serde_json::to_vec(&original).unwrap()).unwrap();
        let mut old_editor_snapshot = original.clone();
        update_export_framing(&primary, |_| {
            Ok(json!({"instagram":{"version":1,"frame":{"x":0.2}}}))
        })
        .unwrap();
        let after_frame: Value = serde_json::from_slice(&fs::read(&primary).unwrap()).unwrap();
        let mut without_frame = after_frame.clone();
        without_frame
            .as_object_mut()
            .unwrap()
            .remove("exportFraming");
        assert_eq!(without_frame, original);
        old_editor_snapshot["adjustments"]["exposure"] = json!(1.2);
        // This save began before the framing transaction and finishes afterward.
        write_file_atomically(&primary, serde_json::to_vec(&old_editor_snapshot).unwrap()).unwrap();
        let saved: Value = serde_json::from_slice(&fs::read(&primary).unwrap()).unwrap();
        assert_eq!(saved["adjustments"], old_editor_snapshot["adjustments"]);
        assert_eq!(saved["exportFraming"], after_frame["exportFraming"]);
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&copy).unwrap()).unwrap(),
            original
        );
        update_export_framing(&primary, |_| {
            Ok(json!({"instagram":{"version":1,"frame":{"x":0.3}}}))
        })
        .unwrap();
        let saved: Value = serde_json::from_slice(&fs::read(&primary).unwrap()).unwrap();
        assert_eq!(saved["adjustments"], old_editor_snapshot["adjustments"]);
        assert_eq!(saved["rating"], original["rating"]);
        assert_eq!(saved["unknown"], original["unknown"]);
    }

    #[test]
    fn framing_refuses_to_replace_unreadable_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("photo.jpg.rrdata");
        fs::write(&sidecar, b"{ broken sidecar").unwrap();
        assert!(update_export_framing(&sidecar, |_| Ok(json!({"instagram":null}))).is_err());
        assert_eq!(fs::read(sidecar).unwrap(), b"{ broken sidecar");
    }
}

#[cfg(test)]
mod lens_policy_version_tests {
    use super::{import_xmp_adjustments_to_sidecar_with_draft, normalize_lens_policy_version};
    use serde_json::Value;

    #[test]
    fn first_tone_import_uses_fresh_policy_but_saved_and_explicit_lens_recipes_keep_their_policy() {
        for (enabled, saved, explicit_lens) in [
            (true, false, false),
            (false, false, false),
            (true, true, false),
            (true, false, true),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let raw = dir.path().join("photo.ARW");
            let sidecar = dir.path().join("photo.ARW.rrdata");
            let xmp = dir.path().join("photo.xmp");
            let bytes = crate::exif_processing::rating_samples::sony_arw(true, Some("4"), None);
            std::fs::write(&raw, &bytes).unwrap();
            if saved {
                let saved_recipe = crate::image_processing::ImageMetadata {
                    adjustments: serde_json::json!({"lensCorrectionMode":"manual","contrast":20}),
                    ..Default::default()
                };
                std::fs::write(&sidecar, serde_json::to_vec(&saved_recipe).unwrap()).unwrap();
                let restored = crate::exif_processing::load_sidecar(&sidecar);
                assert_eq!(restored.adjustments, saved_recipe.adjustments);
            }
            let lens = if explicit_lens {
                r#"crs:LensProfileEnable="1""#
            } else {
                ""
            };
            std::fs::write(&xmp, format!(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Exposure2012="+1.25" {lens}/></rdf:RDF></x:xmpmeta>"#)).unwrap();
            let settings = crate::app_settings::AppSettings {
                auto_lens_correction_on_new_raws: Some(enabled),
                ..Default::default()
            };
            let result = import_xmp_adjustments_to_sidecar_with_draft(
                raw.to_str().unwrap(),
                &xmp,
                None,
                None,
                &settings,
            )
            .unwrap();
            assert_eq!(std::fs::read(&raw).unwrap(), bytes);
            let recipe = result.metadata.adjustments;
            if saved {
                assert_eq!(recipe["lensCorrectionMode"], "manual");
                assert_eq!(recipe["contrast"], 20);
                assert!(recipe.get("cameraLensCorrectionVersion").is_none());
            } else if explicit_lens {
                assert_eq!(recipe["lensCorrectionMode"], "auto");
                assert_eq!(recipe["cameraLensCorrectionVersion"], 0);
            } else if enabled {
                assert_eq!(recipe["lensCorrectionMode"], "auto");
                assert_eq!(recipe["cameraLensCorrectionVersion"], 1);
            } else {
                assert!(recipe.get("lensCorrectionMode").is_none());
                assert!(recipe.get("cameraLensCorrectionVersion").is_none());
            }
        }
    }

    #[test]
    fn batch_merge_matches_the_shared_legacy_camera_policy_contract() {
        let contract: Value = serde_json::from_str(include_str!(
            "../../rapidroom/validation/lens-policy-version-cases.json"
        ))
        .unwrap();
        for case in contract["cases"].as_array().unwrap() {
            let mut merged = case["target"].clone();
            merged
                .as_object_mut()
                .unwrap()
                .extend(case["source"].as_object().unwrap().clone());
            let mut ordinary = merged.clone();
            ordinary
                .as_object_mut()
                .unwrap()
                .remove("cameraLensCorrectionVersion");
            normalize_lens_policy_version(
                &case["target"],
                &case["source"],
                &mut merged,
                case["amount"].as_f64().unwrap(),
            );
            if case["expected_version"].is_null() {
                assert!(
                    merged.get("cameraLensCorrectionVersion").is_none(),
                    "{}",
                    case["name"]
                );
            } else {
                assert_eq!(
                    merged["cameraLensCorrectionVersion"], case["expected_version"],
                    "{}",
                    case["name"]
                );
            }
            merged
                .as_object_mut()
                .unwrap()
                .remove("cameraLensCorrectionVersion");
            assert_eq!(
                merged, ordinary,
                "Version normalization changed numeric/control payload: {}",
                case["name"]
            );
        }
    }
}

#[cfg(test)]
mod photo_stack_virtual_copy_tests {
    use super::card_mode_test_support::{CardMode, folders};
    use super::*;

    #[test]
    fn creating_virtual_copy_clears_stack_but_preserves_other_metadata() {
        let _mode = CardMode::off();
        let f = folders();
        let a = f.library.join("one.jpg");
        let b = f.library.join("two.jpg");
        fs::write(&a, b"fixture").unwrap();
        fs::write(&b, b"fixture").unwrap();
        let paths: Vec<String> = [&a, &b]
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        crate::photo_stacks::edit_photo_stack(
            paths.clone(),
            crate::photo_stacks::StackAction::Stack,
        )
        .unwrap();
        let (_, source_sidecar) = parse_virtual_path(&paths[0]);
        let mut value: Value =
            serde_json::from_str(&fs::read_to_string(&source_sidecar).unwrap()).unwrap();
        value["adjustments"] = serde_json::json!({"exposure":1.25});
        value["futureField"] = serde_json::json!({"preserved":true});
        fs::write(&source_sidecar, serde_json::to_vec(&value).unwrap()).unwrap();
        let copy = create_virtual_copy_on_disk(&paths[0]).unwrap();
        let (_, copied_sidecar) = parse_virtual_path(&copy);
        let copied: Value =
            serde_json::from_str(&fs::read_to_string(&copied_sidecar).unwrap()).unwrap();
        assert!(copied.get("stack").is_none());
        assert_eq!(copied["adjustments"], value["adjustments"]);
        assert_eq!(copied["futureField"], value["futureField"]);
        assert!(
            crate::exif_processing::load_sidecar(&source_sidecar)
                .stack
                .is_some()
        );
    }

    #[test]
    fn whole_stack_metadata_keeps_virtual_paths_in_single_and_mixed_selections() {
        let _mode = CardMode::off();
        let f = folders();
        let a = f.library.join("one.jpg");
        let b = f.library.join("two.jpg");
        fs::write(&a, b"fixture").unwrap();
        fs::write(&b, b"fixture").unwrap();
        let paths: Vec<String> = [&a, &b]
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        crate::photo_stacks::edit_photo_stack(
            paths.clone(),
            crate::photo_stacks::StackAction::Stack,
        )
        .unwrap();
        let copy = create_virtual_copy_on_disk(&paths[0]).unwrap();
        let only_copy = crate::photo_stacks::metadata_paths(std::slice::from_ref(&copy)).unwrap();
        assert_eq!(only_copy, vec![copy.clone()]);
        update_metadata(&only_copy, None, |meta| apply_user_rating(meta, 3)).unwrap();
        update_metadata(&only_copy, None, |meta| {
            apply_user_flag(meta, Some(ImageFlag::Pick))
        })
        .unwrap();
        let (_, sidecar) = parse_virtual_path(&copy);
        let saved = crate::exif_processing::load_sidecar(&sidecar);
        assert_eq!(saved.rating, 3);
        assert_eq!(saved.flag, Some(ImageFlag::Pick));
        let mixed = crate::photo_stacks::metadata_paths(&[copy.clone(), paths[0].clone()]).unwrap();
        assert_eq!(mixed.len(), 3);
        assert!(mixed.contains(&copy));
        assert!(mixed.contains(&paths[1]));
        update_metadata(&mixed, None, |meta| apply_user_rating(meta, 4)).unwrap();
        update_metadata(&mixed, None, |meta| {
            apply_user_flag(meta, Some(ImageFlag::Reject))
        })
        .unwrap();
        for path in mixed {
            let (_, sidecar) = parse_virtual_path(&path);
            let meta = crate::exif_processing::load_sidecar(&sidecar);
            assert_eq!(meta.rating, 4);
            assert_eq!(meta.flag, Some(ImageFlag::Reject));
        }
    }
}
