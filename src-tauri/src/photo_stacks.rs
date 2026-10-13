//! Folder-scoped stacks use an ID and position, never absolute member paths.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::file_management::{
    ensure_card_writable_for_paths, parse_virtual_path, write_file_atomically,
};
use crate::image_processing::ImageMetadata;

static STACK_WRITES: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct StackMember {
    pub id: String,
    pub position: u32,
}

#[derive(Debug, Serialize)]
pub struct StackUpdate {
    path: String,
    stack: Option<StackMember>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StackAction {
    Stack,
    Unstack,
    Cover,
}

fn metadata(path: &Path) -> Result<ImageMetadata, String> {
    if !path.exists() {
        return Ok(ImageMetadata::default());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("Cannot read sidecar {}: {e}", path.display()))
}

fn source(path: &str) -> Result<PathBuf, String> {
    if path.contains('?') {
        return Err("Virtual copies cannot be stacked".into());
    }
    let logical = PathBuf::from(path);
    let physical = std::fs::canonicalize(&logical).map_err(|e| e.to_string())?;
    if !physical.is_file() || !crate::formats::is_supported_image_file(&logical) {
        return Err("Stacks require local image files".into());
    }
    Ok(logical)
}

// Preflight every sidecar, then replace each atomically. Restore exact bytes on
// an I/O error and report any rollback failures rather than claiming success.
fn persist(updates: &[(PathBuf, Option<StackMember>)]) -> Result<Vec<StackUpdate>, String> {
    persist_with(updates, |path, bytes| write_file_atomically(path, bytes))
}

fn persist_with(
    updates: &[(PathBuf, Option<StackMember>)],
    write: impl Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> Result<Vec<StackUpdate>, String> {
    let _metadata_lock = crate::file_management::SIDECAR_METADATA_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    let paths: Vec<String> = updates
        .iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    ensure_card_writable_for_paths(&paths)?;
    let mut writes = Vec::new();
    for (path, stack) in updates {
        let (_, sidecar) = parse_virtual_path(&path.to_string_lossy());
        let old = match std::fs::read(&sidecar) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        let mut meta = metadata(&sidecar)?;
        meta.stack = stack.clone();
        let json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
        writes.push((sidecar, old, json));
    }
    for (index, (sidecar, _, json)) in writes.iter().enumerate() {
        if let Err(error) = write(sidecar, json.as_bytes()) {
            let mut failures = Vec::new();
            for (saved, old, _) in writes[..index].iter().rev() {
                let result = match old {
                    Some(bytes) => write(saved, bytes),
                    None => std::fs::remove_file(saved),
                };
                if let Err(e) = result {
                    failures.push(format!("{}: {e}", saved.display()));
                }
            }
            return Err(format!(
                "Stack save failed: {error}; rollback failures: {}",
                failures.join(", ")
            ));
        }
    }
    Ok(updates
        .iter()
        .map(|(p, stack)| StackUpdate {
            path: p.to_string_lossy().into_owned(),
            stack: stack.clone(),
        })
        .collect())
}

pub(crate) fn expanded(paths: &[String]) -> Result<Vec<PathBuf>, String> {
    let _metadata_lock = crate::file_management::SIDECAR_METADATA_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    let selected: Vec<PathBuf> = paths.iter().map(|p| source(p)).collect::<Result<_, _>>()?;
    let mut all = selected.clone();
    let mut ids: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    for path in &selected {
        let (_, sidecar) = parse_virtual_path(&path.to_string_lossy());
        if let Some(stack) = metadata(&sidecar)?.stack {
            ids.entry(path.parent().unwrap().to_path_buf())
                .or_default()
                .insert(stack.id);
        }
    }
    for (folder, stack_ids) in ids {
        let mut members = Vec::new();
        for entry in std::fs::read_dir(folder).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if !path.is_file() || !crate::formats::is_supported_image_file(&path) {
                continue;
            }
            let (_, sidecar) = parse_virtual_path(&path.to_string_lossy());
            if let Some(stack) = metadata(&sidecar)?.stack
                && stack_ids.contains(&stack.id)
            {
                members.push((stack.position, source(&path.to_string_lossy())?));
            }
        }
        members.sort();
        all.extend(members.into_iter().map(|(_, p)| p));
    }
    let mut seen = HashSet::new();
    all.retain(|p| seen.insert(std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())));
    Ok(all)
}

pub(crate) fn metadata_paths(paths: &[String]) -> Result<Vec<String>, String> {
    let (virtual_paths, physical): (Vec<_>, Vec<_>) =
        paths.iter().cloned().partition(|p| p.contains("?vc="));
    let mut targets: Vec<String> = expanded(&physical)?
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    targets.extend(virtual_paths);
    let mut seen = HashSet::new();
    targets.retain(|p| seen.insert(p.clone()));
    Ok(targets)
}

fn group_updates(paths: &[PathBuf]) -> Result<Vec<(PathBuf, Option<StackMember>)>, String> {
    if paths.len() < 2 {
        return Err("Select at least two images to stack".into());
    }
    let folder = parent_identity(&paths[0]);
    if paths.iter().any(|p| parent_identity(p) != folder) {
        return Err("A stack must be in one folder".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    Ok(paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                p.clone(),
                Some(StackMember {
                    id: id.clone(),
                    position: i as u32,
                }),
            )
        })
        .collect())
}

#[tauri::command]
pub fn edit_photo_stack(
    paths: Vec<String>,
    action: StackAction,
) -> Result<Vec<StackUpdate>, String> {
    let _lock = STACK_WRITES.lock().map_err(|e| e.to_string())?;
    ensure_card_writable_for_paths(&paths)?;
    if paths.is_empty() {
        return Err("No images selected".into());
    }
    let all = expanded(&paths)?;
    let updates = match action {
        StackAction::Stack => group_updates(&all)?,
        StackAction::Unstack => {
            let mut updates = Vec::new();
            for path in all {
                let (_, sidecar) = parse_virtual_path(&path.to_string_lossy());
                if metadata(&sidecar)?.stack.is_some() {
                    updates.push((path, None));
                }
            }
            updates
        }
        StackAction::Cover => {
            if paths.len() != 1 {
                return Err("Choose one cover image".into());
            }
            let (_, sidecar) = parse_virtual_path(&paths[0]);
            if metadata(&sidecar)?.stack.is_none() {
                return Err("Image is not stacked".into());
            }
            group_updates(&all)?
        }
    };
    persist(&updates)
}

fn capture_time(exif: &HashMap<String, String>) -> Option<NaiveDateTime> {
    let date = exif.get("DateTimeOriginal")?.trim().trim_matches('"');
    let base = NaiveDateTime::parse_from_str(date, "%Y:%m:%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(date, "%Y-%m-%d %H:%M:%S"))
        .ok()?;
    let digits = exif
        .get("SubSecTimeOriginal")
        .map(|s| s.trim().trim_matches('"'))
        .unwrap_or("");
    if !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits: String = digits.chars().take(9).collect();
    let nanos = digits.parse::<i64>().unwrap_or(0) * 10i64.pow(9 - digits.len() as u32);
    base.checked_add_signed(chrono::Duration::nanoseconds(nanos))
}

fn parent_identity(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf())
}

fn time_groups(
    mut photos: Vec<(PathBuf, NaiveDateTime)>,
    seconds: f64,
) -> Result<Vec<Vec<String>>, String> {
    if !seconds.is_finite() || !(0.0..=3600.0).contains(&seconds) {
        return Err("Use a capture-time gap from 0 to 3600 seconds".into());
    }
    photos.sort_by(|a, b| {
        parent_identity(&a.0)
            .cmp(&parent_identity(&b.0))
            .then(a.1.cmp(&b.1))
            .then(a.0.cmp(&b.0))
    });
    let gap = chrono::Duration::nanoseconds((seconds * 1e9).round() as i64);
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut previous: Option<(PathBuf, NaiveDateTime)> = None;
    for (path, time) in photos {
        let same = previous.as_ref().is_some_and(|(prev, t)| {
            parent_identity(prev) == parent_identity(&path) && time - *t <= gap
        });
        if !same {
            groups.push(Vec::new());
        }
        groups
            .last_mut()
            .unwrap()
            .push(path.to_string_lossy().into_owned());
        previous = Some((path, time));
    }
    groups.retain(|g| g.len() > 1);
    Ok(groups)
}

#[tauri::command]
pub async fn preview_photo_stacks(
    paths: Vec<String>,
    seconds: f64,
) -> Result<Vec<Vec<String>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut photos = Vec::new();
        let mut seen = HashSet::new();
        for path in paths {
            let path = source(&path)?;
            if !seen.insert(std::fs::canonicalize(&path).map_err(|e| e.to_string())?) {
                continue;
            }
            let (_, sidecar) = parse_virtual_path(&path.to_string_lossy());
            let meta = metadata(&sidecar)?;
            if meta.stack.is_some() {
                continue;
            }
            let exif = meta.exif.unwrap_or_else(|| {
                crate::file_management::read_file_mapped(&path)
                    .map(|bytes| {
                        crate::exif_processing::read_exif_data_from_bytes(
                            &path.to_string_lossy(),
                            &bytes,
                        )
                    })
                    .unwrap_or_default()
            });
            if let Some(time) = capture_time(&exif) {
                photos.push((path, time));
            }
        }
        time_groups(photos, seconds)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn apply_photo_stacks(groups: Vec<Vec<String>>) -> Result<Vec<StackUpdate>, String> {
    let _lock = STACK_WRITES.lock().map_err(|e| e.to_string())?;
    let mut updates = Vec::new();
    let mut seen = HashSet::new();
    for group in groups {
        let paths = group
            .iter()
            .map(|p| source(p))
            .collect::<Result<Vec<_>, _>>()?;
        for path in &paths {
            if !seen.insert(std::fs::canonicalize(path).map_err(|e| e.to_string())?) {
                return Err("An image belongs to multiple preview groups".into());
            }
            let (_, sidecar) = parse_virtual_path(&path.to_string_lossy());
            if metadata(&sidecar)?.stack.is_some() {
                return Err("Stacks changed; preview again before applying".into());
            }
        }
        updates.extend(group_updates(&paths)?);
    }
    persist(&updates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_management::card_mode_test_support::{CardMode, folders, snapshot};

    fn time(date: &str, sub: &str) -> NaiveDateTime {
        capture_time(&HashMap::from([
            ("DateTimeOriginal".into(), date.into()),
            ("SubSecTimeOriginal".into(), sub.into()),
        ]))
        .unwrap()
    }

    #[test]
    fn subseconds_order_and_split_precisely_without_crossing_folders() {
        let date = "2026:10:12 12:00:00";
        let groups = time_groups(
            vec![
                (PathBuf::from("/a/late.jpg"), time(date, "90")),
                (PathBuf::from("/a/first.jpg"), time(date, "1")),
                (PathBuf::from("/a/second.jpg"), time(date, "200000001")),
                (PathBuf::from("/b/other.jpg"), time(date, "1")),
            ],
            0.100000001,
        )
        .unwrap();
        assert_eq!(groups, vec![vec!["/a/first.jpg", "/a/second.jpg"]]);
        assert!(capture_time(&HashMap::new()).is_none());
        assert!(
            capture_time(&HashMap::from([
                ("DateTimeOriginal".into(), date.into()),
                ("SubSecTimeOriginal".into(), "bad".into())
            ]))
            .is_none()
        );
        assert!(time_groups(Vec::new(), f64::NAN).is_err());
        assert!(time_groups(Vec::new(), -1.0).is_err());
    }

    #[test]
    fn membership_survives_rename_and_subset_cover_and_unstack() {
        let _mode = CardMode::off();
        let f = folders();
        let a = f.library.join("one.jpg");
        let b = f.library.join("two.jpg");
        std::fs::write(&a, b"fixture").unwrap();
        std::fs::write(&b, b"fixture").unwrap();
        let paths = vec![
            a.to_string_lossy().into_owned(),
            b.to_string_lossy().into_owned(),
        ];
        let (_, sidecar) = parse_virtual_path(&paths[0]);
        let original = ImageMetadata {
            rating: 4,
            adjustments: serde_json::json!({"exposure": 1.2}),
            ..Default::default()
        };
        write_file_atomically(&sidecar, serde_json::to_vec(&original).unwrap()).unwrap();
        let updates = edit_photo_stack(paths.clone(), StackAction::Stack).unwrap();
        assert_eq!(updates[0].stack.as_ref().unwrap().position, 0);
        assert_eq!(
            updates[0].stack.as_ref().unwrap().id,
            updates[1].stack.as_ref().unwrap().id
        );
        let saved = metadata(&sidecar).unwrap();
        assert_eq!(saved.rating, 4);
        assert_eq!(saved.adjustments, original.adjustments);
        let renamed = f.library.join("renamed.jpg");
        let (_, new_sidecar) = parse_virtual_path(&renamed.to_string_lossy());
        std::fs::rename(&a, &renamed).unwrap();
        std::fs::rename(sidecar, new_sidecar).unwrap();
        let updates = edit_photo_stack(vec![paths[1].clone()], StackAction::Cover).unwrap();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].path, paths[1]);
        assert_eq!(updates[0].stack.as_ref().unwrap().position, 0);
        let updates = edit_photo_stack(vec![paths[1].clone()], StackAction::Unstack).unwrap();
        assert_eq!(updates.len(), 2);
        assert!(updates.iter().all(|u| u.stack.is_none()));
    }

    #[test]
    fn old_sidecars_and_stale_preview_and_card_refusal() {
        let f = folders();
        let _mode = CardMode::on(&f.card);
        let paths: Vec<String> = ["one.jpg", "two.jpg"]
            .map(|n| {
                let p = f.dcim.join(n);
                std::fs::write(&p, b"fixture").unwrap();
                p.to_string_lossy().into_owned()
            })
            .into();
        let before = snapshot(&f.card);
        assert!(edit_photo_stack(paths.clone(), StackAction::Stack).is_err());
        assert!(apply_photo_stacks(vec![paths.clone()]).is_err());
        assert_eq!(snapshot(&f.card), before);
        drop(_mode);
        let _mode = CardMode::off();
        edit_photo_stack(paths.clone(), StackAction::Stack).unwrap();
        assert!(
            apply_photo_stacks(vec![paths])
                .unwrap_err()
                .contains("preview again")
        );
        let old: ImageMetadata =
            serde_json::from_str(r#"{"version":1,"rating":3,"adjustments":null,"tags":null}"#)
                .unwrap();
        assert!(old.stack.is_none());
        assert!(!serde_json::to_string(&old).unwrap().contains("stack"));
    }
}

#[cfg(test)]
mod failure_tests {
    use super::*;
    use crate::file_management::card_mode_test_support::{CardMode, folders};
    #[test]
    fn failed_batch_restores_existing_bytes_and_removes_new_sidecars() {
        let _mode = CardMode::off();
        let f = folders();
        let paths: Vec<PathBuf> = ["a.jpg", "b.jpg", "c.jpg"]
            .map(|n| f.library.join(n))
            .into();
        let (_, first) = parse_virtual_path(&paths[0].to_string_lossy());
        let original = b"{\"version\":1, \"rating\":5, \"adjustments\":null}";
        std::fs::write(&first, original).unwrap();
        let updates = group_updates(&paths).unwrap();
        let (_, last) = parse_virtual_path(&paths[2].to_string_lossy());
        let err = persist_with(&updates, |path, bytes| {
            if path == last {
                return Err(std::io::Error::other("injected I/O failure"));
            }
            write_file_atomically(path, bytes)
        })
        .err()
        .unwrap();
        assert!(err.contains("injected I/O failure"));
        assert_eq!(std::fs::read(first).unwrap(), original);
        let (_, second) = parse_virtual_path(&paths[1].to_string_lossy());
        assert!(!second.exists());
    }
}

#[cfg(all(test, unix))]
mod alias_tests {
    use super::*;
    use crate::file_management::card_mode_test_support::{CardMode, folders};
    #[test]
    fn symlinked_library_updates_preserve_ui_paths_across_stack_actions() {
        let _mode = CardMode::off();
        let f = folders();
        for name in ["one.jpg", "two.jpg"] {
            std::fs::write(f.library.join(name), b"fixture").unwrap();
        }
        let alias = f.card.parent().unwrap().join("alias-library");
        std::os::unix::fs::symlink(&f.library, &alias).unwrap();
        let paths: Vec<String> = ["one.jpg", "two.jpg"]
            .map(|n| alias.join(n).to_string_lossy().into_owned())
            .into();
        let updates = edit_photo_stack(paths.clone(), StackAction::Stack).unwrap();
        assert_eq!(
            updates.iter().map(|u| u.path.clone()).collect::<Vec<_>>(),
            paths
        );
        let updates = edit_photo_stack(vec![paths[1].clone()], StackAction::Cover).unwrap();
        assert!(updates.iter().all(|u| paths.contains(&u.path)));
        assert_eq!(updates[0].path, paths[1]);
        let targets = metadata_paths(&[paths[0].clone()]).unwrap();
        assert!(targets.iter().all(|p| paths.contains(p)));
        assert_eq!(targets.len(), 2);
        let updates = edit_photo_stack(vec![paths[0].clone()], StackAction::Unstack).unwrap();
        assert!(
            updates
                .iter()
                .all(|u| paths.contains(&u.path) && u.stack.is_none())
        );
        let updates = apply_photo_stacks(vec![paths.clone()]).unwrap();
        assert_eq!(
            updates.iter().map(|u| u.path.clone()).collect::<Vec<_>>(),
            paths
        );
        let real = f.library.join("one.jpg").to_string_lossy().into_owned();
        assert_eq!(expanded(&[paths[0].clone(), real]).unwrap().len(), 2);
    }
}

#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use crate::file_management::{
        card_mode_test_support::{CardMode, folders},
        update_sidecar,
    };
    #[test]
    fn stack_batch_and_adjustment_write_are_serialized_without_losing_either_change() {
        let _mode = CardMode::off();
        let f = folders();
        let a = f.library.join("one.jpg");
        let b = f.library.join("two.jpg");
        std::fs::write(&a, b"fixture").unwrap();
        std::fs::write(&b, b"fixture").unwrap();
        let (_, sidecar) = parse_virtual_path(&a.to_string_lossy());
        update_sidecar(&sidecar, |m| {
            m.adjustments = serde_json::json!({"exposure":1.2})
        })
        .unwrap();
        let updates = group_updates(&[a.clone(), b]).unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (allow_tx, allow_rx) = std::sync::mpsc::channel();
        let allow_rx = Mutex::new(allow_rx);
        let batch = std::thread::spawn(move || {
            let first = std::sync::atomic::AtomicBool::new(true);
            persist_with(&updates, |path, bytes| {
                if first.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    entered_tx.send(()).unwrap();
                    allow_rx.lock().unwrap().recv().unwrap();
                }
                write_file_atomically(path, bytes)
            })
            .unwrap()
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let reset_path = sidecar.clone();
        let (reset_started_tx, reset_started_rx) = std::sync::mpsc::channel();
        let reset = std::thread::spawn(move || {
            reset_started_tx.send(()).unwrap();
            update_sidecar(&reset_path, |m| m.adjustments = serde_json::json!({})).unwrap()
        });
        reset_started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        allow_tx.send(()).unwrap();
        batch.join().unwrap();
        reset.join().unwrap();
        let saved = metadata(&sidecar).unwrap();
        assert!(saved.stack.is_some());
        assert_eq!(saved.adjustments, serde_json::json!({}));
        // Reverse order: a reset already committed before a new stack mutation
        // must never be replaced by a previously loaded adjustment snapshot.
        let _snapshot = saved.clone();
        update_sidecar(&sidecar, |m| {
            m.adjustments = serde_json::json!({"exposure":0.3})
        })
        .unwrap();
        edit_photo_stack(vec![a.to_string_lossy().into_owned()], StackAction::Cover).unwrap();
        assert_eq!(
            metadata(&sidecar).unwrap().adjustments,
            serde_json::json!({"exposure":0.3})
        );
    }
}
