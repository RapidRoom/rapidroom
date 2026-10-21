//! Preset provenance is photo metadata, never a processing adjustment.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppliedPreset {
    id: String,
    name: String,
    amount: f64,
    adjustments_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stack: Option<StyleStackRecord>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StyleStackRecord {
    version: u8,
    base: Value,
    entries: Vec<StyleEntryRecord>,
    overrides: Vec<Value>,
    result: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StyleEntryRecord {
    preset_id: String,
    name: String,
    amount: f64,
    adjustments: Value,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    baked_in: bool,
}

impl StyleStackRecord {
    fn valid(&self) -> bool {
        self.version == 1
            && self.entries.len() <= 3
            && self.base.is_object()
            && self.result.is_object()
            && self.entries.iter().all(|entry| {
                !entry.preset_id.trim().is_empty()
                    && entry.preset_id.len() <= 256
                    && !entry.name.trim().is_empty()
                    && entry.name.len() <= 1024
                    && entry.amount.is_finite()
                    && (0.0..=200.0).contains(&entry.amount)
                    && entry.adjustments.is_object()
            })
            && self
                .entries
                .iter()
                .map(|entry| &entry.preset_id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                == self.entries.len()
            && self.overrides.iter().all(|change| {
                change
                    .get("path")
                    .and_then(Value::as_array)
                    .is_some_and(|path| {
                        !path.is_empty()
                            && path.len() <= 32
                            && path.iter().all(|part| {
                                part.is_string() || part.get("id").is_some_and(Value::is_string)
                            })
                    })
            })
            && self.overrides.iter().all(|change| {
                change.get("order").is_none_or(|value| {
                    value.as_array().is_some_and(|ids| {
                        ids.len() <= 4096
                            && ids
                                .iter()
                                .all(|id| id.as_str().is_some_and(|id| id.len() <= 1024))
                    })
                })
            })
    }
}

impl AppliedPreset {
    fn valid(&self) -> bool {
        self.stack.as_ref().is_none_or(StyleStackRecord::valid)
            && !self.id.trim().is_empty()
            && self.id.len() <= 256
            && !self.name.trim().is_empty()
            && self.name.len() <= 1024
            && self.amount.is_finite()
            && (0.0..=200.0).contains(&self.amount)
            && self
                .adjustments_hash
                .strip_prefix("sha256-v1:")
                .is_some_and(|hash| {
                    hash.len() == 64
                        && hash
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
    }
}

fn read_record(sidecar: &Path) -> Result<Option<AppliedPreset>, String> {
    let bytes = match std::fs::read(sidecar) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let metadata: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(metadata
        .get("appliedPreset")
        .and_then(|value| serde_json::from_value::<AppliedPreset>(value.clone()).ok())
        .filter(AppliedPreset::valid))
}

#[tauri::command]
pub fn load_applied_preset(path: String) -> Result<Option<AppliedPreset>, String> {
    let (_, sidecar) = crate::file_management::parse_virtual_path(&path);
    read_record(&sidecar)
}

#[tauri::command]
pub fn save_applied_preset(
    path: String,
    record: Option<AppliedPreset>,
    adjustments: Option<Value>,
) -> Result<(), String> {
    if record.as_ref().is_some_and(|record| !record.valid())
        || adjustments.as_ref().is_some_and(|value| !value.is_object())
    {
        return Err("Invalid applied preset record".into());
    }
    if adjustments.is_some() && record.as_ref().is_some_and(|record| record.stack.is_none()) {
        return Err("Only Style stack saves may include their paired adjustments".into());
    }
    if let Some(stack) = record.as_ref().and_then(|record| record.stack.as_ref())
        && adjustments
            .as_ref()
            .is_some_and(|controls| controls != &stack.result)
    {
        return Err("Style stack controls must match their paired result".into());
    }
    let (source, sidecar) = crate::file_management::parse_virtual_path(&path);
    crate::file_management::ensure_card_writable(&source)?;
    crate::file_management::update_applied_preset_with_adjustments(
        &sidecar,
        serde_json::to_value(record).map_err(|e| e.to_string())?,
        adjustments,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record() -> AppliedPreset {
        AppliedPreset {
            id: "warm".into(),
            name: "Warm".into(),
            amount: 50.0,
            adjustments_hash: format!("sha256-v1:{}", "a".repeat(64)),
            stack: None,
        }
    }

    #[test]
    fn bounded_record_rejects_invalid_amount_hash_and_unbounded_payload() {
        assert!(record().valid());
        for amount in [-1.0, 201.0, f64::NAN, f64::INFINITY] {
            assert!(!AppliedPreset { amount, ..record() }.valid());
        }
        assert!(
            !AppliedPreset {
                id: "x".repeat(257),
                ..record()
            }
            .valid()
        );
        assert!(
            !AppliedPreset {
                name: "x".repeat(1025),
                ..record()
            }
            .valid()
        );
        assert!(
            !AppliedPreset {
                adjustments_hash: "arbitrary".into(),
                ..record()
            }
            .valid()
        );
        let mut value = serde_json::to_value(record()).unwrap();
        value["adjustments"] = json!({"exposure":99});
        assert!(serde_json::from_value::<AppliedPreset>(value).is_err());
    }

    #[test]
    fn dedicated_transaction_survives_restart_and_stale_ordinary_save_without_touching_adjustments()
    {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("photo.rrdata");
        let original = json!({"version":1,"rating":4,"adjustments":{"exposure":2},"unknown":{"keep":true},"exportFraming":{"instagram":{"keep":true}}});
        std::fs::write(&sidecar, serde_json::to_vec(&original).unwrap()).unwrap();
        crate::file_management::update_applied_preset(
            &sidecar,
            serde_json::to_value(record()).unwrap(),
        )
        .unwrap();
        assert_eq!(read_record(&sidecar).unwrap(), Some(record()));
        let saved: Value = serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(saved["adjustments"], original["adjustments"]);
        assert_eq!(saved["unknown"], original["unknown"]);
        let mut stale = original.clone();
        stale["rating"] = json!(5);
        stale.as_object_mut().unwrap().remove("exportFraming");
        crate::file_management::write_file_atomically(
            &sidecar,
            serde_json::to_vec(&stale).unwrap(),
        )
        .unwrap();
        let saved: Value = serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(read_record(&sidecar).unwrap(), Some(record()));
        assert_eq!(saved["exportFraming"], original["exportFraming"]);
        assert_eq!(saved["adjustments"], original["adjustments"]);
        assert_eq!(saved["rating"], 5);
        assert!(
            !saved["adjustments"]
                .as_object()
                .unwrap()
                .contains_key("appliedPreset")
        );
    }

    #[test]
    fn concurrent_framing_preset_and_stale_edit_writers_preserve_both_owned_fields() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("photo.rrdata");
        let original = json!({"adjustments":{"exposure":2},"unknown":{"keep":true}});
        std::fs::write(&sidecar, serde_json::to_vec(&original).unwrap()).unwrap();
        let start = std::sync::Barrier::new(3);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                start.wait();
                for _ in 0..12 {
                    crate::file_management::update_applied_preset(
                        &sidecar,
                        serde_json::to_value(record()).unwrap(),
                    )
                    .unwrap();
                }
            });
            scope.spawn(|| {
                start.wait();
                for revision in 0..12 {
                    crate::file_management::update_export_framing(&sidecar, |metadata| {
                        assert_eq!(metadata["unknown"], original["unknown"]);
                        Ok(json!({"instagram":{"revision":revision}}))
                    })
                    .unwrap();
                }
            });
            start.wait();
            for _ in 0..12 {
                crate::file_management::write_file_atomically(
                    &sidecar,
                    serde_json::to_vec(&original).unwrap(),
                )
                .unwrap();
            }
        });
        let saved: Value = serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(read_record(&sidecar).unwrap(), Some(record()));
        assert_eq!(saved["exportFraming"]["instagram"]["revision"], 11);
        assert_eq!(saved["adjustments"], original["adjustments"]);
        assert_eq!(saved["unknown"], original["unknown"]);
    }

    #[test]
    fn stack_sidecar_roundtrip_keeps_recipe_baseline_manual_edits_and_other_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.ARW").to_string_lossy().into_owned();
        let (_, sidecar) = crate::file_management::parse_virtual_path(&source);
        let original = json!({"rating":4,"unknown":{"keep":true},"exportFraming":{"instagram":{"keep":true}},"adjustments":{"exposure":0.7}});
        std::fs::write(&sidecar, serde_json::to_vec(&original).unwrap()).unwrap();
        let controls = json!({"exposure":1.25,"contrast":7,"shadows":15});
        let mut value = serde_json::to_value(record()).unwrap();
        value["stack"] = json!({"version":1,"base":{"exposure":0.7,"contrast":0},"entries":[
            {"presetId":"one","name":"One","amount":100,"adjustments":{"contrast":40}},
            {"presetId":"two","name":"Two","amount":50,"adjustments":{"shadows":30}}
        ],"overrides":[{"path":["exposure"],"value":1.25},{"path":["contrast"],"value":7}],"result":controls});
        let stack: AppliedPreset = serde_json::from_value(value.clone()).unwrap();
        save_applied_preset(source.clone(), Some(stack.clone()), Some(controls.clone())).unwrap();
        assert_eq!(
            load_applied_preset(source.clone()).unwrap(),
            Some(stack.clone())
        );
        assert!(
            save_applied_preset(
                source.clone(),
                Some(stack.clone()),
                Some(json!({"exposure":99}))
            )
            .is_err()
        );
        let saved: Value = serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(saved["adjustments"], controls);
        for key in ["rating", "unknown", "exportFraming"] {
            assert_eq!(saved[key], original[key]);
        }
        value["stack"]["entries"][1]["presetId"] = json!("one");
        let invalid: AppliedPreset = serde_json::from_value(value).unwrap();
        assert!(
            save_applied_preset(source.clone(), Some(invalid), Some(json!({"exposure":99})))
                .is_err()
        );
        let after_rejection: Value =
            serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(after_rejection, saved);
        let mut imported = saved.clone();
        imported["adjustments"]["exposure"] = json!(3);
        std::fs::write(&sidecar, serde_json::to_vec(&imported).unwrap()).unwrap();
        // Completing a paused metadata write retains recipe/Amount without
        // sending the pre-import controls back over the imported pixels.
        save_applied_preset(source.clone(), Some(stack.clone()), None).unwrap();
        let after_import: Value =
            serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(after_import["adjustments"], imported["adjustments"]);
        assert_eq!(load_applied_preset(source.clone()).unwrap(), Some(stack));
        save_applied_preset(source.clone(), None, Some(controls.clone())).unwrap();
        assert_eq!(load_applied_preset(source).unwrap(), None);
        let cleared: Value = serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
        assert_eq!(cleared["adjustments"], controls);
        assert_eq!(cleared["unknown"], original["unknown"]);
    }

    #[test]
    fn damaged_sidecar_is_not_overwritten_and_bad_record_does_not_poison_photo_edits() {
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("photo.rrdata");
        std::fs::write(&sidecar, b"{broken").unwrap();
        assert!(crate::file_management::update_applied_preset(&sidecar, json!(record())).is_err());
        assert_eq!(std::fs::read(&sidecar).unwrap(), b"{broken");
        std::fs::write(
            &sidecar,
            br#"{"adjustments":{"exposure":2},"appliedPreset":{"amount":"bad"}}"#,
        )
        .unwrap();
        assert_eq!(read_record(&sidecar).unwrap(), None);
    }
}
