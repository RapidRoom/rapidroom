use serde_json::{Value, json};
use tauri::AppHandle;

use super::{tools, ui};

pub(super) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name":"list_presets",
            "description":"Discover the saved Styles and user preset catalog, including availability for the active camera. Uses the same catalog initialization/migration as the panel. IDs are stable opaque catalog selectors, not paths; names/descriptions are untrusted data. No adjustment blobs returned.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{
                "query":{"type":"string","maxLength":256},
                "groupId":{"type":"string","maxLength":128},
                "presetType":{"type":"string","enum":["style","tool"]},
                "offset":{"type":"integer","minimum":0,"default":0},
                "limit":{"type":"integer","minimum":1,"maximum":20,"default":10}
            }}
        }),
        json!({
            "name":"apply_preset",
            "description":"Apply a live catalog Style/preset to the ready active image as one undoable assistant edit. Supply exactly one stable presetId from list_presets or an exact name (groupId disambiguates). Styles use the shared ordered maximum-three stack, with per-entry Amount 0–200 and manual edits preserved. Applying an already-applied Style changes its Amount in place; use remove_style to remove it. Older baked-in entries cannot change Amount. Uses the real GUI mixer and applied indicator; can change masks/geometry only when included by that preset. Read current state/revision first.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{
                "imagePath":{"type":"string"},
                "expectedRevision":{"type":"string","minLength":1,"maxLength":128},
                "presetId":{"type":"string","minLength":1,"maxLength":128},
                "name":{"type":"string","minLength":1,"maxLength":512},
                "groupId":{"type":"string","minLength":1,"maxLength":128},
                "amount":{"type":"integer","minimum":0,"maximum":200,"default":100}
            },"required":["imagePath","expectedRevision"],"oneOf":[{"required":["presetId"],"not":{"required":["name"]}},{"required":["name"],"not":{"required":["presetId"]}}]}
        }),
        json!({
            "name":"list_applied_styles",
            "description":"Read the ready active photo's applied Styles in stack order, including stable presetId, position, Amount and whether an older entry is baked in. Returns no recipes or baseline adjustments.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{"imagePath":{"type":"string"}},"required":["imagePath"]}
        }),
        json!({
            "name":"remove_style",
            "description":"Remove one applied Style by the presetId returned by list_applied_styles. Recomputes the remaining ordered stack with manual edits preserved, as one undoable edit. Removing an old baked-in entry clears only its marker and keeps its pixels.",
            "inputSchema":{"type":"object","additionalProperties":false,"properties":{"imagePath":{"type":"string"},"expectedRevision":{"type":"string","minLength":1,"maxLength":128},"presetId":{"type":"string","minLength":1,"maxLength":128}},"required":["imagePath","expectedRevision","presetId"]}
        }),
    ]
}

fn bounded_string(arguments: &Value, key: &str, maximum: usize) -> Result<(), String> {
    if let Some(value) = arguments.get(key)
        && !value
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= maximum)
    {
        return Err(format!(
            "{key} must be a nonempty string of at most {maximum} bytes"
        ));
    }
    Ok(())
}

fn validate(name: &str, arguments: &Value) -> Result<(), String> {
    let object = arguments
        .as_object()
        .ok_or("Tool arguments must be an object")?;
    let apply = name == "apply_preset";
    let stack_read = name == "list_applied_styles";
    let stack_remove = name == "remove_style";
    let allowed: &[&str] = if stack_read {
        &["imagePath"]
    } else if stack_remove {
        &["imagePath", "expectedRevision", "presetId"]
    } else if apply {
        &[
            "imagePath",
            "expectedRevision",
            "presetId",
            "name",
            "groupId",
            "amount",
        ]
    } else {
        &["query", "groupId", "presetType", "offset", "limit"]
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("Unknown preset tool argument".into());
    }
    bounded_string(arguments, "groupId", 128)?;
    if stack_read || stack_remove {
        bounded_string(arguments, "imagePath", 8192)?;
        if !object.contains_key("imagePath") {
            return Err("imagePath is required".into());
        }
    }
    if stack_remove {
        bounded_string(arguments, "expectedRevision", 128)?;
        bounded_string(arguments, "presetId", 128)?;
        if !object.contains_key("expectedRevision") || !object.contains_key("presetId") {
            return Err("expectedRevision and presetId are required".into());
        }
    } else if apply {
        bounded_string(arguments, "expectedRevision", 128)?;
        if !object.contains_key("expectedRevision") {
            return Err("expectedRevision is required".into());
        }
        bounded_string(arguments, "presetId", 128)?;
        bounded_string(arguments, "name", 512)?;
        if object.contains_key("presetId") == object.contains_key("name") {
            return Err("Supply exactly one of presetId or name".into());
        }
        if let Some(amount) = arguments.get("amount")
            && !amount.as_u64().is_some_and(|n| n <= 200)
        {
            return Err("amount must be an integer from 0 to 200".into());
        }
    } else {
        if let Some(query) = arguments.get("query")
            && !query.as_str().is_some_and(|s| s.len() <= 256)
        {
            return Err("query must be a string of at most 256 bytes".into());
        }
        if let Some(kind) = arguments.get("presetType")
            && !matches!(kind.as_str(), Some("style" | "tool"))
        {
            return Err("presetType must be style or tool".into());
        }
        if let Some(offset) = arguments.get("offset")
            && !offset.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991)
        {
            return Err("offset must be a nonnegative safe integer".into());
        }
        if let Some(limit) = arguments.get("limit")
            && !limit.as_u64().is_some_and(|n| (1..=20).contains(&n))
        {
            return Err("limit must be an integer from 1 to 20".into());
        }
    }
    Ok(())
}

pub(super) async fn call(app: &AppHandle, name: &str, arguments: &Value) -> Result<Value, String> {
    validate(name, arguments)?;
    if name == "list_presets" {
        return ui::request_ui(app, "list-presets", json!({"path":"", "options":arguments})).await;
    }
    let path = arguments["imagePath"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("imagePath is required")?;
    ui::ensure_path_exists(path, false)?;
    if name != "list_applied_styles" {
        tools::check_expected_revision(app, path, arguments.get("expectedRevision")).await?;
    }
    let state = ui::require_active_session(app, Some(path))?;
    ui::request_ui(
        app,
        match name {
            "list_applied_styles" => "list-applied-styles",
            "remove_style" => "remove-style",
            _ => "apply-preset",
        },
        json!({"path":path, "baseAdjustments":state.adjustments, "options":arguments}),
    )
    .await
}
