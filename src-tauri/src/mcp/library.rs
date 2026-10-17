use serde_json::{Value, json};
use tauri::AppHandle;

use super::{tools, ui};

pub(super) fn definitions() -> Vec<Value> {
    let active = json!({"imagePath":{"type":"string"},"expectedRevision":{"type":"string","minLength":1,"maxLength":128}});
    let mut definitions = Vec::new();
    for (name, description) in [
        (
            "create_virtual_copy",
            "Create a virtual copy from the ready active image's current draft, including unsaved edits, without changing its source. Returns the new editable/exportable imagePath and virtualCopy ID. Refuses Card mode. Does not switch images; open the returned copy before editing it.",
        ),
        (
            "assess_edits",
            "Assess the active recipe against fresh/reset defaults for this image. Reports masks, curves, grading, changed settings and history length. A recommendation to use a virtual copy protects substantial user edits; it is a technical heuristic, not a taste judgement. Does not edit or save.",
        ),
        (
            "request_delete_image",
            "Request the app's destructive confirmation dialog for the active image or virtual copy. Never deletes without the user clicking Confirm in the GUI. Deleting an original also deletes its virtual-copy sidecars; prefer keeping both. Returns confirmationPending, never a claim that deletion completed. Refuses Card mode.",
        ),
    ] {
        definitions.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"properties":active,"required":["imagePath","expectedRevision"]}}));
    }
    let mut reference = active;
    reference["referencePath"] = json!({"type":"string"});
    definitions.push(json!({"name":"open_reference_view","description":"Show the current active image alongside a separate read-only reference image/virtual copy using the existing Reference View. No wipe, edit, export or deletion. Read current revision first.","inputSchema":{"type":"object","additionalProperties":false,"properties":reference,"required":["imagePath","expectedRevision","referencePath"]}}));
    definitions.push(json!({"name":"set_image_metadata","description":"Set stars (0–5), colour label (or null), and/or pick/reject flag (or null) on 1–100 explicitly chosen images/virtual copies through the normal sidecar/XMP route. Positive stars clear an existing reject unless flag is explicitly supplied. Refuses Card mode before writes. If the active image is included, expectedRevision is required. Multi-image writes can partially succeed on filesystem errors; inspect failures before retrying.","inputSchema":{"type":"object","additionalProperties":false,"properties":{
        "imagePaths":{"type":"array","minItems":1,"maxItems":100,"uniqueItems":true,"items":{"type":"string"}},
        "expectedRevision":{"type":"string","minLength":1,"maxLength":128},
        "changes":{"type":"object","additionalProperties":false,"minProperties":1,"properties":{
            "rating":{"type":"integer","minimum":0,"maximum":5},
            "colorLabel":{"type":["string","null"],"enum":["red","yellow","green","blue","purple",null]},
            "flag":{"type":["string","null"],"enum":["pick","reject",null]}
        }}
    },"required":["imagePaths","changes"]}}));
    definitions
}

pub(super) fn validate_metadata(changes: &Value) -> Result<(), String> {
    let object = changes
        .as_object()
        .filter(|o| !o.is_empty())
        .ok_or("changes must be a nonempty metadata object")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "rating" | "colorLabel" | "flag"))
    {
        return Err("Unknown metadata field".into());
    }
    if let Some(rating) = changes.get("rating")
        && !rating.as_u64().is_some_and(|n| n <= 5)
    {
        return Err("rating must be an integer from 0 to 5".into());
    }
    if let Some(color) = changes.get("colorLabel")
        && !color.is_null()
        && !matches!(
            color.as_str(),
            Some("red" | "yellow" | "green" | "blue" | "purple")
        )
    {
        return Err("Unsupported colorLabel".into());
    }
    if let Some(flag) = changes.get("flag")
        && !flag.is_null()
        && !matches!(flag.as_str(), Some("pick" | "reject"))
    {
        return Err("flag must be pick, reject, or null".into());
    }
    Ok(())
}

pub(super) fn validate_paths(paths: &[String]) -> Result<(), String> {
    let unique: std::collections::HashSet<_> = paths.iter().collect();
    if paths.is_empty() || paths.len() > 100 || unique.len() != paths.len() {
        return Err("Supply 1–100 unique image paths".into());
    }
    for path in paths {
        if path.is_empty() || path.len() > 4096 || path.contains('\0') {
            return Err("Invalid image path".into());
        }
        ui::ensure_path_exists(path, false)?;
        let (source, sidecar) = crate::file_management::parse_virtual_path(path);
        if path.contains("?vc=") && !sidecar.is_file() {
            return Err("Virtual copy does not exist".into());
        }
        crate::file_management::ensure_card_writable(&source)?;
    }
    Ok(())
}

pub(super) async fn call(app: &AppHandle, name: &str, arguments: &Value) -> Result<Value, String> {
    let object = arguments
        .as_object()
        .ok_or("Tool arguments must be an object")?;
    if name == "set_image_metadata" {
        if let Some(expected) = arguments.get("expectedRevision")
            && !expected.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128)
        { return Err("expectedRevision must be a nonempty string of at most 128 bytes".into()); }
        if object
            .keys()
            .any(|k| !matches!(k.as_str(), "imagePaths" | "expectedRevision" | "changes"))
        {
            return Err("Unknown metadata argument".into());
        }
        let paths: Vec<String> = serde_json::from_value(arguments["imagePaths"].clone())
            .map_err(|_| "imagePaths must be an array of strings")?;
        validate_paths(&paths)?;
        validate_metadata(&arguments["changes"])?;
        let active = ui::active_session(app).filter(|state| paths.contains(&state.path));
        if let Some(state) = &active {
            if !object.contains_key("expectedRevision") {
                return Err("expectedRevision is required when updating the active image".into());
            }
            tools::check_expected_revision(app, &state.path, arguments.get("expectedRevision"))
                .await?;
        }
        return ui::request_ui(app, "set-image-metadata", json!({"path":active.as_ref().map(|s| s.path.as_str()).unwrap_or(""), "baseAdjustments":active.map(|s| s.adjustments), "imagePaths":paths,"changes":arguments["changes"]})).await;
    }
    if object.keys().any(|k| {
        !matches!(
            k.as_str(),
            "imagePath" | "expectedRevision" | "referencePath"
        )
    }) || (name != "open_reference_view" && object.contains_key("referencePath"))
    {
        return Err("Unknown editing workflow argument".into());
    }
    let path = arguments["imagePath"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("imagePath is required")?;
    let expected = arguments
        .get("expectedRevision")
        .filter(|v| v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128))
        .ok_or("expectedRevision is required (1–128 bytes)")?;
    ui::ensure_path_exists(path, false)?;
    tools::check_expected_revision(app, path, Some(expected)).await?;
    let current = ui::require_active_session(app, Some(path))?;
    if matches!(name, "create_virtual_copy" | "request_delete_image") {
        validate_paths(&[path.to_owned()])?;
    }
    let reference = if name == "open_reference_view" {
        let reference = arguments["referencePath"]
            .as_str()
            .filter(|s| !s.is_empty() && *s != path)
            .ok_or("referencePath must identify a different image")?;
        ui::ensure_path_exists(reference, false)?;
        let (_, sidecar) = crate::file_management::parse_virtual_path(reference);
        if reference.contains("?vc=") && !sidecar.is_file() {
            return Err("Reference virtual copy does not exist".into());
        }
        Some(reference)
    } else {
        None
    };
    let kind = match name {
        "create_virtual_copy" => "create-virtual-copy",
        "assess_edits" => "assess-edits",
        "open_reference_view" => "open-reference-view",
        _ => "request-delete-image",
    };
    ui::request_ui(
        app,
        kind,
        json!({"path":path,"baseAdjustments":current.adjustments,"referencePath":reference}),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsupported_metadata_and_preserves_explicit_null_semantics() {
        assert!(validate_metadata(&json!({"rating":5,"flag":null,"colorLabel":null})).is_ok());
        for invalid in [
            json!({}),
            json!({"rating":6}),
            json!({"rating":-1}),
            json!({"rating":2.5}),
            json!({"rating":null}),
            json!({"flag":false}),
            json!({"flag":"picked"}),
            json!({"colorLabel":"orange"}),
            json!({"adjustments":{}}),
        ] {
            assert!(validate_metadata(&invalid).is_err(), "{invalid}");
        }
    }
}
