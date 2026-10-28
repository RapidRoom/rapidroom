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
    restored_hash: Option<blake3::Hash>,
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
    warnings: Vec<Failure>,
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
    let label = match label.as_deref() {
        Some(value) => jpeg_metadata::LabelUpdate::Set(value),
        None if old.tags.as_deref().unwrap_or_default().iter().any(|tag| {
            tag.strip_prefix(crate::tagging::COLOR_TAG_PREFIX)
                .is_some_and(|value| !value.is_empty())
        }) =>
        {
            jpeg_metadata::LabelUpdate::Clear
        }
        None => jpeg_metadata::LabelUpdate::Preserve,
    };
    let jpeg = jpeg_metadata::write(&source, rating, label)?;
    Ok(UndoItem {
        path: path.to_string(),
        source,
        sidecar,
        before: old.clone(),
        expected: current,
        jpeg,
        restored_hash: None,
    })
}
fn restore_one(
    item: &mut UndoItem,
    settings: &crate::app_settings::AppSettings,
) -> Result<(), String> {
    restore_one_with(item, settings, || {})
}
fn restore_one_with(
    item: &mut UndoItem,
    settings: &crate::app_settings::AppSettings,
    before_sidecar_write: impl FnOnce(),
) -> Result<(), String> {
    file_management::ensure_card_writable(&item.source)?;
    let mut metadata = exif_processing::load_sidecar(&item.sidecar);
    if Fields::from(&metadata) != item.expected {
        return Err("Culling metadata changed since this write; Undo refused".into());
    }
    let restored = jpeg_metadata::restore(
        &item.source,
        &item.jpeg.before,
        item.restored_hash.unwrap_or(item.jpeg.after_hash),
    )?;
    item.restored_hash = Some(restored.after_hash);
    item.jpeg.warning = restored.warning;
    // Dedicated writers do not all share ACTION. Lock their read/modify/write
    // transaction after the potentially long JPEG restore, then merge current edits.
    let _sidecar_guard = file_management::lock_sidecar(&item.sidecar).map_err(|e| {
        format!("JPEG restored, but sidecar lock failed: {e}. Undo retained; fix the write error and retry")
    })?;
    metadata = exif_processing::load_sidecar(&item.sidecar);
    if Fields::from(&metadata) != item.expected {
        return Err(
            "JPEG restored; sidecar culling values changed during restore. Undo retained".into(),
        );
    }
    before_sidecar_write();
    item.before.apply(&mut metadata);
    let text = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
    file_management::write_sidecar_locked(&item.sidecar, text).map_err(|e| {
        format!("JPEG restored, but sidecar save failed: {e}. Undo retained; fix the write error and retry")
    })?;
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

fn restore_batch_with(
    undo: BatchUndo,
    mut restore: impl FnMut(&mut UndoItem) -> Result<(), String>,
) -> (Report, Option<BatchUndo>) {
    let mut report = Report {
        action: "undo",
        ..Default::default()
    };
    let mut failed = Vec::new();
    for mut item in undo.items {
        let result = restore(&mut item);
        if let Some(reason) = item.jpeg.warning.take() {
            report.warnings.push(Failure {
                path: item.path.clone(),
                reason,
            });
        }
        match result {
            Ok(()) => report.written += 1,
            Err(reason) => {
                report.failures.push(Failure {
                    path: item.path.clone(),
                    reason,
                });
                failed.push(item);
            }
        }
    }
    let retry = if failed.is_empty() {
        None
    } else {
        report.undo_token = Some(undo.token.clone());
        Some(BatchUndo {
            token: undo.token,
            items: failed,
        })
    };
    (report, retry)
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
                if let Some(reason) = &item.jpeg.warning {
                    report.warnings.push(Failure {
                        path: item.path.clone(),
                        reason: reason.clone(),
                    });
                }
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
        let settings =
            crate::app_settings::load_settings(app_handle.clone()).unwrap_or_else(|_| {
                crate::app_settings::AppSettings {
                    enable_xmp_sync: Some(false),
                    ..Default::default()
                }
            });
        let (report, retry) = restore_batch_with(undo, |item| restore_one(item, &settings));
        *LAST_UNDO.lock().map_err(|_| "Metadata Undo unavailable")? = retry;
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
        let mut item = write_one(&path, &old).unwrap();
        assert_eq!(
            exif_processing::read_embedded_rating(std::path::Path::new(&path)),
            None
        );
        metadata.adjustments = json!({"exposure":1.5});
        save(&sidecar, &metadata);
        restore_one_with(&mut item, &Default::default(), || {
            let lock_path = sidecar.with_file_name(".culling.jpg.rrdata.rapidroom-lock");
            let competing_writer = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(lock_path)
                .unwrap();
            assert!(matches!(
                competing_writer.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
        })
        .unwrap();
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
    fn culling_rating_preserves_an_external_label_but_explicit_colour_removal_clears_it() {
        let (_dir, path, sidecar) = fixture();
        let old = Fields::from(&ImageMetadata::default());
        jpeg_metadata::write(std::path::Path::new(&path), 2, Some("Approved")).unwrap();
        let mut metadata = ImageMetadata {
            rating: 4,
            rating_is_explicit: true,
            ..Default::default()
        };
        save(&sidecar, &metadata);
        let mut rated = write_one(&path, &old).unwrap();
        assert!(String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains("Approved"));
        restore_one(&mut rated, &Default::default()).unwrap();
        metadata.tags = Some(vec!["color:red".into(), "user:x".into()]);
        let old = Fields::from(&metadata);
        metadata.tags = Some(vec!["user:x".into()]);
        save(&sidecar, &metadata);
        write_one(&path, &old).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("Approved"));
        assert_eq!(exif_processing::load_sidecar(&sidecar).tags, metadata.tags);
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
        let mut item = write_one(&path, &old).unwrap();
        let written = std::fs::read(&path).unwrap();
        metadata.rating = 5;
        save(&sidecar, &metadata);
        assert!(restore_one(&mut item, &Default::default()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), written);
        assert_eq!(exif_processing::load_sidecar(&sidecar).rating, 5);
    }
    #[test]
    fn develop_autosave_during_jpeg_restore_merges_after_undo_without_losing_culling_or_adjustments()
     {
        use std::sync::{Arc, Barrier, mpsc};
        let (_dir, path, sidecar) = fixture();
        let old = Fields::from(&ImageMetadata::default());
        let metadata = ImageMetadata {
            rating: 4,
            rating_is_explicit: true,
            ..Default::default()
        };
        save(&sidecar, &metadata);
        let mut item = write_one(&path, &old).unwrap();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let (entered_undo, release_undo) = (entered.clone(), release.clone());
        let undo = std::thread::spawn(move || {
            let _guard = action_guard().unwrap();
            restore_one_with(&mut item, &Default::default(), || {
                entered_undo.wait();
                release_undo.wait();
            })
            .unwrap();
        });
        entered.wait();
        let (attempted_tx, attempted_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let save_path = path.clone();
        let save_sidecar = sidecar.clone();
        let autosave = std::thread::spawn(move || {
            attempted_tx.send(()).unwrap();
            file_management::save_develop_metadata(
                std::path::Path::new(&save_path),
                &save_sidecar,
                &Default::default(),
                |metadata| {
                    assert!(matches!(
                        ACTION.try_lock(),
                        Err(std::sync::TryLockError::WouldBlock)
                    ));
                    metadata.adjustments = json!({"exposure":1.5});
                },
            )
            .unwrap();
            done_tx.send(()).unwrap();
        });
        attempted_rx.recv().unwrap();
        assert!(done_rx.try_recv().is_err());
        release.wait();
        undo.join().unwrap();
        autosave.join().unwrap();
        let actual = exif_processing::load_sidecar(&sidecar);
        assert_eq!(actual.adjustments, json!({"exposure":1.5}));
        assert_eq!(actual.rating, 0);
        assert!(!actual.rating_is_explicit);
        assert_eq!(
            exif_processing::resolve_rating(std::path::Path::new(&path), &actual),
            3
        );
    }
    #[test]
    fn partially_restored_undo_retains_its_token_and_retries_the_sidecar_write() {
        let (_dir, path, sidecar) = fixture();
        let metadata = ImageMetadata {
            rating: 4,
            rating_is_explicit: true,
            ..Default::default()
        };
        save(&sidecar, &metadata);
        let old = Fields::from(&ImageMetadata::default());
        let item = write_one(&path, &old).unwrap();
        let token = "retained-undo".to_string();
        let undo = BatchUndo {
            token: token.clone(),
            items: vec![item],
        };
        let (failed, retry) = restore_batch_with(undo, |item| {
            restore_one_with(item, &Default::default(), || {
                // Force the write to fail after the original JPEG has been restored.
                std::fs::remove_file(&sidecar).unwrap();
                std::fs::create_dir(&sidecar).unwrap();
            })
        });
        assert_eq!(failed.written, 0);
        assert_eq!(failed.undo_token, Some(token));
        assert!(
            failed.failures[0]
                .reason
                .contains("JPEG restored, but sidecar save failed")
        );
        let retry = retry.unwrap();
        assert!(retry.items[0].restored_hash.is_some());
        std::fs::remove_dir(&sidecar).unwrap();
        save(&sidecar, &metadata);
        let (done, retry) =
            restore_batch_with(retry, |item| restore_one(item, &Default::default()));
        assert_eq!(done.written, 1);
        assert!(retry.is_none());
        assert!(done.undo_token.is_none());
        let actual = exif_processing::load_sidecar(&sidecar);
        assert_eq!(actual.rating, 0);
        assert!(!actual.rating_is_explicit);
    }
    #[test]
    #[cfg(unix)]
    fn directory_sync_warning_keeps_write_undo_and_reports_successful_restoration() {
        let (_dir, path, sidecar) = fixture();
        let metadata = ImageMetadata {
            rating: 4,
            rating_is_explicit: true,
            ..Default::default()
        };
        save(&sidecar, &metadata);
        let item = jpeg_metadata::with_directory_sync_failure(|| {
            write_one(&path, &Fields::from(&Default::default()))
        })
        .unwrap();
        assert!(item.jpeg.warning.is_some());
        let undo = BatchUndo {
            token: "durability-warning".into(),
            items: vec![item],
        };
        let (report, retry) = restore_batch_with(undo, |item| {
            jpeg_metadata::with_directory_sync_failure(|| restore_one(item, &Default::default()))
        });
        assert_eq!(report.written, 1);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.failures.is_empty());
        assert!(retry.is_none());
        assert!(report.warnings[0].reason.contains("metadata committed"));
        let actual = exif_processing::load_sidecar(&sidecar);
        assert_eq!(actual.rating, 0);
        assert!(!actual.rating_is_explicit);
        assert_eq!(
            exif_processing::resolve_rating(std::path::Path::new(&path), &actual),
            3
        );
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
