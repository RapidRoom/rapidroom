use crate::formats::is_supported_image_file;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};
use walkdir::WalkDir;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoCount {
    pub count: usize,
    pub complete: bool,
}

type CachedCount = (Option<SystemTime>, Instant, PhotoCount);
static CACHE: LazyLock<Mutex<HashMap<PathBuf, CachedCount>>> = LazyLock::new(Mutex::default);
static WORKERS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(2)));

pub fn count_photos(path: &Path, recursive: bool) -> Result<PhotoCount, String> {
    if !path.is_dir() {
        return Err("Folder is no longer available".into());
    }
    let modified = path.metadata().ok().and_then(|m| m.modified().ok());
    if !recursive
        && let Some((mtime, time, result)) = CACHE.lock().unwrap().get(path)
        && *mtime == modified
        && time.elapsed() < Duration::from_secs(5)
    {
        return Ok(result.clone());
    }
    let start = Instant::now();
    let mut result = PhotoCount {
        count: 0,
        complete: true,
    };
    let depth = if recursive { 64 } else { 1 };
    let entries = WalkDir::new(path)
        .follow_links(false)
        .max_open(8)
        .max_depth(depth)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'));
    for (index, entry) in entries.enumerate() {
        if index >= 20_000 || start.elapsed() >= Duration::from_millis(200) {
            result.complete = false;
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                result.complete = false;
                continue;
            }
        };
        if recursive && entry.depth() == depth && entry.file_type().is_dir() {
            result.complete = false;
        }
        if entry.file_type().is_file() && is_supported_image_file(entry.path()) {
            result.count += 1;
        }
    }
    if !recursive {
        let mut cache = CACHE.lock().unwrap();
        if cache.len() >= 512
            && let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, (_, time, _))| *time)
                .map(|(path, _)| path.clone())
        {
            cache.remove(&oldest);
        }
        cache.insert(
            path.to_path_buf(),
            (modified, Instant::now(), result.clone()),
        );
    }
    Ok(result)
}

#[tauri::command]
pub async fn count_folder_photos(path: String, recursive: bool) -> Result<PhotoCount, String> {
    let permit = WORKERS
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        count_photos(Path::new(&path), recursive)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_folder_counts_ignore_hidden_video_and_symlink_loops_and_refresh_after_changes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("day-1")).unwrap();
        std::fs::create_dir(root.join("directory.jpg")).unwrap();
        std::fs::create_dir(root.join(".hidden")).unwrap();
        for file in [
            "one.JPG",
            "video.mp4",
            ".hidden.jpg",
            "day-1/two.ARW",
            ".hidden/three.jpg",
        ] {
            std::fs::write(root.join(file), []).unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(root, root.join("day-1/loop")).unwrap();
        let direct = count_photos(root, false).unwrap();
        assert_eq!(direct.count, 1);
        assert!(direct.complete);
        let recursive = count_photos(root, true).unwrap();
        assert_eq!(recursive.count, 2);
        assert!(recursive.complete);
        std::fs::write(root.join("new.PNG"), []).unwrap();
        assert_eq!(count_photos(root, false).unwrap().count, 2);
        assert!(count_photos(&root.join("missing"), false).is_err());
    }
}
