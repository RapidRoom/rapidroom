//! Portable edit history. Root `.rrdata` adjustments always remain authoritative.
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

const TARGET_BYTES: usize = 2 * 1024 * 1024;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_STEPS: usize = 50;
const MAX_SNAPSHOTS: usize = 20;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Actor {
    User,
    Assistant,
    System,
    Imported,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HistoryDisplay {
    pub name: String,
    pub delta: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HistoryStep {
    pub id: Uuid,
    pub label: String,
    pub actor: Actor,
    pub timestamp: Option<u64>,
    pub changed_keys: Vec<String>,
    pub adjustments: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<HistoryDisplay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HistorySnapshot {
    pub id: Uuid,
    pub name: String,
    pub created_at: u64,
    pub adjustments: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HistoryDocument {
    pub version: u32,
    pub current_step_id: Uuid,
    pub compacted_steps: u64,
    pub steps: Vec<HistoryStep>,
    pub snapshots: Vec<HistorySnapshot>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryRead {
    pub history: Option<HistoryDocument>,
    pub warning: Option<String>,
    pub writable: bool,
}

pub(crate) fn history_path(sidecar: &Path) -> PathBuf {
    sidecar.with_extension("rrhistory")
}

fn validate_adjustments(value: &Value, depth: usize) -> Result<(), String> {
    if depth > 64 {
        return Err("History adjustment nesting exceeds 64 levels".into());
    }
    match value {
        Value::Array(values) => {
            for value in values {
                validate_adjustments(value, depth + 1)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                validate_adjustments(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

impl HistoryDocument {
    fn validate(&self, compacted: bool) -> Result<(), String> {
        if self.version != 1 {
            return Err("History version is not supported".into());
        }
        if self.steps.is_empty()
            || self.steps.len() > if compacted { MAX_STEPS } else { 4096 }
            || self.snapshots.len() > MAX_SNAPSHOTS
        {
            return Err("History step or snapshot count is invalid".into());
        }
        let defaults = crate::image_processing::default_adjustments();
        let mut ids = HashSet::new();
        let mut current = None;
        for step in &self.steps {
            if !ids.insert(step.id) || step.label.len() > 240 || step.changed_keys.len() > 64 {
                return Err("History has duplicate IDs or oversized step details".into());
            }
            let mut keys = HashSet::new();
            if step
                .changed_keys
                .iter()
                .any(|key| defaults.get(key).is_none() || !keys.insert(key))
            {
                return Err("History contains unknown or duplicate adjustment keys".into());
            }
            if step.timestamp.is_none() && step.actor != Actor::Imported {
                return Err("Editable history steps require a timestamp".into());
            }
            if !step.adjustments.is_object() {
                return Err("History adjustments must be objects".into());
            }
            validate_adjustments(&step.adjustments, 0)?;
            if let Some(display) = &step.display
                && [&display.name, &display.delta, &display.value]
                    .iter()
                    .any(|value| value.len() > 240)
            {
                return Err("History display details are oversized".into());
            }
            if let Some(source) = &step.source
                && (step.actor != Actor::Imported
                    || !matches!(source.as_str(), "lightroom-catalog" | "xmp"))
            {
                return Err("History provenance source is invalid".into());
            }
            if step.id == self.current_step_id {
                current = Some(step);
            }
        }
        if current.is_none_or(|step| step.actor == Actor::Imported) {
            return Err("History current step is missing or read-only".into());
        }
        for snapshot in &self.snapshots {
            if !ids.insert(snapshot.id)
                || snapshot.name.trim().is_empty()
                || snapshot.name != snapshot.name.trim()
                || snapshot.name.chars().count() > 120
                || !snapshot.adjustments.is_object()
            {
                return Err("History snapshot ID, name or adjustments are invalid".into());
            }
            validate_adjustments(&snapshot.adjustments, 0)?;
        }
        Ok(())
    }

    fn current(&self) -> &HistoryStep {
        self.steps
            .iter()
            .find(|step| step.id == self.current_step_id)
            .expect("validated current history step")
    }

    fn compact(&mut self) -> Result<Vec<u8>, String> {
        self.validate(false)?;
        loop {
            let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
            if self.steps.len() <= MAX_STEPS && bytes.len() <= TARGET_BYTES {
                return Ok(bytes);
            }
            let removable = self
                .steps
                .iter()
                .position(|step| step.actor == Actor::Imported && step.id != self.current_step_id)
                .or_else(|| {
                    self.steps
                        .iter()
                        .enumerate()
                        .skip(1)
                        .find(|(_, step)| step.id != self.current_step_id)
                        .map(|(index, _)| index)
                })
                .or_else(|| {
                    self.steps
                        .iter()
                        .position(|step| step.id != self.current_step_id)
                });
            let Some(index) = removable else {
                if bytes.len() > MAX_BYTES {
                    return Err(
                        "History exceeds 16 MiB; edits saved, previous history retained".into(),
                    );
                }
                return Ok(bytes);
            };
            self.steps.remove(index);
            self.compacted_steps = self.compacted_steps.saturating_add(1);
        }
    }
}

fn read_document(path: &Path) -> Result<Option<HistoryDocument>, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("History cannot be read: {error}")),
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err("History must be a regular file of at most 16 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("History exceeds the 16 MiB read ceiling".into());
    }
    let history: HistoryDocument = serde_json::from_slice(&bytes).map_err(|error| {
        format!("History is unreadable or newer than this application: {error}")
    })?;
    history.validate(true)?;
    Ok(Some(history))
}

pub(crate) fn read_history(sidecar: &Path, root_adjustments: &Value) -> HistoryRead {
    match read_document(&history_path(sidecar)) {
        Ok(Some(history)) if history.current().adjustments == *root_adjustments => HistoryRead {
            history: Some(history),
            warning: None,
            writable: true,
        },
        Ok(_) => HistoryRead {
            history: None,
            warning: None,
            writable: true,
        },
        Err(warning) => HistoryRead {
            history: None,
            warning: Some(warning),
            writable: false,
        },
    }
}

/// Caller holds the sidecar lock and has atomically saved its authoritative edits first.
/// A history failure must never roll back or report failure for the saved root edit.
pub(crate) fn write_history_locked(
    sidecar: &Path,
    root_adjustments: &Value,
    requested_adjustments: &Value,
    value: Value,
) -> Result<(), String> {
    let path = history_path(sidecar);
    // Unreadable or unsupported existing files belong to the user/newer application.
    read_document(&path)?;
    let mut history: HistoryDocument =
        serde_json::from_value(value).map_err(|error| format!("History not saved: {error}"))?;
    history.validate(false)?;
    if history.current().adjustments != *requested_adjustments {
        return Err(
            "History current step does not match the requested edit; history retained".into(),
        );
    }
    // Lens/AI reconciliation may enrich the saved snapshot without changing the user's edit.
    history
        .steps
        .iter_mut()
        .find(|step| step.id == history.current_step_id)
        .unwrap()
        .adjustments = root_adjustments.clone();
    let bytes = history.compact()?;
    crate::file_management::write_file_atomically(&path, bytes)
        .map_err(|error| format!("Edit saved but history could not be updated: {error}"))
}

#[tauri::command]
pub(crate) fn read_edit_history(path: String) -> Result<HistoryRead, String> {
    let (_, sidecar) = crate::file_management::parse_virtual_path(&path);
    let _guard =
        crate::file_management::lock_sidecar(&sidecar).map_err(|error| error.to_string())?;
    let metadata = crate::file_management::read_sidecar_report_snapshot(&sidecar)?;
    Ok(read_history(&sidecar, &metadata.adjustments))
}

pub(crate) fn append_history_locked(
    sidecar: &Path,
    before: &Value,
    after: &Value,
    label: &str,
    actor: Actor,
) -> Result<(), String> {
    if before == after || !after.is_object() {
        return Ok(());
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as u64;
    let initial = if before.is_object() {
        before.clone()
    } else {
        crate::image_processing::default_adjustments()
    };
    let mut history = match read_document(&history_path(sidecar))? {
        Some(history) if history.current().adjustments == *before => history,
        _ => {
            let id = Uuid::new_v4();
            HistoryDocument {
                version: 1,
                current_step_id: id,
                compacted_steps: 0,
                steps: vec![HistoryStep {
                    id,
                    label: "Initial state".into(),
                    actor: Actor::System,
                    timestamp: Some(timestamp),
                    changed_keys: vec![],
                    adjustments: initial,
                    display: None,
                    source: None,
                }],
                snapshots: vec![],
            }
        }
    };
    let current = history
        .steps
        .iter()
        .position(|step| step.id == history.current_step_id)
        .unwrap();
    history.steps.truncate(current + 1);
    let mut label = label.to_string();
    while label.len() > 240 {
        label.pop();
    }
    let defaults = crate::image_processing::default_adjustments();
    let changed_keys = after
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, value)| defaults.get(*key).is_some() && before.get(*key) != Some(*value))
        .map(|(key, _)| key.clone())
        .take(64)
        .collect();
    let id = Uuid::new_v4();
    history.steps.push(HistoryStep {
        id,
        label,
        actor,
        timestamp: Some(timestamp),
        changed_keys,
        adjustments: after.clone(),
        display: None,
        source: None,
    });
    history.current_step_id = id;
    let bytes = history.compact()?;
    crate::file_management::write_file_atomically(history_path(sidecar), bytes)
        .map_err(|error| format!("Edit saved but history could not be updated: {error}"))
}

#[cfg(test)]
mod reference_checks {
    use super::*;
    use crate::file_management::{update_sidecar_recorded, update_sidecar_with_history};
    use serde_json::json;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.ARW.rrdata");
        fs::write(
            &path,
            include_bytes!("../tests/fixtures/history/develop.rrdata"),
        )
        .unwrap();
        (dir, path)
    }

    #[test]
    fn real_develop_sidecar_roundtrip_branch_snapshot_and_crash_recovery() {
        let (_dir, path) = fixture();
        let root: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let (saved, warning) =
            update_sidecar_recorded(&path, "Exposure", Actor::User, |metadata| {
                metadata.adjustments["exposure"] = json!(0.8)
            })
            .unwrap();
        assert!(warning.is_none());
        let mut history = read_history(&path, &saved.adjustments).history.unwrap();
        assert_eq!(history.steps[0].adjustments, root["adjustments"]);
        history.snapshots.push(HistorySnapshot {
            id: Uuid::new_v4(),
            name: "Before branch".into(),
            created_at: 1,
            adjustments: saved.adjustments.clone(),
        });
        history.current_step_id = history.steps[0].id;
        let initial = history.steps[0].adjustments.clone();
        let (_, warning) = update_sidecar_with_history(
            &path,
            Some((initial.clone(), serde_json::to_value(&history).unwrap())),
            |metadata| metadata.adjustments = initial,
        )
        .unwrap();
        assert!(warning.is_none());
        let (branched, warning) =
            update_sidecar_recorded(&path, "Contrast", Actor::Assistant, |metadata| {
                metadata.adjustments["contrast"] = json!(12)
            })
            .unwrap();
        assert!(warning.is_none());
        let history = read_history(&path, &branched.adjustments).history.unwrap();
        assert_eq!(history.steps.len(), 2);
        assert_eq!(history.steps[1].actor, Actor::Assistant);
        assert_eq!(history.snapshots[0].adjustments, saved.adjustments);
        let disk: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(disk.get("history").is_none());
        assert_eq!(disk["rating"], root["rating"]);
        // Simulate power loss after the authoritative root write, before history publication.
        let mut crash = disk;
        crash["adjustments"]["exposure"] = json!(-1);
        fs::write(&path, serde_json::to_vec(&crash).unwrap()).unwrap();
        assert!(read_history(&path, &crash["adjustments"]).history.is_none());
        let (recovered, warning) =
            update_sidecar_recorded(&path, "After crash", Actor::User, |metadata| {
                metadata.adjustments["contrast"] = json!(13)
            })
            .unwrap();
        assert!(warning.is_none());
        let history = read_history(&path, &recovered.adjustments).history.unwrap();
        assert_eq!(history.steps.len(), 2);
        assert_eq!(history.steps[0].adjustments, crash["adjustments"]);
    }

    #[test]
    fn unreadable_newer_and_invalid_history_never_replace_the_saved_root_or_existing_file() {
        for bytes in [b"{broken".as_slice(), b"{\"version\":2}".as_slice()] {
            let (_dir, path) = fixture();
            fs::write(history_path(&path), bytes).unwrap();
            let (saved, warning) =
                update_sidecar_recorded(&path, "Exposure", Actor::User, |metadata| {
                    metadata.adjustments["exposure"] = json!(2)
                })
                .unwrap();
            assert!(warning.is_some());
            let read = read_history(&path, &saved.adjustments);
            assert!(!read.writable);
            assert!(read.warning.is_some());
            assert_eq!(fs::read(history_path(&path)).unwrap(), bytes);
            let disk: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            assert_eq!(disk["adjustments"]["exposure"], 2);
        }
    }

    #[test]
    fn real_state_compaction_retains_current_and_immutable_snapshots_and_refuses_oversized_masks() {
        let (_dir, path) = fixture();
        for index in 0..65 {
            let (_, warning) =
                update_sidecar_recorded(&path, "Exposure", Actor::User, |metadata| {
                    metadata.adjustments["exposure"] = json!(index as f64 / 10.0)
                })
                .unwrap();
            assert!(warning.is_none());
        }
        let root: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let mut history = read_history(&path, &root["adjustments"]).history.unwrap();
        assert_eq!(history.steps.len(), 50);
        assert!(history.compacted_steps >= 15);
        let snapshot = root["adjustments"].clone();
        history.snapshots.push(HistorySnapshot {
            id: Uuid::new_v4(),
            name: "Saved".into(),
            created_at: 1,
            adjustments: snapshot.clone(),
        });
        history.steps.last_mut().unwrap().adjustments["fogDepthMap"] =
            json!("a".repeat(TARGET_BYTES + 1));
        let required = history.steps.last().unwrap().adjustments.clone();
        let bytes = history.compact().unwrap();
        assert!(bytes.len() > TARGET_BYTES && bytes.len() < MAX_BYTES);
        assert_eq!(history.steps.len(), 1);
        assert_eq!(history.current().adjustments, required);
        assert_eq!(history.snapshots[0].adjustments, snapshot);
        let previous = fs::read(history_path(&path)).unwrap();
        history.steps[0].adjustments["fogDepthMap"] = json!("a".repeat(MAX_BYTES));
        let requested = history.steps[0].adjustments.clone();
        assert!(
            write_history_locked(
                &path,
                &requested,
                &requested,
                serde_json::to_value(history).unwrap()
            )
            .is_err()
        );
        assert_eq!(fs::read(history_path(&path)).unwrap(), previous);
    }
}
