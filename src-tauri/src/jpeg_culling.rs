//! Batch reporting and guarded Undo for opt-in JPEG culling metadata.
use crate::{
    exif_processing, file_management,
    image_processing::{ImageFlag, ImageMetadata},
    jpeg_metadata,
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};
use tauri::{AppHandle, Emitter};

const MAX_BATCH: usize = 1024;
const MAX_TAG_BYTES: usize = 64 * 1024;
static ACTION: Mutex<()> = Mutex::new(());
static LAST_UNDO: Mutex<Option<BatchUndo>> = Mutex::new(None);
#[derive(Clone, PartialEq)]
pub(crate) struct Fields {
    rating: u8,
    explicit: bool,
    flag: Option<ImageFlag>,
    flag_explicit: bool,
    tags: Option<Vec<String>>,
}
impl Fields {
    fn from(metadata: &ImageMetadata) -> Self {
        Self {
            rating: metadata.rating,
            explicit: metadata.rating_is_explicit,
            flag: metadata.flag,
            flag_explicit: metadata.flag_is_explicit,
            tags: metadata.tags.clone(),
        }
    }
    fn apply(&self, metadata: &mut ImageMetadata) {
        metadata.rating = self.rating;
        metadata.rating_is_explicit = self.explicit;
        metadata.flag = self.flag;
        metadata.flag_is_explicit = self.flag_explicit;
        metadata.tags = self.tags.clone();
    }
}
struct UndoItem {
    path: String,
    source: PathBuf,
    sidecar: PathBuf,
    before: Fields,
    expected: Fields,
    jpeg: jpeg_metadata::Change,
}
struct BatchUndo {
    token: String,
    items: Vec<UndoItem>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Failure {
    path: String,
    reason: String,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    action: &'static str,
    written: usize,
    failures: Vec<Failure>,
    undo_token: Option<String>,
}

pub(crate) fn action_guard() -> Result<MutexGuard<'static, ()>, String> {
    ACTION
        .lock()
        .map_err(|_| "Metadata writer unavailable".into())
}
pub(crate) fn is_jpeg(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("jpg") || s.eq_ignore_ascii_case("jpeg"))
}
fn jpeg_path(path: &str) -> bool {
    is_jpeg(&file_management::parse_virtual_path(path).0)
}

pub(crate) fn capture(paths: &[String]) -> HashMap<String, std::result::Result<Fields, String>> {
    let mut seen = HashSet::new();
    paths
        .iter()
        .filter(|p| jpeg_path(p) && seen.insert(p.as_str()))
        .take(MAX_BATCH)
        .map(|path| {
            let (_, sidecar) = file_management::parse_virtual_path(path);
            let metadata = exif_processing::load_sidecar(&sidecar);
            let value = if tag_bytes(&metadata) > MAX_TAG_BYTES {
                Err("Culling metadata exceeds the Undo memory limit".into())
            } else {
                Ok(Fields::from(&metadata))
            };
            (path.clone(), value)
        })
        .collect()
}
fn tag_bytes(metadata: &ImageMetadata) -> usize {
    metadata
        .tags
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(String::len)
        .sum()
}
fn write_one(path: &str, old: &Fields) -> Result<UndoItem, String> {
    let (source, sidecar) = file_management::parse_virtual_path(path);
    if !jpeg_path(path) {
        return Err("Only JPEG files are supported".into());
    }
    if source.to_string_lossy() != path {
        return Err("Virtual copies do not rewrite the original JPEG".into());
    }
    let metadata = exif_processing::load_sidecar(&sidecar);
    if old
        .tags
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(String::len)
        .sum::<usize>()
        + tag_bytes(&metadata)
        > MAX_TAG_BYTES
    {
        return Err("Culling metadata exceeds the Undo memory limit".into());
    }
    let current = Fields::from(&metadata);
    let rating = if metadata.flag == Some(ImageFlag::Reject) {
        -1
    } else {
        i16::from(exif_processing::resolve_rating(&source, &metadata))
    };
    let label = metadata
        .tags
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find_map(|tag| tag.strip_prefix(crate::tagging::COLOR_TAG_PREFIX))
        .filter(|s| !s.is_empty())
        .map(|color| {
            let mut chars = color.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        });
    let jpeg = jpeg_metadata::write(&source, rating, label.as_deref())?;
    Ok(UndoItem {
        path: path.to_string(),
        source,
        sidecar,
        before: old.clone(),
        expected: current,
        jpeg,
    })
}
fn restore_one(item: &UndoItem, settings: &crate::app_settings::AppSettings) -> Result<(), String> {
    file_management::ensure_card_writable(&item.source)?;
    let mut metadata = exif_processing::load_sidecar(&item.sidecar);
    if Fields::from(&metadata) != item.expected {
        return Err("Culling metadata changed since this write; Undo refused".into());
    }
    jpeg_metadata::restore(&item.source, &item.jpeg.before, item.jpeg.after_hash)?;
    item.before.apply(&mut metadata);
    let text = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
    file_management::write_file_atomically(&item.sidecar, text).map_err(|e| e.to_string())?;
    if settings.enable_xmp_sync.unwrap_or(false) {
        metadata.rating = exif_processing::resolve_rating(&item.source, &metadata);
        file_management::sync_metadata_to_xmp(
            &item.source,
            &item.sidecar,
            &metadata,
            settings.create_xmp_if_missing.unwrap_or(false),
        );
    }
    Ok(())
}

fn emit(app: &AppHandle, report: &Report) {
    if let Err(error) = app.emit("jpeg-metadata-write-report", report) {
        log::warn!("JPEG metadata report delivery failed: {error}");
    }
}
pub(crate) fn sync(
    paths: &[String],
    before: HashMap<String, std::result::Result<Fields, String>>,
    app: &AppHandle,
    automatic: bool,
) {
    let mut report = Report {
        action: "write",
        ..Default::default()
    };
    let mut undo = Vec::new();
    let mut seen = HashSet::new();
    for path in paths
        .iter()
        .filter(|p| (!automatic || jpeg_path(p)) && seen.insert(p.as_str()))
    {
        let result = before
            .get(path)
            .ok_or_else(|| {
                if jpeg_path(path) {
                    "Select at most 1024 JPEGs per batch".into()
                } else {
                    "Only JPEG files are supported".into()
                }
            })
            .and_then(|old| old.as_ref().map_err(Clone::clone))
            .and_then(|old| write_one(path, old));

        match result {
            Ok(item) => {
                report.written += 1;
                undo.push(item);
            }
            Err(reason) => report.failures.push(Failure {
                path: path.clone(),
                reason,
            }),
        }
    }
    if !undo.is_empty() {
        let token = uuid::Uuid::new_v4().to_string();
        report.undo_token = Some(token.clone());
        if let Ok(mut last) = LAST_UNDO.lock() {
            *last = Some(BatchUndo { token, items: undo });
        }
    }
    if report.written > 0 || !report.failures.is_empty() {
        emit(app, &report);
    }
}
#[tauri::command]
pub async fn write_jpeg_metadata_for_paths(
    paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = action_guard()?;
        let before = capture(&paths);
        sync(&paths, before, &app_handle, false);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn undo_jpeg_metadata_write(token: String, app_handle: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = action_guard()?;
        let undo = {
            let mut last = LAST_UNDO.lock().map_err(|_| "Metadata Undo unavailable")?;
            if last.as_ref().is_none_or(|v| v.token != token) {
                return Err("This metadata Undo is no longer available".into());
            }
            last.take().unwrap()
        };
        let mut report = Report {
            action: "undo",
            ..Default::default()
        };
        let settings = crate::app_settings::load_settings(app_handle.clone()).unwrap_or_default();
        for item in undo.items {
            let result = restore_one(&item, &settings);
            match result {
                Ok(()) => report.written += 1,
                Err(reason) => report.failures.push(Failure {
                    path: item.path,
                    reason,
                }),
            }
        }
        emit(&app_handle, &report);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (tempfile::TempDir, String, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("culling.jpg");
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes)
            .encode(&[80; 12], 2, 2, image::ExtendedColorType::Rgb8)
            .unwrap();
        std::fs::write(&path, bytes).unwrap();
        jpeg_metadata::write(&path, 3, Some("Blue")).unwrap();
        let sidecar = path.with_file_name("culling.jpg.rrdata");
        (dir, path.to_string_lossy().into_owned(), sidecar)
    }
    fn save(path: &std::path::Path, metadata: &ImageMetadata) {
        std::fs::write(path, serde_json::to_vec(metadata).unwrap()).unwrap();
    }
    #[test]
    fn undo_restores_culling_values_and_keeps_later_develop_adjustments() {
        let (_dir, path, sidecar) = fixture();
        let old = Fields::from(&ImageMetadata::default());
        let mut metadata = ImageMetadata {
            rating: 3,
            flag: Some(ImageFlag::Reject),
            flag_is_explicit: true,
            ..Default::default()
        };
        save(&sidecar, &metadata);
        let item = write_one(&path, &old).unwrap();
        assert_eq!(
            exif_processing::read_embedded_rating(std::path::Path::new(&path)),
            None
        );
        metadata.adjustments = json!({"exposure":1.5});
        save(&sidecar, &metadata);
        restore_one(&item, &Default::default()).unwrap();
        let actual = exif_processing::load_sidecar(&sidecar);
        assert_eq!(actual.adjustments, json!({"exposure":1.5}));
        assert_eq!(actual.flag, None);
        assert_eq!(actual.rating, 0);
        assert!(!actual.rating_is_explicit);
        assert_eq!(
            exif_processing::resolve_rating(std::path::Path::new(&path), &actual),
            3
        );
    }
    #[test]
    fn undo_refuses_later_culling_changes_and_virtual_copies_never_touch_master() {
        let (_dir, path, sidecar) = fixture();
        let before = std::fs::read(&path).unwrap();
        let old = Fields::from(&Default::default());
        assert!(write_one(&format!("{path}?vc=abcdef"), &old).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let mut metadata = ImageMetadata {
            rating: 4,
            rating_is_explicit: true,
            ..Default::default()
        };
        save(&sidecar, &metadata);
        let item = write_one(&path, &old).unwrap();
        let written = std::fs::read(&path).unwrap();
        metadata.rating = 5;
        save(&sidecar, &metadata);
        assert!(restore_one(&item, &Default::default()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), written);
        assert_eq!(exif_processing::load_sidecar(&sidecar).rating, 5);
    }
    #[test]
    fn undo_capture_is_deduplicated_and_bounds_tag_memory() {
        let (_dir, path, sidecar) = fixture();
        let metadata = ImageMetadata {
            tags: Some(vec!["x".repeat(MAX_TAG_BYTES + 1)]),
            ..Default::default()
        };
        save(&sidecar, &metadata);
        let captured = capture(&vec![path.clone(); MAX_BATCH + 1]);
        assert_eq!(captured.len(), 1);
        assert!(captured[&path].is_err());
    }
}
